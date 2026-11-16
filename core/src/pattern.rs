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
pub const LEAST_GAP: f64 = 1.0;

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
/// (each fit may stray half a cell from its stone, and the least gap is one
/// cell) is the later stone fitted again a little smaller.
#[allow(clippy::too_many_arguments)]
pub fn lay_stones(region: &dyn Fn(DVec2) -> f64, settings: &Stones, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>) -> Fit {
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
    for mut stone in stones(region, min, max, settings) {
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
    /// how many cells across one tile of the mosaic is
    pub tile: i32,
    /// how many tiles wide the plain border round the edge is, at least 1
    pub border: i32,
    /// which filling: any number, each a different mosaic
    pub seed: u64,
}

/// A style of mosaic: a small sample whose motifs wave function collapse
/// lays out afresh over the floor (see [`crate::wfc`]).
pub struct Style {
    pub name: &'static str,
    /// the sample, rows of digits, each digit one of the form's colours
    sample: &'static str,
    /// whether the motifs may lie any way round
    turned: bool,
}

/// The styles on offer. Each sample is a text file beside this one, so
/// that a style can be drawn and changed without reading any Rust.
pub const STYLES: [Style; 5] = [
    Style { name: "key", sample: include_str!("mosaics/key.txt"), turned: true },
    Style { name: "diamonds", sample: include_str!("mosaics/diamonds.txt"), turned: true },
    Style { name: "weave", sample: include_str!("mosaics/weave.txt"), turned: true },
    Style { name: "flowers", sample: include_str!("mosaics/flowers.txt"), turned: true },
    Style { name: "waves", sample: include_str!("mosaics/waves.txt"), turned: false },
];

/// How many times a mosaic is started again after a contradiction before
/// the floor is laid plain.
const TRIES: usize = 30;

/// Fit a mosaic over a region and push it through cells `w0..w0 + depth`
/// across `axis`, onto `bricks`, with each brick's colour (a digit of the
/// style's sample) onto `shades`. Returns the fit of the region.
///
/// The region is fitted first as the one solid it is, so that its edge is
/// as smooth as any flat form's. Then it is coloured. It is cut into square
/// tiles, `tile` cells across, on the brick grid, and wave function
/// collapse gives each tile a colour. The sloped bricks of the edge keep
/// their places, in the style's first colour. Everything else the fit made
/// is whole cells, and those are sorted by their tile's colour and joined
/// again into the largest bricks of one colour, so that tiles of a colour
/// side by side are one brick and not a brick each.
///
/// The colours were first fitted one at a time, each colour's tiles as a
/// solid of its own. The outline fitter is made to prefer a slope to a
/// staircase, and so it turned the stepped edge between two colours of
/// small tiles into a slope, on both sides: bricks of two colours in one
/// place. Sorting whole cells cannot do that.
///
/// A tile that a sloped brick of the edge reaches into is the first
/// colour throughout, since a brick is one colour, and so are the tiles
/// within `border` of the edge: a floor mosaic has a border, and it keeps
/// the pattern clear of the part of the floor the edge has cut up.
///
/// If the pattern cannot be completed (see `wfc`), the floor is laid plain,
/// in the style's first colour.
#[allow(clippy::too_many_arguments)]
pub fn lay_mosaic(region: &dyn Fn(DVec2) -> f64, settings: &Mosaic, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
    use crate::wfc::{learn, Grid, Sample};
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let tile = settings.tile.clamp(1, 64);
    let style = &STYLES[settings.style.min(STYLES.len() - 1)];
    let fit = fit_planar(&|x, y| region(DVec2::new(x, y)), min, max, opts);
    // the tiles that cover the box, set on multiples of a tile's width so
    // that moving the form does not slide the pattern under it
    let first = [min[0].div_euclid(tile), min[1].div_euclid(tile)];
    let across = ((max[0] + tile - 1).div_euclid(tile) - first[0]).max(1) as usize;
    let down = ((max[1] + tile - 1).div_euclid(tile) - first[1]).max(1) as usize;
    let tile_of = |x: i32, y: i32| (y.div_euclid(tile) - first[1]) as usize * across + (x.div_euclid(tile) - first[0]) as usize;
    let middle = |k: usize| DVec2::new((first[0] + (k % across) as i32) as f64 + 0.5, (first[1] + (k / across) as i32) as f64 + 0.5) * tile as f64;
    // how deep inside the region a tile's middle is, in tiles: under nothing, it is outside
    let inside: Vec<f64> = (0..across * down).map(|k| -region(middle(k)) / tile as f64).collect();
    let mut colours: Vec<u8> = Sample::read(style.sample)
        .and_then(|sample| learn(&sample, 3, style.turned))
        .and_then(|learnt| {
            let mut grid = Grid::new(&learnt.rules, [across, down]);
            for k in (0..across * down).filter(|k| inside[*k] < -0.75) {
                grid.leave_out(k % across, k / across);
            }
            let filled = grid.fill(settings.seed, TRIES)?;
            Ok(filled.iter().map(|tile| tile.map_or(0, |t| learnt.colour[t])).collect())
        })
        .unwrap_or_else(|_| vec![0; across * down]);
    // the border, and the tiles the edge's sloped bricks reach into
    let edge = settings.border.clamp(0, 8) as f64 - 0.5;
    for k in (0..across * down).filter(|k| inside[*k] < edge) {
        colours[k] = 0;
    }
    let cells = |piece: &crate::fit_planar::Piece| {
        let (x, y, w, h) = (piece.x, piece.y, piece.w, piece.h);
        (0..w * h).map(move |k| (x + k % w, y + k / w))
    };
    for piece in fit.pieces.iter().filter(|piece| piece.mask != 15) {
        for (x, y) in cells(piece) {
            colours[tile_of(x, y)] = 0;
        }
    }
    let mut push = |mask4: u8, x: i32, y: i32, w: i32, h: i32, shade: u8| {
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
    // the whole cells, each with its tile's colour
    let mut whole: Vec<(i32, i32, u8)> = Vec::new();
    for piece in &fit.pieces {
        if piece.mask == 15 {
            whole.extend(cells(piece).map(|(x, y)| (y, x, colours[tile_of(x, y)])));
        } else {
            push(piece.mask, piece.x, piece.y, piece.w, piece.h, 0);
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
        for (style, named) in STYLES.iter().enumerate() {
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            let settings = Mosaic { style, tile: 2, border: 1, seed: 7 };
            let fit = lay_mosaic(&disc, &settings, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
            let name = named.name;
            assert_eq!(bricks.len(), shades.len(), "{name}");
            // the whole disc is covered, closely, with no cell in two bricks
            let area = std::f64::consts::PI * 900.0;
            assert!((fit.solid - area).abs() < 0.03 * area && fit.error < 0.03 * area, "{name}: {} of {area}, {} wrong", fit.solid, fit.error);
            let mut seen = HashSet::new();
            let squares = bricks.iter().filter(|b| b.mask == 0xff);
            assert!(squares.clone().all(|b| (0..b.ext[0]).all(|x| (0..b.ext[1]).all(|y| seen.insert((b.min[0] + x, b.min[1] + y))))), "{name}: two bricks share a cell");
            // more than one colour, and far fewer bricks than tiles (706 tiles of 2 by 2 in the disc)
            let used: HashSet<u8> = shades.iter().copied().collect();
            assert!(used.len() >= 2, "{name} is all one colour");
            assert!(bricks.len() < 700, "{name}: {} bricks", bricks.len());
            // the sloped pieces of the edge are all the border's colour
            assert!(bricks.iter().zip(&shades).filter(|(b, _)| b.mask != 0xff).all(|(_, shade)| *shade == 0), "{name}: a sloped piece is not the border's colour");
            // one seed is one mosaic, another seed another
            let again = |seed: u64| {
                let (mut b, mut s) = (Vec::new(), Vec::new());
                lay_mosaic(&disc, &Mosaic { seed, ..settings }, 2, min, max, 0, 1, &Opts::default(), &mut b, &mut s);
                (b, s)
            };
            assert_eq!(again(7), (bricks.clone(), shades.clone()), "{name}");
            if name != "waves" {
                assert_ne!(again(8).0, bricks, "{name}: two seeds gave one mosaic");
            }
        }
    }

    #[test]
    fn a_path_is_laid_as_separate_stones_with_sloped_edges() {
        let mut bricks = Vec::new();
        let fit = lay_stones(&band, &usual(), 2, BOX.0, BOX.1, 0, 2, &Opts::default(), &mut bricks);
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
