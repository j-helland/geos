mod format;
mod geom;
mod geom_cmd;
mod h3_cmd;
mod nvec;
mod rand_cmd;
mod s2_cmd;
mod samplers;

use std::{error::Error, io};

use clap::{command, Parser, Subcommand};

use geom_cmd::{handle_geom_subcommand, GeomArgs};
use h3_cmd::{handle_h3_subcommand, H3Args};
use rand_cmd::{handle_rand_subcommand, RandArgs};
use s2_cmd::{handle_s2_subcommand, S2Args};

//==================================================
// CLI spec.
//==================================================
#[derive(Parser)]
#[command(name = "GeoS")]
#[command(author = "jwh")]
#[command(version = "0.0.0")]
#[command(about = "GeoS: Commandline tool for some handy geographic operations.", long_about = None)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    S2(S2Args),
    H3(H3Args),
    Geom(GeomArgs),
    Rand(RandArgs),
}

//==================================================
// CLI runtime logic.
//==================================================
fn collect_args() -> Vec<String> {
    // Args read from the commandline.
    let mut args: Vec<String> = std::env::args().collect();

    // Args possibly read from stdin via redirection. This allows for piping values from other
    // commands.
    if !atty::is(atty::Stream::Stdin) {
        // Redirection has occurred.
        let stdin = io::stdin();
        let stdin_args: Vec<String> = stdin.lines().map(Result::unwrap).collect();
        args.extend_from_slice(stdin_args.as_slice());
    }

    args
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse_from(collect_args().iter());
    match &cli.command {
        Some(Commands::S2(s2)) => handle_s2_subcommand(s2),
        Some(Commands::H3(h3)) => handle_h3_subcommand(h3),
        Some(Commands::Geom(geom)) => handle_geom_subcommand(geom),
        Some(Commands::Rand(rand)) => handle_rand_subcommand(rand),
        None => Ok(()),
    }
}
