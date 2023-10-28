use geo::{Area, BooleanOps, BoundingRect, Intersects, Polygon, Rect};
use geo_types::{Coord, Line};

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
