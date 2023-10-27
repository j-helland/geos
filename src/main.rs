mod format;
mod geom;
mod geom_cmd;
mod nvec;
mod rand_cmd;
mod s2_cmd;
mod samplers;

use std::{error::Error, io};

use clap::{command, Parser, Subcommand};

use geom_cmd::{GeomArgs, handle_geom_subcommand};
use rand_cmd::{RandArgs, handle_rand_subcommand};
use s2_cmd::{S2Args, handle_s2_subcommand};

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
        Some(Commands::Geom(geom)) => handle_geom_subcommand(geom),
        Some(Commands::Rand(rand)) => handle_rand_subcommand(rand),
        None => Ok(()),
    }
}
