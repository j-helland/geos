use std::error::Error;

use clap::{command, Args, Subcommand};
use clap_stdin::MaybeStdin;
use geo::{Triangle, TriangulateEarcut};
use geo_types::{Geometry, Polygon};
use itertools::Itertools;
use wkt::TryFromWkt;

use crate::format::{fmt_geometry, OutputFormat};
use crate::geom::partition_region;

#[derive(Debug, Args)]
#[command(about = "General geometry commands.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
pub struct GeomArgs {
    #[command(subcommand)]
    command: Option<GeomCommands>,
}

#[derive(Debug, Subcommand)]
pub enum GeomCommands {
    #[command(arg_required_else_help = true)]
    Split {
        #[arg(
            last = true,
            help = "A valid WKT string encoding some geometry that will be subdivided."
        )]
        wkt: MaybeStdin<String>,

        #[arg(
            short,
            long,
            help = "Dictates the proportion that each subdivision's edge length should have relative to the geometry. For example, 0.5 subdivides into 4 quadrants, whild 0.3 subdivides into 9 quadrants. For values >= 1.0, the minimal bounding box will be returned."
        )]
        edge_proportion: f64,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each subdivision region as a WKT POLYGON on separate lines. Specifying the wkt format will consolidate these lines into a WKT GEOMETRYCOLLECTION and output a single line.")]
        format: OutputFormat,

        #[arg(
            short,
            long,
            help = "[optional] Any subdivisions must intersect with the geometry by at least this threshold. For example, 0.5 requires 50% overlap, while 1.0 can be used to select only subdivisions that are interior to the geometry. This argument may behave unintuitively for multi-geometries."
        )]
        threshold: Option<f64>,
    },

    Triangulate {
        #[arg(last = true)]
        wkt: MaybeStdin<String>,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each subdivision region as a WKT POLYGON on separate lines. Specifying the oneline format will consolidate these lines into a WKT GEOMETRYCOLLECTION and output a single line.")]
        format: OutputFormat,
    },
}

/**
 * Commands that operate primarily on geometries.
 */
pub fn handle_geom_subcommand(geom: &GeomArgs) -> Result<(), Box<dyn Error>> {
    match &geom.command {
        // Split geometry.
        Some(GeomCommands::Split {
            wkt,
            edge_proportion,
            format,
            threshold,
        }) => {
            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
            let polygon: Polygon = geometry.try_into()?;
            let partitions = partition_region(&polygon, *edge_proportion, *threshold)
                .into_iter()
                .map(Geometry::from)
                .collect_vec();
            fmt_geometry(format, partitions);
        }

        Some(GeomCommands::Triangulate { wkt, format }) => {
            let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
            let polygon: Polygon = geometry.try_into()?;
            let triangles: Vec<Geometry> = polygon
                .earcut_triangles_iter()
                .into_iter()
                .map(Triangle::into)
                .collect();
            fmt_geometry(format, triangles);
        }

        None => {}
    }
    Ok(())
}
