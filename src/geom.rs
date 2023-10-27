use geo::{Area, BooleanOps, BoundingRect, Intersects, Point, Polygon, Rect};
use geo_types::{polygon, Coord, Geometry, Line};
use itertools::Itertools;
use s2::{cell::Cell, cellid::CellID, latlng::LatLng};

use crate::nvec::NVec;

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

/**
 * Computes an S2 cell covering of the given geometry by first computing a bounding box and then
 * covering the bounding box. This is efficient but imprecise.
 */
pub fn get_s2_covering(geometry: &Geometry, level: u8, max_cells: usize) -> Vec<CellID> {
    let bbox = geometry.bounding_rect().unwrap();
    let pmin: Point = bbox.min().try_into().unwrap();
    let pmax: Point = bbox.max().try_into().unwrap();
    let region = s2::rect::Rect::from_degrees(pmin.y(), pmin.x(), pmax.y(), pmax.x());

    // compute covering of the bounding box.
    let rc = s2::region::RegionCoverer {
        min_level: level,
        max_level: level,
        level_mod: 1,
        max_cells,
    };
    rc.covering(&region).0
}

/**
 * Creates a polygon from the vertices of an S2 cell.
 */
pub fn s2_cell_to_poly(cell: &Cell) -> Polygon {
    let vertices: [Coord; 4] = cell.vertices().map(LatLng::from).map(|c| Coord {
        x: c.lng.deg(),
        y: c.lat.deg(),
    });
    polygon!(vertices[0], vertices[1], vertices[2], vertices[3])
}

/**
 * Cuts a region using S2 cells. Each returned geometry in the collection will be a partition of
 * the geometry bounded to a passed in S2 cell.
 */
pub fn cut_region(polygon: &Polygon, s2_cells: &Vec<Cell>) -> Vec<Polygon> {
    s2_cells
        .iter()
        .map(s2_cell_to_poly)
        //// We want each distinct polygon separated. No multipolygons.
        //.flat_map(|p| p.intersection(&polygon).into_iter())
        .collect_vec()
}

/**
 * This algorithm approximately partitions a geometry into uniform subregions. First, the geometry
 * is approximated by its minimal bounding box. Then, the bounding box is divided into regions. The
 * edge_proportion argument determines the region size. For example, edge_proportion = 0.5 would divide into 4 regions.
 * edge_proportion = 0.33 would divide into 9 regions.
 */
pub fn partition_region(
    polygon: &Polygon,
    edge_proportion: f64,
    area_threshold: Option<f64>,
) -> Vec<Polygon> {
    let mut partitions: Vec<Polygon> = vec![];
    let bbox = polygon.bounding_rect().unwrap();

    // This ensures that we return bbox in cases where edge_proportion > 1.0 i.e. would correspond
    // to a dilation.
    let edge_budget = f64::max(1.0, edge_proportion);

    // Bounding box corners.
    //  c1 -- c2
    //  |     |
    // c0 -- c3
    let c0 = bbox.min();
    let c1 = Coord {
        x: bbox.min().x,
        y: bbox.max().y,
    };
    let c2 = bbox.max();
    let c3 = Coord {
        x: bbox.max().x,
        y: bbox.min().y,
    };

    // Forward axes.
    let l01_lerp = |t| lerp(t, c0, c1);
    let l32_lerp = |t| lerp(t, c3, c2);

    // Sweep in the c0 -> c1 direction.
    let mut fx = 0.0;
    let mut lp_prev = Line { start: c0, end: c3 };
    while fx < edge_budget {
        // This is a line in the c0 -> c3 direction that is lerped towards c1. We use this to
        // compute the upper right corner of the rect.
        let lp = Line {
            start: l01_lerp(fx + edge_proportion),
            end: l32_lerp(fx + edge_proportion),
        };

        // Sweep in the c0 -> c3 direction.
        let mut fy = 0.0;
        while fy < edge_budget {
            // The rect can be defined in terms of two corners.
            let partition = Rect::new(
                lerp(fy, lp_prev.start, lp_prev.end),
                lerp(fy + edge_proportion, lp.start, lp.end),
            );

            // Not all partitions computed from the minimal bounding box intersect with the
            // underlying geometry.
            match area_threshold {
                Some(threshold) => {
                    // More expensive selection criterion based on the amount of intersection.
                    let intersection = polygon.intersection(&partition.to_polygon());
                    let area_ratio = intersection.unsigned_area() / partition.unsigned_area();
                    if area_ratio >= threshold {
                        partitions.push(partition.into());
                    }
                }

                None => {
                    // Fast selection criterion of detecting any intersection. This is the deafult.
                    if partition.intersects(polygon) {
                        partitions.push(partition.into());
                    }
                }
            }

            fy += edge_proportion;
        }
        fx += edge_proportion;
        lp_prev = lp;
    }

    partitions
}
