use geo_types::Coord;
use s2::r3::vector::Vector;
use std::ops::{Add, Mul};

/**
 * n-vectors are essentially elliptical surface normals that provide an alternate representation
 * for geographic coordinates in which certain operations like interpolation are straightforward.
 */
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct NVec {
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
