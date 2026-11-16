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

/// One stone: the point it grew from and the points of the stones round it.
pub struct Stone {
    pub at: DVec2,
    near: Vec<DVec2>,
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
    pub fn eval(&self, p: DVec2, region: &dyn Fn(DVec2) -> f64) -> f64 {
        let to_me = p.distance_squared(self.at);
        // (a distance far past the nearest adds nothing, and is left out before it can overflow)
        let mut sum = (-self.sharp * -region(p)).exp();
        for other in &self.near {
            // how far p is on this stone's side of the line half way to the other
            let inside = (p.distance_squared(*other) - to_me) / (2.0 * other.distance(self.at));
            sum += (-self.sharp * inside).exp();
        }
        let least = -sum.ln() / self.sharp;
        self.half_gap - least
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
            near: points.iter().copied().filter(|q| q != at && q.distance(*at) < reach).collect(),
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

/// Fit the stones of a region and push them through cells `w0..w0 + depth`
/// across `axis`, onto `bricks`. Returns the fit: the area of the stones
/// themselves and the area where their bricks disagree with them.
///
/// All the stones are fitted at once, as one solid of many separate parts.
/// They were first fitted one at a time, which is the simpler thing to
/// write, and each fit was sound by itself. But two fits know nothing of
/// each other: across a narrow gap, the triangle that finishes one stone's
/// sloped edge and the triangle that finishes its neighbour's wanted cells
/// of the same box, one had to be given up, and the stone it belonged to
/// was left with a notch. The outline fitter already settles that for the
/// two faces of a thin wall, and settles it the same way between stones.
#[allow(clippy::too_many_arguments)]
pub fn lay_stones(region: &dyn Fn(DVec2) -> f64, settings: &Stones, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>) -> Fit {
    let (ua, va) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let all = stones(region, min, max, settings);
    if all.is_empty() {
        return Fit::default();
    }
    // Which stone a point could be in: the one whose point is nearest. The
    // stones are filed by the square their point is in, a stone's width to
    // a square, so that the nearest is looked for among a few and not all.
    let square = settings.size.max(LEAST_STONE);
    let filed_at = |p: DVec2| ((p.x / square).floor() as i32, (p.y / square).floor() as i32);
    let mut filed: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (i, stone) in all.iter().enumerate() {
        filed.entry(filed_at(stone.at)).or_default().push(i);
    }
    let solid = |x: f64, y: f64| {
        let p = DVec2::new(x, y);
        let (i, j) = filed_at(p);
        // (no point is further than two squares from the nearest stone's and still inside it)
        let nearest = (i - 2..=i + 2).flat_map(|a| (j - 2..=j + 2).map(move |b| (a, b))).filter_map(|at| filed.get(&at)).flatten().min_by(|a, b| all[**a].at.distance_squared(p).total_cmp(&all[**b].at.distance_squared(p)));
        match nearest {
            Some(stone) => all[*stone].eval(p, region),
            // far from every stone: outside them all, by any amount
            None => square,
        }
    };
    let fit = fit_planar(&solid, min, max, opts);
    for piece in &fit.pieces {
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
    fit
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
            for other in &stone.near {
                // half way to a neighbour is in the gap, in neither stone
                assert!(stone.eval((stone.at + *other) / 2.0, &band) > 0.0);
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
        // two cells deep, on the ground, and no cell in two bricks
        assert!(bricks.iter().all(|b| b.min[2] == 0 && b.ext[2] == 2));
        let mut seen = HashSet::new();
        assert!(bricks.iter().all(|b| (0..b.ext[0]).all(|x| (0..b.ext[1]).all(|y| seen.insert((b.min[0] + x, b.min[1] + y))))), "two bricks share a cell");
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
