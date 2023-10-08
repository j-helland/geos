use std::fmt::{Display, Formatter};

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_stdin::MaybeStdin;
use geo::{
    Area, BooleanOps, BoundingRect, Intersects, LineInterpolatePoint, Triangle, TriangulateEarcut,
};
use geo_types::{Coord, Geometry, GeometryCollection, Line, Point, Polygon, Rect};
use itertools::Itertools;
use s2::cellid::CellID;
use wkt::{ToWkt, TryFromWkt};

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

        #[arg(short, long, default_value_t = S2CellFormat::Long, help = "Format for the S2 cell IDs.")]
        s2_cell_format: S2CellFormat,

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each cell ID on separate lines.")]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum GeomCommands {
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

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each subdivision region as a WKT POLYGON on separate lines. Specifying the wkt format will consolidate these lines into a WKT GEOMETRYCOLLECTION and output a single line.")]
        format: OutputFormat,
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
enum OutputFormat {
    CSV,
    Oneline,
}
impl Display for OutputFormat {
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
    let region = s2::rect::Rect {
        lat: s2::r1::interval::Interval {
            lo: pmin.x(),
            hi: pmax.x(),
        },
        lng: s2::s1::Interval {
            lo: pmin.y(),
            hi: pmax.y(),
        },
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
 * edge_proportion argument determines the region size. For example, edge_proportion = 0.5 would divide into 4 regions.
 * edge_proportion = 0.33 would divide into 9 regions.
 */
fn partition_region(
    geometry: Geometry,
    edge_proportion: f64,
    area_threshold: Option<f64>,
) -> GeometryCollection {
    let mut partitions: Vec<Geometry> = vec![];
    let bbox = geometry.bounding_rect().unwrap();

    // This ensures that we return bbox in cases where edge_proportion > 1.0 i.e. would correspond
    // to a dilation.
    let edge_budget = f64::max(1.0, edge_proportion);

    // Bounding box corners.
    //  c1 -- c2
    //  |     |
    // c0 -- c3
    let c0 = bbox.min();
    let c1 = Coord {
        x: bbox.min().x,
        y: bbox.max().y,
    };
    let c2 = bbox.max();
    let c3 = Coord {
        x: bbox.max().x,
        y: bbox.min().y,
    };

    // Forward axes.
    let l01 = Line { start: c0, end: c1 };
    let l32 = Line { start: c3, end: c2 };

    // Sweep in the c0 -> c1 direction.
    let mut fx = 0.0;
    let mut lp_prev = Line { start: c0, end: c3 };
    while fx < edge_budget {
        // This is a line in the c0 -> c3 direction that is lerped towards c1. We use this to
        // compute the upper right corner of the rect.
        let lp = Line {
            start: l01
                .line_interpolate_point(fx + edge_proportion)
                .unwrap()
                .into(),
            end: l32
                .line_interpolate_point(fx + edge_proportion)
                .unwrap()
                .into(),
        };

        // Sweep in the c0 -> c3 direction.
        let mut fy = 0.0;
        while fy < edge_budget {
            // The rect can be defined in terms of two corners.
            let partition = Rect::new(
                lp_prev.line_interpolate_point(fy).unwrap(),
                lp.line_interpolate_point(fy + edge_proportion).unwrap(),
            );

            // Not all partitions computed from the minimal bounding box intersect with the
            // underlying geometry.
            match area_threshold {
                Some(threshold) => {
                    // More expensive selection criterion based on the amount of intersection.
                    let p: Polygon = geometry.clone().try_into().unwrap();
                    let intersection = p.intersection(&partition.to_polygon());
                    let area_ratio = intersection.unsigned_area() / partition.unsigned_area();
                    if area_ratio >= threshold {
                        partitions.push(partition.into());
                    }
                }

                None => {
                    // Fast selection criterion of detecting any intersection. This is the deafult.
                    if partition.intersects(&geometry) {
                        partitions.push(partition.into());
                    }
                }
            }

            fy += edge_proportion;
        }
        fx += edge_proportion;
        lp_prev = lp;
    }

    GeometryCollection::new_from(partitions)
}

fn triangulate_region(geometry: Geometry) -> GeometryCollection {
    let polygon: Polygon = geometry.try_into().unwrap();
    let triangles: Vec<Geometry> = polygon
        .earcut_triangles_iter()
        .map(Triangle::into)
        .collect();
    GeometryCollection::new_from(triangles)
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        // Commands that input and/or output S2 cells.
        Some(Commands::S2(s2)) => {
            match &s2.command {
                // Cover geometry.
                Some(S2Commands::Cover {
                    wkt,
                    level,
                    s2_cell_format,
                    format,
                }) => {
                    let fmt_cell = |c: CellID| match s2_cell_format {
                        S2CellFormat::Long => format!("{}", c.0),
                        S2CellFormat::Hex => format!("{}", c.to_token()),
                        S2CellFormat::Quad => format!("{:#?}", c),
                    };

                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt).unwrap();
                    let cover = get_s2_covering(geometry, *level, 128);

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

                None => {}
            }
        }

        // Commands that input/output WKT geometries.
        Some(Commands::Geom(geom)) => {
            let fmt_geometry = |fmt: &OutputFormat, gc: &GeometryCollection| match fmt {
                OutputFormat::CSV => {
                    gc.iter().for_each(|p| println!("{}", p.wkt_string()));
                }
                OutputFormat::Oneline => {
                    println!("{}", gc.wkt_string());
                }
            };

            match &geom.command {
                // Split geometry.
                Some(GeomCommands::Split {
                    wkt,
                    edge_proportion,
                    format,
                    threshold,
                }) => {
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt).unwrap();
                    let partitions = partition_region(geometry, *edge_proportion, *threshold);
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
