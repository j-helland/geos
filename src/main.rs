use std::fmt::{Display, Formatter};

use clap::{Parser, Subcommand, Args, ValueEnum};
use clap_stdin::MaybeStdin;
use s2::cellid::CellID;
use wkt::{TryFromWkt, ToWkt};
use geo::{BoundingRect, LineInterpolatePoint, Intersects, Triangle, TriangulateEarcut};
use geo_types::{Coord, Point, Geometry, Line, Rect, GeometryCollection, Polygon};


#[derive(Parser)]
#[command(name = "GeoS")]
#[command(author = "jwh")]
#[command(version = "0.0.0")]
#[command(about = "GeoS: Commandline tool for some handy geographic operations.", long_about = None)]
#[command(arg_required_else_help = true)]
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
    Geom(GeomArgs),
}

#[derive(Debug, Args)]
#[command(about = "Commands related to S2 cells.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
struct S2Args {
    #[command(subcommand)]
    command: Option<S2Commands>,
}

#[derive(Debug, Args)]
#[command(about = "General geometry commands.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
struct GeomArgs {
    #[command(subcommand)]
    command: Option<GeomCommands>,
}

#[derive(Debug, Subcommand)]
enum S2Commands {
    #[command(arg_required_else_help = true)]
    Cover {
        /// S2 Cell Commands
        #[arg(last = true)]
        wkt: MaybeStdin<String>,

        #[arg(short, long, default_value_t = 12)]
        level: u8,

        #[arg(short, long, default_value_t = S2CellFormat::Long)]
        s2_cell_format: S2CellFormat,
    },    
}

#[derive(Debug, Subcommand)]
enum GeomCommands {
    #[command(arg_required_else_help = true)]
    Split {
        #[arg(last = true)]
        wkt: MaybeStdin<String>,

        #[arg(short, long)]
        lerp: f64,

        #[arg(short, long, default_value_t = SplitFormat::CSV)]
        format: SplitFormat,
    },
    Triangulate {
        #[arg(last = true)]
        wkt: MaybeStdin<String>,

        #[arg(short, long, default_value_t = SplitFormat::CSV)]
        format: SplitFormat,
    },
}

fn _fmt<T: ValueEnum>(t: &T, f: &mut Formatter<'_>) -> std::fmt::Result {
    t.to_possible_value()
        .expect("no values are skipped")
        .get_name()
        .fmt(f)
}

#[derive(Debug, Clone, ValueEnum)]
enum S2CellFormat {
    Long,
    Hex,
    Quad,
}
impl Display for S2CellFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        _fmt(self, f)
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum SplitFormat {
    WKT,
    CSV,
}
impl Display for SplitFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        _fmt(self, f)
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum SplitStrategy {
    Bbox,
    Triangulate,
}
impl Display for SplitStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        _fmt(self, f)
    }
}


/**
 * Computes an S2 cell covering of the given geometry by first computing a bounding box and then
 * covering the bounding box. This is efficient but imprecise.
 */
fn get_s2_covering(geometry: Geometry, level: u8, max_cells: usize) -> Vec<CellID> {
    let bbox = geometry.bounding_rect().unwrap();
    let pmin: Point = <Coord as Into<Point>>::into(bbox.min()).to_radians();
    let pmax: Point = <Coord as Into<Point>>::into(bbox.max()).to_radians();
    let region = s2::rect::Rect{
        lat: s2::r1::interval::Interval{lo: pmin.x(), hi: pmax.x()},
        lng: s2::s1::Interval{lo: pmin.y(), hi: pmax.y()},
    };

    // compute covering
    let rc = s2::region::RegionCoverer { 
        min_level: level, 
        max_level: level, 
        level_mod: 1, 
        max_cells, 
    };

    rc.covering(&region).0
}

/**
 * This algorithm approximately partitions a geometry into uniform subregions. First, the geometry
 * is approximated by its minimal bounding box. Then, the bounding box is divided into regions. The
 * lerp argument determines the region size. For example, lerp = 0.5 would divide into 4 regions.
 * lerp = 0.33 would divide into 9 regions.
 */
fn partition_region(geometry: Geometry, lerp: f64) -> GeometryCollection {
    let mut partitions: Vec<Geometry> = vec![];    
    let bbox = geometry.bounding_rect().unwrap();

    // Bounding box corners.
    //  c1 -- c2
    //  |     |
    // c0 -- c3
    let c0 = bbox.min();
    let c1 = Coord{x: bbox.min().x, y: bbox.max().y};
    let c2 = bbox.max();
    let c3 = Coord{x: bbox.max().x, y: bbox.min().y};

    // Forward axes.
    let l01 = Line{start: c0, end: c1};
    let l32 = Line{start: c3, end: c2};

    // Sweep in the c0 -> c1 direction.
    let mut fx = 0.0;
    let mut lp_prev = Line{start: c0, end: c3};
    while fx < 1.0 {
        // This is a line in the c0 -> c3 direction that is lerped towards c1. We use this to
        // compute the upper right corner of the rect.
        let lp = Line{
            start: l01.line_interpolate_point(fx + lerp).unwrap().into(), 
            end: l32.line_interpolate_point(fx + lerp).unwrap().into(),
        };

        // Sweep in the c0 -> c3 direction. 
        let mut fy = 0.0;
        while fy < 1.0 {
            // The rect can be defined in terms of two corners. 
            let partition = Rect::new(
                lp_prev.line_interpolate_point(fy).unwrap(), 
                lp.line_interpolate_point(fy + lerp).unwrap());

            // Not all partitions computed from the minimal bounding box intersect with the
            // underlying geometry.
            if partition.intersects(&geometry) {
                partitions.push(partition.into());
            }

            fy += lerp;
        }
        fx += lerp;
        lp_prev = lp;
    }

    GeometryCollection::new_from(partitions)
}

fn triangulate_region(geometry: Geometry) -> GeometryCollection {
    let polygon: Polygon = geometry.try_into().unwrap();
    let triangles: Vec<Geometry> = polygon.earcut_triangles_iter().map(Triangle::into).collect();
    GeometryCollection::new_from(triangles)
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::S2(s2)) => {
            match &s2.command {
                // Cover geometry.
                Some(S2Commands::Cover { wkt, level, s2_cell_format }) => {
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt).unwrap();
                    get_s2_covering(geometry, *level, 128)
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
        Some(Commands::Geom(geom)) => {
            match &geom.command {
                // Split geometry.
                Some(GeomCommands::Split { wkt, lerp, format }) => {
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt).unwrap();
                    let partitions = partition_region(geometry, *lerp);
                    fmt_geometry(format, &partitions);
                }

                Some(GeomCommands::Triangulate { wkt, format }) => {
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt).unwrap();
                    let triangles = triangulate_region(geometry);
                    fmt_geometry(format, &triangles);
                }

                None => {}
            }
        }
        None => {}
    }
}

fn fmt_geometry(format: &SplitFormat, gc: &GeometryCollection) {
    match format {
        SplitFormat::CSV => {
            gc.iter().for_each(|p| println!("{}", p.wkt_string()));
        }
        SplitFormat::WKT => {
            println!("{}", gc.wkt_string());
        }
    }
}
