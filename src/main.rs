mod geos_rand;

use std::{
    error::Error,
    fmt::{Display, Formatter},
    io,
};

use clap::{command, Args, Parser, Subcommand, ValueEnum};
use clap_stdin::MaybeStdin;
use geo::{Area, BooleanOps, BoundingRect, Intersects, Triangle, TriangulateEarcut};
use geo_types::{
    polygon, Coord, Geometry, GeometryCollection, Line, Point, Polygon,
    Rect,
};
use geos_rand::{create_rng, lerp};
use itertools::Itertools;
use s2::{cell::Cell, cellid::CellID, latlng::LatLng};
use wkt::{ToWkt, TryFromWkt};

use crate::geos_rand::{GeoSampler, PolygonalSampler, UniformSampler};

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
    Rand(RandArgs),
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

#[derive(Debug, Args)]
#[command(about = "Commands involving RNG.")]
#[command(args_conflicts_with_subcommands = false)]
#[command(arg_required_else_help = true)]
struct RandArgs {
    #[arg(short, long, default_value_t = 0, help = "Random seed to use")]
    seed: u64,

    #[command(subcommand)]
    command: Option<RandCommands>,
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

        #[arg(short, long, default_value_t = OutputFormat::CSV, help = "By default, outputs each subdivision region as a WKT POLYGON on separate lines. Specifying the oneline format will consolidate these lines into a WKT GEOMETRYCOLLECTION and output a single line.")]
        format: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum RandCommands {
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
fn get_s2_covering(geometry: &Geometry, level: u8, max_cells: usize) -> Vec<CellID> {
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
    let l01_lerp = |t| lerp(t, c0, c1);
    let l32_lerp = |t| lerp(t, c3, c2);

    // Sweep in the c0 -> c1 direction.
    let mut fx = 0.0;
    let mut lp_prev = Line { start: c0, end: c3 };
    while fx < edge_budget {
        // This is a line in the c0 -> c3 direction that is lerped towards c1. We use this to
        // compute the upper right corner of the rect.
        let lp = Line {
            start: l01_lerp(fx + edge_proportion),
            end: l32_lerp(fx + edge_proportion),
        };

        // Sweep in the c0 -> c3 direction.
        let mut fy = 0.0;
        while fy < edge_budget {
            // The rect can be defined in terms of two corners.
            let partition = Rect::new(
                lerp(fy, lp_prev.start, lp_prev.end),
                lerp(fy + edge_proportion, lp.start, lp.end),
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

fn s2_cell_to_poly(cell: Cell) -> Polygon {
    let vertices: [Coord; 4] = cell
        .vertices()
        .map(LatLng::from)
        .map(|c| Coord{
            x: c.lat.deg(), 
            y: c.lng.deg(),
        });
    polygon!(vertices[0], vertices[1], vertices[2], vertices[3])
}

fn cut_region(polygon: Polygon, s2_cells: Vec<Cell>) -> GeometryCollection {
    let cuts = s2_cells
        .into_iter()
        .map(s2_cell_to_poly)
        .map(|p| p.intersection(&polygon))
        .map(Geometry::from)
        .collect_vec();

    GeometryCollection::new_from(cuts)
}

fn fmt_geometry(fmt: &OutputFormat, gc: GeometryCollection) {
    match fmt {
        OutputFormat::CSV => {
            gc.iter().for_each(|p| println!("{}", p.wkt_string()));
        }
        OutputFormat::Oneline => {
            println!("{}", gc.wkt_string());
        }
    }
}

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
        // Commands that input and/or output S2 cells.
        Some(Commands::S2(s2)) => {
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
                    let cuts = cut_region(geometry.try_into()?, cover);
                    fmt_geometry(format, cuts);
                }

                None => {}
            }
        }

        // Commands that input/output WKT geometries.
        Some(Commands::Geom(geom)) => {
            match &geom.command {
                // Split geometry.
                Some(GeomCommands::Split {
                    wkt,
                    edge_proportion,
                    format,
                    threshold,
                }) => {
                    let geometry = Geometry::<f64>::try_from_wkt_str(wkt)?;
                    let partitions = partition_region(geometry, *edge_proportion, *threshold);
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
                    fmt_geometry(format, GeometryCollection::new_from(triangles));
                }

                None => {}
            }
        }

        Some(Commands::Rand(rand)) => {
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

                    fmt_geometry(format, GeometryCollection::new_from(samples));
                }

                None => {}
            }
        }

        None => {}
    }

    Ok(())
}
