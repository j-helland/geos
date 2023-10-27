use geo::{Area, TriangulateEarcut};
use geo_types::{Coord, Point, Polygon, Triangle};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use weighted_rand::{
    builder::{NewBuilder, WalkerTableBuilder},
    table::WalkerTable,
};

use crate::nvec::NVec;

const MIN_LAT: f64 = -90.0;
const MAX_LAT: f64 = 90.0;
const MIN_LNG: f64 = -180.0;
const MAX_LNG: f64 = 180.0;

pub fn create_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

pub trait GeoSampler<R> {
    fn sample_coord(&self, rng: &mut R) -> Coord;
}

pub struct UniformSampler;
impl<R: Rng> GeoSampler<R> for UniformSampler {
    fn sample_coord(&self, rng: &mut R) -> Coord {
        let dist_lat = Uniform::new(MIN_LAT, MAX_LAT);
        let dist_lng = Uniform::new(MIN_LNG, MAX_LNG);
        Coord {
            x: dist_lat.sample(rng),
            y: dist_lng.sample(rng),
        }
    }
}

/**
 * GeoSampler uniformly samples random coordinates within a polygonal geometry. Each GeoSampler
 * instance is tied to a specific polygon, which allows for more efficient repeated sampling calls.
 *
 * The underlying sampling algorithm is:
 * 1. Triangulate the polygon.
 * 2. Select a random triangle (with probability poroportional to the triangle's area).
 * 3. Sample a random point within the triangle.
 */
pub struct PolygonalSampler {
    triangulation: Vec<Triangle>,
    walker_table: WalkerTable,
}
impl<R: Rng> GeoSampler<R> for PolygonalSampler {
    fn sample_coord(&self, rng: &mut R) -> Coord {
        // Select a triangle with probability proportional to its area.
        let triangle = self.triangulation[self.walker_table.next_rng(rng)];
        sample_point_in_triangle(rng, triangle).into()
    }
}
impl PolygonalSampler {
    pub fn new(polygon: Polygon) -> Self {
        let mut cum_area: f32 = 0.0;
        let mut areas: Vec<f32> = vec![];
        let triangulation: Vec<Triangle> = polygon
            .earcut_triangles_iter()
            .map(|triangle| {
                let area: f32 = triangle.unsigned_area() as f32;
                cum_area += area;
                areas.push(area);
                triangle
            })
            .collect();

        let weights: Vec<f32> = areas.iter().map(|a| a / cum_area).collect();
        let builder = WalkerTableBuilder::new(&weights);

        Self {
            triangulation,
            walker_table: builder.build(),
        }
    }
}

/** Uniformly samples coordinates within a triangular region on the Earth's surface. */
fn sample_point_in_triangle<R: Rng>(rng: &mut R, triangle: Triangle) -> Point {
    let dist: Uniform<f64> = Uniform::new_inclusive(0.0, 1.0);
    let r1_sqrt = f64::sqrt(dist.sample(rng));
    let r2 = dist.sample(rng);

    // Randomly select a starting triangle vertex. Call this vertex `a`.
    let vertices = triangle.to_array();
    let na: NVec = vertices[0].into();
    let nb: NVec = vertices[1].into();
    let nc: NVec = vertices[2].into();

    let c: Coord = ((1.0 - r1_sqrt) * na + r1_sqrt * (1.0 - r2) * nb + r2 * r1_sqrt * nc).into();
    c.into()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use geo::Area;
    use geo_types::{Geometry, Polygon};
    use itertools::Itertools;
    use s2::{cell::Cell, cellid::CellID, latlng::LatLng};
    use statrs::distribution::{ChiSquared, ContinuousCDF};
    use wkt::TryFromWkt;

    use crate::{
        geom::{cut_region, get_s2_covering, s2_cell_to_poly},
        samplers::{create_rng, GeoSampler, PolygonalSampler},
    };

    const TEST_SEED: u64 = 0;

    /**
     * This test performs a chi squared fitness test for the polygon sampler. The implementation
     * indicates lack of uniformity; it may be necessary to tune the algorithm and/or fitness test
     * if true uniformity is required. Until then, the sampler should only be used in scenarios
     * where approximate uniformity is acceptable.
     *
     * This code is messy and not intended for anyone but me to read; I'm fine with that for now.
     */
    #[ignore]
    #[test]
    fn test_uniformity() {
        const WKT_STR: &str = "POLYGON ((-109.950142 38.19799, -109.888687 38.236292, -109.807663 38.157237, -109.929199 38.146438, -109.950142 38.19799))";

        let geometry = Geometry::<f64>::try_from_wkt_str(WKT_STR).unwrap();
        let sampler = PolygonalSampler::new(geometry.clone().try_into().unwrap());

        let level: u8 = 13;
        let s2_cover = get_s2_covering(&geometry, level, usize::max_value())
            .into_iter()
            .map(Cell::from)
            .collect_vec();

        //let mut bin_areas: HashMap<u64, f64> = HashMap::new();
        //s2_cover.iter().for_each(|c| {
        //    bin_areas.insert(c.id.0, s2_cell_to_poly(c).unsigned_area());
        //});
        //let bin_areas = bin_areas;

        let polygon: Polygon = geometry.try_into().unwrap();
        let mut cut_areas: HashMap<u64, f64> = HashMap::new();
        s2_cover.iter().for_each(|c| {
            let cuts = cut_region(&polygon, &vec![c.clone()]);
            if !cuts.is_empty() {
                let area = cuts[0].unsigned_area();
                cut_areas.insert(c.id.0, area);
            }
        });
        let cut_areas = cut_areas;

        let polygon_area = polygon.unsigned_area();
        println!("polygon_area {}", polygon_area);
        let mut bin_weights: HashMap<u64, f64> = HashMap::new();
        s2_cover.iter().map(|c| c.id.0).for_each(|id| {
            let ca = cut_areas.get(&id).unwrap_or(&0.0);
            bin_weights.insert(id, ca / polygon_area);
        });
        let bin_weights = bin_weights;

        //let mut s = 0.0;
        //for (id, bin_weight) in &bin_weights {
        //    s += bin_weight;
        //}
        //println!("bin_weights prob sum: {}", s);

        let mut bin_counts: HashMap<u64, u64> = HashMap::new();
        let bin_count = |id: CellID| *bin_counts.entry(id.0).or_default() += 1;

        let mut rng = create_rng(TEST_SEED);
        let num_samples = 1024;
        (0..num_samples)
            .map(|_| sampler.sample_coord(&mut rng))
            .map(|c| CellID::from(LatLng::from_degrees(c.y, c.x)))
            .map(|c| c.parent(level as u64))
            .for_each(bin_count);
        let bin_counts = bin_counts;

        let mut errors: Vec<f64> = vec![];
        for (id, bin_count) in &bin_counts {
            let weight = bin_weights.get(id).unwrap();
            let expected = weight * (num_samples as f64);
            let error = ((*bin_count as f64) - expected).powi(2) / expected;
            errors.push(error);
        }

        let degrees_of_freedom = (bin_counts.len() - 1) as f64;
        let chisq = ChiSquared::new(degrees_of_freedom).unwrap();

        let sum: f64 = errors.iter().sum();
        let pval = chisq.cdf(sum);

        println!("sum: {}, pval: {}", sum, pval);

        // p-value must be small enough to reject the null-hypothesis.
        assert!(pval < 0.05);
        assert!(false);
    }
}
