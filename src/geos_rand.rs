use geo::{Area, TriangulateEarcut};
use geo_types::{Coord, Point, Polygon, Triangle};
use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use s2::r3::vector::Vector;
use std::ops::{Add, Mul};
use weighted_rand::{
    builder::{NewBuilder, WalkerTableBuilder},
    table::WalkerTable,
};

const MIN_LAT: f64 = -90.0;
const MAX_LAT: f64 = 90.0;
const MIN_LNG: f64 = -180.0;
const MAX_LNG: f64 = 180.0;

/**
 * Linearly interpolate between two geographic coordinates.
 *
 * This implementation uses n-vectors as the underlying coordinate representation to perform linear
 * interpolation in a tangent space to the ellipsoid model of the Earth's surface.
 *
 * https://en.wikipedia.org/wiki/N-vector
 */
pub fn lerp(t: f64, c1: Coord, c2: Coord) -> Coord {
    let nvec_c1: NVec = c1.into();
    let nvec_c2: NVec = c2.into();

    let nv = (1.0 - t) * nvec_c1 + t * nvec_c2;
    let nv = (1.0 / (nv.norm() + 1e-8)) * nv;

    nv.into()
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

/**
 * Uniformly samples coordinates within a triangular region on the Earth's surface.
 * 1. Select a random vertex.
 * 2. Sample a point along the edge opposing the vertex.
 * 3. Sample a point along the edge connecting the vertex and previously sampled point.
 */
fn sample_point_in_triangle<R: Rng>(rng: &mut R, triangle: Triangle) -> Point {
    let dist_vertex: Uniform<usize> = Uniform::new(0, 3);
    let dist: Uniform<f64> = Uniform::new_inclusive(0.0, 1.0);

    // Randomly select a starting triangle vertex. Call this vertex `a`.
    let vertices = triangle.to_array();
    let idx = dist_vertex.sample(rng);
    let a = vertices[idx];
    let b = vertices[(idx + 1) % 3];
    let c = vertices[(idx + 2) % 3];

    // Select a random point along the edge opposing vertex `a`.
    let p_bc = lerp(dist.sample(rng), b, c);

    // Select a random point along the line connecting the selected vertex and the point on the
    // opposing edge.
    lerp(dist.sample(rng), a, p_bc).into()
}

/**
 * n-vectors are essentially elliptical surface normals that provide an alternate representation
 * for geographic coordinates in which certain operations like interpolation are straightforward.
 */
#[derive(Debug, Copy, Clone, PartialEq)]
struct NVec {
    x: f64,
    y: f64,
    z: f64,
}

impl NVec {
    pub fn norm(&self) -> f64 {
        // Lazy: re-use math from the S2 library since we already have that dependency.
        Vector {
            x: self.x,
            y: self.y,
            z: self.z,
        }
        .norm()
    }
}

impl Into<Coord> for NVec {
    fn into(self) -> Coord {
        let lat = f64::atan2(self.z, f64::sqrt(self.y * self.y + self.x * self.x));
        let lng = f64::atan2(self.y, self.x);
        Coord {
            x: to_angle(lng),
            y: to_angle(lat),
        }
    }
}

impl From<Coord> for NVec {
    fn from(c: Coord) -> NVec {
        let (lng, lat) = (to_radians(c.x), to_radians(c.y));
        let cos_lat = f64::cos(lat);
        NVec {
            x: f64::cos(lng) * cos_lat,
            y: f64::sin(lng) * cos_lat,
            z: f64::sin(lat),
        }
    }
}

impl Mul<f64> for NVec {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        return Self::Output {
            x: rhs * self.x,
            y: rhs * self.y,
            z: rhs * self.z,
        };
    }
}

impl Mul<NVec> for f64 {
    type Output = NVec;

    fn mul(self, rhs: Self::Output) -> Self::Output {
        return rhs * self;
    }
}

impl Add for NVec {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

fn to_radians(angle: f64) -> f64 {
    const CONVERT: f64 = std::f64::consts::PI / 180.0;
    CONVERT * angle
}

fn to_angle(rad: f64) -> f64 {
    const CONVERT: f64 = 180.0 / std::f64::consts::PI;
    CONVERT * rad
}

pub fn create_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}
