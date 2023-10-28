use std::error::Error;
use std::fmt::{Display, Formatter};

use clap::{command, Args, Subcommand, ValueEnum};
use clap_stdin::MaybeStdin;
use geo::{BooleanOps, BoundingRect, Point, Polygon};
use geo_types::{polygon, Coord, Geometry};
use itertools::Itertools;
use s2::{cell::Cell, cellid::CellID, latlng::LatLng};
use wkt::{TryFromWkt, ToWkt};

use crate::format::{fmt_geometry, fmt_value_enum, OutputFormat};

//==================================================
// CLI spec.
//==================================================
#[derive(Debug, Args)]
#[command(about = "Commands related to S2 cells.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
pub struct S2Args {
    #[command(subcommand)]
    command: Option<S2Commands>,
}

#[derive(Debug, Subcommand)]
pub enum S2Commands {
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
            help = "The S2 cell level [1, 30] at which to perform the covering."
        )]
        level: u8,

        #[arg(long, default_value_t = S2CellFormat::Long, help = "Format for the S2 cell IDs.")]
        s2_cell_format: S2CellFormat,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each cell ID on separate lines.")]
        format: OutputFormat,

        #[arg(short, long, help = "Max number of S2 cells to return.")]
        max_num_s2_cells: Option<usize>,
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
            default_value_t = 12,
            help = "The S2 cell level at which to perform the covering."
        )]
        level: u8,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each cell ID on separate lines.")]
        format: OutputFormat,

        #[arg(short, long, help = "Max number of S2 cells to return.")]
        max_num_s2_cells: Option<usize>,
    },

    #[command(arg_required_else_help = true)]
    CellToPoly {
        #[arg(last = true, help = "A valid S2 cell index.")]
        cell: String,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum S2CellFormat {
    Long,
    Hex,
    Quad,
}
impl Display for S2CellFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fmt_value_enum(self, f)
    }
}

//==================================================
// Core subcommand logic.
//==================================================
pub fn handle_s2_subcommand(s2: &S2Args) -> Result<(), Box<dyn Error>> {
    match &s2.command {
        // Cover geometry.
        Some(S2Commands::Cover {
            wkt,
            level,
            s2_cell_format,
            format,
            max_num_s2_cells,
        }) => {
            let fmt_cell = |c: CellID| match s2_cell_format {
                S2CellFormat::Long => format!("{}", c.0),
                S2CellFormat::Hex => format!("{}", c.to_token()),
                S2CellFormat::Quad => format!("{:#?}", c),
            };

            let max_num_s2_cells = max_num_s2_cells.unwrap_or(usize::max_value());

            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
            let cover = get_s2_covering(&geometry, *level, max_num_s2_cells);

            match format {
                OutputFormat::Oneline => {
                    println!("{}", cover.into_iter().map(fmt_cell).join(","))
                }
                OutputFormat::CSV => cover
                    .into_iter()
                    .map(fmt_cell)
                    .for_each(|c| println!("{}", c)),
            }
        }

        // Cut a geometry by S2 cell regions.
        Some(S2Commands::Cut {
            wkt,
            level,
            format,
            max_num_s2_cells,
        }) => {
            let max_num_s2_cells = max_num_s2_cells.unwrap_or(usize::max_value());
            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
            let cover = get_s2_covering(&geometry, *level, max_num_s2_cells)
                .into_iter()
                .map(Cell::from)
                .collect_vec();
            let cuts = cut_region(geometry.try_into()?, &cover)
                .into_iter()
                .map(Geometry::from)
                .collect_vec();
            fmt_geometry(format, cuts);
        }

        Some(S2Commands::CellToPoly { cell }) => {
            let cell_id = CellID{ 0: cell.parse()? };
            let poly = s2_cell_to_poly(&cell_id.into());
            println!("{}", poly.wkt_string());
        }

        None => {}
    }
    Ok(())
}

//==================================================
// Geometry utils.
//==================================================
/**
 * Computes an S2 cell covering of the given geometry by first computing a bounding box and then
 * covering the bounding box. This is efficient but imprecise.
 */
fn get_s2_covering(geometry: &Geometry, level: u8, max_cells: usize) -> Vec<CellID> {
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
fn s2_cell_to_poly(cell: &Cell) -> Polygon {
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
fn cut_region(polygon: Polygon, s2_cells: &Vec<Cell>) -> Vec<Polygon> {
    s2_cells
        .iter()
        .map(s2_cell_to_poly)
        // We want each distinct polygon separated. No multipolygons.
        .flat_map(|p| p.intersection(&polygon).into_iter())
        .collect_vec()
}
