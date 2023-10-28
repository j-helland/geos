use std::error::Error;

use clap::{command, Args, Subcommand};
use geo_types::{Coord, Geometry, Point};
use wkt::TryFromWkt;

use crate::format::{fmt_geometry, OutputFormat};
use crate::samplers::{create_rng, GeoSampler, PolygonalSampler, UniformSampler};

//==================================================
// CLI spec.
//==================================================
#[derive(Debug, Args)]
#[command(about = "Commands involving RNG.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
pub struct RandArgs {
    #[arg(short, long, default_value_t = 0, help = "Random seed to use")]
    seed: u64,

    #[command(subcommand)]
    command: Option<RandCommands>,
}

#[derive(Debug, Subcommand)]
pub enum RandCommands {
    Point {
        #[arg(short, long, help = "TODO")]
        wkt: Option<String>,

        #[arg(
            short,
            long,
            default_value_t = 1,
            help = "Number of samples to return."
        )]
        num_samples: u64,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each sampled point on a separate line. Specifying the oneline format will consolidate lines into a WKT GEOMETRYCOLLECTION on a single line.")]
        format: OutputFormat,
    },
}

//==================================================
// Core subcommand logic.
//==================================================
pub fn handle_rand_subcommand(rand: &RandArgs) -> Result<(), Box<dyn Error>> {
    let mut rng = create_rng(rand.seed);

    match &rand.command {
        Some(RandCommands::Point {
            wkt,
            num_samples,
            format,
        }) => {
            let coords: Vec<Coord> = match wkt {
                None => (0..*num_samples)
                    .map(|_| UniformSampler.sample_coord(&mut rng))
                    .collect(),

                Some(wkt) => {
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
                    let sampler = PolygonalSampler::new(geometry.try_into()?);
                    (0..*num_samples)
                        .map(|_| sampler.sample_coord(&mut rng))
                        .collect()
                }
            };

            let samples: Vec<Geometry> = coords
                .into_iter()
                .map(Point::from)
                .map(Geometry::from)
                .collect();

            fmt_geometry(format, samples);
        }

        None => {}
    }
    Ok(())
}
