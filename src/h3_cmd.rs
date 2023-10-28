use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use clap::{command, Args, Subcommand, ValueEnum};
use clap_stdin::MaybeStdin;
use geo::{coord, polygon, BoundingRect, Geometry, LineString, Point, Polygon};
use h3o::error::InvalidLatLng;
use h3o::geom::{ContainmentMode, PolyfillConfig, ToCells};
use h3o::{CellIndex, LatLng, Resolution};
use itertools::Itertools;
use wkt::TryFromWkt;

use crate::format::{fmt_geometry, fmt_value_enum, OutputFormat};
use crate::geom::{cut_region, get_s2_covering};

#[derive(Debug, Args)]
#[command(about = "Commands related to H3 cells.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
pub struct H3Args {
    #[command(subcommand)]
    command: Option<H3Commands>,
}

#[derive(Debug, Subcommand)]
pub enum H3Commands {
    #[command(arg_required_else_help = true)]
    Cover {
        #[arg(
            last = true,
            help = "A valid WKT string encoding some geometry that will be subdivided."
        )]
        wkt: MaybeStdin<String>,

        #[arg(
            short,
            long,
            default_value_t = 12,
            help = "The H3 cell level [0, 15] at which to perform the covering."
        )]
        level: u8,

        #[arg(short, long, default_value_t = H3CoveringMode(ContainmentMode::IntersectsBoundary), help = "Mode for the polyfill algorithm. By default, this will choose the minimal covering that completely contains the argument WKT geometry.")]
        mode: H3CoveringMode,

        #[arg(long, default_value_t = H3CellFormat::Hex, help = "The output format for H3 cells.")]
        h3_cell_format: H3CellFormat,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each cell ID on separate lines.")]
        format: OutputFormat,
    },
}

#[derive(Debug, Copy, Clone)]
pub struct H3CoveringMode(ContainmentMode);
impl From<&str> for H3CoveringMode {
    fn from(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "containscentroid" | "centroid" => H3CoveringMode(ContainmentMode::ContainsCentroid),
            "containsboundary" | "contains" => H3CoveringMode(ContainmentMode::ContainsBoundary),
            _ => H3CoveringMode(ContainmentMode::IntersectsBoundary),
        }
    }
}
impl Display for H3CoveringMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl Into<ContainmentMode> for H3CoveringMode {
    fn into(self) -> ContainmentMode {
        self.0
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum H3CellFormat {
    Hex,
    Octal,
    Binary,
}
impl Display for H3CellFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fmt_value_enum(self, f)
    }
}

pub fn handle_h3_subcommand(h3: &H3Args) -> Result<(), Box<dyn Error>> {
    match &h3.command {
        Some(H3Commands::Cover {
            wkt,
            level,
            mode,
            h3_cell_format,
            format,
        }) => {
            let fmt_cell = |c: CellIndex| match &h3_cell_format {
                H3CellFormat::Hex => format!("{}", c),
                H3CellFormat::Octal => format!("{:o}", c),
                H3CellFormat::Binary => format!("{:b}", c),
            };

            // convenience shadow copies
            let mode: ContainmentMode = (*mode).into();
            let resolution: Resolution = Resolution::try_from(*level)?;
            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;

            let cells = get_h3_covering(geometry, resolution, mode)?;
            let mut cells = cells.into_iter().map(fmt_cell);
            match &format {
                OutputFormat::Oneline => println!("{}", cells.join(",")),
                OutputFormat::CSV => cells.for_each(|c| println!("{}", c)),
            }
        }

        None => {}
    }
    Ok(())
}

fn get_h3_covering(
    geometry: Geometry,
    resolution: Resolution,
    mode: ContainmentMode,
) -> Result<Vec<CellIndex>, Box<dyn Error>> {
    match geometry {
        // Point and point composite types.
        Geometry::Point(point) => get_h3_point_covering(point, resolution).map(|p| vec![p]),
        Geometry::MultiPoint(mpoint) => {
            mpoint
                .into_iter()
                .map(|p| get_h3_point_covering(p, resolution))
                .collect::<Result<Vec<CellIndex>, _>>()
        }

        // Polygon and polygon composite types.
        Geometry::Polygon(poly) => get_h3_polygon_covering(poly, resolution, mode),
        Geometry::MultiPolygon(mpoly) => {
            mpoly
                .into_iter()
                .map(|p| get_h3_polygon_covering(p, resolution, mode))
                .flatten_ok()
                .collect::<Result<Vec<CellIndex>, _>>()
        }

        // Recurse on geometry collection.
        Geometry::GeometryCollection(collection) => {
            collection
                .into_iter()
                .map(|g| get_h3_covering(g, resolution, mode))
                .flatten_ok()
                .collect::<Result<Vec<CellIndex>, _>>()
        }

        // Default to trying a polygon conversion for the remaining geometries.
        _ => get_h3_polygon_covering(geometry.try_into()?, resolution, mode),
    }
}

fn get_h3_point_covering(
    point: Point,
    resolution: Resolution,
) -> Result<CellIndex, Box<dyn Error>> {
    Ok(LatLng::from_radians(point.y(), point.x()).map(|c| c.to_cell(resolution))?)
}

fn get_h3_polygon_covering(
    polygon: Polygon,
    resolution: Resolution,
    mode: ContainmentMode,
) -> Result<Vec<CellIndex>, Box<dyn Error>> {
    let h3_poly = h3o::geom::Polygon::from_degrees(polygon.try_into()?)?;
    let config = PolyfillConfig::new(resolution).containment_mode(mode);
    let cells = h3_poly.to_cells(config).collect_vec();
    Ok(cells)
}
