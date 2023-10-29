use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use clap::{command, Args, Subcommand, ValueEnum};
use clap_stdin::MaybeStdin;
use geo::{BooleanOps, Geometry, LineString, Point, Polygon};
use geo_types::coord;
use h3o::geom::{ContainmentMode, PolyfillConfig, ToCells};
use h3o::{CellIndex, LatLng, Resolution};
use itertools::Itertools;
use wkt::{ToWkt, TryFromWkt};

use crate::format::{fmt_geometry, fmt_value_enum, OutputFormat};

//==================================================
// CLI spec.
//==================================================
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

        #[arg(
            short,
            long,
            default_value_t = H3CoveringMode(ContainmentMode::IntersectsBoundary),
            help = "Mode for the polyfill algorithm. By default, this will choose the minimal covering that completely contains the geometry."
        )]
        mode: H3CoveringMode,

        #[arg(
            long,
            default_value_t = H3CellFormat::Hex,
            help = "The output format for H3 cells."
        )]
        h3_cell_format: H3CellFormat,

        #[arg(
            short,
            long,
            default_value_t = OutputFormat::CSV,
            help = "By default, outputs each cell ID on separate lines."
        )]
        format: OutputFormat,
    },

    #[command(arg_required_else_help = true)]
    Cut {
        #[arg(
            last = true,
            help = "A valid WKT string encoding some geometry that will be subdivided."
        )]
        wkt: MaybeStdin<String>,

        #[arg(
            short,
            long,
            default_value_t = 6,
            help = "The H3 cell level at which to perform the covering."
        )]
        level: u8,

        #[arg(
            short,
            long,
            default_value_t = OutputFormat::CSV,
            help = "By default, outputs each cell ID on separate lines."
        )]
        format: OutputFormat,
    },

    #[command(arg_required_else_help = true)]
    CellToPoly {
        #[arg(last = true, help = "A valid H3 cell index.")]
        cell: String,
    },

    #[command(arg_required_else_help = true)]
    Compact {
        #[arg(
            last = true,
            num_args = 1..,
            use_value_delimiter = true,
            value_delimiter = ',',
            help = "A comma-separated list of H3 cell indices to compact."
        )]
        cells: Vec<String>,

        #[arg(
            long,
            default_value_t = H3CellFormat::Hex,
            help = "The output format for H3 cells."
        )]
        h3_cell_format: H3CellFormat,

        #[arg(
            short,
            long,
            default_value_t = OutputFormat::CSV,
            help = "By default, outputs each cell ID on separate lines."
        )]
        format: OutputFormat,
    },

    #[command(arg_required_else_help = true)]
    Uncompact {
        #[arg(
            last = true,
            num_args = 1..,
            use_value_delimiter = true,
            value_delimiter = ',',
            help = "A comma-separated list of H3 cell indices to uncompact."
        )]
        cells: Vec<String>,

        #[arg(short, long, help = "The H3 cell level at which to uncompact to.")]
        level: u8,

        #[arg(long, default_value_t = H3CellFormat::Hex, help = "The output format for H3 cells.")]
        h3_cell_format: H3CellFormat,

        #[arg(
            short,
            long,
            default_value_t = OutputFormat::CSV,
            help = "By default, outputs each cell ID on separate lines."
        )]
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

//==================================================
// Core logic for subcommands.
//==================================================
fn fmt_cell(format: &H3CellFormat, c: &CellIndex) -> String {
    match &format {
        H3CellFormat::Hex => format!("{}", c),
        H3CellFormat::Octal => format!("{:o}", c),
        H3CellFormat::Binary => format!("{:b}", c),
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
            // convenience shadow copies
            let mode: ContainmentMode = (*mode).into();
            let resolution = Resolution::try_from(*level)?;
            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
            let cells = get_h3_covering(&geometry, resolution, mode)?;

            // Output
            let mut cells = cells.iter().map(|c| fmt_cell(h3_cell_format, c));
            match &format {
                OutputFormat::Oneline => println!("{}", cells.join(",")),
                OutputFormat::CSV => cells.for_each(|c| println!("{}", c)),
            }
        }

        Some(H3Commands::Cut { wkt, level, format }) => {
            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
            let resolution = Resolution::try_from(*level)?;
            let cover =
                get_h3_covering(&geometry, resolution, ContainmentMode::IntersectsBoundary)?;
            let cuts = cut_geometry(&geometry, &cover)?
                .into_iter()
                .map(Geometry::from)
                .collect_vec();
            fmt_geometry(format, cuts)
        }

        Some(H3Commands::CellToPoly { cell }) => {
            let cell = CellIndex::from_str(cell)?;
            let poly = h3_cell_to_poly(&cell);
            println!("{}", poly.wkt_string());
        }

        Some(H3Commands::Compact {
            cells,
            h3_cell_format,
            format,
        }) => {
            let cells: Vec<CellIndex> = cells
                .into_iter()
                .map(|s| s.as_str())
                .map(CellIndex::from_str)
                .try_collect()?;
            let cells_compacted = CellIndex::compact(cells)?.collect_vec();

            // Output
            let mut cells_compacted = cells_compacted.iter().map(|c| fmt_cell(h3_cell_format, c));
            match &format {
                OutputFormat::Oneline => println!("{}", cells_compacted.join(",")),
                OutputFormat::CSV => cells_compacted.for_each(|c| println!("{}", c)),
            }
        }

        Some(H3Commands::Uncompact {
            cells,
            level,
            h3_cell_format,
            format,
        }) => {
            let resolution = Resolution::try_from(*level)?;
            let cells: Vec<CellIndex> = cells
                .into_iter()
                .map(|s| s.as_str())
                .map(CellIndex::from_str)
                .try_collect()?;
            let cells_uncompacted = CellIndex::uncompact(cells, resolution).collect_vec();

            // Output
            let mut cells_uncompacted = cells_uncompacted
                .iter()
                .map(|c| fmt_cell(h3_cell_format, c));
            match &format {
                OutputFormat::Oneline => println!("{}", cells_uncompacted.join(",")),
                OutputFormat::CSV => cells_uncompacted.for_each(|c| println!("{}", c)),
            }
        }

        None => {}
    }
    Ok(())
}

//==================================================
// Geometry utils
//==================================================
fn cut_geometry(
    geometry: &Geometry,
    cells: &Vec<CellIndex>,
) -> Result<Vec<Polygon>, Box<dyn Error>> {
    let partitions = cells.iter().map(h3_cell_to_poly).collect_vec();

    Ok(match &geometry {
        Geometry::Polygon(poly) => partitions
            .iter()
            .map(|p| p.intersection(poly))
            .flatten()
            .collect_vec(),

        Geometry::MultiPolygon(mpoly) => mpoly
            .iter()
            .flat_map(|mp| partitions.iter().map(|p| p.intersection(mp)).flatten())
            .collect_vec(),

        // Recurse.
        Geometry::GeometryCollection(collection) => collection
            .into_iter()
            .map(|g| cut_geometry(g, cells))
            .flatten_ok()
            .collect::<Result<Vec<Polygon>, _>>()?,

        // Default to trying a polygon conversion.
        _ => {
            let poly = Polygon::try_from(geometry.clone())?;
            partitions
                .iter()
                .map(|p| p.intersection(&poly))
                .flatten()
                .collect_vec()
        }
    })
}

/**
 * Creates a polygon from the vertices of an H3 cell. This will be a hexagon in most cases, except
 * for the pentagons on icosahedron vertices.
 */
fn h3_cell_to_poly(cell_id: &CellIndex) -> Polygon {
    let boundary = cell_id.boundary();
    let vertices = boundary
        .iter()
        .map(|v| coord![x: v.lng(), y: v.lat()])
        .collect_vec();
    Polygon::new(LineString::from(vertices), vec![])
}

fn get_h3_covering(
    geometry: &Geometry,
    resolution: Resolution,
    mode: ContainmentMode,
) -> Result<Vec<CellIndex>, Box<dyn Error>> {
    match geometry {
        // Point and point composite types.
        Geometry::Point(point) => get_h3_point_covering(point, resolution).map(|p| vec![p]),
        Geometry::MultiPoint(mpoint) => mpoint
            .into_iter()
            .map(|p| get_h3_point_covering(p, resolution))
            .collect::<Result<Vec<CellIndex>, _>>(),

        // Polygon and polygon composite types.
        Geometry::Polygon(poly) => get_h3_polygon_covering(poly, resolution, mode),
        Geometry::MultiPolygon(mpoly) => mpoly
            .into_iter()
            .map(|p| get_h3_polygon_covering(p, resolution, mode))
            .flatten_ok()
            .collect::<Result<Vec<CellIndex>, _>>(),

        // Recurse on geometry collection.
        Geometry::GeometryCollection(collection) => collection
            .into_iter()
            .map(|g| get_h3_covering(g, resolution, mode))
            .flatten_ok()
            .collect::<Result<Vec<CellIndex>, _>>(),

        // Default to trying a polygon conversion for the remaining geometries.
        _ => get_h3_polygon_covering(&geometry.clone().try_into()?, resolution, mode),
    }
}

fn get_h3_point_covering(
    point: &Point,
    resolution: Resolution,
) -> Result<CellIndex, Box<dyn Error>> {
    Ok(LatLng::from_radians(point.y(), point.x()).map(|c| c.to_cell(resolution))?)
}

fn get_h3_polygon_covering(
    polygon: &Polygon,
    resolution: Resolution,
    mode: ContainmentMode,
) -> Result<Vec<CellIndex>, Box<dyn Error>> {
    let h3_poly = h3o::geom::Polygon::from_degrees(polygon.clone().try_into()?)?;
    let config = PolyfillConfig::new(resolution).containment_mode(mode);
    let cells = h3_poly.to_cells(config).collect_vec();
    Ok(cells)
}
