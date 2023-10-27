use clap::ValueEnum;
use geo_types::{Geometry, GeometryCollection};
use std::fmt::{Display, Formatter};
use wkt::ToWkt;

pub fn fmt_geometry(fmt: &OutputFormat, geometries: Vec<Geometry>) {
    match fmt {
        OutputFormat::CSV => {
            geometries
                .iter()
                .for_each(|p| println!("{}", p.wkt_string()));
        }
        OutputFormat::Oneline => {
            println!("{}", GeometryCollection::new_from(geometries).wkt_string());
        }
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum S2CellFormat {
    Long,
    Hex,
    Quad,
}
impl Display for S2CellFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fmt_enum(self, f)
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    CSV,
    Oneline,
}
impl Display for OutputFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fmt_enum(self, f)
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SplitStrategy {
    Bbox,
    Triangulate,
}
impl Display for SplitStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fmt_enum(self, f)
    }
}

fn fmt_enum<T: ValueEnum>(t: &T, f: &mut Formatter<'_>) -> std::fmt::Result {
    t.to_possible_value()
        .expect("no values are skipped")
        .get_name()
        .fmt(f)
}
