use std::error::Error;
use std::fmt::{Display, Formatter};

use clap::{command, Args, Subcommand, ValueEnum};
use clap_stdin::MaybeStdin;
use geo_types::Geometry;
use itertools::Itertools;
use s2::{cell::Cell, cellid::CellID};
use wkt::TryFromWkt;

use crate::format::{fmt_geometry, fmt_value_enum, OutputFormat};
use crate::geom::{cut_region, get_s2_covering};

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

/**
 * Commands that operate primarily on S2 cells.
 */
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
            let cuts = cut_region(&geometry.try_into()?, &cover)
                .into_iter()
                .map(Geometry::from)
                .collect_vec();
            fmt_geometry(format, cuts);
        }

        None => {}
    }
    Ok(())
}
