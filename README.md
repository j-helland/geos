# About

`geos` is a CLI tool that provides some geometric commands that I personally find useful. For example, I frequently find myself wanting to do various operations on S2 cells without opening a debugger.

GeoS stands for **Geo Stuff** formally and **Geo Shit** colloquially.

# Installation

Build it yourself; `geos` is not currently available from any package management artifactories. 

```bash
> cargo build -r
> alias geos='$(pwd)/target/release/geos'
```

# Usage

Many commands use [WKT format](https://en.wikipedia.org/wiki/Well-known_text_representation_of_geometry) for input and output geometries.

## `s2` commands

These commands work with [S2 cells](https://s2geometry.io/).

### `cover`

The most basic example is computing the S2 cell that contains a point.
```bash
> geos s2 cover -l 14 -s quad -- "POINT(-122.38894169588661 37.76935778889086)"
4/00101323333202
```
<img src="./artifacts/s2-l14.png" alt="drawing" width="420"/>

<br><br>
Other geometries can also be covered:
```bash
> geos s2 cover -l 20 -- "POLYGON ((-122.389181 37.769693, -122.388672 37.769718, -122.388602 37.768972, -122.389112 37.768942, -122.389181 37.769693))"
```

Original Geometry | Covered Geometry
:----------------:|:----------------:
<img src="./artifacts/uncovered.png" alt="drawing" width="420"/> | <img src="./artifacts/covered.png" alt="drawing" width="402"/>


### `cut`

A geometry can be cut by an S2 grid at a given level. 
```bash
> geos s2 cut -l 18 -f oneline -- "POLYGON ((-122.389181 37.769693, -122.388672 37.769718, -122.388602 37.768972, -122.389112 37.768942, -122.389181 37.769693))"
```
<img src="./artifacts/cut.png" alt="drawing" width="420"/>

The `-f oneline` arg will merge the geometries resulting from the cut into a single line `GEOMETRYCOLLECTION`. Otherwise, each constituent polygon will be printed on a separate line.

### `cell-to-poly`

To convert an S2 cell into a polygon representation:
```bash
> geos s2 cell-to-poly -- 9263763445025603584

POLYGON((-122.39009006966613 37.769200437923466,-122.39009006966613 37.76800891143169,-122.38867383494343 37.76844387567673,-122.38867383494343 37.76963540683453,-122.39009006966613 37.769200437923466))
```


## `h3` Commands

These commands work with [H3 cells](https://h3geo.org).

### `cover`

Similarly to the [S2 cover command](#cover), you can cover geometries with H3 cells at a given level:

```bash
> geos h3 cover -l 3 -- "POLYGON ((-106.369629 39.588757, -104.864502 40.32142, -104.886475 38.985033, -102.359619 39.918163, -105.545654 37.701207, -105.611572 39.385264, -107.995605 38.719805, -107.567139 40.472024, -106.369629 39.588757))"
```

Various covering modes exist. The default is to compute the minimal covering that fully contains the geometry. Some use-cases like geometry approximation may instead prefer to only include cells whose centroid is contained in the geometry via the `-m centroid` argument. 

`geos h3 cover -l 3` | `geos h3 cover -l 4 -m centroid`
:-------------------:|:-------------------------------:
<img src="./artifacts/h3-cover.png" alt="drawing" width="375"/> | <img src="./artifacts/h3-cover-centroid.png" alt="drawing" width="420"/>


### `compact`

```bash
> geos h3 compact
```

This command allows for cell compaction (iterative pruning of full H3 branches) via `--compact`, which is invertible using the [uncompact command](#uncompact). The intended use-case for H3 cell compaction is to compress an approximated geometry for sending over a wire.

`geos h3 cover -l 5 --compact` |
:-:
<img src="./artifacts/h3-cover-compact.png" alt="drawing" width="420"/>

Note that the compaction of a full covering (default covering mode) may no longer completely cover the geometry, as can be seen in the image above upon close inspection. This is due to a fundamental property of hexagons, which cannot be perfectly subdivided into sub-hexagons (contrast this to a quad tree like S2, where quads are always perfectly divisible into smaller quads). The consequence of this is that H3 compaction is not generally suitable for use-cases like request fanout to a spatial API; the gaps between H3 cells at disparate levels can result in missed data. Such applications should prefer S2 cells instead.


### `uncompact`

This is the inverse of the [compact command](#compact).

```bash
# Get some cells.
> geos h3 cover -l 5 -f oneline -- "POLYGON ((-106.369629 39.588757, -104.864502 40.32142, -104.886475 38.985033, -102.359619 39.918163, -105.545654 37.701207, -105.611572 39.385264, -107.995605 38.719805, -107.567139 40.472024, -106.369629 39.588757))" > cover.txt

# Noop
> cat cover.txt | geos h3 compact -f oneline -- | geos h3 uncompact -l 5 -f oneline --
```


### `cut`

A geometry can be cut by an H3 grid at a given resolution. This is analogous to the [S2 cut command](#cut).

```bash
> geos h3 cut
```

`geos h3 cut -l 3` | `geos h3 cut -l 5`
:----------------:|:----------------:
<img src="./artifacts/h3-cut-3.png" alt="drawing" width="420"/> | <img src="./artifacts/h3-cut-5.png" alt="drawing" width="400"/>

### `cell-to-poly`

To convert an H3 cell into a polygon representation:

```bash
> geos h3 cell-to-poly -- 81703ffffffffff

POLYGON((-173.38014762578527 7.9727938308414075,-174.31673738369324 3.8210244943304392,-171.37544324872502 0.6498705655763978,-167.31261713417402 1.5147974903819605,-166.16940101623453 5.76714668637842,-169.2931299839693 9.060308038526605,-173.38014762578527 7.9727938308414075))
```


## `geom` commands

### `split`

You may want to partition a geometry into more regular sub-geometries than `geos s2 cut`.  This could be handy if you're trying to test / debug a spatial API query with evenly sized sub-queries.

```bash
> geos geom split -e 0.25 -f oneline -- "POLYGON ((-122.389181 37.769693, -122.388672 37.769718, -122.388602 37.768972, -122.389112 37.768942, -122.389181 37.769693))"
````


Original Geometry | Split Geometry
:----------------:|:----------------:
<img src="./artifacts/uncovered.png" alt="drawing" width="420"/> | <img src="./artifacts/split.png" alt="drawing" width="328"/>

The `-e` or `--edge-proportion` arg dicatates the relative proportion that each partition will take up of the original geometry (more precisely, the original geometry's minimal bounding box). If you specify a proportion that does not evenly divide the edge (e.g. `0.33`), you'll obtain possibly unintuitive splits.


Uneven Split `-e 0.33` | Thresholded Split `-t 0.25`
:----------------:|:----------------:
<img src="./artifacts/split-unexpected.png" alt="drawing" width="420"/> | <img src="./artifacts/split-threshold.png" alt="drawing" width="400"/>


### `triangulate`

You can triangulate a geometry using the [ear clipping algorithm](https://en.wikipedia.org/wiki/Polygon_triangulation#Ear_clipping_method).

```bash
> geos geom triangulate -f oneline -- "POLYGON ((-122.389181 37.769693, -122.388672 37.769718, -122.388602 37.768972, -122.389112 37.768942, -122.389181 37.769693))"
```

<img src="./artifacts/triangulate.png" alt="drawing" width="420"/>


## `rand`

These commands involve random sampling. A typical use-case would be generating arbitrary inputs to test some spatial algorithm / API.

The `-s` or `--seed` argument to the `rand` command can be used to control the RNG for all subcommands.

### `point`

The simplest usage is to sample a point uniformly at random from anywhere on the Earth's surface

```bash
> geos rand -s 420 point
POINT(3.1614064126337524 -37.42948767053008)
```

You can generate more than one point
```bash
# Count the output lines.
> geos rand -s 420 point -n 69 | wc -l
      69
```

<br><br>
You can get fancier and restrict the sampling to a geometry. The sampling algorithm is relatively efficient since it is a direct sampler (i.e. no rejection sampling).

```bash
> geos rand -s 420 point -n 69 -f oneline -w "POLYGON ((-122.388994 37.769426, -122.38894 37.770028, -122.388591 37.768913, -122.388157 37.76915, -122.388951 37.768455, -122.388827 37.769349, -122.389771 37.768493, -122.389954 37.76965, -122.38961 37.768849, -122.389584 37.770062, -122.389278 37.769252, -122.389219 37.769684, -122.388994 37.769426))"
```

<img src="./artifacts/rand-in-geom.png" alt="drawing" width="420"/>
