//use std::path::PathBuf;

use std::fmt::{Display, Formatter};

use clap::{Parser, Subcommand, Args, ValueEnum};
use wkt::TryFromWkt;
use geo::BoundingRect;
use geo_types::{Coord, Point, Geometry};


#[derive(Parser)]
#[command(name = "GeoS")]
#[command(author = "jwh")]
#[command(version = "0.0.0")]
#[command(about = "Commandline tool for some handy geographic operations.", long_about = None)]
struct Cli {
    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    S2(S2Args),
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = false)]
struct S2Args {
    #[command(subcommand)]
    command: Option<S2Commands>,
}

#[derive(Debug, Subcommand)]
enum S2Commands {
    Covering {
        /// S2 Cell Commands
        #[arg(short, long)]
        wkt: String,

        #[arg(short, long, default_value_t = 12)]
        level: u8,

        #[arg(short, long, default_value_t = S2CellFormat::Long)]
        s2_cell_format: S2CellFormat,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum S2CellFormat {
    Long,
    Hex,
    Quad,
}
impl Display for S2CellFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.to_possible_value()
            .expect("no values are skipped")
            .get_name()
            .fmt(f)
    }
}

fn main() {
    let cli = Cli::parse();

    //// You can see how many times a particular flag or argument occurred
    //// Note, only flags can have multiple occurrences
    //match cli.debug {
    //    0 => println!("Debug mode is off"),
    //    1 => println!("Debug mode is kind of on"),
    //    2 => println!("Debug mode is on"),
    //    _ => println!("Don't be crazy"),
    //}

    match &cli.command {
        Some(Commands::S2(s2)) => {
            match &s2.command {
                Some(S2Commands::Covering { wkt, level, s2_cell_format }) => {
                    // Derive region to cover as the minimal axis-aligned bounding box of the geometry.
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt).unwrap();
                    let bbox = geometry.bounding_rect().unwrap();

                    let pmin: Point = <Coord as Into<Point>>::into(bbox.min()).to_radians();
                    let pmax: Point = <Coord as Into<Point>>::into(bbox.max()).to_radians();
                    let region = s2::rect::Rect{
                        lat: s2::r1::interval::Interval{lo: pmin.x(), hi: pmax.x()},
                        lng: s2::s1::Interval{lo: pmin.y(), hi: pmax.y()},
                    };

                    // compute covering
                    let rc = s2::region::RegionCoverer { 
                        min_level: *level, 
                        max_level: *level, 
                        level_mod: 1, 
                        max_cells: 128, 
                    };
                    let cover = rc.covering(&region);

                    cover.0
                        .into_iter()
                        .for_each(|c| match s2_cell_format {
                            S2CellFormat::Long => println!("{}", c.0),
                            S2CellFormat::Hex => println!("{}", c.to_token()),
                            S2CellFormat::Quad => println!("{:#?}", c),
                        });
                            
                }
                None => {}
            }
        }
        None => {}
    }
}
