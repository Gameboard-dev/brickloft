//! Patterns that fill a flat region in place of solid brick: so far, a path
//! of stepping stones.
//!
//! A flat form is fitted as one outline (see `fit_planar`). A pattern cuts
//! the same region into many small outlines first and fits each as its own,
//! so the pieces of a pattern have the sloped, continuous edges every flat
//! form has, and a pattern follows whatever region the form gives it: the
//! band along a curved path, a ring, a disc, a filled outline.
//!
//! ## Stepping stones
//!
//! The stones are the cells of a Voronoi diagram: points are scattered over
//! the region, and each stone is the ground nearer its point than any
//! other, drawn in from its neighbours by half the gap between stones and
//! with its corners rounded off.
//! https://en.wikipedia.org/wiki/Voronoi_diagram
//!
//! The patterns were asked for under the name of wave function collapse,
//! which fills a grid with tiles that agree with their neighbours:
//! https://github.com/mxgmn/WaveFunctionCollapse
//! That suits a pattern made of repeated motifs, and a floor mosaic or a
//! rug will use it. It does not suit stones. Its rules are local, a few
//! cells across, and nothing local says how large a stone has grown: the
//! tiles that make a stone's inside can be laid for ever. Its output is
//! also the grid's own squares, where a stone wants an outline at any
//! angle. Scattered points give stones of a chosen size by construction,
//! and their outlines are exact curves for the outline fitter to follow.
//! The same author's MarkovJunior grows its own regions from scattered
//! points for the same reason: https://github.com/mxgmn/MarkovJunior
//!
//! The points are scattered by throwing darts: a point is kept if it is no
//! nearer than a least distance to any point already kept ("Poisson disc"
//! sampling), which spreads them evenly without a grid showing through.
//! https://en.wikipedia.org/wiki/Supersampling#Poisson_disk
//! They come from a small generator of numbers started from the form's
//! seed, so one seed is always one path of stones, on every machine.

use crate::fit_planar::{fit_planar, Fit, Opts};
use crate::shapes::CellBrick;
use glam::DVec2;
use std::collections::HashMap;

/// How a path of stones is laid: the settings a form carries for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stones {
    /// how far apart the stones' middles are, about: a stone is a little
    /// under this across
    pub size: f64,
    /// the gap between neighbouring stones
    pub gap: f64,
    /// 0 for stones as alike as scattered points allow, 1 for stones of
    /// very different sizes and shapes
    pub uneven: f64,
    /// which path of stones: any number, each a different scattering
    pub seed: u64,
}

/// The least a stone may be across and the least gap, in cells: under
/// these the outline fitter has no room to make a stone or to keep two apart.
pub const LEAST_STONE: f64 = 3.0;
pub const LEAST_GAP: f64 = 0.0;

/// Numbers that look random and are the same every time for one seed.
///
/// SplitMix64: one multiply-and-shift mix of a counter. Any generator would
/// do for scattering stones; this one is a dozen lines with no state beyond
/// a number, where the `rand` crate would be a dependency for it.
/// https://prng.di.unimi.it/splitmix64.c
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 up to, not including, 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The points the stones grow from: scattered over the part of cells
/// `min..max` where `region` is negative (inside), none nearer another than
/// a least distance that `uneven` loosens.
pub fn scatter(region: &dyn Fn(DVec2) -> f64, min: [i32; 2], max: [i32; 2], stones: &Stones) -> Vec<DVec2> {
    let size = stones.size.max(LEAST_STONE);
    let uneven = stones.uneven.clamp(0.0, 1.0);
    // Even stones: points kept well apart, which leaves them close to a
    // honeycomb. Uneven: points allowed nearly twice as close, so that
    // some stones are crowded small and others left large.
    let apart = size * (0.86 - 0.36 * uneven);
    let (lo, span) = (DVec2::new(min[0] as f64, min[1] as f64), DVec2::new((max[0] - min[0]) as f64, (max[1] - min[1]) as f64));
    // a grid of squares one least distance wide: a new point need only be
    // compared with the points in the nine squares about it
    let across = ((span.x / apart).ceil() as usize + 1, (span.y / apart).ceil() as usize + 1);
    let square = |p: DVec2| (((p.x - lo.x) / apart) as usize, ((p.y - lo.y) / apart) as usize);
    let mut held: HashMap<(usize, usize), Vec<DVec2>> = HashMap::new();
    let mut points: Vec<DVec2> = Vec::new();
    let mut numbers = Numbers(stones.seed ^ 0x51ED_270B_7F4A_7C15);
    // Enough darts that the region is full: thirty for each stone the box
    // has room for. Past that a dart almost never finds a place, and a
    // fixed count keeps the time in hand on a region with little inside it.
    let darts = ((span.x * span.y / (apart * apart)).ceil() as usize * 30).clamp(30, 400_000);
    for _ in 0..darts {
        let p = lo + DVec2::new(numbers.unit(), numbers.unit()) * span;
        // a stone's middle is kept in from the region's edge, so that the
        // edge cuts a little off a stone and not most of it
        if region(p) > -size * 0.3 {
            continue;
        }
        let (i, j) = square(p);
        let crowded = (i.saturating_sub(1)..=(i + 1).min(across.0)).any(|a| (j.saturating_sub(1)..=(j + 1).min(across.1)).any(|b| held.get(&(a, b)).is_some_and(|there| there.iter().any(|q| q.distance(p) < apart))));
        if !crowded {
            held.entry((i, j)).or_default().push(p);
            points.push(p);
        }
    }
    points
}

/// One stone: the point it grew from and the lines half way to the stones
/// round it.
pub struct Stone {
    pub at: DVec2,
    /// For each stone round it, the line half way between the two, as the
    /// direction towards the other stone and how far along that direction
    /// the line is: a place `p` is `far - towards.dot(p)` on this stone's
    /// side of it.
    lines: Vec<(DVec2, f64)>,
    half_gap: f64,
    /// how sharply its corners are rounded: higher is sharper
    sharp: f64,
}

impl Stone {
    /// The stone as a solid for the outline fitter: negative inside.
    ///
    /// A point is inside by its distance from each line half way to a
    /// neighbouring stone, and from the region's edge, less half the gap.
    /// The least of those distances would give the Voronoi cell exactly,
    /// drawn in by half the gap, with sharp corners. A smooth least
    /// (the "log-sum-exp" one) gives the same edges and rounds the corners
    /// where two of them meet, as a stone's are.
    /// https://en.wikipedia.org/wiki/LogSumExp
    ///
    /// The smooth least differs from the plain least only where two of the
    /// distances are within a cell or so of each other, which is at the
    /// corners. So the plain least is found first, and a place it puts
    /// well inside the stone or well outside is answered from it. Taking
    /// the exponential of every distance at every place asked about was
    /// most of the time a path of stones took: 170 ms for a path that is
    /// 20 ms as solid brick.
    pub fn eval(&self, p: DVec2, region: &dyn Fn(DVec2) -> f64) -> f64 {
        // how far the smooth least can fall below the plain least, with room to spare
        let near = 2.0 / self.sharp;
        let mut least = f64::INFINITY;
        for (towards, far) in &self.lines {
            least = least.min(far - towards.dot(p));
        }
        if least < self.half_gap - near {
            return self.half_gap - least; // well past a neighbour's line: outside
        }
        let edge = -region(p);
        let least = least.min(edge);
        if least < self.half_gap - near || least > self.half_gap + near {
            return self.half_gap - least;
        }
        let mut sum = (-self.sharp * edge).exp();
        for (towards, far) in &self.lines {
            sum += (-self.sharp * (far - towards.dot(p))).exp();
        }
        self.half_gap + sum.ln() / self.sharp
    }
}

/// The stones of a region: one for each scattered point, knowing the
/// stones round it.
pub fn stones(region: &dyn Fn(DVec2) -> f64, min: [i32; 2], max: [i32; 2], settings: &Stones) -> Vec<Stone> {
    let points = scatter(region, min, max, settings);
    let size = settings.size.max(LEAST_STONE);
    // a stone's edges come from the points within two stones' width of its own
    let reach = size * 2.2;
    points
        .iter()
        .map(|at| Stone {
            at: *at,
            lines: points
                .iter()
                .filter(|q| *q != at && q.distance(*at) < reach)
                .map(|q| {
                    let towards = (*q - *at).normalize();
                    (towards, towards.dot((*q + *at) / 2.0))
                })
                .collect(),
            half_gap: settings.gap.max(LEAST_GAP) / 2.0,
            // Corners rounded over about half a cell, whatever the stone's
            // size. Rounder than that and the smooth least falls short of
            // the true least along the whole of every edge, not only at
            // the corners, since a small stone has several neighbours'
            // lines within a cell or two of any point on its edge: at a
            // sixth of a stone's width the stones came out a cell and a
            // half short all round, with gaps as wide as themselves.
            sharp: 2.5,
        })
        .collect()
}

/// Whether a place, given within a piece's box as two numbers from 0 to 1,
/// is in the piece itself: anywhere, for a rectangle; for a triangle, on
/// the near side of its sloped edge.
fn in_piece(mask: u8, u: f64, v: f64) -> bool {
    if mask == 15 {
        return true;
    }
    // the corner the triangle lacks, and how far the place is from the one opposite it
    let missing = (!mask & 15).trailing_zeros();
    let (from_u, from_v) = (if missing & 1 == 1 { u } else { 1.0 - u }, if missing & 2 == 2 { v } else { 1.0 - v });
    from_u + from_v < 1.0
}

/// Fit the stones of a region and push them through cells `w0..w0 + depth`
/// across `axis`, onto `bricks`. Returns the sum of the stones' fits: the
/// area of the stones themselves and the area where their bricks disagree
/// with them.
///
/// Each stone is fitted by itself, so that every edge of every stone gets
/// the sloped bricks an outline of its own gets. Fitted all at once as one
/// solid of many parts, two stones either side of a narrow gap wanted
/// cells of the same box for the triangles of their facing edges, the
/// fitter gave the box to one, and the other's edge was finished in steps:
/// a stone with a corner missing its wedge.
///
/// Fitted apart, the triangles of two facing edges may lie in boxes that
/// share cells, and both are kept, since what the two bricks fill does not
/// meet: the gap runs between them. Only where what they fill would meet
/// (each fit may stray half a cell from its stone, and the gap may be
/// nothing) is the later stone fitted again a little smaller.
#[allow(clippy::too_many_arguments)]
pub fn lay_stones(region: &dyn Fn(DVec2) -> f64, settings: &Stones, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
    use crate::fit_planar::Piece;
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let size = settings.size.max(LEAST_STONE);
    let mut whole = Fit::default();
    // the pieces laid so far, and for each cell the pieces whose boxes cover it
    let mut laid: Vec<Piece> = Vec::new();
    let mut over: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let cells = |piece: &Piece| {
        let (x, y, w, h) = (piece.x, piece.y, piece.w, piece.h);
        (0..w * h).map(move |k| (x + k % w, y + k / w))
    };
    // whether a piece fills any of what a piece already laid fills: nine places to a cell they share
    let meets = |piece: &Piece, laid: &[Piece], over: &HashMap<(i32, i32), Vec<usize>>| {
        let within = |q: &Piece, x: f64, y: f64| in_piece(q.mask, (x - q.x as f64) / q.w as f64, (y - q.y as f64) / q.h as f64);
        cells(piece).any(|cell| {
            over.get(&cell).is_some_and(|others| {
                (0..9).any(|k| {
                    let (x, y) = (cell.0 as f64 + (k % 3) as f64 / 3.0 + 0.17, cell.1 as f64 + (k / 3) as f64 / 3.0 + 0.16);
                    within(piece, x, y) && others.iter().any(|o| within(&laid[*o], x, y))
                })
            })
        })
    };
    // (each stone has a number, for its bricks to be coloured alike by: see `colour_of`)
    for (nth, mut stone) in stones(region, min, max, settings).into_iter().enumerate() {
        // No place is further from the nearest stone's point than the
        // points are kept apart, or a dart would have landed there: so a
        // stone reaches no further than that from its own.
        let reach = size * 0.9 + 1.0;
        let (lo, hi) = (stone.at - DVec2::splat(reach), stone.at + DVec2::splat(reach));
        let (from, to) = ([(lo.x.floor() as i32).max(min[0]), (lo.y.floor() as i32).max(min[1])], [(hi.x.ceil() as i32).min(max[0]), (hi.y.ceil() as i32).min(max[1])]);
        if to[0] <= from[0] || to[1] <= from[1] {
            continue;
        }
        // as it is; then, if it would meet a stone already laid, drawn in by half a cell, and by a whole one
        let mut fit = fit_planar(&|x, y| stone.eval(DVec2::new(x, y), region), from, to, opts);
        for _ in 0..2 {
            if !fit.pieces.iter().any(|piece| meets(piece, &laid, &over)) {
                break;
            }
            stone.half_gap += 0.5;
            fit = fit_planar(&|x, y| stone.eval(DVec2::new(x, y), region), from, to, opts);
        }
        whole.solid += fit.solid;
        whole.error += fit.error;
        for piece in fit.pieces {
            // (still meeting one after that: left out)
            if meets(&piece, &laid, &over) {
                continue;
            }
            for cell in cells(&piece) {
                over.entry(cell).or_default().push(laid.len());
            }
            laid.push(piece);
            let mask = (0..8u8).fold(0u8, |m, c| m | (piece.mask >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c);
            let mut at = 0;
            while at < depth {
                let run = (depth - at).min(opts.max_ext.max(1));
                let (mut min, mut ext) = ([0; 3], [0; 3]);
                (min[ua], min[va], min[axis]) = (piece.x, piece.y, w0 + at);
                (ext[ua], ext[va], ext[axis]) = (piece.w, piece.h, run);
                bricks.push(CellBrick { mask, min, ext });
                shades.push((nth % 8) as u8);
                at += run;
            }
        }
    }
    whole
}

/// How a mosaic is laid: the settings a form carries for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mosaic {
    /// which of [`STYLES`]
    pub style: usize,
    /// how large the boards, bricks or slabs of a style that has them
    /// are: 2 is the usual, 1 half that, 4 twice it
    pub size: i32,
    /// how many tiles are cut off each corner of a slab
    pub bevel: i32,
    /// how many tiles wide the plain border round the edge is
    pub border: i32,
    /// which filling: any number, each a different mosaic
    pub seed: u64,
}

/// A style of mosaic: a pattern drawn by rule (see [`drawn`]).
///
/// Styles were first small samples whose motifs wave function collapse
/// strewed over the floor (see [`crate::wfc`], which is kept for the
/// patterns still to come). Those floors were even and aimless beside the
/// drawn ones, which have a middle and are their own mirror image, and
/// the sampled styles were taken out.
pub struct Style {
    pub name: &'static str,
    drawn: Drawn,
}

/// The styles on offer.
pub const STYLES: [Style; 6] = [
    Style { name: "medallions", drawn: Drawn::Medallions },
    Style { name: "rug", drawn: Drawn::Rug },
    Style { name: "snowflake", drawn: Drawn::Snowflake },
    Style { name: "planks", drawn: Drawn::Planks },
    Style { name: "bricks", drawn: Drawn::Bricks },
    Style { name: "slabs", drawn: Drawn::Slabs },
];

/// The longest a sloped brick at the edge of a mosaic may be, in cells.
const EDGE_BRICK: i32 = 5;

/// A tile of a mosaic, as the colour at each of its four corners: top
/// left, top right, bottom left, bottom right, with the rows of tiles
/// running the way y does. A tile of one colour has it at all four. A tile cut
/// along a diagonal into two triangles has one colour at one end of the
/// other diagonal and the second at its other end; the two corners the cut
/// runs through belong to neither, and are [`ON_CUT`].
///
/// Told by its corners, a tile is turned by moving its corners round,
/// which is all a quarter turn or a mirror image does to it.
type Tile = [u8; 4];
const ON_CUT: u8 = 255;

/// Which of `count` colours a brick takes whose pattern gives it the
/// number `shade` (see `spec::Built::shades`).
///
/// A pattern numbers its parts and not its colours: 0 is the ground and
/// each number above it one feature of the pattern (a rug's frame, its
/// medallion, a snowflake's branches). The ground has the first colour to
/// itself. The features share the rest out in turn, going round again when
/// there are more features than colours. So two colours give the whole
/// pattern in one colour on the other, and every colour added picks one
/// more feature out, until each has its own.
///
/// Going round all the colours, the ground's among them, was the first
/// rule, and it painted some feature the colour of the ground whenever
/// there were fewer colours than features: that feature vanished.
pub fn colour_of(shade: u8, count: usize) -> usize {
    match (shade, count) {
        (0, _) | (_, 0 | 1) => 0,
        _ => 1 + (shade as usize - 1) % (count - 1),
    }
}

/// The styles that are drawn by rule and not learnt from a sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drawn {
    /// a rug in the Persian manner: a medallion of petals at the middle,
    /// flowers and leaves over the field, and a border (see [`rug_garden`])
    Rug,
    /// one eight-armed snowflake at the middle of the floor, its arms
    /// branching and branching again
    Snowflake,
    /// a few figures repeated over the floor like a tiled ceiling, each
    /// repeat its own mirror image eight ways
    Medallions,
    /// a floor of boards (see [`planks`])
    Planks,
    /// courses of bricks with mortar between (see [`walled`])
    Bricks,
    /// squared stones of many sizes in courses, with mortar between (see [`walled`])
    Slabs,
}

/// The tile at a place, from the colour at any place: the colour is taken
/// at four places in the tile, one in each of the quarters its two
/// diagonals cut it into. Where all four are one colour the tile is that
/// colour. Where the two quarters on one side of a diagonal are one
/// colour and the two on the other side another, the tile is cut along
/// that diagonal. Any other tile takes the colour at its middle.
///
/// So an edge between two colours that runs along the grid or on its
/// diagonals is drawn exactly, and one at any other angle as the nearest
/// run of whole and cut tiles.
fn tile_at(colour: &dyn Fn(f64, f64) -> u8, x: f64, y: f64) -> Tile {
    // the quarters towards the tile's top, right, bottom and left sides
    let [top, right, bottom, left] = [(0.5, 1.0 / 6.0), (5.0 / 6.0, 0.5), (0.5, 5.0 / 6.0), (1.0 / 6.0, 0.5)].map(|(dx, dy)| colour(x + dx, y + dy));
    if top == left && bottom == right && top != bottom {
        [top, ON_CUT, ON_CUT, bottom] // cut from bottom left to top right
    } else if top == right && bottom == left && top != bottom {
        [ON_CUT, top, bottom, ON_CUT] // cut from top left to bottom right
    } else if top == right && right == bottom && bottom == left {
        [top; 4]
    } else {
        [colour(x + 0.5, y + 0.5); 4]
    }
}

/// A wall of bricks or of squared stones, as a grid of tiles: the mortar
/// is the ground, 0, and every brick or stone has one of seven numbers
/// for whoever paints it to give a shade to (see [`colour_of`]).
///
/// Bricks are all one size, laid in level courses with a tile of mortar
/// between courses and between bricks. At the usual size, 2, a brick is
/// eight tiles by three. Each course is moved along by its own amount,
/// so the upright joints fall anywhere against the course below, as in
/// a wall laid without a bond in mind.
/// <https://en.wikipedia.org/wiki/Brickwork>
///
/// Slabs are stones squared but not matched: courses of several depths,
/// and in each course stones of many lengths, larger than bricks. Each
/// may have its four corners cut off by `bevel` tiles, which is what
/// uses the tiles cut in two.
///
/// Stones of any shape at all, fitted together, are the stepping stones
/// with no gap between them (see [`lay_stones`]).
fn walled(kind: Drawn, [across, down]: [usize; 2], seed: u64, size: i32, bevel: i32) -> Vec<Tile> {
    let size = size.clamp(1, 16) as f64;
    if kind == Drawn::Bricks {
        // a number from 0 to 1 that belongs to a pair of whole numbers
        let number = |a: i64, b: i64, which: u64| Numbers(seed ^ (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ which.wrapping_mul(0x1656_67B1_9E37_79F9)).unit();
        // how many tiles a course is deep and a brick long, each with its tile of mortar
        let (course, brick) = ((1.5 * size).round() + 1.0, 4.0 * size + 1.0);
        let colour = |x: f64, y: f64| {
            let row = (y / course).floor();
            let along = x + (number(row as i64, 0, 5) * brick).floor();
            if y - row * course >= course - 1.0 || along.rem_euclid(brick) >= brick - 1.0 {
                0
            } else {
                1 + (number(row as i64, (along / brick).floor() as i64, 3) * 7.0) as u8 % 7
            }
        };
        return (0..across * down).map(|k| tile_at(&colour, (k % across) as f64, (k / across) as f64)).collect();
    }
    // the slab each tile is part of: where the slab starts, how many tiles it is each way with its mortar, and its number
    let mut slab: Vec<[i32; 5]> = vec![[0; 5]; across * down];
    let mut numbers = Numbers(seed ^ 0xBB67_AE85_84CA_A73B);
    let mut whole = |low: f64, high: f64| (low + numbers.unit() * (high - low + 1.0)).floor().min(high) as usize;
    let mut y = 0;
    while y < down {
        let deep = whole(2.0 * size + 1.0, 4.0 * size + 2.0);
        let mut x = 0;
        while x < across {
            // (the first of a course may be an offcut)
            let long = whole(if x == 0 { size + 2.0 } else { 3.0 * size + 2.0 }, 8.0 * size + 2.0);
            let shade = 1 + whole(0.0, 6.0) as i32;
            for j in y..(y + deep).min(down) {
                for i in x..(x + long).min(across) {
                    slab[j * across + i] = [x as i32, y as i32, long as i32, deep as i32, shade];
                }
            }
            x += long;
        }
        y += deep;
    }
    let bevel = bevel.clamp(0, 8) as f64;
    let colour = |x: f64, y: f64| {
        let [x0, y0, long, deep, shade] = slab[(y.floor() as usize).min(down - 1) * across + (x.floor() as usize).min(across - 1)];
        // where in the stone the place is, the stone being its box less the mortar along two sides
        let (a, b, long, deep) = (x - x0 as f64, y - y0 as f64, (long - 1) as f64, (deep - 1) as f64);
        if a >= long || b >= deep || a.min(long - a) + b.min(deep - b) < bevel {
            0
        } else {
            shade as u8
        }
    };
    (0..across * down).map(|k| tile_at(&colour, (k % across) as f64, (k / across) as f64)).collect()
}

/// A floor of boards, as a grid of tiles: boards laid end to end in rows,
/// with the joins of one row falling where they will against the next,
/// as a carpenter lays them. At the usual size, 2, a board is two tiles
/// broad and eight to twenty-four long; other sizes are in proportion.
/// Every board has one of eight numbers, never the number of the board
/// before it in its row, and whoever paints the bricks gives each number
/// a brown (see [`colour_of`]).
///
/// Wave function collapse lays this floor too, from a sample of boards.
/// But a board's length is then whatever the sample's windows allow, a
/// few fixed lengths, and the lengths are the whole look of the thing.
fn planks([across, down]: [usize; 2], seed: u64, size: i32) -> Vec<Tile> {
    let broad = size.clamp(1, 16) as usize;
    let mut tiles: Vec<Tile> = vec![[0; 4]; across * down];
    for row in 0..down {
        // (a row of boards' own numbers, the same for every row of tiles it is, and so that one row does not follow from the last)
        let mut numbers = Numbers(seed ^ ((row / broad) as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let (mut at, mut last) = (0, u8::MAX);
        while at < across {
            // (the first board of a row is the offcut of the row before, of any length)
            let long = if at == 0 { 1 + (numbers.unit() * 12.0 * broad as f64) as usize } else { 4 * broad + (numbers.unit() * (8 * broad + 1) as f64) as usize };
            let mut shade = (numbers.unit() * 8.0) as u8 % 8;
            if shade == last {
                shade = (shade + 1) % 8;
            }
            for tile in &mut tiles[row * across + at..row * across + (at + long).min(across)] {
                *tile = [shade; 4];
            }
            (at, last) = (at + long, shade);
        }
    }
    tiles
}

/// A style drawn by rule, as a grid of tiles: a dense pattern of outlined
/// lozenges, octagons, hexagons, squares and bars, ring within ring, whose
/// outlines run into one another where the figures meet, and which is its
/// own mirror image from side to side and from end to end.
///
/// ## How it is drawn
///
/// Every figure is a way of measuring how far a place is from the figure:
/// across plus along for a lozenge, the greater of the two for a square,
/// and mixtures of the two for an octagon, a hexagon and a bar. The measure
/// is nothing on the figure's outline, below nothing inside it, and grows
/// steadily away from it. One number is worked out for every place: the
/// least of all the figures' measures there. The floor is coloured by that
/// number in bands a tile wide.
///
/// Medallions turn the bands on and off by a short repeating run, so each
/// figure is ringed inside and out by bands of its own shape, and where
/// the bands of two figures meet they join into one outline round both,
/// which is what gives the crowded, interlocked look. It is how a
/// distance function joins two solids into one (see `sdf`), used flat. A
/// snowflake's figures are lines, each on for its own first band alone.
///
/// ## Colours
///
/// A band that is on takes the number of the figure nearest it, by what
/// part of the pattern that figure is: a rug's medallion, its straps, the
/// figures of its odd rows; a snowflake's arms, its branches, its tips. A band that
/// is off is the ground, 0. The numbers go up to 7, and whoever paints the
/// bricks shares them out among however many colours there are: with two
/// colours the whole pattern is one colour on the other, and each colour
/// added picks another part of it out.
///
/// ## Why not wave function collapse
///
/// The patterns were asked for under that name, and the sampled mosaics
/// use it. It was tried for a rug's field and gave a rug its scattered
/// small motifs, which is a plain rug. What makes a rug or a snowflake
/// look like one is a few large figures that govern the whole of it, and
/// wave function collapse knows only what may sit beside what, a few tiles
/// across.
///
/// ## The tiles
///
/// The number is taken at four places in every tile, one in each of the
/// quarters its two diagonals cut it into. Where all four are one colour
/// the tile is that colour. Where the two quarters on one side of a
/// diagonal are one colour and the two on the other side another, the tile
/// is cut along that diagonal. Every measure used here changes by whole
/// tiles along the grid's lines and its diagonals, so those are the only
/// two cases but for a few tiles where several figures' bands meet, which
/// take the colour at their middle.
///
/// `unbroken` says which tiles are whole tiles of the floor. The pattern
/// is set out about the middle of the rectangle of tiles that takes them
/// all in.
/// A rug in the Persian manner, as the colour at each place of one
/// quarter of it (so many tiles out from its middle each way; the other
/// three quarters are this one's mirror images).
///
/// ## What is on it
///
/// From the edge inwards, a rug of this kind has a border between two
/// lines with one small motif repeated along it, then the field. At the
/// middle of the field is the medallion, a flower of several layers one
/// inside the next, with a smaller flower hung from it at each end. A
/// quarter of another medallion fills each corner. Over the rest of the
/// field flowers are scattered, each with a leaf or two, joined to their
/// neighbours by stems.
///
/// ## How a flower is drawn
///
/// A flower is one outline round a middle, further out where a petal is
/// and nearer between two. Going round the middle by the angle `a`, with
/// `n` petals each standing `d` of the flower's size `R` proud:
///
///   r(a) = R (1 - d + d |cos(n a / 2)|^0.6)
///
/// A place is in the flower if it is nearer the middle than `r` at its
/// angle. The power makes a petal round at its tip and the notch between
/// two petals sharp, as the petals of a drawn flower are; without it the
/// outline is a wavy circle. (It is the rose curve of Guido Grandi, with
/// the size added so that the petals do not reach the middle:
/// <https://en.wikipedia.org/wiki/Rose_(mathematics)>.) A medallion is
/// three or four of these on one middle, each smaller than the last,
/// with other numbers of petals and every other one turned by half a
/// petal, so that the petals of one layer show between those of the
/// next. A leaf is two bows meeting in a point at each end.
///
/// The seed chooses the number of layers and of petals, the sizes, and
/// where the flowers of the field are, which is by throwing darts and
/// keeping those that land clear of the medallion, the corners and one
/// another, as the stepping stones' points are found.
///
/// ## Why not pictures of flowers
///
/// A rug's flowers could be small pictures, drawn by hand and stamped
/// over the field. A picture is one size, and a flower drawn in tiles
/// for a rug forty tiles across is a blot on one two hundred across.
/// An outline given as a rule is drawn afresh at whatever size the rug
/// is, and every seed is another flower.
fn rug_garden(half_wide: f64, half_deep: f64, pick: &mut dyn FnMut(f64, f64) -> f64) -> Box<dyn Fn(f64, f64) -> u8> {
    use std::f64::consts::PI;
    #[derive(Clone, Copy)]
    struct Flower {
        x: f64,
        y: f64,
        petals: f64,
        size: f64,
        /// how much of its size a petal stands proud
        proud: f64,
        /// by half a petal
        turned: bool,
    }
    // how far inside a flower's outline a place is: under nothing, outside it
    fn within(flower: &Flower, u: f64, v: f64) -> f64 {
        let (dx, dy) = (u - flower.x, v - flower.y);
        let swell = (flower.petals * dy.atan2(dx) / 2.0 + if flower.turned { PI / 2.0 } else { 0.0 }).cos().abs().powf(0.6);
        flower.size * (1.0 - flower.proud + flower.proud * swell) - dx.hypot(dy)
    }
    // The features: the lines of the border and the outlines of the
    // medallion's layers; its layers by turns; the field's flowers and
    // their leaves; the corner pieces; and the hearts of things.
    const LINE: u8 = 1;
    const LAYER: u8 = 2;
    const OTHER_LAYER: u8 = 3;
    const BLOOM: u8 = 4;
    const LEAF: u8 = 5;
    const CORNER: u8 = 6;
    const HEART: u8 = 7;
    let least = half_wide.min(half_deep);
    // the border: an even number of tiles, so that its motifs sit on the grid
    let border = (2.0 * (least * 0.1).round()).clamp(2.0, 8.0);
    let (wide, deep) = (half_wide - border, half_deep - border);
    let field = wide.min(deep);
    // the medallion, outermost layer first: petals in fours, so that it is its own mirror image both ways
    let size = (field * pick(55.0, 75.0) / 100.0).max(2.0);
    let layers = pick(3.0, 4.0) as usize;
    let medallion: Vec<Flower> = (0..layers)
        .map(|k| Flower { x: 0.0, y: 0.0, petals: 4.0 * if k == 0 { pick(3.0, 4.0) } else if k == 1 { pick(2.0, 3.0) } else { pick(1.0, 2.0) }, size: size * (1.0 - k as f64 / (layers as f64 + 0.3)), proud: pick(20.0, 40.0) / 100.0, turned: k % 2 == 1 })
        .collect();
    // the flower hung from each end of it, along the longer way of the rug, if there is room
    let hung = (size / 4.0).max(1.5);
    let pendant = (wide.max(deep) - size > 2.0 * hung + 2.0).then(|| {
        let out = (size + hung + 1.0).round();
        Flower { x: if wide >= deep { out } else { 0.0 }, y: if wide >= deep { 0.0 } else { out }, petals: 8.0, size: hung, proud: 0.35, turned: false }
    });
    // the corner pieces: two layers about the field's corner
    let corner_size = field * pick(35.0, 50.0) / 100.0;
    let corner: Vec<Flower> = (0..2).map(|k| Flower { x: wide, y: deep, petals: 4.0 * pick(2.0, 4.0), size: corner_size * (1.0 - 0.4 * k as f64), proud: pick(20.0, 40.0) / 100.0, turned: k == 1 }).collect();
    // The flowers of the field, by darts. One that lands within two
    // tiles of a middle line is moved onto it, since the fold would cut
    // it in two there; then it is whole, and its own mirror image.
    let apart = pick(5.0, 7.0);
    let mut blooms: Vec<Flower> = Vec::new();
    let mut leaves: Vec<[f64; 6]> = Vec::new(); // where, which way (as a cosine and a sine), half its length and half its breadth
    let mut stems: Vec<[f64; 4]> = Vec::new();
    for _ in 0..400 {
        let (mut x, mut y) = (pick(0.0, wide - 2.0), pick(0.0, deep - 2.0));
        if x < 2.0 {
            x = 0.0;
        }
        if y < 2.0 {
            y = 0.0;
        }
        let clear = x.hypot(y) > size + 3.0
            && (wide - x).hypot(deep - y) > corner_size + 3.0
            && pendant.is_none_or(|hung| (x - hung.x).hypot(y - hung.y) > hung.size + 3.0)
            // (clear of its own mirror image too)
            && (x == 0.0 || x >= apart / 2.0)
            && (y == 0.0 || y >= apart / 2.0)
            && blooms.iter().all(|other| (x - other.x).hypot(y - other.y) >= apart);
        if !clear {
            continue;
        }
        // (on a middle line, four petals or eight: those are the flowers that are their own mirror image there)
        let petals = if x == 0.0 || y == 0.0 { 4.0 * pick(1.0, 2.0) } else { pick(4.0, 8.0) };
        let bloom = Flower { x, y, petals, size: pick(16.0, 28.0) / 10.0, proud: pick(30.0, 50.0) / 100.0, turned: pick(0.0, 1.0) < 1.0 && x != 0.0 && y != 0.0 };
        // a stem to the nearest flower already there
        if let Some(near) = blooms.iter().min_by(|a, b| (x - a.x).hypot(y - a.y).total_cmp(&(x - b.x).hypot(y - b.y))).filter(|near| (x - near.x).hypot(y - near.y) < 2.0 * apart) {
            stems.push([x, y, near.x, near.y]);
        }
        // a leaf out from it one way, and often another the opposite way
        let way = pick(0.0, 359.0).to_radians();
        for side in 0..if pick(0.0, 2.0) > 0.0 { 2 } else { 1 } {
            let (c, s) = ((way + PI * side as f64).cos(), (way + PI * side as f64).sin());
            let long = pick(12.0, 20.0) / 10.0;
            leaves.push([x + c * (bloom.size + long * 0.8), y + s * (bloom.size + long * 0.8), c, s, long, pick(6.0, 9.0) / 10.0]);
        }
        blooms.push(bloom);
    }
    // a layered flower's colour at a place: `None` outside it
    fn layered(layers: &[Flower], fills: [u8; 2], u: f64, v: f64) -> Option<u8> {
        // (the innermost layer the place is in is the one seen)
        layers.iter().enumerate().rev().find_map(|(k, layer)| {
            let deep = within(layer, u, v);
            (deep > 0.0).then(|| if deep < 1.0 { LINE } else { fills[k % 2] })
        })
    }
    Box::new(move |u, v| {
        // the border: a line, a band of small lozenges with hearts, and a line
        let inward = (half_wide - u).min(half_deep - v);
        if inward < border {
            if inward < 1.0 || inward >= border - 1.0 {
                return LINE;
            }
            let along = if half_wide - u < half_deep - v { v } else { u };
            let from = (along.rem_euclid(border) - border / 2.0).abs() + (inward - border / 2.0).abs();
            return if from < 1.0 { HEART } else if from < border / 2.0 - 1.0 { BLOOM } else { 0 };
        }
        if u.hypot(v) < 1.2 {
            return HEART;
        }
        if let Some(colour) = layered(&medallion, [LAYER, OTHER_LAYER], u, v).or_else(|| pendant.and_then(|hung| layered(&[hung], [OTHER_LAYER, LAYER], u, v))).or_else(|| layered(&corner, [CORNER, LAYER], u, v)) {
            return colour;
        }
        if let Some(bloom) = blooms.iter().find(|bloom| within(bloom, u, v) > 0.0) {
            return if (u - bloom.x).hypot(v - bloom.y) < bloom.size * 0.35 { HEART } else { BLOOM };
        }
        let in_leaf = |[x, y, c, s, long, broad]: &[f64; 6]| {
            let (along, across) = ((u - x) * c + (v - y) * s, (v - y) * c - (u - x) * s);
            along.abs() < *long && across.abs() < broad * (1.0 - (along / long).powi(2))
        };
        let on_stem = |[ax, ay, bx, by]: &[f64; 4]| {
            let (dx, dy) = (bx - ax, by - ay);
            let along = (((u - ax) * dx + (v - ay) * dy) / (dx * dx + dy * dy).max(1e-9)).clamp(0.0, 1.0);
            (u - ax - dx * along).hypot(v - ay - dy * along) < 0.45
        };
        if leaves.iter().any(in_leaf) || stems.iter().any(on_stem) {
            return LEAF;
        }
        0
    })
}

fn drawn(kind: Drawn, [across, down]: [usize; 2], unbroken: &[bool], settings: &Mosaic) -> Vec<Tile> {
    let seed = settings.seed;
    match kind {
        Drawn::Planks => return planks([across, down], seed, settings.size),
        Drawn::Bricks | Drawn::Slabs => return walled(kind, [across, down], seed, settings.size, settings.bevel),
        _ => {}
    }
    let mut tiles: Vec<Tile> = vec![[0; 4]; across * down];
    let whole = |k: usize| unbroken[k];
    let (Some(i0), Some(i1)) = ((0..across).find(|i| (0..down).any(|j| whole(j * across + i))), (0..across).rfind(|i| (0..down).any(|j| whole(j * across + i)))) else { return tiles };
    let (Some(j0), Some(j1)) = ((0..down).find(|j| (0..across).any(|i| whole(j * across + i))), (0..down).rfind(|j| (0..across).any(|i| whole(j * across + i)))) else { return tiles };
    // Half the rectangle, in whole tiles, so that its middle is a corner of
    // the grid: a figure about a corner is made of whole and half tiles,
    // and about the middle of a tile it is not. (A floor an odd number of
    // tiles across has one row left over, which stays plain.)
    let (half_wide, half_deep) = (((i1 + 1 - i0) / 2) as f64, ((j1 + 1 - j0) / 2) as f64);
    if half_wide < 2.0 || half_deep < 2.0 {
        return tiles;
    }
    let middle = (i0 as f64 + half_wide, j0 as f64 + half_deep);
    let least = half_wide.min(half_deep);
    let mut numbers = Numbers(seed ^ 0x6A09_E667_F3BC_C908);
    // a whole number from `low` to `high`
    let mut pick = |low: f64, high: f64| (low + numbers.unit() * (high - low + 1.0)).floor().clamp(low, high.max(low));
    // A measure: how far a folded place is from a figure. Boxed, since the
    // figures of one pattern are of several kinds in one list, and there
    // are a few dozen at most.
    type Measure = Box<dyn Fn(f64, f64) -> f64>;
    const NONE: f64 = f64::INFINITY;
    // A figure about a middle: within `across` of it one way and `along`
    // the other, and within `both` of it by the two added together. Left
    // out (`NONE`), a limit is no limit: the third alone is a lozenge, the
    // first two a rectangle, all three an octagon or a pointed bar.
    let shape = |cx: f64, cy: f64, across: f64, along: f64, both: f64| -> Measure {
        Box::new(move |u, v| {
            let (a, b) = ((u - cx).abs(), (v - cy).abs());
            (a - across).max(b - along).max(a + b - both)
        })
    };
    // a lozenge, a square or an octagon, as the seed has it
    let any = |which: f64, cx: f64, cy: f64, size: f64, cut: f64| match which as i32 {
        0 => shape(cx, cy, NONE, NONE, size),
        1 => shape(cx, cy, size, size, NONE),
        _ => shape(cx, cy, size, size, size + cut.min(size)),
    };
    // which of a figure's bands are on
    #[derive(Clone, Copy, PartialEq)]
    enum Bands {
        /// by the run, inside and out
        Run,
        /// the outline alone
        Outline,
        /// all within, and no outline
        Within,
        /// the outline and all within
        Filled,
        /// the outline and all within but for the band next inside it
        Ringed,
    }
    // the figures of the part everything is folded onto, each with the number of the feature it is
    let mut figures: Vec<(u8, Measure, Bands)> = Vec::new();
    // how many tiles a repeat is, for a pattern that repeats
    let mut period = 2.0;
    match kind {
        Drawn::Snowflake => {
            // The eight ways a line may run: along the grid and along
            // its diagonals, which are the ways a tile can be cut.
            const WAYS: [(f64, f64); 8] = [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0), (-1.0, -1.0), (0.0, -1.0), (1.0, -1.0)];
            // A line from a place, one of the eight ways, for so many
            // tiles (counted along the grid, for a diagonal too). A
            // thick line is nothing along its length and grows to each
            // side, so that its first band is the line two tiles
            // thick. A thin one is below nothing in a strip one tile
            // wide to one side of the line: with the line down its
            // middle its edges would run through the middles of tiles.
            let line = |x: f64, y: f64, way: usize, long: f64, thick: bool| -> Measure {
                let (dx, dy) = WAYS[way % 8];
                let end = if dx != 0.0 && dy != 0.0 { 2.0 * long } else { long };
                Box::new(move |u, v| {
                    let (a, b) = (u - x, v - y);
                    let (along, side) = (a * dx + b * dy, if dx != 0.0 && dy != 0.0 { a * dx - b * dy } else { a * dy - b * dx });
                    (if thick { side.abs() } else { (side - 0.5).abs() - 0.5 }).max(-along).max(along - end)
                })
            };
            // The heart first: a figure shows wherever it is on, and the first to be on is the one seen.
            let long = (least - 2.0).max(4.0);
            figures.push((4, any(pick(0.0, 2.0), 0.0, 0.0, pick(2.0, (long / 5.0).max(2.0)), 1.0), Bands::Ringed));
            // How a real flake grows is the rule: an arm puts out side
            // branches at a fixed angle, shorter towards its tip, and
            // each branch puts out its own (Nakaya's dendrites; see
            // Libbrecht, "The physics of snow crystals", Rep. Prog.
            // Phys. 68, 2005). The automata that grow flakes cell by
            // cell (Reiter 2005; Gravner and Griffeath 2008) work on a
            // lattice of hexagons and give a shape of thousands of
            // cells, which bricks on a square grid would only show as
            // steps. So the rule is kept and the growing left out: the
            // arms are lines along the grid and its diagonals, eight of
            // them where ice has six.
            // How far apart an arm's branches are, and a branch's own:
            // by the arm's length, so that a large flake has as many
            // branches as a small one and not more of them closer
            // together. Set a fixed three tiles apart, the branches of
            // a flake sixty tiles across filled it solid.
            let apart = (long / pick(5.0, 8.0)).round().max(4.0);
            let spaced = [apart, (apart / pick(2.0, 3.0)).round().max(2.0)];
            let share = [pick(40.0, 60.0) / 100.0, pick(35.0, 55.0) / 100.0];
            let first = pick(2.0, 4.0);
            let short = pick((long * 0.45).ceil(), (long * 0.7).floor());
            // (a line still to be drawn: where from, which way, how long, how many branchings in, and its feature)
            let mut todo = vec![(0.0, 0.0, 1usize, short, 0usize, 2u8), (0.0, 0.0, 0, long, 0, 1)];
            while let Some((x, y, way, long, depth, feature)) = todo.pop() {
                let (dx, dy) = WAYS[way % 8];
                if depth < 2 {
                    // a filled lozenge at its tip
                    figures.push((5, shape(x + dx * long, y + dy * long, NONE, NONE, 1.0 - depth as f64), Bands::Filled));
                }
                figures.push((feature, line(x, y, way, long, depth == 0), if depth == 0 { Bands::Outline } else { Bands::Within }));
                if depth < 2 && long >= 3.0 {
                    let mut at = if depth == 0 { first } else { spaced[1] };
                    while at < long {
                        // (a branch's own branches stop short of the branch beside it)
                        let most = if depth == 0 { f64::INFINITY } else { (apart / 2.0 - 1.0).max(1.0) };
                        let branch = ((long - at) * share[depth]).round().clamp(1.0, most);
                        for turn in [1, 7] {
                            todo.push((x + dx * at, y + dy * at, (way + turn) % 8, branch, depth + 1, if depth == 0 { 3 } else { 6 }));
                        }
                        at += spaced[depth];
                    }
                }
            }
            // The plates: one or two outlines about the heart that the
            // arms pass through, as the ridges of a flake's plate are.
            for n in 0..pick(1.0, 2.0) as i32 {
                let out = (long * (0.35 + 0.3 * n as f64)).round();
                figures.push((7, if pick(0.0, 1.0) < 1.0 { shape(0.0, 0.0, NONE, NONE, out) } else { shape(0.0, 0.0, out, out, out + (out / 2.0).round()) }, Bands::Outline));
            }
        }
        Drawn::Rug | Drawn::Planks | Drawn::Bricks | Drawn::Slabs => {}
        Drawn::Medallions => {
            let half = pick(4.0, 8.0).min(least.max(2.0));
            period = half * 2.0;
            // at the middle of a repeat, at its corners, at the middle of its sides, and one between
            figures.push((1, any(pick(0.0, 2.0), 0.0, 0.0, pick(1.0, (half / 2.0).floor().max(1.0)), pick(1.0, 2.0)), Bands::Run));
            figures.push((2, any(pick(0.0, 2.0), half, half, pick(1.0, (half / 2.0).floor().max(1.0)), pick(1.0, 2.0)), Bands::Run));
            figures.push((3, any(pick(0.0, 2.0), half, 0.0, pick(0.0, (half / 3.0).floor()), 1.0), Bands::Run));
            let out = pick(2.0, (half - 1.0).max(2.0));
            figures.push((4, shape(out, pick(1.0, (out - 1.0).max(1.0)), NONE, NONE, pick(0.0, 1.0)), Bands::Run));
        }
    }
    let garden = (kind == Drawn::Rug).then(|| rug_garden(half_wide, half_deep, &mut pick));
    // The bands, outwards from a figure's outline: on and off by turns, one
    // or two bands of each. The first is on, so that every figure has its
    // outline.
    let mut run: Vec<bool> = Vec::new();
    for turn in 0..pick(1.0, 3.0) as usize * 2 {
        for _ in 0..pick(1.0, 2.0) as usize {
            run.push(turn % 2 == 0);
        }
    }
    // a place folded onto the part the figures are in
    let fold = |x: f64, y: f64| {
        let ordered = |u: f64, v: f64| (u.max(v), u.min(v));
        match kind {
            Drawn::Rug | Drawn::Planks | Drawn::Bricks | Drawn::Slabs => (x.abs(), y.abs()),
            Drawn::Snowflake => ordered(x.abs(), y.abs()),
            Drawn::Medallions => ordered((x - period * (x / period).round()).abs(), (y - period * (y / period).round()).abs()),
        }
    };
    // the colour at a place, in tiles from the middle
    let colour = |x: f64, y: f64| {
        let (u, v) = fold(x, y);
        if let Some(garden) = &garden {
            return garden(u, v);
        }
        // whether a figure is on at a place so far from it
        let lit = |bands: Bands, level: f64| {
            let band = level.floor() as i64;
            match bands {
                Bands::Run => run[band.rem_euclid(run.len() as i64) as usize],
                Bands::Outline => band == 0,
                Bands::Within => level < 0.0,
                Bands::Filled => band <= 0,
                Bands::Ringed => band == 0 || band <= -2,
            }
        };
        if kind == Drawn::Snowflake {
            // Every line of a snowflake shows wherever it is on, since
            // a plate's outline is to cross the arms and not hide them.
            // A branch that runs out of the eighth the flake is folded
            // onto is met again coming the other way, as the mirror
            // image of itself: so a figure is asked about a place and
            // about the place's two mirror images in the eighth's sides.
            return figures.iter().find(|(_, far, bands)| lit(*bands, far(u, v).min(far(v, u)).min(far(u, -v)))).map_or(0, |figure| figure.0);
        }
        let (mut level, mut feature, mut bands) = (f64::INFINITY, 0u8, Bands::Run);
        for (number, far, which) in &figures {
            let here = far(u, v);
            // Nearer by more than rounding: a place just as far from two
            // figures goes to the first of them, and so does its mirror
            // image, which is worked out by other sums and may come out
            // a hair nearer the second.
            if here < level - 1e-9 {
                (level, feature, bands) = (here, *number, *which);
            }
        }
        if lit(bands, level) {
            feature
        } else {
            0
        }
    };
    for j in j0..j0 + 2 * half_deep as usize {
        for i in i0..i0 + 2 * half_wide as usize {
            let (x, y) = (i as f64 - middle.0, j as f64 - middle.1);
            tiles[j * across + i] = tile_at(&colour, x, y);
        }
    }
    tiles
}

/// Fit a mosaic over a region and push it through cells `w0..w0 + depth`
/// across `axis`, onto `bricks`, with each brick's colour (0 to 7) onto
/// `shades`. Returns the fit of the region.
///
/// The region is fitted first as the one solid it is, so that its edge is
/// as smooth as any flat form's. Then it is coloured. It is cut into
/// tiles, `tile` cells across and `tall` deep, on the brick grid, and wave
/// function collapse gives each tile one of the style's tiles: one colour,
/// or two triangles of two colours either side of a diagonal.
///
/// A tile cut into triangles is two sloped bricks, each a tile across and
/// a tile deep, which for a tile wider than it is deep is a long ramp. The
/// two fill one box between them, as the two halves of a sloped pair do
/// anywhere else. Everything else the fit made is whole cells, and those
/// are sorted by their tile's colour and joined again into the largest
/// bricks of one colour, so that tiles of a colour side by side are one
/// brick and not a brick each.
///
/// The colours were first fitted one at a time, each colour's tiles as a
/// solid of its own. The outline fitter is made to prefer a slope to a
/// staircase, and so it turned the stepped edge between two colours of
/// small tiles into a slope, on both sides: bricks of two colours in one
/// place. Sorting whole cells cannot do that.
///
/// A tile is plain ground, the style's first colour, wherever it is not
/// whole: where the edge of the region or one of the edge's sloped bricks
/// cuts into it, since a brick is one colour. So are the tiles within
/// `border` of the edge. A floor mosaic has a border, and it keeps the
/// pattern clear of the part of the floor the edge has cut up.
#[allow(clippy::too_many_arguments)]
pub fn lay_mosaic(region: &dyn Fn(DVec2) -> f64, settings: &Mosaic, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    // A tile is one cell. It could once be set larger, and wider than
    // deep, and that did nothing a player wanted: it drew the same
    // pattern coarser, or stretched. (The sums below still carry a
    // tile's two sizes, for a pattern of larger tiles to be had again by
    // changing this one line.)
    let (wide, tall): (i32, i32) = (1, 1);
    let style = &STYLES[settings.style.min(STYLES.len() - 1)];
    // The floor is fitted with no brick longer than a few cells. A sloped
    // brick is one colour, and along a slanting edge the fit otherwise
    // lays sloped bricks twenty cells long, which stood in the pattern as
    // bare triangles the size of a medallion. The whole cells are joined
    // again into long bricks below, by colour, so only the sloped bricks
    // of the edge are the more for it. The price is the edge: a slant
    // drawn in bricks of five cells follows the form a little less
    // closely than one drawn in bricks of twenty (on a band 20 wide
    // curving through a third of a turn, 1.5% of its area amiss against
    // 0.6%). At three cells the edge was plainly ragged, and at eight
    // the triangles were still plain to see.
    let fit = fit_planar(&|x, y| region(DVec2::new(x, y)), min, max, &Opts { max_ext: opts.max_ext.min(EDGE_BRICK), ..*opts });
    // the tiles that cover the box, set on multiples of a tile's size so
    // that moving the form does not slide the pattern under it
    // A style that is its own mirror image has its tiles set out from the
    // middle of the floor instead. Set on multiples of a tile's size, a
    // floor that did not happen to lie evenly on them had a part row of
    // tiles down one side and none down the other, and the pattern sat
    // to one side of the floor.
    let mirrored = matches!(style.drawn, Drawn::Rug | Drawn::Snowflake | Drawn::Medallions);
    let from = if mirrored { [(min[0] + max[0]).div_euclid(2), (min[1] + max[1]).div_euclid(2)] } else { [0, 0] };
    let first = [(min[0] - from[0]).div_euclid(wide), (min[1] - from[1]).div_euclid(tall)];
    let across = ((max[0] - from[0] + wide - 1).div_euclid(wide) - first[0]).max(1) as usize;
    let down = ((max[1] - from[1] + tall - 1).div_euclid(tall) - first[1]).max(1) as usize;
    let tile_of = |x: i32, y: i32| ((y - from[1]).div_euclid(tall) - first[1]) as usize * across + ((x - from[0]).div_euclid(wide) - first[0]) as usize;
    let middle = |k: usize| DVec2::new(from[0] as f64 + ((first[0] + (k % across) as i32) as f64 + 0.5) * wide as f64, from[1] as f64 + ((first[1] + (k / across) as i32) as f64 + 0.5) * tall as f64);
    // how deep inside the region a tile's middle is, in tiles (by its shorter side): under nothing, it is outside
    let inside: Vec<f64> = (0..across * down).map(|k| -region(middle(k)) / wide.min(tall) as f64).collect();
    let cells = |piece: &crate::fit_planar::Piece| {
        let (x, y, w, h) = (piece.x, piece.y, piece.w, piece.h);
        (0..w * h).map(move |k| (x + k % w, y + k / w))
    };
    // The fit's pieces, with each long sloped one taken apart. A sloped
    // brick is one colour, and along a slanting edge the fit lays sloped
    // bricks many cells long, which showed as plain triangles standing in
    // the pattern all down the edge. A triangle whose two sides share a
    // divisor is that many smaller triangles of the same slope along its
    // sloped side, over a staircase of whole cells, and the whole cells
    // take the pattern like any others. (A triangle of 8 by 4 is four of
    // 2 by 1; one of 7 by 3 has no smaller and stays as it is.) The
    // surface is the same either way: the small triangles' sloped sides
    // are the large one's, end to end.
    let mut pieces: Vec<crate::fit_planar::Piece> = Vec::new();
    for piece in &fit.pieces {
        let steps = {
            let (mut a, mut b) = (piece.w, piece.h);
            while b != 0 {
                (a, b) = (b, a % b);
            }
            a.max(1)
        };
        if piece.mask == 15 || steps == 1 {
            pieces.push(*piece);
            continue;
        }
        let missing = (!piece.mask & 15).trailing_zeros() as i32;
        let (w, h) = (piece.w / steps, piece.h / steps);
        for j in 0..steps {
            for i in 0..steps {
                // how many steps towards the corner the triangle lacks, each way
                let towards = (if missing & 1 == 1 { i } else { steps - 1 - i }) + (if missing & 2 == 2 { j } else { steps - 1 - j });
                if towards < steps {
                    pieces.push(crate::fit_planar::Piece { mask: if towards == steps - 1 { piece.mask } else { 15 }, x: piece.x + i * w, y: piece.y + j * h, w, h });
                }
            }
        }
    }
    // how many of each tile's cells the fit made whole: a tile short of
    // any, or within the border, is plain
    let mut whole_cells = vec![0; across * down];
    for piece in pieces.iter().filter(|piece| piece.mask == 15) {
        for (x, y) in cells(piece) {
            whole_cells[tile_of(x, y)] += 1;
        }
    }
    let unbroken: Vec<bool> = whole_cells.iter().map(|n| *n == wide * tall).collect();
    let edge = settings.border.clamp(0, 8) as f64 - 0.5;
    let plain: Vec<bool> = (0..across * down).map(|k| !unbroken[k] || inside[k] < edge).collect();
    // A snowflake is the flake alone: where its pattern is bare ground no
    // brick is laid, so that it can be stood up, or laid on a floor of
    // another form, with nothing round it or in the gaps between its arms.
    let open = style.drawn == Drawn::Snowflake;
    let plain_colour = 0;
    let mut tiles = drawn(style.drawn, [across, down], &unbroken, settings);
    // (the pattern as it was drawn, for the sloped bricks of the edge to be coloured by)
    let pattern = tiles.clone();
    for k in (0..across * down).filter(|k| plain[*k]) {
        tiles[k] = [plain_colour; 4];
    }
    let mut push = |mask4: u8, x: i32, y: i32, w: i32, h: i32, shade: u8| {
        if open && shade == 0 {
            return;
        }
        let mask = (0..8u8).fold(0u8, |m, c| m | (mask4 >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c);
        let mut at = 0;
        while at < depth {
            let run = (depth - at).min(opts.max_ext.max(1));
            let (mut min, mut ext) = ([0; 3], [0; 3]);
            (min[ua], min[va], min[axis]) = (x, y, w0 + at);
            (ext[ua], ext[va], ext[axis]) = (w, h, run);
            bricks.push(CellBrick { mask, min, ext });
            shades.push(shade);
            at += run;
        }
    };
    // the tiles cut into two triangles: a sloped brick each, a tile in size
    // (a piece's mask has a bit for each corner it fills: 1 top left, 2 top
    // right, 4 bottom left, 8 bottom right, rows running the way y does)
    for (k, tile) in tiles.iter().enumerate().filter(|(_, tile)| tile.contains(&ON_CUT)) {
        let (x, y) = (from[0] + (first[0] + (k % across) as i32) * wide, from[1] + (first[1] + (k / across) as i32) * tall);
        if tile[1] == ON_CUT {
            // cut from bottom left to top right
            push(0b0111, x, y, wide, tall, tile[0]);
            push(0b1110, x, y, wide, tall, tile[3]);
        } else {
            // cut from top left to bottom right
            push(0b1101, x, y, wide, tall, tile[2]);
            push(0b1011, x, y, wide, tall, tile[1]);
        }
    }
    // the whole cells of every other tile, each with its tile's colour
    let mut whole: Vec<(i32, i32, u8)> = Vec::new();
    for piece in &pieces {
        if piece.mask == 15 {
            whole.extend(cells(piece).filter(|(x, y)| !tiles[tile_of(*x, *y)].contains(&ON_CUT)).map(|(x, y)| (y, x, tiles[tile_of(x, y)][0])));
        } else {
            // A sloped brick of the edge: the colour the pattern has
            // where the middle of its solid half lies, a third of the
            // way in from its square corner, unless that is within the
            // border. Left plain, the edge of a mosaic with no border
            // was a row of bare triangles.
            let missing = (!piece.mask & 15).trailing_zeros();
            let (x, y) = (piece.x as f64 + piece.w as f64 * if missing & 1 == 1 { 1.0 / 3.0 } else { 2.0 / 3.0 }, piece.y as f64 + piece.h as f64 * if missing & 2 == 2 { 1.0 / 3.0 } else { 2.0 / 3.0 });
            let k = tile_of(x.floor() as i32, y.floor() as i32);
            // (how far across its tile the place is, and how far down, each from 0 to 1)
            let (across_tile, down_tile) = (((x - from[0] as f64) / wide as f64).rem_euclid(1.0), ((y - from[1] as f64) / tall as f64).rem_euclid(1.0));
            let colour = match pattern[k] {
                _ if inside[k] < edge => plain_colour,
                // (of a tile cut in two, the half the place is in)
                [a, ON_CUT, ON_CUT, b] => if across_tile + down_tile < 1.0 { a } else { b },
                [ON_CUT, a, b, ON_CUT] => if across_tile > down_tile { a } else { b },
                [colour, ..] => colour,
            };
            push(piece.mask, piece.x, piece.y, piece.w, piece.h, colour);
        }
    }
    // Joined into bricks of one colour: along each row into runs, and a run
    // onto the brick above it where that is the same run of the same
    // colour. (The largest rectangles would be fewer bricks still, and are
    // a harder problem than a floor's worth of bricks is worth.)
    whole.sort();
    let limit = opts.max_ext.max(1);
    let mut open: Vec<(i32, i32, i32, i32, u8)> = Vec::new(); // x, y, w, h, colour: bricks that end on the row above
    let mut at = 0;
    while at < whole.len() {
        let row = whole[at].0;
        let mut runs: Vec<(i32, i32, u8)> = Vec::new(); // x, w, colour
        while at < whole.len() && whole[at].0 == row {
            let (_, x, colour) = whole[at];
            match runs.last_mut() {
                Some(run) if run.0 + run.1 == x && run.2 == colour && run.1 < limit => run.1 += 1,
                _ => runs.push((x, 1, colour)),
            }
            at += 1;
        }
        let mut still = Vec::new();
        for (x, w, colour) in runs {
            match open.iter().position(|b| b.0 == x && b.2 == w && b.4 == colour && b.1 + b.3 == row && b.3 < limit) {
                Some(above) => {
                    let mut brick = open.swap_remove(above);
                    brick.3 += 1;
                    still.push(brick);
                }
                None => still.push((x, row, w, 1, colour)),
            }
        }
        // a brick nothing was joined to this row is finished
        for (x, y, w, h, colour) in open.drain(..) {
            push(15, x, y, w, h, colour);
        }
        open = still;
    }
    for (x, y, w, h, colour) in open {
        push(15, x, y, w, h, colour);
    }
    fit
}

/// Which separate piece of a flat build each brick belongs to: bricks that
/// touch are one piece, and each stone of a path is a piece of its own.
/// Returns a number for each brick, the same for bricks of one piece.
///
/// The pieces are found from the bricks alone, with no memory of the stones
/// they were laid as, so that a build can be flipped, turned, scaled or
/// read back from a file and still be told apart into its stones. Whether
/// two bricks touch is asked of their solid parts and not of their boxes:
/// across a narrow gap, the empty half of one stone's sloped brick shares
/// cells with its neighbour's. So each brick is sampled, four samples to a
/// cell each way, seen along the axis the build is pushed through, and
/// samples side by side are one piece.
///
/// A build that is not flat (some brick is not the same all the way
/// through along any one axis) is one piece.
pub fn pieces(bricks: &[CellBrick]) -> Vec<u32> {
    const FINE: i32 = 4;
    // the axis every brick is a prism along: its corners at one end match those at the other
    let through = (0..3).find(|axis| bricks.iter().all(|b| (0..8u8).filter(|c| c >> axis & 1 == 0).all(|c| (b.mask >> c & 1) == (b.mask >> (c | 1 << axis) & 1))));
    let Some(axis) = through else { return vec![0; bricks.len()] };
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    // each sample that some brick's solid part covers, and one brick that covers it
    let mut held: HashMap<(i32, i32), usize> = HashMap::new();
    for (n, b) in bricks.iter().enumerate() {
        let Some(info) = crate::shapes::shape_info(b.mask) else { continue };
        for j in 0..b.ext[va] * FINE {
            for i in 0..b.ext[ua] * FINE {
                let mut at = [0.5; 3];
                (at[ua], at[va]) = ((i as f64 + 0.5) / (b.ext[ua] * FINE) as f64, (j as f64 + 0.5) / (b.ext[va] * FINE) as f64);
                if crate::shapes::inside_shape(info, at[0], at[1], at[2]) {
                    held.insert((b.min[ua] * FINE + i, b.min[va] * FINE + j), n);
                }
            }
        }
    }
    // spread a piece's number from sample to sample
    let mut piece_of_sample: HashMap<(i32, i32), u32> = HashMap::new();
    let mut piece_of_brick: Vec<Option<u32>> = vec![None; bricks.len()];
    let mut next = 0;
    let mut starts: Vec<(i32, i32)> = held.keys().copied().collect();
    starts.sort();
    for start in starts {
        if piece_of_sample.contains_key(&start) {
            continue;
        }
        let mut open = vec![start];
        while let Some(at) = open.pop() {
            let Some(brick) = held.get(&at) else { continue };
            if piece_of_sample.insert(at, next).is_some() {
                continue;
            }
            piece_of_brick[*brick].get_or_insert(next);
            open.extend([(at.0 + 1, at.1), (at.0 - 1, at.1), (at.0, at.1 + 1), (at.0, at.1 - 1)]);
        }
        next += 1;
    }
    piece_of_brick.into_iter().map(|piece| piece.unwrap_or(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A band 16 wide along the x axis, 120 long.
    fn band(p: DVec2) -> f64 {
        (p.y.abs() - 8.0).max(p.x.abs() - 60.0)
    }

    const BOX: ([i32; 2], [i32; 2]) = ([-62, -10], [62, 10]);

    fn usual() -> Stones {
        Stones { size: 7.0, gap: 1.5, uneven: 0.5, seed: 3 }
    }

    #[test]
    fn stones_are_scattered_inside_the_region_apart_and_the_same_for_one_seed() {
        let points = scatter(&band, BOX.0, BOX.1, &usual());
        // a band 120 by 16 has room for some forty stones 7 across
        assert!((25..70).contains(&points.len()), "{} stones", points.len());
        assert!(points.iter().all(|p| band(*p) < 0.0), "a stone's middle is outside the path");
        let nearest = |p: &DVec2| points.iter().filter(|q| *q != p).map(|q| q.distance(*p)).fold(f64::INFINITY, f64::min);
        assert!(points.iter().all(|p| nearest(p) >= 7.0 * 0.6), "two stones grew from nearly one place");
        assert_eq!(scatter(&band, BOX.0, BOX.1, &usual()), points, "one seed gave two scatterings");
        assert_ne!(scatter(&band, BOX.0, BOX.1, &Stones { seed: 4, ..usual() }), points, "two seeds gave one scattering");
        // uneven stones are let closer together: there are more of them
        let (even, uneven) = (scatter(&band, BOX.0, BOX.1, &Stones { uneven: 0.0, ..usual() }), scatter(&band, BOX.0, BOX.1, &Stones { uneven: 1.0, ..usual() }));
        assert!(uneven.len() > even.len(), "{} uneven, {} even", uneven.len(), even.len());
    }

    #[test]
    fn a_stone_holds_its_own_point_and_stops_short_of_its_neighbours_and_the_edge() {
        let all = stones(&band, BOX.0, BOX.1, &usual());
        for stone in &all {
            assert!(stone.eval(stone.at, &band) < 0.0, "a stone does not hold the point it grew from");
            for (towards, far) in &stone.lines {
                // on the line half way to a neighbour: in the gap, in neither stone
                let on_line = stone.at + *towards * (far - towards.dot(stone.at));
                assert!(stone.eval(on_line, &band) > 0.0);
            }
        }
        // no stone reaches outside the path
        for k in 0..400 {
            let p = DVec2::new(-61.0 + (k % 40) as f64 * 3.1, if k % 2 == 0 { 8.3 } else { -8.3 });
            assert!(all.iter().all(|s| s.eval(p, &band) > 0.0), "a stone holds {p:?}, outside the path");
        }
    }

    #[test]
    fn a_mosaic_covers_the_floor_in_coloured_tiles_with_a_plain_border() {
        // a disc of radius 30
        let disc = |p: DVec2| p.length() - 30.0;
        let (min, max) = ([-32, -32], [32, 32]);
        let whole_floor = fit_planar(&|x, y| disc(DVec2::new(x, y)), min, max, &Opts { max_ext: EDGE_BRICK, ..Opts::default() });
        for (style, named) in STYLES.iter().enumerate() {
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            let settings = Mosaic { style, size: 2, bevel: 2, border: 1, seed: 7 };
            let fit = lay_mosaic(&disc, &settings, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
            let name = named.name;
            assert_eq!(bricks.len(), shades.len(), "{name}");
            // the floor is the form's own fit, coloured: the same area, as closely fitted
            assert_eq!((fit.solid, fit.error), (whole_floor.solid, whole_floor.error), "{name}");
            // every place of the floor is in one brick, and none in two
            let holding = |x: f64, y: f64| bricks.iter().filter(|b| x >= b.min[0] as f64 && x < (b.min[0] + b.ext[0]) as f64 && y >= b.min[1] as f64 && y < (b.min[1] + b.ext[1]) as f64).filter(|b| crate::shapes::inside_shape(crate::shapes::shape_info(b.mask).unwrap(), (x - b.min[0] as f64) / b.ext[0] as f64, (y - b.min[1] as f64) / b.ext[1] as f64, 0.5)).count();
            for k in 0..120 * 120 {
                let p = DVec2::new(-30.0 + (k % 120) as f64 / 2.0 + 0.13, -30.0 + (k / 120) as f64 / 2.0 + 0.19);
                let count = holding(p.x, p.y);
                assert!(count <= 1, "{name}: {count} bricks hold {p:?}");
                assert!(count == 1 || disc(p) > -1.5 || name == "snowflake", "{name}: no brick holds {p:?}, well inside the floor");
            }
            // more than one colour, and fewer bricks than two to each of the 2,800 cells of the disc
            let used: HashSet<u8> = shades.iter().copied().collect();
            assert!(used.len() >= 2 && used.iter().all(|shade| *shade < 8), "{name}: colours {used:?}");
            assert!(bricks.len() < 5000, "{name}: {} bricks", bricks.len());
            // one seed is one mosaic
            let again = |seed: u64| {
                let (mut b, mut s) = (Vec::new(), Vec::new());
                lay_mosaic(&disc, &Mosaic { seed, ..settings }, 2, min, max, 0, 1, &Opts::default(), &mut b, &mut s);
                (b, s)
            };
            assert_eq!(again(7), (bricks.clone(), shades.clone()), "{name}");
        }
    }

    #[test]
    fn a_drawn_style_is_a_dense_pattern_of_cut_tiles_and_its_own_mirror_image() {
        // a floor 120 by 80, in tiles of 2: 60 tiles by 40
        let floor = |p: DVec2| (p.x.abs() - 60.0).max(p.y.abs() - 40.0);
        let (min, max) = ([-60, -40], [60, 40]);
        for (style, named) in STYLES.iter().enumerate().filter(|(_, style)| matches!(style.drawn, Drawn::Rug | Drawn::Snowflake | Drawn::Medallions)) {
            let name = named.name;
            let lay = |seed: u64| {
                let (mut bricks, mut shades) = (Vec::new(), Vec::new());
                lay_mosaic(&floor, &Mosaic { style, size: 2, bevel: 2, border: 1, seed }, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
                (bricks, shades)
            };
            for seed in [1, 5, 12] {
                let (bricks, shades) = lay(seed);
                // the colours at a place: of the bricks that hold it
                let colours = |x: f64, y: f64| -> Vec<u8> { bricks.iter().zip(&shades).filter(|(b, _)| x >= b.min[0] as f64 && x < (b.min[0] + b.ext[0]) as f64 && y >= b.min[1] as f64 && y < (b.min[1] + b.ext[1]) as f64 && crate::shapes::inside_shape(crate::shapes::shape_info(b.mask).unwrap(), (x - b.min[0] as f64) / b.ext[0] as f64, (y - b.min[1] as f64) / b.ext[1] as f64, 0.5)).map(|(_, shade)| *shade).collect() };
                // every place is one colour, and the same colour as its mirror images left to right and top to bottom
                for k in 0..2000 {
                    let (x, y) = ((k % 50) as f64 * 1.19 + 0.313, (k / 50) as f64 * 0.97 + 0.211);
                    let here = colours(x, y);
                    assert!((here.len() == 1 || (name == "snowflake" && here.is_empty())) && here == colours(-x, y) && here == colours(x, -y), "{name}, seed {seed}: the pattern differs across its middle at {x}, {y}: {here:?}, {:?}, {:?}", colours(-x, y), colours(x, -y));
                }
                // a dense pattern: several features, many sloped bricks, and none of it most of the floor
                let sloped = bricks.iter().filter(|b| b.mask != 0xff).count();
                assert!(sloped > 150, "{name}, seed {seed}: {sloped} sloped bricks");
                let used: HashSet<u8> = shades.iter().copied().collect();
                assert!(used.len() >= 4 && used.iter().all(|shade| *shade < 8), "{name}, seed {seed}: features {used:?}");
                for shade in 1..8u8 {
                    let area: f64 = bricks.iter().zip(&shades).filter(|(_, s)| **s == shade).map(|(b, _)| (b.ext[0] * b.ext[1]) as f64 * if b.mask == 0xff { 1.0 } else { 0.5 }).sum();
                    assert!(area < 0.5 * 9600.0, "{name}, seed {seed}: feature {shade} covers {area} of 9600");
                }
            }
            // one seed is one pattern, and another seed another
            assert_eq!(lay(5), lay(5), "{name}");
            assert_ne!(lay(5), lay(6), "{name}");
        }
    }

    #[test]
    fn features_share_the_colours_and_the_ground_keeps_its_own() {
        // two colours: every feature is the second
        assert_eq!((0..8).map(|shade| colour_of(shade, 2)).collect::<Vec<_>>(), [0, 1, 1, 1, 1, 1, 1, 1]);
        // four: the features go round the three that are not the ground's
        assert_eq!((0..8).map(|shade| colour_of(shade, 4)).collect::<Vec<_>>(), [0, 1, 2, 3, 1, 2, 3, 1]);
        // as many as there are features: each its own
        assert_eq!((0..8).map(|shade| colour_of(shade, 8)).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!((colour_of(5, 1), colour_of(5, 0)), (0, 0));
    }

    #[test]
    fn a_floor_of_planks_is_rows_of_boards_of_many_lengths() {
        let (across, down) = (200, 12);
        let tiles = planks([across, down], 3, 2);
        let mut lengths = HashSet::new();
        for row in (0..down).step_by(2) {
            // a row of boards is two rows of tiles
            assert_eq!(tiles[row * across..(row + 1) * across], tiles[(row + 1) * across..(row + 2) * across]);
            // the boards of the row: runs of one number
            let mut run = 1;
            for at in 1..=across {
                if at < across && tiles[row * across + at] == tiles[row * across + at - 1] {
                    run += 1;
                } else {
                    // (a board is eight to twenty-four tiles, but for the offcuts at a row's two ends)
                    assert!(run <= 24 && (run >= 8 || at == across || at == run), "row {row}: a board of {run} tiles ending at {at}");
                    lengths.insert(run);
                    run = 1;
                }
            }
        }
        assert!(lengths.len() >= 8, "boards of only {lengths:?} tiles");
        // no tile is cut, every number is used, and the rows differ
        assert!(tiles.iter().all(|tile| tile[0] < 8 && *tile == [tile[0]; 4]));
        assert_eq!(tiles.iter().map(|tile| tile[0]).collect::<HashSet<_>>().len(), 8);
        assert_ne!(tiles[..across], tiles[2 * across..3 * across]);
        assert_eq!(planks([across, down], 3, 2), tiles);
        // half the size is boards half as broad
        let narrow = planks([across, down], 3, 1);
        assert_ne!(narrow[..across], narrow[across..2 * across]);
    }

    #[test]
    fn a_wall_is_bricks_in_courses_or_slabs_with_mortar_between() {
        let (across, down) = (90, 40);
        // bricks: no tile is cut; a course is three tiles of brick and one of mortar; a brick is eight tiles and one of mortar
        let bricks = walled(Drawn::Bricks, [across, down], 4, 2, 0);
        assert!(bricks.iter().all(|tile| *tile == [tile[0]; 4] && tile[0] < 8));
        for row in 0..down {
            assert_eq!(bricks[row * across..(row + 1) * across].iter().all(|tile| tile[0] == 0), row % 4 == 3, "row {row}");
        }
        let joints = |row: usize| (0..across).filter(|at| bricks[row * across + at][0] == 0).collect::<Vec<_>>();
        let mut starts = HashSet::new();
        for row in (0..down).step_by(4) {
            assert!(joints(row).windows(2).all(|pair| pair[1] - pair[0] == 9), "row {row}: joints at {:?}", joints(row));
            starts.insert(joints(row)[0]);
        }
        // the courses are moved along by different amounts
        assert!(starts.len() >= 4, "every course starts at one of {starts:?}");
        assert_eq!(bricks.iter().map(|tile| tile[0]).collect::<HashSet<_>>().len(), 8);
        // a larger size is larger bricks
        assert!(walled(Drawn::Bricks, [across, down], 4, 4, 0)[..across].iter().filter(|tile| tile[0] == 0).count() < joints(0).len());
        // slabs with square corners: no tile cut, mortar between a tenth and a half of the wall, stones of many sizes
        let slabs = walled(Drawn::Slabs, [across, down], 4, 2, 0);
        assert!(slabs.iter().all(|tile| *tile == [tile[0]; 4] && tile[0] < 8));
        let mortar = slabs.iter().filter(|tile| **tile == [0; 4]).count() as f64 / (across * down) as f64;
        assert!((0.1..0.5).contains(&mortar), "{mortar} of the wall is mortar");
        let lengths: HashSet<usize> = (0..across).scan(0, |run, at| { *run = if slabs[at][0] == 0 { 0 } else { *run + 1 }; Some(*run) }).collect();
        assert!(lengths.len() > 12, "stones of lengths up to {:?}", lengths.iter().max());
        // with their corners cut off: tiles cut in two at every corner
        let bevelled = walled(Drawn::Slabs, [across, down], 4, 2, 2);
        assert!(bevelled.iter().filter(|tile| tile.contains(&ON_CUT)).count() > 60);
        assert_eq!(walled(Drawn::Slabs, [across, down], 4, 2, 2), bevelled);
        assert_ne!(walled(Drawn::Slabs, [across, down], 5, 2, 2), bevelled);
    }

    #[test]
    fn a_mirrored_style_sits_in_the_middle_of_a_floor_that_is_off_the_grid() {
        // a floor 121 by 79 whose corner is at no multiple of a tile of 4 by 3
        let floor = |p: DVec2| ((p.x - 62.5).abs() - 60.5).max((p.y - 46.5).abs() - 39.5);
        let style = STYLES.iter().position(|s| s.name == "rug").unwrap();
        let (mut bricks, mut shades) = (Vec::new(), Vec::new());
        lay_mosaic(&floor, &Mosaic { style, size: 4, bevel: 3, border: 0, seed: 2 }, 2, [2, 7], [123, 86], 0, 1, &Opts::default(), &mut bricks, &mut shades);
        // the coloured bricks reach as near one side of the floor as the other
        let coloured = || bricks.iter().zip(&shades).filter(|(_, shade)| **shade != 0).map(|(brick, _)| brick);
        for (axis, (low, high)) in [(2, 123), (7, 86)].into_iter().enumerate() {
            let (least, most) = (coloured().map(|b| b.min[axis]).min().unwrap(), coloured().map(|b| b.min[axis] + b.ext[axis]).max().unwrap());
            assert!(((least - low) - (high - most)).abs() <= 1, "along axis {axis} the pattern runs from {least} to {most} on a floor from {low} to {high}");
        }
    }

    #[test]
    fn a_path_is_laid_as_separate_stones_with_sloped_edges() {
        let mut bricks = Vec::new();
        let mut shades = Vec::new();
        let fit = lay_stones(&band, &usual(), 2, BOX.0, BOX.1, 0, 2, &Opts::default(), &mut bricks, &mut shades);
        assert_eq!(shades.len(), bricks.len());
        assert!(!bricks.is_empty() && fit.solid > 0.0);
        // the stones cover most of the path and not all of it: there are gaps
        let cover = fit.solid / (120.0 * 16.0);
        assert!((0.45..0.9).contains(&cover), "{cover} of the path is stone");
        // closely fitted, with sloped pieces and not only squares
        assert!(fit.error < 0.2 * fit.solid, "{} wrong of {}", fit.error, fit.solid);
        assert!(bricks.iter().any(|b| b.mask != 0xff), "the stones are all square blocks");
        // two cells deep, on the ground, and no place in two bricks: boxes
        // may share cells across a gap, so it is places that are asked about
        assert!(bricks.iter().all(|b| b.min[2] == 0 && b.ext[2] == 2));
        for k in 0..124 * 20 * 16 {
            let (x, y) = (-62.0 + (k % (124 * 4)) as f64 / 4.0 + 0.13, -10.0 + (k / (124 * 4)) as f64 / 4.0 + 0.11);
            let holding = bricks.iter().filter(|b| x >= b.min[0] as f64 && x < (b.min[0] + b.ext[0]) as f64 && y >= b.min[1] as f64 && y < (b.min[1] + b.ext[1]) as f64).filter(|b| crate::shapes::inside_shape(crate::shapes::shape_info(b.mask).unwrap(), (x - b.min[0] as f64) / b.ext[0] as f64, (y - b.min[1] as f64) / b.ext[1] as f64, 0.5)).count();
            assert!(holding <= 1, "{holding} bricks hold the place {x}, {y}");
        }
        // every stone's edge is finished: among the bricks on a stone's rim
        // there are sloped ones, and few stones are all square blocks
        // the bricks fall into separate stones, about as many as were scattered
        let scattered = scatter(&band, BOX.0, BOX.1, &usual()).len();
        let of = pieces(&bricks);
        let stones: HashSet<u32> = of.iter().copied().collect();
        assert!(stones.len() * 10 >= scattered * 8 && stones.len() <= scattered, "{} separate stones from {scattered} points", stones.len());
        // and the same stones however the bricks are listed
        let mut turned: Vec<CellBrick> = bricks.iter().rev().copied().collect();
        turned.iter_mut().for_each(|b| (b.min[0], b.min[2], b.ext[0], b.ext[2]) = (b.min[2], b.min[0], b.ext[2], b.ext[0]));
        assert_eq!(pieces(&turned[..0]).len(), 0);
        // a build that is not flat is one piece
        let lump = [CellBrick { mask: 0x17, min: [0; 3], ext: [1; 3] }, CellBrick { mask: 0xff, min: [5, 5, 5], ext: [1; 3] }];
        assert_eq!(pieces(&lump), [0, 0]);
    }
}
