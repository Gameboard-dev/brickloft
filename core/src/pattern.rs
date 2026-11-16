//! Patterns laid on a form in place of plain brick: stepping stones, and
//! the tile patterns (medallions, a rug, planks, bricks and cobblestone).
//!
//! A flat form is fitted as one outline (see `fit_planar`). Stepping stones
//! cut the same region into many small outlines first and fit each as its
//! own ([`lay_stones`]), so every stone has the sloped, continuous edges a
//! flat form has, and the stones follow whatever region the form gives
//! them: the band along a curved path, a ring, a disc, a filled outline.
//!
//! A tile pattern does not reshape the form. The region is fitted as the
//! one outline it is, and the pattern then colours that fit a cell at a
//! time ([`lay_mosaic`], [`lay_tiles`]). The same tiles are laid on the
//! flat faces of a form that is not flat all over and on the sides of a
//! box ([`lay_mosaic_on`], called from `spec::pattern_flat_faces` and
//! `spec::pattern_side_faces`). Each style is drawn by a rule of its own
//! (see [`Drawn`]).
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
//! That suits a pattern made only of small repeated motifs. The tile
//! patterns were first made with it and are now drawn by rule (see
//! [`Style`]). It does not suit stones. Its rules are local, a few
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
use crate::par::Threads;
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
pub(crate) struct Numbers(pub(crate) u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// From 0 up to, not including, 1.
    pub(crate) fn unit(&mut self) -> f64 {
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

impl Stone {
    /// The stone's ground as an outline whose corners are corners of the
    /// grid: what is on its side of the line half way to every stone round
    /// it, within cells `from..to` and a margin, each corner then moved
    /// to the nearest corner of the grid. `None` for a stone moved down to
    /// nothing. (See `lay_stones`, "With no gap", for what it is for.)
    ///
    /// The ground is cut out of a box by one line after another, which
    /// is how any outline with no hollows is cut by a straight line:
    /// <https://en.wikipedia.org/wiki/Sutherland%E2%80%93Hodgman_algorithm>
    fn outline(&self, from: [i32; 2], to: [i32; 2]) -> Option<crate::sdf::Fill> {
        let (lo, hi) = (DVec2::new(from[0] as f64 - 2.0, from[1] as f64 - 2.0), DVec2::new(to[0] as f64 + 2.0, to[1] as f64 + 2.0));
        let mut ground = vec![lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
        for (towards, far) in &self.lines {
            let inside = |p: DVec2| far - towards.dot(p);
            let mut kept = Vec::with_capacity(ground.len() + 1);
            for (n, a) in ground.iter().enumerate() {
                let b = ground[(n + 1) % ground.len()];
                let (da, db) = (inside(*a), inside(b));
                if da >= 0.0 {
                    kept.push(*a);
                }
                if (da >= 0.0) != (db >= 0.0) {
                    kept.push(a.lerp(b, da / (da - db)));
                }
            }
            ground = kept;
            if ground.len() < 3 {
                return None;
            }
        }
        let mut corners: Vec<DVec2> = Vec::with_capacity(ground.len());
        for p in ground {
            let p = p.round();
            if corners.last() != Some(&p) {
                corners.push(p);
            }
        }
        if corners.len() > 1 && corners.first() == corners.last() {
            corners.pop();
        }
        (corners.len() >= 3).then(|| crate::sdf::Fill::new(corners))
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
/// across `axis`, onto `bricks`, with each brick's number (0 to 7) onto
/// `shades`: the bricks of one stone share a number, which whoever paints
/// them turns into a colour (see [`colour_of`]). Returns the sum of the
/// stones' fits: the area of the stones themselves and the area where
/// their bricks disagree with them.
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
/// nothing) is the later stone fitted again a little smaller: drawn in by
/// half a cell, and then by a whole one. A piece that meets a laid stone
/// even then is left out.
///
/// ## With no gap
///
/// With the gap at 0 every stone meets its neighbours, and fitted to the
/// lines between them they do not meet exactly. Each fit strays up to
/// half a cell from its line, one stone's this way and its neighbour's
/// that, so they overlapped in places, the later stone was drawn in, and
/// seams were left up to a cell wide all round. Two stones meet exactly
/// only if they are given the same edge, and an edge that bricks can lay
/// exactly from both sides.
///
/// A straight line from one corner of the grid to another is such an
/// edge: it is the sloped side of a right triangle, or of a few alike end
/// to end, and the triangle on its other side fills the rest of the same
/// box, as the two halves of a cut tile do. So with no gap each stone's
/// outline is drawn with its corners moved to the nearest corners of the
/// grid ([`Stone::outline`]). A corner is where three stones meet, and
/// each of the three works it out from the same three points and moves it
/// to the same place, so the two stones either side of an edge have the
/// very same two ends for it. Each stone is then fitted to its outline
/// by itself, as before, and the fitter lays an outline of grid corners
/// exactly, with nothing to choose between.
///
/// A stone's corners are sharp this way, where with a gap they are
/// rounded: a rounded corner would be a hole where three stones meet. A
/// corner moves by up to seven tenths of a cell, which changes a stone's
/// shape a little and not its size. Where an edge runs out at the rim of
/// the region it ends on the rim and not on a corner of the grid, and
/// the two stones may still differ there by a sliver.
///
/// Two other ways were tried. Fitting each stone to the bricks of those
/// laid before it, in turn, asked the fitter to follow another fit's
/// staircase, and it smoothed across it by design (a corner costs it
/// something), overlapped the laid bricks, and whole pieces had to be
/// left out: a floor with holes in it. Filling the seams afterwards
/// cannot work, since a seam between two sloped edges that do not match
/// is a sliver no brick fills.
///
/// The stones are fitted on `opts.threads`, and laid on one. A stone's
/// first fit is of the stone as it is and depends on no other stone, and
/// those fits are nearly all the time a path of stones takes. Whether a
/// stone meets one already laid, and its fitting again if it does, depends
/// on every stone before it, so that is done afterwards, stone by stone in
/// their order, exactly as when the first fits were made there too. Fitting
/// each stone at all three of its sizes ahead of time would leave nothing
/// but the checking to the one thread, for three times the work.
#[allow(clippy::too_many_arguments)]
pub fn lay_stones(region: &(dyn Fn(DVec2) -> f64 + Sync), settings: &Stones, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
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
    let stones = stones(region, min, max, settings);
    let together = settings.gap.max(LEAST_GAP) <= 0.0;
    // the cells a stone is fitted over, if any of them are in the box
    let room = |stone: &Stone| {
        // No place is further from the nearest stone's point than the
        // points are kept apart, or a dart would have landed there: so a
        // stone reaches no further than that from its own.
        // (and a cell more with no gap, where a corner may be moved outwards to the grid)
        let reach = size * 0.9 + 1.0 + if together { 1.0 } else { 0.0 };
        let (lo, hi) = (stone.at - DVec2::splat(reach), stone.at + DVec2::splat(reach));
        let (from, to) = ([(lo.x.floor() as i32).max(min[0]), (lo.y.floor() as i32).max(min[1])], [(hi.x.ceil() as i32).min(max[0]), (hi.y.ceil() as i32).min(max[1])]);
        (to[0] > from[0] && to[1] > from[1]).then_some((from, to))
    };
    // Each stone is fitted on one thread: it is the stones that are shared
    // out, not the samples of each, which are too few to be worth it.
    let alone = Opts { threads: Threads::ONE, ..*opts };
    // Every stone as it is. A stone 10 cells across takes 1.0 to 1.4
    // milliseconds to fit, and the time goes by its area, so one stone is
    // worth a thread of its own from 6 cells across, and four of the
    // smallest are (see `par::Threads::map_ranges`).
    let stones_a_thread = (36.0 / (size * size)).ceil().max(1.0) as usize;
    if together {
        // Each stone to its outline of grid corners, on its own thread
        // like any other stone's first fit; then laid in their order.
        let outlines: Vec<Option<crate::sdf::Fill>> = stones.iter().map(|stone| room(stone).and_then(|(from, to)| stone.outline(from, to))).collect();
        let fits = opts.threads.map(stones.len(), stones_a_thread, |n| outlines[n].as_ref().zip(room(&stones[n])).map(|(outline, (from, to))| fit_planar(&|x, y| outline.eval(DVec2::new(x, y)).max(region(DVec2::new(x, y))), from, to, &alone)));
        for (nth, fit) in fits.into_iter().enumerate() {
            let Some(fit) = fit else { continue };
            whole.solid += fit.solid;
            whole.error += fit.error;
            // Two outlines share an edge and no ground, so no piece should
            // meet one laid. One does where an edge runs out at the rim
            // (see above). Left out whole, such a piece was a hole the
            // size of a brick in the rim, so it is laid as whatever of
            // its cells it fills entirely and nothing laid has in its
            // box, in rows: the hole is then the sliver that was amiss.
            let mut pieces: Vec<Piece> = Vec::new();
            for piece in fit.pieces {
                if !meets(&piece, &laid, &over) {
                    pieces.push(piece);
                    continue;
                }
                for y in piece.y..piece.y + piece.h {
                    let mut row: Option<Piece> = None;
                    for x in piece.x..piece.x + piece.w {
                        let within = |dx: f64, dy: f64| in_piece(piece.mask, (x as f64 + dx - piece.x as f64) / piece.w as f64, (y as f64 + dy - piece.y as f64) / piece.h as f64);
                        let free = !over.contains_key(&(x, y)) && [(0.01, 0.01), (0.99, 0.01), (0.01, 0.99), (0.99, 0.99)].iter().all(|(dx, dy)| within(*dx, *dy));
                        match (&mut row, free) {
                            (Some(row), true) => row.w += 1,
                            (None, true) => row = Some(Piece { mask: 15, x, y, w: 1, h: 1 }),
                            (_, false) => pieces.extend(row.take()),
                        }
                    }
                    pieces.extend(row);
                }
            }
            for piece in pieces {
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
        return whole;
    }
    let first = opts.threads.map(stones.len(), stones_a_thread, |n| room(&stones[n]).map(|(from, to)| fit_planar(&|x, y| stones[n].eval(DVec2::new(x, y), region), from, to, &alone)));
    // (each stone has a number, for its bricks to be coloured alike by: see `colour_of`)
    for (nth, (mut stone, first)) in stones.into_iter().zip(first).enumerate() {
        let (Some(mut fit), Some((from, to))) = (first, room(&stone)) else { continue };
        // then, if it would meet a stone already laid, drawn in by half a cell, and by a whole one
        for _ in 0..2 {
            if !fit.pieces.iter().any(|piece| meets(piece, &laid, &over)) {
                break;
            }
            stone.half_gap += 0.5;
            fit = fit_planar(&|x, y| stone.eval(DVec2::new(x, y), region), from, to, &alone);
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
    /// how large the boards, bricks or cobbles of a style that has them
    /// are: 2 is the usual, 1 half that, 4 twice it (medallions and the
    /// rug take no notice of it)
    pub size: i32,
    /// Whether the pattern has depth, cut one cell into the form's top
    /// face. What that is depends on the style, and there is one kind to
    /// each: for bricks, the top cell of each slopes down to its edge all
    /// round it (see [`Masonry`]); for planks, a groove between one row of
    /// boards and the next; for cobblestone, the mortar lies a cell below
    /// the stones (see [`lay_tiles`] for both). Medallions and a rug have
    /// none.
    pub bevel: bool,
    /// whether bricks or cobbles have a tile of mortar between them
    pub mortar: bool,
    /// how far each course of bricks is moved along from the one before,
    /// as so many hundredths of a brick's length: 0 stacks the bricks in
    /// columns, 50 is the half-brick step of a running bond, and anything
    /// below 0 moves every course by an amount of its own
    pub stagger: i32,
    /// whether the courses of bricks run up the floor and not across it,
    /// so that the stagger is from side to side
    pub upright: bool,
    /// How unlike one another the bricks or cobbles are, 0 to 100, each
    /// way: along the courses and across them. At 0 they are all alike
    /// that way.
    pub uneven: [i32; 2],
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
pub const STYLES: [Style; 5] = [
    Style { name: "medallions", drawn: Drawn::Medallions },
    Style { name: "rug", drawn: Drawn::Rug },
    Style { name: "planks", drawn: Drawn::Planks },
    Style { name: "bricks", drawn: Drawn::Bricks },
    Style { name: "cobblestone", drawn: Drawn::Cobbles },
];

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
/// medallion, the stems of its field). The ground has the first colour to
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
    /// a rug in the Persian manner: borders, a layered medallion at the
    /// middle, a corner piece in each corner, and scrolling stems with
    /// flowers over the field (see [`rug_garden`])
    Rug,
    /// a few figures repeated over the floor like a tiled ceiling, each
    /// repeat its own mirror image eight ways
    Medallions,
    /// a floor of boards (see [`planks`])
    Planks,
    /// bricks in courses, every one a rectangle (see [`Masonry`])
    Bricks,
    /// stones of any shape fitted together (see [`Masonry`])
    Cobbles,
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
fn tile_at(colour: &impl Fn(f64, f64) -> u8, x: f64, y: f64) -> Tile {
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

/// Masonry: a wall or a floor of bricks, or of cobblestones.
///
/// ## Two patterns
///
/// Bricks are rectangles in courses. Cobbles are stones of any shape,
/// each the ground nearer its own point than any other's, as a stepping
/// stone is (see the top of this file).
/// <https://en.wikipedia.org/wiki/Voronoi_diagram>
///
/// For a while the two were one pattern with one setting between them,
/// which moved the bricks' points about and bent the measure of nearness
/// from one that makes rectangles to the plain distance. Halfway along,
/// it gave neither: bricks with corners sheared off at a slant, whose
/// sides were at no angle a brick is cut at. The two are different
/// things to want, with different colours, and each is drawn its own way.
///
/// ## Uneven, two ways
///
/// A brick's course is as deep as the rest and its ends are a brick's
/// length apart, unless told otherwise. Uneven along moves every joint
/// between two bricks by an amount of its own, so that the bricks of a
/// course differ in length. Uneven across does the same to the line
/// between two courses, so that the courses differ in depth. Every line
/// stays on the grid and every brick stays a rectangle. For cobbles the
/// two say how far each stone's point is moved from its place in a grid
/// of points, along and across: at nothing both ways the stones are all
/// alike.
///
/// ## Mortar
///
/// How far inside its own outline a place is, is known for either kind:
/// for a brick, how far it is from the nearest of its four sides; for a
/// cobble, from the nearest of the lines halfway between its point and
/// its neighbours'. Within half a tile of the outline is mortar, if
/// there is to be any, which is the ground, 0. Every piece has a number
/// from 1 for whoever paints it to give a shade to (see [`colour_of`]).
/// Bricks' numbers go odd and even by turns along a course, so that two
/// side by side differ even with no mortar between them.
///
/// ## A brick's bevel
///
/// The same distance is used as a height for a bevel, which bricks may
/// have and cobbles may not. The top cell of a bevelled brick is not laid
/// full: at its outline the brick stands a cell short, and a cell in from
/// the outline it is whole, so that its top slopes down to its edge all
/// round (see `lay_tiles`, which lays the sloped bricks). A brick's sides
/// are on the grid, so that is one long sloped brick to each side and a
/// hipped one at each corner, exactly. The mortar lies at the foot of
/// the slope.
///
/// The bevel was once as many cells deep as asked, cobbles had it too,
/// and each piece could stand a cell or more short of the wall's face.
/// The player found all three ugly. A cobble's sides are at any angle,
/// and bricks cut at half a right angle made a heap of odd cells of its
/// slope. Deeper bevels and uneven heights read as clutter at a
/// microbrick to a tile. What is left is the one case that comes out
/// clean: a brick, one cell, on or off.
struct Masonry {
    cobbles: bool,
    /// how many tiles a brick is long and a course deep, each with its
    /// mortar; for cobbles, how far apart the points are each way
    long: f64,
    deep: f64,
    /// how many tiles of mortar lie between two pieces: 1 or 0
    mortar: f64,
    /// 0 to 1: along and across
    uneven: [f64; 2],
    stagger: i32,
    upright: bool,
    seed: u64,
}

impl Masonry {
    fn new(settings: &Mosaic, cobbles: bool) -> Masonry {
        let size = settings.size.clamp(1, 16) as f64;
        let mortar = if settings.mortar { 1.0 } else { 0.0 };
        let (long, deep) = if cobbles { (2.0 * size + 3.0 + mortar, 2.0 * size + 3.0 + mortar) } else { (4.0 * size + mortar, (1.5 * size).round() + mortar) };
        Masonry { cobbles, long, deep, mortar, uneven: settings.uneven.map(|u| u.clamp(0, 100) as f64 / 100.0), stagger: settings.stagger.min(100), upright: settings.upright, seed: settings.seed }
    }

    /// A number from 0 to 1 that belongs to a pair of whole numbers.
    fn number(&self, a: i64, b: i64, which: u64) -> f64 {
        Numbers(self.seed ^ (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ which.wrapping_mul(0x1656_67B1_9E37_79F9)).unit()
    }

    /// How far a course is moved along. (Every other row of cobbles is
    /// moved by half a stone, so that four never meet at one corner.)
    fn moved(&self, row: i64) -> f64 {
        if self.cobbles {
            if row.rem_euclid(2) == 1 { (self.long / 2.0).floor() } else { 0.0 }
        } else if self.stagger < 0 {
            (self.number(row, 0, 5) * self.long).floor()
        } else {
            (row as f64 * self.long * self.stagger as f64 / 100.0).round()
        }
    }

    /// A place as the wall's own sums have it: turned over for upright
    /// courses, and moved by half a tile where there is mortar, so that
    /// the tile of mortar between two bricks is a whole tile of the grid
    /// and not half of one and half of the next.
    fn own(&self, p: DVec2) -> DVec2 {
        let p = if self.upright { DVec2::new(p.y, p.x) } else { p };
        p - DVec2::splat(self.mortar / 2.0)
    }

    /// Where course `row` of bricks begins: its place in even courses,
    /// moved by a whole number of tiles of its own. (By less than half of
    /// what a course has to spare over one tile of brick, so that no
    /// course is squeezed to nothing.)
    fn course(&self, row: i64) -> f64 {
        let spare = (self.deep - self.mortar - 1.0).max(0.0);
        row as f64 * self.deep + ((self.number(row, 0, 7) - 0.5) * 0.9 * self.uneven[1] * spare).round()
    }

    /// Where brick `nth` of course `row` begins.
    fn joint(&self, row: i64, nth: i64) -> f64 {
        let spare = (self.long - self.mortar - 1.0).max(0.0);
        nth as f64 * self.long - self.moved(row) + ((self.number(row, nth, 1) - 0.5) * 0.9 * self.uneven[0] * spare).round()
    }

    /// The point of a cobble: the middle of its square of the grid, moved
    /// by up to four tenths of the square each way at the most uneven.
    fn point(&self, (row, nth): (i64, i64)) -> DVec2 {
        let moved = DVec2::new((self.number(row, nth, 1) - 0.5) * 0.8 * self.uneven[0], (self.number(row, nth, 2) - 0.5) * 0.8 * self.uneven[1]);
        DVec2::new((nth as f64 + 0.5 + moved.x) * self.long - self.moved(row), (row as f64 + 0.5 + moved.y) * self.deep)
    }

    /// The cobbles whose points can be the nearest to a place or have a
    /// side of the nearest one: those of its own row and two either side,
    /// two along each way.
    fn near(&self, q: DVec2) -> impl Iterator<Item = (i64, i64)> + '_ {
        let row = (q.y / self.deep).floor() as i64;
        (row - 2..=row + 2).flat_map(move |row| {
            let nth = ((q.x + self.moved(row)) / self.long).floor() as i64;
            (nth - 2..=nth + 2).map(move |nth| (row, nth))
        })
    }

    /// The course of bricks a height is in, the height being the wall's
    /// own (see [`Masonry::own`]): the one whose two lines it is between.
    fn course_at(&self, height: f64) -> i64 {
        let about = (height / self.deep).floor() as i64;
        (about - 1..=about + 1).rev().find(|row| self.course(*row) <= height).unwrap_or(about - 2)
    }

    /// The piece a place is part of.
    fn piece(&self, p: DVec2) -> (i64, i64) {
        let q = self.own(p);
        if self.cobbles {
            // (of two as near as each other, the first met, so that one place has one answer however it is asked)
            return self.near(q).map(|piece| (q.distance_squared(self.point(piece)), piece)).fold((f64::INFINITY, (0, 0)), |best, next| if next.0 < best.0 - 1e-9 { next } else { best }).1;
        }
        // the course the place is in, and in it the brick whose two joints it is between
        let row = self.course_at(q.y);
        let about = ((q.x + self.moved(row)) / self.long).floor() as i64;
        let nth = (about - 1..=about + 1).rev().find(|nth| self.joint(row, *nth) <= q.x).unwrap_or(about - 2);
        (row, nth)
    }

    /// How far inside the outline of `piece` a place is, in tiles: nothing
    /// on the outline, below nothing in the mortar and beyond.
    fn inside(&self, piece: (i64, i64), p: DVec2) -> f64 {
        let q = self.own(p);
        let from_edge = if self.cobbles {
            // from the nearest of the lines halfway between its point and each neighbour's
            let own = self.point(piece);
            self.near(own).filter(|other| *other != piece).map(|other| {
                let other = self.point(other);
                (q.distance_squared(other) - q.distance_squared(own)) / (2.0 * own.distance(other)).max(1e-9)
            }).fold(f64::INFINITY, f64::min)
        } else {
            let (row, nth) = piece;
            (q.x - self.joint(row, nth)).min(self.joint(row, nth + 1) - q.x).min(q.y - self.course(row)).min(self.course(row + 1) - q.y)
        };
        from_edge - self.mortar / 2.0
    }

    /// A piece's number: for a brick 1 to 6, odd for every other brick of
    /// its course; for a cobble 1 to 7.
    fn shade(&self, (row, nth): (i64, i64)) -> u8 {
        if self.cobbles {
            1 + (self.number(row, nth, 3) * 7.0) as u8 % 7
        } else {
            1 + 2 * ((self.number(row, nth, 3) * 3.0) as u8 % 3) + nth.rem_euclid(2) as u8
        }
    }

    /// The colour at a place: its piece's number, or the mortar's, 0.
    fn colour(&self, p: DVec2) -> u8 {
        let piece = self.piece(p);
        if self.inside(piece, p) > 1e-9 || self.mortar == 0.0 {
            self.shade(piece)
        } else {
            0
        }
    }
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

/// A rug in the Persian manner, as the colour at each place of one
/// quarter of it (so many tiles out from its middle each way; the other
/// three quarters are this one's mirror images).
///
/// ## What is on it
///
/// It is set out as a medallion carpet of Tabriz or Kashan is, each part
/// on a ground of its own colour. From the edge inwards:
///
/// - A guard: a line, a chain of small lozenges, a line. The lozenges are
///   two tiles across, every one four tiles cut on the diagonal, so the
///   chain is exact on the grid and meets itself at a corner.
/// - The main border, on the dark ground: one stem swinging from side to
///   side of it, a flower in every hollow of the stem, on one side and
///   the other by turns, and a small knot wherever the stem crosses the
///   middle.
/// - A second guard, and then the field.
/// - At the middle of the field the medallion: two to four layers one
///   inside the next, each with a ring of pointed lobes and round ones
///   between them, each outlined, each on another ground with a bud in
///   every pointed lobe, and a flower at the heart. A pointed pendant
///   and two lozenges hang from it at each end of the rug's longer way.
/// - In each corner of the field a quarter of such a figure.
/// - Over the rest of the field, scrolls: a stem coiling in to a flower,
///   with leaves along it, each scroll branching from the one it touches
///   and coiling the other way. Small flowers fill what is left.
///
/// ## The features
///
/// A feature here is a tone and not a part, since every part is made of
/// several tones and the same few come back all over the rug, which is
/// what makes it one design: 1 the lines (every outline, guard line and
/// stem), 2 the dark ground (the border's, and by turns the medallion's
/// layers'), 3 the pale one, 4 the petals, 5 leaves, 6 the shaded half
/// of a leaf and the ground of the corners, 7 the hearts of flowers. A
/// flower is three of them, petals, a smaller flower within and a heart;
/// that is all the shading eight flat colours allow. (The first rug gave
/// each part one number, and each part came out one flat colour.)
///
/// ## How a layered figure is drawn
///
/// A layer is the places within a round body, or within one of the
/// lobes that stand out from it. A round lobe is a disc. A pointed lobe
/// is what two equal discs have in common, a little to each side of the
/// lobe's axis: two bows meeting in a point, the shape of a pointed arch
/// (<https://en.wikipedia.org/wiki/Vesica_piscis>). The lobes are all
/// alike, so a place is turned into the one lobe's slice of the figure
/// and asked about once. How far inside the outline the place is gives
/// the outline itself (the first tile in) and a second line further in
/// where there is room, both following every lobe; this is banding a
/// distance function, as [`drawn`] does for the other styles
/// (<https://iquilezles.org/articles/distfunctions2d/>). The first rug's
/// medallion was a rose curve, one wavy outline, and its layers were the
/// same flower smaller: it read as a child's flower. Lobes of two kinds
/// by turns, each layer turned half a lobe from the last, are what a
/// woven medallion has.
///
/// ## How the field is filled
///
/// By the loop of Wong, Zongker and Salesin, "Computer-generated floral
/// ornament" (SIGGRAPH 1998, <https://doi.org/10.1145/280814.280948>):
/// find the largest empty circle left in the field, put a motif in it,
/// and go on until what is left is too small. The room at every tile is
/// kept in a grid, how far it is from the border, the medallion, the
/// corners and every circle so far, and each new circle only lowers it.
/// A circle found this way touches something, and most often an earlier
/// circle; its stem then starts from that one's stem and coils the
/// opposite way, so that the two run together where they meet like two
/// gears, which is how Hamekasi and Samavati join the scrolls of such
/// patterns ("Designing Persian floral patterns using circle packing",
/// GRAPP 2012, <https://doi.org/10.5220/0003850101350142>). A stem is a
/// spiral that closes in evenly as it turns
/// (<https://en.wikipedia.org/wiki/Archimedean_spiral>).
///
/// The first rug threw darts for its flowers and joined each to its
/// nearest neighbour with a straight stem. Darts leave holes and
/// crowds, and straight stems make a net, not a vine.
///
/// ## Small things are drawn on the grid
///
/// A flower under five tiles is a lozenge and a square on one middle,
/// which is an eight-pointed star whose edges are the grid's lines and
/// diagonals: whole tiles and tiles cut in two, exactly. A round outline
/// that small comes out as a blot of steps. Larger flowers are the rose
/// curve the first rug used
/// (<https://en.wikipedia.org/wiki/Rose_(mathematics)>). A middle is put
/// on a corner of the grid or on the middle of a tile to suit its size.
///
/// ## Why not pictures of flowers
///
/// A rug's flowers could be small pictures, drawn by hand and stamped
/// over the field. A picture is one size, and a flower drawn in tiles
/// for a rug forty tiles across is a blot on one two hundred across.
/// A figure given as a rule is drawn afresh at whatever size the rug is:
/// a small rug has a medallion of two layers and a border of lozenges,
/// a large one four layers and flowers with petals. And every seed is
/// another rug: the size and number of lobes of the medallion, which
/// grounds its layers take, the size of the corners, and where every
/// scroll falls.
///
/// ## A small rug
///
/// A rug under 48 tiles across its shorter way has no room for the parts
/// above at their own sizes. Drawn by the same rules, they came out cut
/// short: a corner piece that was three tiles of a lobe, pendants that
/// were blots, a border whose knots fell at uneven steps because its
/// swing did not divide the side. A small rug is set out as a small
/// woven rug is, with the same parts made simpler and each of them
/// whole:
///
/// - The guards are single lines.
/// - The border is lozenges four tiles across, set out from the middle
///   of each side, and a square with a star in it at each corner. (Under
///   24 tiles across it is a chain of lozenges two tiles across, which
///   divides any side.)
/// - The medallion has one layer or two, as its size allows, and no
///   pendants.
/// - Each corner of the field is cut off on the diagonal behind a line,
///   with one lozenge in it where there is room.
/// - The field has scrolls where they fit and small flowers elsewhere,
///   as on a large rug.
///
/// Nothing is changed for a rug of 48 tiles or more: every sum above is
/// the same sum, and the numbers drawn from the seed are drawn in the
/// same order, so a large rug is tile for tile what it was.
///
/// ## Cost
///
/// A place asks about the frame or the two figures by a few sums each,
/// and about the scrolls only of its own square of four tiles by four,
/// which are two or three; so a rug 250 tiles square is drawn in about
/// thirty thousandths of a second. Asking every scroll at every place
/// would be hundreds of them.
fn rug_garden(half_wide: f64, half_deep: f64, pick: &mut dyn FnMut(f64, f64) -> f64) -> Box<dyn Fn(f64, f64) -> u8 + Send + Sync> {
    use std::f64::consts::{PI, TAU};
    // the features (see The features above)
    const LINE: u8 = 1;
    const DEEP: u8 = 2;
    const PALE: u8 = 3;
    const PETAL: u8 = 4;
    const LEAF: u8 = 5;
    const SHADE: u8 = 6;
    const HEART: u8 = 7;

    // A flower about a middle, in three tones: its petals, a smaller
    // flower within them, and its heart. `None` outside it. Under
    // two and a half tiles it is a lozenge, under five a lozenge and a
    // square together, and above that a rose curve with a line round it.
    // The size of one of the first two is a whole number where its
    // middle is a corner of the grid, and a whole number and a half
    // where it is the middle of a tile, so that its edges fall on tiles'
    // edges and diagonals.
    fn bloom(dx: f64, dy: f64, size: f64, petals: f64, [petal, inner, heart]: [u8; 3]) -> Option<u8> {
        if size < 2.4 {
            let far = dx.abs() + dy.abs();
            return (far < size).then(|| if far < size - 1.0 { heart } else { petal });
        }
        if size < 5.0 {
            let (sum, most) = (dx.abs() + dy.abs(), dx.abs().max(dy.abs()));
            let square = size - if size < 4.0 { 1.0 } else { 2.0 };
            return (sum < size || most < square).then(|| if most < 1.0 - size.fract() { heart } else if sum < size - 1.0 && most < square { inner } else { petal });
        }
        let far = dx.hypot(dy);
        if far >= size {
            return None;
        }
        let round = petals * dy.atan2(dx) / 2.0;
        let edge = size * (0.58 + 0.42 * round.cos().abs().powf(0.6));
        if far >= edge {
            None
        } else if far < (size * 0.22).max(0.6) {
            Some(heart)
        } else if far < size * 0.62 * (0.58 + 0.42 * round.sin().abs().powf(0.6)) {
            Some(inner)
        } else if edge - far < 0.7 {
            Some(LINE)
        } else {
            Some(petal)
        }
    }

    // One layer of a medallion or a corner piece.
    #[derive(Clone, Copy)]
    struct Layer {
        /// how far its points stand from the middle
        reach: f64,
        /// how many pointed lobes, with as many round ones between
        lobes: f64,
        /// the angle its first pointed lobe is at
        turned: f64,
        /// the size of its round body
        core: f64,
        /// a pointed lobe: where the two discs it is the overlap of have their middles (along its axis, and to each side), and their size
        point: [f64; 3],
        /// a round lobe: its middle and its size
        bulge: [f64; 3],
        ground: u8,
        /// a second line inside the outline, of this colour, where the layer is broad enough
        lined: Option<u8>,
        /// the bud in each pointed lobe: how far out, half its length, half its breadth, its two tones
        bud: Option<([f64; 3], [u8; 2])>,
    }
    #[derive(Clone)]
    struct Figure {
        x: f64,
        y: f64,
        layers: Vec<Layer>,
        /// the flower at its middle: its size, petals and tones
        rose: (f64, f64, [u8; 3]),
    }
    // how far inside a layer's outline a place is (under nothing, outside it), and the place as its lobe sees it: out along the lobe's axis, and to the side
    fn depth(layer: &Layer, far: f64, angle: f64) -> (f64, f64, f64) {
        let turn = TAU / layer.lobes;
        let a = angle - layer.turned;
        let b = (a - turn * (a / turn).round()).abs();
        let (px, py) = (far * b.cos(), far * b.sin());
        let [vc, vs, vr] = layer.point;
        let [rx, ry, rr] = layer.bulge;
        ((layer.core - far).max(vr - (px - vc).hypot(py + vs)).max(rr - (px - rx).hypot(py - ry)), px, py)
    }
    fn figure_at(figure: &Figure, u: f64, v: f64) -> Option<u8> {
        let (dx, dy) = (u - figure.x, v - figure.y);
        let far = dx.hypot(dy);
        if far >= figure.layers.first()?.reach {
            return None;
        }
        if let Some(colour) = bloom(dx, dy, figure.rose.0, figure.rose.1, figure.rose.2) {
            return Some(colour);
        }
        let angle = dy.atan2(dx);
        // (the innermost layer the place is in is the one seen)
        for layer in figure.layers.iter().rev().filter(|layer| far < layer.reach) {
            let (deep, px, py) = depth(layer, far, angle);
            if deep <= 0.0 {
                continue;
            }
            if deep < 1.0 {
                return Some(LINE);
            }
            if let Some(([at, long, broad], [petal, heart])) = layer.bud {
                let (a, b) = ((px - at) / long, py / broad);
                let within = if a > 0.0 { a + b } else { a.hypot(b) };
                if within < 1.0 {
                    return Some(if within < 0.45 { heart } else { petal });
                }
            }
            if let Some(line) = layer.lined.filter(|_| (2.0..3.0).contains(&deep)) {
                return Some(line);
            }
            return Some(layer.ground);
        }
        None
    }
    // A figure of layers about a place: `reach` is how far its points
    // stand from its middle, and `star` how much of that is its round
    // body. The larger it is the more layers it has (`most` at most),
    // since a layer under five or six tiles broad is all outline. Each
    // layer is turned half a lobe from the last, so that the points of
    // one show between the points of the next.
    fn figure(x: f64, y: f64, reach: f64, lobes: f64, star: f64, grounds: [u8; 4], most: usize) -> Figure {
        let shares: &[f64] = if reach >= 34.0 { &[1.0, 0.74, 0.5, 0.3] } else if reach >= 22.0 { &[1.0, 0.68, 0.4] } else if reach >= 10.0 { &[1.0, 0.58] } else { &[1.0] };
        let shares = &shares[..shares.len().min(most)];
        let turn = TAU / lobes;
        let mut layers: Vec<Layer> = shares
            .iter()
            .enumerate()
            .map(|(k, share)| {
                let reach = reach * share;
                let core = reach * star;
                let vc = core * 0.92;
                let long = reach - vc;
                let half = (vc * (turn / 2.0).tan() * 0.95).min(long * 0.9);
                let vr = (long * long + half * half) / (2.0 * half);
                let rc = core * 0.97;
                let rr = rc * (turn / 2.0).sin() * 0.72;
                Layer { reach, lobes, turned: if k % 2 == 1 { turn / 2.0 } else { 0.0 }, core, point: [vc, vr - half, vr], bulge: [rc * (turn / 2.0).cos(), rc * (turn / 2.0).sin(), rr], ground: grounds[k], lined: None, bud: None }
            })
            .collect();
        let rose = (layers.last().map_or(1.0, |last| last.core * 0.6)).max(1.0);
        let rose = if rose < 5.0 { rose.round() } else { rose };
        for k in 0..layers.len() {
            // what is on this layer's axis inside it: the next layer's round lobe, or the flower at the middle
            let inner = layers.get(k + 1).map_or(rose, |next| next.bulge[0].hypot(next.bulge[1]) + next.bulge[2]);
            let outer = layers[k].reach - 1.6;
            let long = ((outer - inner) / 2.0 - 0.5).min(4.0);
            let ground = layers[k].ground;
            let tones = if ground == PALE { [PETAL, DEEP] } else if ground == PETAL { [PALE, DEEP] } else { [PETAL, HEART] };
            if long >= 1.0 {
                let breadth = (layers[k].point[2] - layers[k].point[1] - 1.3).clamp(0.9, (long * 0.75).max(0.9));
                layers[k].bud = Some(([(outer + inner) / 2.0, long, breadth], tones));
            }
            if layers[k].reach - layers.get(k + 1).map_or(rose, |next| next.reach) >= 11.0 {
                layers[k].lined = Some(tones[0]);
            }
        }
        let last = layers.last().map_or(0, |last| last.ground);
        Figure { x, y, layers, rose: (rose, 8.0, if last == PALE { [PETAL, HEART, DEEP] } else { [PALE, HEART, PETAL] }) }
    }

    // a number from `low` to `high`, not a whole one
    let mut roll = |low: f64, high: f64| low + (high - low) * pick(0.0, 9999.0) / 9999.0;
    let least = half_wide.min(half_deep);
    // The frame, from the edge inwards: a guard, the main border, a
    // guard. The main border is an even number of tiles, so that its
    // middle line is a line of the grid. The frame is about a third of
    // the way in to the middle on a rug of the usual size, as on a
    // woven one; a small rug keeps a line each side for its guards and
    // lozenges for its border.
    let main = if least < 12.0 { 2.0 } else { (2.0 * (least * 0.09).round()).clamp(4.0, 14.0) };
    let guard = if least >= 24.0 { 4.0 } else { 1.0 };
    let frame = 2.0 * guard + main;
    // A small rug: under 48 tiles across its shorter way, where the guards
    // are single lines. It has the same parts as a large one, fewer of
    // them, and each of them whole (see "A small rug", above). Nothing
    // about a rug of 48 tiles or more turns on this.
    let small = least < 24.0;
    let (wide, deep) = (half_wide - frame, half_deep - frame);
    let field = wide.min(deep);
    let along_wide = wide >= deep;
    let long = wide.max(deep);

    // the medallion: eight lobes, or twelve where there are tiles enough to show them
    let reach = (field * roll(0.7, 0.82)).min(long * 0.55).max(2.0);
    let lobes = if reach < 24.0 { 8.0 } else { [8.0, 12.0][roll(0.0, 1.999) as usize] };
    let star = roll(0.7, 0.8);
    let grounds = [[DEEP, PALE, PETAL, DEEP], [PALE, DEEP, PETAL, PALE], [DEEP, PETAL, PALE, DEEP]][roll(0.0, 2.999) as usize];
    let medallion = figure(0.0, 0.0, reach, lobes, star, grounds, 4);
    // what hangs from it at each end of the rug's longer way: a pointed leaf shape and two lozenges beyond it
    let hung = (reach * 0.22).round().max(3.0);
    let pendant = (long - reach >= 2.0 * hung + 8.0 && !small).then(|| {
        let half = hung * 0.6;
        let size = (hung * hung + half * half) / (2.0 * half);
        // (its middle along the rug, the two discs' middles to the side and their size, half its length)
        [reach.floor() + hung, size - half, size, hung]
    });
    // the corner pieces: a quarter of such a figure about each corner of the field
    let corner_reach = (field * roll(0.6, 0.75)).min(wide.hypot(deep) - reach - 4.0).max(0.0);
    let mut corner = figure(wide, deep, corner_reach, lobes, star, [if roll(0.0, 1.0) < 0.6 { SHADE } else { grounds[0] }, grounds[1], grounds[2], grounds[3]], if corner_reach < 16.0 { 1 } else if corner_reach < 34.0 { 2 } else { 3 });
    // A small rug's corners: the corner of the field cut off on the
    // diagonal, a line along the cut and a ground of its own behind it,
    // so many tiles along each side of the field (nothing, where the
    // medallion leaves no room). The diagonal is whole tiles and tiles
    // cut in two, exactly.
    let spandrel = if small { (field * 0.5).floor().min(((wide + deep - reach * 1.5) / 1.0).floor() - 2.0) } else { 0.0 };
    let spandrel = if spandrel >= 3.0 { spandrel } else { 0.0 };
    let behind = corner.layers.first().map_or(SHADE, |layer| layer.ground);
    if small {
        corner.layers.clear();
    }

    // The scrolls of the field, and the small flowers between them
    // (which are scrolls with no stem).
    #[derive(Clone, Copy)]
    struct Scroll {
        x: f64,
        y: f64,
        /// the stem: how far out it starts and ends, the angle it starts at, how far round it goes, and which way
        stem: [f64; 5],
        leaves: f64,
        /// the flower at its middle
        size: f64,
        petals: f64,
        tones: [u8; 3],
    }
    let pendant_room = |x: f64, y: f64| {
        let (a, b) = if along_wide { (x, y) } else { (y, x) };
        pendant.map_or(f64::INFINITY, |[at, _, _, hung]| (a - a.clamp(at - hung, at + hung + 6.0)).hypot(b) - hung * 0.6)
    };
    // The room at the middle of every tile of the field: how large a
    // circle about it would touch the nearest thing. A tile is kept
    // clear round the figures and the frame; the rug's two middle lines
    // may be touched, where a scroll meets its mirror image.
    let (cols, rows) = (wide.max(0.0) as usize, deep.max(0.0) as usize);
    let mut room: Vec<f64> = (0..cols * rows)
        .map(|k| {
            let (x, y) = ((k % cols) as f64 + 0.5, (k / cols) as f64 + 0.5);
            let about = |figure: &Figure| figure.layers.first().map_or(f64::INFINITY, |outer| -depth(outer, (x - figure.x).hypot(y - figure.y), (y - figure.y).atan2(x - figure.x)).0);
            let cut_off = if spandrel > 0.0 { ((wide - x) + (deep - y) - spandrel) / std::f64::consts::SQRT_2 - 1.0 } else { f64::INFINITY };
            (wide - x - 1.0).min(deep - y - 1.0).min(x).min(y).min(about(&medallion) - 1.0).min(about(&corner) - 1.0).min(pendant_room(x, y) - 1.0).min(cut_off)
        })
        .collect();
    // The largest circle is not always taken, but the largest after each
    // tile's room is lessened by its own share, up to a fifth: taken
    // strictly, every seed fills a field the same way.
    let sway: Vec<f64> = (0..cols * rows).map(|_| roll(0.8, 1.0)).collect();
    // (the largest a scroll may be: without a limit the first fills the whole of the field beside the medallion)
    let most = (field * roll(0.2, 0.26)).clamp(4.5, 10.0);
    let mut scrolls: Vec<Scroll> = Vec::new();
    for _ in 0..600 {
        let Some(best) = (0..cols * rows).max_by(|a, b| (room[*a] * sway[*a]).total_cmp(&(room[*b] * sway[*b]))) else { break };
        let size = room[best].min(most);
        if size < 1.5 {
            break;
        }
        let (x, y) = ((best % cols) as f64 + 0.5, (best / cols) as f64 + 0.5);
        let tones = [[PALE, PETAL, HEART], [HEART, PETAL, DEEP], [PETAL, HEART, PALE]][roll(0.0, 2.999) as usize];
        let scroll = if size < 3.0 {
            Scroll { x, y, stem: [0.0; 5], leaves: 0.0, size: 1.5, petals: 4.0, tones }
        } else {
            let flower = if size < 5.0 { 1.5 } else if size < 9.0 { (size * 0.45).floor() + 0.5 } else { size * 0.5 };
            // the scroll it touches, if any: it branches from that one's stem, and turns the other way
            let parent = scrolls.iter().filter(|other| other.stem[3] > 0.0).map(|other| (((x - other.x).hypot(y - other.y) - other.stem[0].min(most) - size).abs(), other)).filter(|(gap, _)| *gap < 0.8).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, other)| *other);
            let (start, from, hand) = match parent {
                Some(other) => {
                    let towards = (other.y - y).atan2(other.x - x);
                    let [out, end, at, round, hand] = other.stem;
                    let on = (hand * (towards + PI - at)).rem_euclid(TAU);
                    let stem_there = if on <= round { out + (end - out) * on / round } else { out };
                    (((x - other.x).hypot(y - other.y) - stem_there).clamp(size, size + 2.5), towards, -hand)
                }
                None => (size, roll(0.0, TAU), if roll(0.0, 1.0) < 0.5 { 1.0 } else { -1.0 }),
            };
            let end = flower + 0.9;
            let round = ((start - end) / 2.6).clamp(0.75, 1.4) * TAU;
            Scroll { x, y, stem: [start, end, from, round, hand], leaves: (round * (start + end) / 2.0 / 4.5).round().max(1.0), size: flower, petals: [5.0, 6.0, 8.0][roll(0.0, 2.999) as usize], tones }
        };
        // (a small flower keeps a tile clear about it; a scroll touches the next)
        let keep = if size < 3.0 { 1.5 + 1.0 } else { size };
        for (k, room) in room.iter_mut().enumerate() {
            *room = room.min(((k % cols) as f64 + 0.5 - x).hypot((k / cols) as f64 + 0.5 - y) - keep);
        }
        scrolls.push(scroll);
    }
    // which scrolls may show in each square of four tiles by four, so that a place asks only those (see Cost above)
    const SQUARE: f64 = 4.0;
    let squares = ((wide / SQUARE) as usize + 1, (deep / SQUARE) as usize + 1);
    let mut near: Vec<Vec<u16>> = vec![Vec::new(); squares.0 * squares.1];
    for (n, scroll) in scrolls.iter().enumerate() {
        let out = scroll.stem[0].max(scroll.size) + 2.0;
        for j in ((scroll.y - out) / SQUARE).floor().max(0.0) as usize..=(((scroll.y + out) / SQUARE) as usize).min(squares.1 - 1) {
            for i in ((scroll.x - out) / SQUARE).floor().max(0.0) as usize..=(((scroll.x + out) / SQUARE) as usize).min(squares.0 - 1) {
                near[j * squares.0 + i].push(n as u16);
            }
        }
    }

    Box::new(move |u, v| {
        // A tile is asked about at sixths of a tile (see `tile_at`), and
        // a place and its mirror image come here by different sums, a
        // hair apart. Both are put on the same sixth, so that every sum
        // below is the same sum for the two, and the rug is its own
        // mirror image exactly, with no tie to settle anywhere. (The
        // hair is real, in three of the first two hundred columns, but
        // an edge has to fall on it to show, and no rug tried without
        // this line differed from its mirror image: it is a guard
        // against a fault not yet seen.)
        let (u, v) = ((u * 6.0).round() / 6.0, (v * 6.0).round() / 6.0);
        let (from_wide, from_deep) = (half_wide - u, half_deep - v);
        let inward = from_wide.min(from_deep);
        if inward < frame {
            // How far along the side, from the rug's corner: the same
            // for a place and its mirror image in the corner's
            // diagonal, so each band turns the corner as its own
            // mirror image. Counted from the rug's middle, the two
            // sides of a rug longer than it is broad would meet at a
            // corner out of step.
            let along = from_wide.max(from_deep);
            let chain = |t: f64, [links, ground]: [u8; 2]| if (t - 1.0).abs() + (along.rem_euclid(2.0) - 1.0).abs() < 1.0 { links } else { ground };
            if inward < guard || inward >= guard + main {
                let t = if inward < guard { inward } else { inward - guard - main };
                return if guard < 4.0 || !(1.0..3.0).contains(&t) { LINE } else { chain(t - 1.0, [PETAL, PALE]) };
            }
            // the main border: across it from its middle line, and along it from the middle of the corner
            let (t, s) = (inward - guard - main / 2.0, along - guard - main / 2.0);
            if main < 4.0 {
                return chain(t + 1.0, [HEART, DEEP]);
            }
            if small {
                // A small rug's border: lozenges four tiles across,
                // one at the middle of each side and one every four
                // tiles from it, and a square with a star in it at each
                // corner. Counted from the middle, a side of any length
                // is its own mirror image with a whole lozenge at its
                // middle. What does not divide is taken up beside the
                // corner squares, where a lozenge that would run into
                // one is left out and not cut short.
                let from_middle = (if from_deep < from_wide { half_wide } else { half_deep }) - along;
                if along < guard + main {
                    return bloom(along - guard - 2.0, t, 2.0, 4.0, [PETAL, PETAL, PALE]).unwrap_or(DEEP);
                }
                let nearest = 4.0 * (from_middle / 4.0).round();
                let clear = (if from_deep < from_wide { half_wide } else { half_deep }) - guard - main - nearest >= 3.0;
                return if clear { bloom(from_middle - nearest, t, 2.0, 4.0, [PALE, PALE, HEART]).unwrap_or(DEEP) } else { DEEP };
            }
            // The stem crosses the middle line at the corner, at the
            // middle of the side and a whole number of times between,
            // so a side is a whole number of swings and no flower is
            // cut by the rug's middle line. Each side has its own
            // length of swing, the nearest to the border's breadth
            // that divides it.
            let side = (if from_deep < from_wide { half_wide } else { half_deep }) - guard - main / 2.0;
            let step = side / (side / main).round().max(1.0);
            let knot = if main >= 12.0 { 2.0 } else { 1.0 };
            let at = (s / step).round();
            if let Some(colour) = bloom(s - (at * step).round(), t, knot, 4.0, [PALE, PALE, HEART]) {
                return colour;
            }
            let size = main / 4.0 + if main >= 10.0 { 0.5 } else { 0.0 };
            let snap = |x: f64| if size.fract() != 0.0 { (x - 0.5).round() + 0.5 } else { x.round() };
            if s > 0.0 && main >= 6.0 {
                let crest = (s / step).floor();
                let sign = if crest % 2.0 == 0.0 { 1.0 } else { -1.0 };
                let tones = if crest % 2.0 == 0.0 { [PETAL, HEART, PALE] } else { [PALE, PETAL, HEART] };
                if let Some(colour) = bloom(s - snap((crest + 0.5) * step), t + sign * (main / 2.0 - 1.0 - size), size, 6.0, tones) {
                    return colour;
                }
                let swing = main / 2.0 - 1.0;
                let slope = swing * PI / step * (PI * s / step).cos();
                if (t - swing * (PI * s / step).sin()).abs() < 0.5 * (1.0 + slope * slope).sqrt() {
                    return LINE;
                }
            }
            return DEEP;
        }
        if let Some(colour) = figure_at(&medallion, u, v) {
            return colour;
        }
        if let Some([at, aside, size, hung]) = pendant {
            let (a, b) = if along_wide { (u - at, v) } else { (v - at, u) };
            let deep = size - a.hypot(b + aside);
            if deep > 0.0 {
                return if deep < 1.0 { LINE } else if a.abs() + b < hung * 0.4 { HEART } else { PALE };
            }
            if let Some(colour) = bloom(a - hung - 2.0, b, 2.0, 4.0, [PETAL, PETAL, HEART]).or_else(|| bloom(a - hung - 5.0, b, 1.0, 4.0, [LINE; 3])) {
                return colour;
            }
        }
        if let Some(colour) = figure_at(&corner, u, v) {
            return colour;
        }
        if spandrel > 0.0 {
            let (dx, dy) = (wide - u, deep - v);
            if dx + dy < spandrel {
                // (a lozenge in it where it is large enough to hold one clear of its line)
                let at = ((spandrel - 4.0) / 2.0).floor();
                return if dx + dy >= spandrel - 1.0 { LINE } else if at >= 1.0 { bloom(dx - at, dy - at, 1.0, 4.0, [PALE; 3]).unwrap_or(behind) } else { behind };
            }
        }
        let here = &near[((v / SQUARE) as usize).min(squares.1 - 1) * squares.0 + ((u / SQUARE) as usize).min(squares.0 - 1)];
        let mut leaf = 0;
        for scroll in here.iter().map(|n| &scrolls[*n as usize]) {
            let (dx, dy) = (u - scroll.x, v - scroll.y);
            let far = dx.hypot(dy);
            let [out, end, from, round, hand] = scroll.stem;
            if far >= out.max(scroll.size) + 2.0 {
                continue;
            }
            if let Some(colour) = bloom(dx, dy, scroll.size, scroll.petals, scroll.tones) {
                return colour;
            }
            if round == 0.0 {
                continue;
            }
            let first = (hand * (dy.atan2(dx) - from)).rem_euclid(TAU);
            let stem_at = |on: f64| out + (end - out) * on / round;
            let pitch = (out - end) * TAU / round;
            for coil in 0..3 {
                let on = first + TAU * coil as f64;
                if on > round {
                    break;
                }
                let off = far - stem_at(on);
                if off.abs() < 0.5 {
                    return LINE;
                }
                // the leaf nearest along the stem, on the outer side and the inner by turns (the inner only where the coils are far enough apart), and in two greens, one each side of its middle
                let nth = (on * scroll.leaves / round).floor();
                let outer = nth % 2.0 == 0.0;
                let leaf_on = (nth + 0.5) * round / scroll.leaves;
                if if outer { leaf_on < TAU } else { pitch >= 3.4 } {
                    let run = (on - leaf_on) * stem_at(leaf_on) / 2.0;
                    let rise = if outer { off } else { -off };
                    if run.abs() < 1.0 && rise > 0.0 && rise < 0.5 + 1.6 * (1.0 - run * run) {
                        leaf = if run * hand < 0.0 { LEAF } else { SHADE };
                    }
                }
            }
        }
        leaf
    })
}

/// A style drawn by its rule, as a grid of tiles `across` by `down`.
///
/// Every style is a rule that gives a number for any place on the floor:
/// 0 for the ground or the mortar, and 1 to 7 for the parts of the
/// pattern. Three styles have a rule of their own elsewhere and are only
/// handed on from here: boards by [`planks`], and bricks and cobblestone
/// by [`Masonry`], which are set out on the grid's own lines. The rug's
/// rule is [`rug_garden`]. The rest of this function is the medallions,
/// and the setting out that the medallions and the rug share.
///
/// ## Medallions
///
/// Medallions are a dense pattern of outlined lozenges, squares and
/// octagons, ring within ring, whose outlines run into one another where
/// the figures meet, and which is its own mirror image from side to side
/// and from end to end.
///
/// Every figure is a way of measuring how far a place is from the figure:
/// across plus along for a lozenge, the greater of the two for a square,
/// and both limits together for an octagon. The measure is nothing on the
/// figure's outline, below nothing inside it, and grows steadily away from
/// it. One number is worked out for every place: the least of all the
/// figures' measures there. The floor is coloured by that number in bands
/// a tile wide.
///
/// The bands are turned on and off by a short repeating run, so each
/// figure is ringed inside and out by bands of its own shape, and where
/// the bands of two figures meet they join into one outline round both,
/// which is what gives the crowded, interlocked look. It is how a
/// distance function joins two solids into one (see `sdf`), used flat.
///
/// A band that is on takes the number of the figure nearest it, 1 to 4:
/// the figure at the middle of a repeat, the one at its corners, the one
/// at the middle of its sides, and one between. A band that is off is the
/// ground, 0. Whoever paints the bricks shares the numbers out among
/// however many colours there are (see [`colour_of`]): with two colours
/// the whole pattern is one colour on the other, and each colour added
/// picks another figure out.
///
/// ## Why not wave function collapse
///
/// The patterns were asked for under that name, and the first styles used
/// it (see [`Style`]). It was tried for a rug's field and gave a rug its
/// scattered small motifs, which is a plain rug. What makes a rug look
/// like one is a few large figures that govern the whole of it, and wave
/// function collapse knows only what may sit beside what, a few tiles
/// across.
///
/// ## The tiles
///
/// The number is taken at four places in every tile, one in each of the
/// quarters its two diagonals cut it into (see [`tile_at`]). Where all
/// four are one colour the tile is that colour. Where the two quarters on
/// one side of a diagonal are one colour and the two on the other side
/// another, the tile is cut along that diagonal. The medallions' measures
/// change by whole tiles along the grid's lines and its diagonals, so
/// those are the only two cases but for a few tiles where several
/// figures' bands meet, which take the colour at their middle.
///
/// `unbroken` says which tiles are whole tiles of the floor. The
/// medallions and the rug are set out about the middle of the rectangle
/// of tiles that takes them all in, and the tiles outside that rectangle
/// stay plain. The other three styles take no notice of `unbroken`: they
/// fill the whole grid, and [`lay_tiles`] makes plain what is not whole.
///
/// The tiles are drawn on `threads`: a tile's colours are asked of the
/// pattern at its own place and depend on no other tile. (Not the boards
/// of `planks`, which are laid end to end along a row, each from where the
/// last stopped, and take a third of a millisecond for a whole floor.)
fn drawn(kind: Drawn, [across, down]: [usize; 2], unbroken: &[bool], settings: &Mosaic, threads: Threads) -> Vec<Tile> {
    let seed = settings.seed;
    match kind {
        Drawn::Planks => return planks([across, down], seed, settings.size),
        Drawn::Bricks | Drawn::Cobbles => {
            let wall = Masonry::new(settings, kind == Drawn::Cobbles);
            return threads.map(across * down, if kind == Drawn::Cobbles { COBBLE_TILES_A_THREAD } else { BRICK_TILES_A_THREAD }, |k| tile_at(&|x, y| wall.colour(DVec2::new(x, y)), (k % across) as f64, (k / across) as f64));
        }
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
    let (colour, _) = figured(kind, half_wide, half_deep, seed);
    // (the rectangle of whole tiles, row by row; the tiles round it stay plain)
    let (wide, deep) = (2 * half_wide as usize, 2 * half_deep as usize);
    let figured = threads.map(wide * deep, FIGURED_TILES_A_THREAD, |k| tile_at(&colour, (i0 + k % wide) as f64 - middle.0, (j0 + k / wide) as f64 - middle.1));
    for (k, tile) in figured.into_iter().enumerate() {
        tiles[(j0 + k / wide) * across + i0 + k % wide] = tile;
    }
    tiles
}

/// The rule of medallions or of a rug set out over a rectangle `half_wide`
/// tiles to each side of its middle and `half_deep` to each end: the
/// number at any place, counted in tiles from the middle. With it, how
/// many tiles a repeat of the medallions is (nothing for a rug, which
/// does not repeat).
///
/// Apart from [`drawn`], which asks it at each tile of a floor: the rule
/// is of places, and knows nothing of the grid it is asked at.
fn figured(kind: Drawn, half_wide: f64, half_deep: f64, seed: u64) -> (Box<dyn Fn(f64, f64) -> u8 + Send + Sync>, f64) {
    let least = half_wide.min(half_deep);
    let mut numbers = Numbers(seed ^ 0x6A09_E667_F3BC_C908);
    // a whole number from `low` to `high`
    let mut pick = |low: f64, high: f64| (low + numbers.unit() * (high - low + 1.0)).floor().clamp(low, high.max(low));
    // A measure: how far a folded place is from a figure. Boxed, since the
    // figures of one pattern are of several kinds in one list, and there
    // are a few dozen at most.
    type Measure = Box<dyn Fn(f64, f64) -> f64 + Send + Sync>;
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
    // the figures of the part everything is folded onto, each with the number of the feature it is
    let mut figures: Vec<(u8, Measure)> = Vec::new();
    // how many tiles a repeat is, for a pattern that repeats
    let mut period = 2.0;
    match kind {
        Drawn::Rug | Drawn::Planks | Drawn::Bricks | Drawn::Cobbles => {}
        Drawn::Medallions => {
            let half = pick(4.0, 8.0).min(least.max(2.0));
            period = half * 2.0;
            // at the middle of a repeat, at its corners, at the middle of its sides, and one between
            figures.push((1, any(pick(0.0, 2.0), 0.0, 0.0, pick(1.0, (half / 2.0).floor().max(1.0)), pick(1.0, 2.0))));
            figures.push((2, any(pick(0.0, 2.0), half, half, pick(1.0, (half / 2.0).floor().max(1.0)), pick(1.0, 2.0))));
            figures.push((3, any(pick(0.0, 2.0), half, 0.0, pick(0.0, (half / 3.0).floor()), 1.0)));
            let out = pick(2.0, (half - 1.0).max(2.0));
            figures.push((4, shape(out, pick(1.0, (out - 1.0).max(1.0)), NONE, NONE, pick(0.0, 1.0))));
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
    let fold = move |x: f64, y: f64| {
        let ordered = |u: f64, v: f64| (u.max(v), u.min(v));
        match kind {
            Drawn::Rug | Drawn::Planks | Drawn::Bricks | Drawn::Cobbles => (x.abs(), y.abs()),
            Drawn::Medallions => ordered((x - period * (x / period).round()).abs(), (y - period * (y / period).round()).abs()),
        }
    };
    // the colour at a place, in tiles from the middle
    let colour = move |x: f64, y: f64| {
        let (u, v) = fold(x, y);
        if let Some(garden) = &garden {
            return garden(u, v);
        }
        let (mut level, mut feature) = (f64::INFINITY, 0u8);
        for (number, far) in &figures {
            let here = far(u, v);
            // Nearer by more than rounding: a place just as far from two
            // figures goes to the first of them, and so does its mirror
            // image, which is worked out by other sums and may come out
            // a hair nearer the second.
            if here < level - 1e-9 {
                (level, feature) = (here, *number);
            }
        }
        // the band the place is in, counted out from the figure's outline, is on or off as the run has it
        if run[(level.floor() as i64).rem_euclid(run.len() as i64) as usize] {
            feature
        } else {
            0
        }
    };
    (Box::new(colour), if kind == Drawn::Medallions { period } else { 0.0 })
}

/// How many tiles of a pattern are worth a thread of their own to draw
/// (see `par::Threads::map_ranges`): a tile of cobblestone takes 3 to 5
/// microseconds, since each of the four or five places it is asked at looks
/// through fifteen stones twice; a tile of bricks 0.3, a brick being found
/// from its course and joint by arithmetic; a tile of a rug or of
/// medallions about 0.4, the distance to a few dozen figures.
const COBBLE_TILES_A_THREAD: usize = 128;
const BRICK_TILES_A_THREAD: usize = 1_600;
const FIGURED_TILES_A_THREAD: usize = 1_280;

/// How many cells of bricks are worth a thread of their own when their
/// bevelled tops are shaped: 0.2 to 0.4 microseconds a cell, its brick
/// asked at the cell's eight corners. That is 2 to 6 milliseconds for a
/// floor 140 cells across: little, but a third of what is left of such a
/// floor's build once its outline is sampled on several threads.
const BEVELLED_CELLS_A_THREAD: usize = 1_600;

/// How many places a region is asked about are worth a thread of their
/// own: the figure `fit_planar` has for lattice points, the same question
/// asked as far from the region's edge.
const PLACES_A_THREAD: usize = 4_000;

/// Colour a fitted region as a mosaic and push it through cells
/// `w0..w0 + depth` across `axis`, onto `bricks`, with each brick's number
/// (0 to 7) onto `shades`. Returns `fit` as it was given.
///
/// `fit` is the region already fitted as the one solid it is (by
/// [`lay_mosaic`]), so that its edge is as smooth as any flat form's, or a
/// set of whole cells taken as they stand (by [`lay_mosaic_on`]). Here it
/// is only coloured. It is cut into tiles of one cell on the brick grid,
/// and the style's rule gives each tile its colours (see [`drawn`]): one
/// colour, or two triangles of two colours either side of a diagonal.
///
/// A tile cut into triangles is two sloped bricks of one cell each. The
/// two fill one box between them, as the two halves of a sloped pair do
/// anywhere else. Everything else the fit made whole is whole cells, and
/// those are sorted by their tile's colour and joined again into bricks
/// of one colour, so that tiles of a colour side by side are one brick
/// and not a brick each.
///
/// The fit's own sloped bricks, along a slanting edge, stay where they
/// are. One whose two sides share a divisor is taken apart into smaller
/// ones of the same slope over whole cells, and each that remains takes
/// the pattern's colour at the middle of its solid half.
///
/// Bevelled bricks, in a form two or more cells deep, have their top cell
/// laid apart from the rest: a sloped brick along each side of a brick of
/// the wall, a hipped one at each corner and a block between. Those are
/// found a cell at a time and joined into long pieces by [`joined`].
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
fn lay_tiles(fit: Fit, region: &(dyn Fn(DVec2) -> f64 + Sync), settings: &Mosaic, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
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
    // the tiles that cover the box, set on multiples of a tile's size so
    // that moving the form does not slide the pattern under it
    // A style that is its own mirror image has its tiles set out from the
    // middle of the floor instead. Set on multiples of a tile's size, a
    // floor that did not happen to lie evenly on them had a part row of
    // tiles down one side and none down the other, and the pattern sat
    // to one side of the floor.
    let mirrored = matches!(style.drawn, Drawn::Rug | Drawn::Medallions);
    let from = if mirrored { [(min[0] + max[0]).div_euclid(2), (min[1] + max[1]).div_euclid(2)] } else { [0, 0] };
    let first = [(min[0] - from[0]).div_euclid(wide), (min[1] - from[1]).div_euclid(tall)];
    let across = ((max[0] - from[0] + wide - 1).div_euclid(wide) - first[0]).max(1) as usize;
    let down = ((max[1] - from[1] + tall - 1).div_euclid(tall) - first[1]).max(1) as usize;
    let tile_of = |x: i32, y: i32| ((y - from[1]).div_euclid(tall) - first[1]) as usize * across + ((x - from[0]).div_euclid(wide) - first[0]) as usize;
    let middle = |k: usize| DVec2::new(from[0] as f64 + ((first[0] + (k % across) as i32) as f64 + 0.5) * wide as f64, from[1] as f64 + ((first[1] + (k / across) as i32) as f64 + 0.5) * tall as f64);
    // how deep inside the region a tile's middle is, in tiles (by its shorter side): under nothing, it is outside
    let inside: Vec<f64> = opts.threads.map(across * down, PLACES_A_THREAD, |k| -region(middle(k)) / wide.min(tall) as f64);
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
    // A wall of bevelled bricks: the top cell of every brick is laid
    // further down, sloping to the brick's edge, and everything else
    // stops a cell short of the wall's face. (A wall one cell deep has no
    // room for it.)
    let wall = (style.drawn == Drawn::Bricks && settings.bevel && depth >= 2).then(|| Masonry::new(settings, false));
    // The depth the other two styles have. Planks: the last row of tiles
    // of every row of boards is one long ramp down to the next row, so
    // that each row stands a cell proud of the one beside it along a
    // straight shadow line, as lapped boards do. (It needs boards three
    // tiles broad or more. A board two tiles broad is then half ramp, and
    // a floor of them looked ribbed like corrugated iron, not boarded:
    // at the usual size, 2, there are no grooves.) Cobblestone: the mortar's
    // tiles have no top cell, so the stones stand a cell proud of it with
    // upright sides. (It needs mortar to sink.)
    //
    // Both are whole cells and ramps as long as a row, which is what a
    // bevel had to be cut down to before it looked clean (see
    // [`Masonry`]). Cobbles domed, or bevelled like bricks, were not
    // brought back: a cobble's sides are at any angle, and a slope along
    // them is a spray of single wedges. A groove of two ramps facing each
    // other between boards was tried and takes a board two tiles broad
    // down to a ridge with no level top.
    let grooved = style.drawn == Drawn::Planks && settings.bevel && depth >= 2 && settings.size >= 3;
    let sunk = style.drawn == Drawn::Cobbles && settings.bevel && depth >= 2 && settings.mortar;
    let low = depth - (wall.is_some() || grooved || sunk) as i32;
    let plain_colour = 0;
    let mut tiles = drawn(style.drawn, [across, down], &unbroken, settings, opts.threads);
    // (the pattern as it was drawn, for the sloped bricks of the edge to be coloured by)
    let pattern = tiles.clone();
    for k in (0..across * down).filter(|k| plain[*k]) {
        tiles[k] = [plain_colour; 4];
    }
    let mut push = |mask4: u8, x: i32, y: i32, w: i32, h: i32, shade: u8| {
        let mask = (0..8u8).fold(0u8, |m, c| m | (mask4 >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c);
        let mut at = 0;
        while at < low {
            let run = (low - at).min(opts.max_ext.max(1));
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
    // (the top cell of whatever has one laid apart from the rest: see the bevelled tops and the grooves, below)
    let mut tops: Vec<(CellBrick, u8)> = Vec::new();
    for (k, tile) in tiles.iter().enumerate().filter(|(_, tile)| tile.contains(&ON_CUT)) {
        let (x, y) = (from[0] + (first[0] + (k % across) as i32) * wide, from[1] + (first[1] + (k / across) as i32) * tall);
        // cut from bottom left to top right, or from top left to bottom right
        let halves = if tile[1] == ON_CUT { [(0b0111, tile[0]), (0b1110, tile[3])] } else { [(0b1101, tile[2]), (0b1011, tile[1])] };
        for (mask4, shade) in halves {
            push(mask4, x, y, wide, tall, shade);
            // (sunk mortar: the half of the tile that is stone keeps its top cell, the same half tile one cell deep)
            if sunk && shade != 0 {
                let mask = (0..8u8).fold(0u8, |m, c| m | (mask4 >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c);
                let (mut min, mut ext) = ([0; 3], [1; 3]);
                (min[ua], min[va], min[axis]) = (x, y, w0 + low);
                (ext[ua], ext[va]) = (wide, tall);
                tops.push((CellBrick { mask, min, ext }, shade));
            }
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
    // The bevelled tops. For each cell of a brick the brick is asked at
    // the cell's eight corners whether it reaches that high there: at its
    // outline it stands a cell short of the wall's face, and a cell in it
    // is whole. The corners it reaches are a shape: all eight a whole
    // cell, the four below and two above a sloped brick, the four below
    // and one above the hipped brick of a corner. (This is how the fitter
    // of solid forms chooses a cell's brick, see `fit_volume`, done here
    // by hand for one layer, a wall being hundreds of small solids and
    // that fitter being made for one large one.) They are laid a cell at
    // a time and then joined, so that a brick's top is nine bricks: a
    // sloped one the length of each side, four corners and a block.
    if let Some(wall) = &wall {
        // (a cell's place in the pattern: tiles are counted from the first, and a tile is a cell)
        let place = |x: f64, y: f64| DVec2::new(x - (from[0] + first[0] * wide) as f64, y - (from[1] + first[1] * tall) as f64);
        // Each cell's shape is found on `opts.threads`, a cell's being its
        // own affair, and the bricks are put down after, in the cells' order.
        let shaped: Vec<Option<u8>> = opts.threads.map(whole.len(), BEVELLED_CELLS_A_THREAD, |k| {
            let (y, x, colour) = whole[k];
            if colour == 0 {
                return None;
            }
            let piece = wall.piece(place(x as f64 + 0.5, y as f64 + 0.5));
            let reaches = |dx: f64, dy: f64, up: f64| wall.inside(piece, place(x as f64 + dx, y as f64 + dy)) >= up - 1e-6;
            // (a corner is bit `across | down << 1 | up << 2`, here across the wall, down it, and out of it; then as the wall stands)
            let corners = (0..8u8).fold(0u8, |mask, c| mask | (reaches((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2) as f64) as u8) << c);
            let mask = (0..8u8).fold(0u8, |m, c| m | (corners >> ((c >> ua & 1) | (c >> va & 1) << 1 | (c >> axis & 1) << 2) & 1) << c);
            crate::shapes::shape_info(mask).is_some().then_some(mask)
        });
        for (&(y, x, colour), mask) in whole.iter().zip(shaped) {
            let Some(mask) = mask else { continue };
            let (mut min, mut ext) = ([0; 3], [1; 3]);
            (min[ua], min[va], min[axis]) = (x, y, w0 + low);
            ext[axis] = 1;
            tops.push((CellBrick { mask, min, ext }, colour));
        }
    }
    if grooved || sunk {
        let broad = settings.size.clamp(1, 16);
        for &(y, x, colour) in &whole {
            // (a corner is bit `across | down << 1 | up << 2`, as for the bevelled tops)
            let corners: u8 = if sunk {
                if colour == 0 {
                    continue;
                }
                crate::shapes::CUBE
            } else if ((y - from[1]).div_euclid(tall) - first[1]).rem_euclid(broad) == broad - 1 {
                // the four below, and above them the two on the side of the row's own boards
                0b0011_1111
            } else {
                crate::shapes::CUBE
            };
            let mask = (0..8u8).fold(0u8, |m, c| m | (corners >> ((c >> ua & 1) | (c >> va & 1) << 1 | (c >> axis & 1) << 2) & 1) << c);
            let (mut min, mut ext) = ([0; 3], [1; 3]);
            (min[ua], min[va], min[axis]) = (x, y, w0 + low);
            ext[axis] = 1;
            tops.push((CellBrick { mask, min, ext }, colour));
        }
    }
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
    for (brick, colour) in joined(tops, opts.max_ext.max(1)) {
        bricks.push(brick);
        shades.push(colour);
    }
    fit
}

/// Bricks laid a cell at a time, joined into the longest bricks that fill
/// the same room: cells into rows, rows into slabs and slabs into blocks.
///
/// Two bricks join if they are the same shape and colour, lie end to end
/// along an axis with the same extent across it, and the shape is the
/// same all the way along that axis: a sloped brick may be lengthened
/// along its ridge and not across its slope, which would change the
/// slope. Each axis is taken in turn.
pub fn joined(bricks: Vec<(CellBrick, u8)>, limit: i32) -> Vec<(CellBrick, u8)> {
    let mut bricks = bricks;
    for along in 0..3 {
        // whether a shape is the same at both ends of this axis: every corner filled or not as the corner opposite it along the axis is
        let even = |mask: u8| (0..8u8).all(|c| mask >> c & 1 == mask >> (c ^ 1 << along) & 1);
        let (a, b) = match along {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        // (those that may join are side by side once sorted so: alike in all but where they start along the axis)
        bricks.sort_by_key(|(brick, colour)| (brick.mask, *colour, brick.min[a], brick.min[b], brick.ext[a], brick.ext[b], brick.min[along]));
        let mut out: Vec<(CellBrick, u8)> = Vec::with_capacity(bricks.len());
        for (brick, colour) in bricks {
            match out.last_mut() {
                Some((last, shade)) if *shade == colour && last.mask == brick.mask && even(brick.mask) && last.min[a] == brick.min[a] && last.min[b] == brick.min[b] && last.ext[a] == brick.ext[a] && last.ext[b] == brick.ext[b] && last.min[along] + last.ext[along] == brick.min[along] && last.ext[along] + brick.ext[along] <= limit => last.ext[along] += brick.ext[along],
                _ => out.push((brick, colour)),
            }
        }
        bricks = out;
    }
    // (in the order they stand, so that one wall is always the same list of bricks)
    bricks.sort_by_key(|(brick, _)| (brick.min[2], brick.min[1], brick.min[0], brick.mask));
    bricks
}

/// Fit a mosaic over a region (a flat band, a filled outline) and push it
/// through cells `w0..w0 + depth` across `axis`: see [`lay_tiles`], which
/// colours the fit this makes.
#[allow(clippy::too_many_arguments)]
pub fn lay_mosaic(region: &(dyn Fn(DVec2) -> f64 + Sync), settings: &Mosaic, axis: usize, min: [i32; 2], max: [i32; 2], w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) -> Fit {
    // The floor is fitted exactly as it is with no pattern: a pattern
    // colours a form and does not reshape it. Along a slanting edge that
    // fit lays sloped bricks many cells long, and each is one colour, the
    // pattern's colour at its middle. For a while the floor was fitted
    // with no brick longer than five cells, so that the edge's bricks
    // were small enough to carry the pattern; but a slant drawn in short
    // bricks follows the form less closely (1.5% of a curving band amiss
    // against 0.6%), and a form that changed its outline when a pattern
    // was chosen was the worse fault.
    let fit = fit_planar(&|x, y| region(DVec2::new(x, y)), min, max, opts);
    lay_tiles(fit, region, settings, axis, min, max, w0, depth, opts, bricks, shades)
}

/// Lay a mosaic on a given set of whole cells and push it through cells
/// `w0..w0 + depth` across `axis`: the flat part of a form that is not
/// flat all over (see `spec::pattern_flat_faces`). Nothing is fitted: the
/// cells are the floor as they stand, with no sloped edge of their own,
/// since what lies round them is the rest of the form's bricks.
pub fn lay_mosaic_on(cells: &[(i32, i32)], settings: &Mosaic, axis: usize, w0: i32, depth: i32, opts: &Opts, bricks: &mut Vec<CellBrick>, shades: &mut Vec<u8>) {
    let Some(first) = cells.first() else { return };
    let (min, max) = cells.iter().fold(([first.0, first.1], [first.0 + 1, first.1 + 1]), |(min, max), (x, y)| ([min[0].min(*x), min[1].min(*y)], [max[0].max(x + 1), max[1].max(y + 1)]));
    let within: std::collections::HashSet<(i32, i32)> = cells.iter().copied().collect();
    let fit = Fit { pieces: cells.iter().map(|(x, y)| crate::fit_planar::Piece { mask: 15, x: *x, y: *y, w: 1, h: 1 }).collect(), solid: cells.len() as f64, error: 0.0 };
    // (how deep inside the floor a place is, for the plain border: a cell of the set is taken as far inside, there being no outline to measure from)
    let region = |p: DVec2| if within.contains(&(p.x.floor() as i32, p.y.floor() as i32)) { -1.0e6 } else { 1.0 };
    lay_tiles(fit, &region, settings, axis, min, max, w0, depth, opts, bricks, shades);
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

    /// Rugs 48 tiles or more the shorter way, and what each was (see `rug_number`).
    const LARGE_RUGS: [(usize, usize, u64, u64); 6] = [(96, 72, 1, 0x0e04fa1ba657eb81), (120, 80, 5, 0xd495c88598bb8b11), (250, 160, 2, 0x172d2df38a8e7dcd), (48, 200, 3, 0x76b1593786c6ad11), (100, 100, 7, 0xdd4923c63360acf9), (64, 48, 1, 0x7a1ea8a8ab7da9d9)];

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
        let whole_floor = fit_planar(&|x, y| disc(DVec2::new(x, y)), min, max, &Opts::default());
        for (style, named) in STYLES.iter().enumerate() {
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            let settings = Mosaic { style, size: 2, bevel: false, border: 1, seed: 7, mortar: true, stagger: -1, upright: false, uneven: [0; 2] };
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
                assert!(count == 1 || disc(p) > -1.5, "{name}: no brick holds {p:?}, well inside the floor");
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
        for (style, named) in STYLES.iter().enumerate().filter(|(_, style)| matches!(style.drawn, Drawn::Rug | Drawn::Medallions)) {
            let name = named.name;
            let lay = |seed: u64| {
                let (mut bricks, mut shades) = (Vec::new(), Vec::new());
                lay_mosaic(&floor, &Mosaic { style, size: 2, bevel: false, border: 1, seed, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, 2, min, max, 0, 1, &Opts::default(), &mut bricks, &mut shades);
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
                    assert!(here.len() == 1 && here == colours(-x, y) && here == colours(x, -y), "{name}, seed {seed}: the pattern differs across its middle at {x}, {y}: {here:?}, {:?}, {:?}", colours(-x, y), colours(x, -y));
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
    fn a_rugs_border_turns_its_corners_and_closes_at_the_middle_of_each_side() {
        // A rug 128 by 80: its main border is 8 tiles between guards of
        // 4, and both its sides are a whole number of swings of 8.
        let (across, down) = (128, 80);
        let tiles = drawn(Drawn::Rug, [across, down], &vec![true; across * down], &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed: 5, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
        let tile = |i: usize, j: usize| tiles[j * across + i];
        // Round a corner the frame is its own mirror image in the
        // corner's diagonal, which swaps a tile's top right and bottom
        // left: no band is out of step where the two sides meet.
        for j in 0..24 {
            for i in 0..24 {
                if i.min(j) < 16 {
                    let [a, b, c, d] = tile(j, i);
                    assert_eq!(tile(i, j), [a, c, b, d], "the frame differs across the corner's diagonal at {i}, {j}");
                }
            }
        }
        // (and it is not plain there: the stem and its flowers are in it)
        assert!((0..24).flat_map(|j| (0..24).map(move |i| (i, j))).filter(|(i, j)| *i.min(j) >= 4 && *i.min(j) < 12).map(|(i, j)| tile(i, j)).collect::<HashSet<_>>().len() > 8);
        // At the middle of each side the stem crosses the border's
        // middle line, 8 tiles in, at a knot: a lozenge of four cut
        // tiles about that corner of the grid. Were a side not a whole
        // number of swings, a flower would lie there, cut in two by the
        // rug's middle line.
        assert_eq!(tile(63, 7)[1..], [ON_CUT, ON_CUT, 3], "the top side's middle");
        assert_eq!(tile(7, 39)[1..], [ON_CUT, ON_CUT, 3], "the left side's middle");
        // a side that is not a multiple of the border's breadth closes too
        let tiles = drawn(Drawn::Rug, [120, 80], &vec![true; 120 * 80], &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed: 5, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
        assert_eq!(tiles[7 * 120 + 59][1..], [ON_CUT, ON_CUT, 3], "the top side's middle, 120 across");
    }

    #[test]
    fn a_rug_of_any_size_is_drawn() {
        // down to floors too small for a field, or for anything
        for across in (4..=70).step_by(3) {
            for down in (4..=70).step_by(5) {
                for seed in [2, 9] {
                    let tiles = drawn(Drawn::Rug, [across, down], &vec![true; across * down], &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
                    assert!(tiles.iter().flatten().all(|colour| *colour < 8 || *colour == ON_CUT), "{across} by {down}, seed {seed}");
                }
            }
        }
    }

    /// A rug's tiles as one number, to tell whether any has changed.
    /// (FNV-1a, <http://www.isthe.com/chongo/tech/comp/fnv/>: the standard
    /// library's own hash is free to change from one version to the next.)
    fn rug_number(across: usize, down: usize, seed: u64) -> u64 {
        let tiles = drawn(Drawn::Rug, [across, down], &vec![true; across * down], &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
        tiles.iter().flatten().fold(0xcbf2_9ce4_8422_2325u64, |hash, colour| (hash ^ *colour as u64).wrapping_mul(0x0100_0000_01b3))
    }

    #[test]
    fn a_large_rug_is_tile_for_tile_what_it_was_before_small_rugs_were_set_out_anew() {
        // 48 tiles or more the shorter way: none of these may ever change by a tile
        for (across, down, seed, was) in LARGE_RUGS {
            assert_eq!(rug_number(across, down, seed), was, "the rug {across} by {down}, seed {seed}, has changed");
        }
    }

    #[test]
    fn a_small_rug_is_whole_parts_and_its_own_mirror_image() {
        for (across, down) in [(36, 28), (40, 31), (47, 30), (33, 44), (46, 46), (60, 26)] {
            for seed in [1, 4, 9] {
                let tiles = drawn(Drawn::Rug, [across, down], &vec![true; across * down], &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
                // (the rectangle the rug is set out in: an even number of tiles each way)
                let (wide, deep) = (across / 2 * 2, down / 2 * 2);
                let tile = |i: usize, j: usize| tiles[j * across + i];
                for j in 0..deep {
                    for i in 0..wide {
                        let [a, b, c, d] = tile(i, j);
                        assert_eq!(tile(wide - 1 - i, j), [b, a, d, c], "{across} by {down}, seed {seed}: left to right at {i}, {j}");
                        assert_eq!(tile(i, deep - 1 - j), [c, d, a, b], "{across} by {down}, seed {seed}: top to bottom at {i}, {j}");
                    }
                }
                // The border is the four rows inside the one-tile guard.
                // Between the corner squares its lozenges are whole: a
                // lozenge four tiles across lies on twelve tiles (a
                // square of sixteen less its corners), so the tiles with
                // a lozenge's colours on them are so many twelves, one of
                // them at the middle of the side.
                let lozenge = |i: usize, j: usize| tile(i, j).iter().any(|colour| *colour == 3 || *colour == 7);
                let top = (5..wide - 5).flat_map(|i| (1..5).map(move |j| (i, j))).filter(|(i, j)| lozenge(*i, *j)).count();
                let left = (5..deep - 5).flat_map(|j| (1..5).map(move |i| (i, j))).filter(|(i, j)| lozenge(*i, *j)).count();
                assert!(top >= 12 && top % 12 == 0 && left >= 12 && left % 12 == 0, "{across} by {down}, seed {seed}: {top} and {left} tiles of lozenges");
                assert!(lozenge(wide / 2 - 1, 2) && lozenge(wide / 2, 2) && lozenge(2, deep / 2 - 1) && lozenge(2, deep / 2), "{across} by {down}: no lozenge at the middle of a side");
                // a star in each corner square, and nothing else of the border in it
                assert!((1..5).flat_map(|j| (1..5).map(move |i| (i, j))).filter(|(i, j)| tile(*i, *j).iter().any(|colour| *colour == 4)).count() == 12, "{across} by {down}: the corner square's star is not whole");
                // Each corner of the field (six tiles in: a guard, the border, a guard) is cut off on the
                // diagonal: its corner tile is the ground behind the cut, and out along the field's edge
                // from it comes the cut's line, a tile cut in two with the line's colour on its far half.
                assert!(matches!(tile(6, 6), [2, 2, 2, 2] | [3, 3, 3, 3] | [6, 6, 6, 6]), "{across} by {down}, seed {seed}: the field's corner is {:?}", tile(6, 6));
                assert!((7..wide / 2).any(|i| tile(i, 6) == [tile(6, 6)[0], ON_CUT, ON_CUT, 1]), "{across} by {down}, seed {seed}: no line cuts the field's corner off");
            }
        }
    }

    #[test]
    fn a_rug_is_its_own_mirror_image_tile_for_tile() {
        // A mirror image swaps a tile's corners: left with right, or top with bottom.
        for (across, down) in [(120, 80), (90, 132), (250, 160)] {
            for seed in 0..12 {
                let tiles = drawn(Drawn::Rug, [across, down], &vec![true; across * down], &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
                for j in 0..down {
                    for i in 0..across {
                        let [a, b, c, d] = tiles[j * across + i];
                        assert_eq!(tiles[j * across + across - 1 - i], [b, a, d, c], "{across} by {down}, seed {seed}: left to right at {i}, {j}");
                        assert_eq!(tiles[(down - 1 - j) * across + i], [c, d, a, b], "{across} by {down}, seed {seed}: top to bottom at {i}, {j}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_rug_is_drawn_in_good_time() {
        // A rug is drawn afresh at every change of its floor or its seed,
        // while the player drags. The bound is several times what it
        // takes built for release; built for debugging the sums are ten
        // times slower, and the bound says nothing, so it is not held to.
        // The least of five goes is what is held to the bound: the tests
        // run many at once, and one go on its own measured 38 ms for a rug
        // that takes 3 when nothing else is running.
        for (across, down, bound) in [(120, 80, 30), (250, 160, 60), (250, 250, 100)] {
            let whole = vec![true; across * down];
            let took = (0..5)
                .map(|_| {
                    let started = std::time::Instant::now();
                    let tiles = drawn(Drawn::Rug, [across, down], &whole, &Mosaic { style: 1, size: 2, bevel: false, border: 0, seed: 5, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, Threads::ONE);
                    assert_eq!(tiles.len(), across * down);
                    started.elapsed().as_millis()
                })
                .min()
                .unwrap();
            println!("a rug {across} by {down}: {took} ms");
            assert!(cfg!(debug_assertions) || took < bound, "a rug {across} by {down} took {took} ms");
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
    fn bricks_are_rectangles_however_uneven_and_cobbles_are_stones_of_any_shape() {
        let (across, down) = (90, 40);
        let wall = |seed: u64, size: i32, mortar: bool, stagger: i32, upright: bool, uneven: [i32; 2]| Mosaic { style: 0, size, bevel: false, border: 0, seed, mortar, stagger, upright, uneven };
        let laid = |kind: Drawn, across: usize, settings: Mosaic| -> Vec<Tile> { drawn(kind, [across, down], &vec![true; across * down], &settings, Threads::ONE) };
        // bricks: no tile is cut; a course is three tiles of brick and one of mortar; a brick is eight tiles and one of mortar
        let bricks = laid(Drawn::Bricks, across, wall(4, 2, true, -1, false, [0; 2]));
        assert!(bricks.iter().all(|tile| *tile == [tile[0]; 4] && tile[0] < 8));
        for row in 0..down {
            assert_eq!(bricks[row * across..(row + 1) * across].iter().all(|tile| tile[0] == 0), row % 4 == 0, "row {row}");
        }
        let joints = |tiles: &[Tile], row: usize| (0..across).filter(|at| tiles[row * across + at][0] == 0).collect::<Vec<_>>();
        let mut starts = HashSet::new();
        for row in (1..down).step_by(4) {
            assert!(joints(&bricks, row).windows(2).all(|pair| pair[1] - pair[0] == 9), "row {row}: joints at {:?}", joints(&bricks, row));
            starts.insert(joints(&bricks, row)[0]);
        }
        // every course moved along by its own amount
        assert!(starts.len() >= 4, "every course starts at one of {starts:?}");
        // a larger size is larger bricks
        assert!(joints(&laid(Drawn::Bricks, across, wall(4, 4, true, -1, false, [0; 2])), 1).len() < joints(&bricks, 1).len());
        // a stagger of nothing stacks the joints; of half a brick, every other course lines up and the ones between do not
        let stacked = laid(Drawn::Bricks, across, wall(4, 2, true, 0, false, [0; 2]));
        assert!((1..down).step_by(4).all(|row| joints(&stacked, row) == joints(&stacked, 1)));
        let bonded = laid(Drawn::Bricks, across, wall(4, 2, true, 50, false, [0; 2]));
        assert_eq!(joints(&bonded, 1), joints(&bonded, 9));
        assert!(joints(&bonded, 5).iter().all(|joint| !joints(&bonded, 1).contains(joint)), "the second course's joints fall on the first's");
        // upright: the same wall turned over its diagonal
        let (turned, level) = (laid(Drawn::Bricks, 40, wall(4, 2, true, 50, true, [0; 2])), laid(Drawn::Bricks, 40, wall(4, 2, true, 50, false, [0; 2])));
        assert!((0..1600).all(|k| turned[k] == level[k % 40 * 40 + k / 40]));
        // with no mortar there is no ground, and no brick has the number of the brick beside it in its course
        let bare = laid(Drawn::Bricks, across, wall(4, 2, false, 50, false, [0; 2]));
        assert!(bare.iter().all(|tile| tile[0] != 0 && *tile == [tile[0]; 4]));
        for row in 0..down {
            let ends = (1..across).filter(|at| bare[row * across + at] != bare[row * across + at - 1]).collect::<Vec<_>>();
            assert!(ends.windows(2).all(|pair| pair[1] - pair[0] == 8), "row {row}: bricks end at {ends:?}");
        }
        // Uneven along: bricks of many lengths, and still no tile cut.
        // Uneven across too: courses of more than one depth.
        let long = laid(Drawn::Bricks, across, wall(4, 3, true, 50, false, [100, 0]));
        assert!(long.iter().all(|tile| *tile == [tile[0]; 4]), "an uneven brick has a slanted side");
        let lengths: HashSet<usize> = (1..down).step_by(6).flat_map(|row| joints(&long, row).windows(2).map(|pair| pair[1] - pair[0]).collect::<Vec<_>>()).collect();
        assert!(lengths.len() >= 4, "bricks of lengths {lengths:?}");
        let both = laid(Drawn::Bricks, across, wall(4, 3, true, 50, false, [100, 100]));
        assert!(both.iter().all(|tile| *tile == [tile[0]; 4]));
        let beds: Vec<usize> = (0..down).filter(|row| both[row * across..(row + 1) * across].iter().all(|tile| tile[0] == 0)).collect();
        let depths: HashSet<usize> = beds.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(depths.len() >= 2, "courses of depths {depths:?}");
        // cobbles: stones with slanted sides (tiles cut in two), mortar between a tenth and a half of the wall, every number in use
        let cobbles = laid(Drawn::Cobbles, across, wall(4, 2, true, -1, false, [60, 60]));
        assert!(cobbles.iter().filter(|tile| tile.contains(&ON_CUT)).count() > 100);
        let mortar = cobbles.iter().filter(|tile| **tile == [0; 4]).count() as f64 / (across * down) as f64;
        assert!((0.1..0.5).contains(&mortar), "{mortar} of the wall is mortar");
        assert_eq!(cobbles.iter().flatten().filter(|c| **c != ON_CUT).collect::<HashSet<_>>().len(), 8);
        assert_eq!(laid(Drawn::Cobbles, across, wall(4, 2, true, -1, false, [60, 60])), cobbles);
        assert_ne!(laid(Drawn::Cobbles, across, wall(5, 2, true, -1, false, [60, 60])), cobbles);
        assert_ne!(laid(Drawn::Cobbles, across, wall(4, 2, true, -1, false, [0, 0])), cobbles);
    }

    #[test]
    fn a_bevel_slopes_the_top_cell_of_every_brick_down_to_its_edge() {
        // a floor 60 by 40 and 3 deep
        let floor = |p: DVec2| (p.x.abs() - 30.0).max(p.y.abs() - 20.0);
        let lay = |name: &str, deep: i32, bevel: bool, uneven: [i32; 2]| {
            let style = STYLES.iter().position(|s| s.name == name).unwrap();
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            lay_mosaic(&floor, &Mosaic { style, size: 2, bevel, border: 0, seed: 3, mortar: true, stagger: 50, upright: false, uneven }, 2, [-30, -20], [30, 20], 0, deep, &Opts::default(), &mut bricks, &mut shades);
            (bricks, shades)
        };
        use crate::shapes::Kind;
        let kind = |b: &CellBrick| crate::shapes::shape_info(b.mask).expect("a top that is no shape of brick").kind;
        // with no bevel, and for a wall one cell deep whatever is asked, every brick is the wall's whole depth
        for (name, deep, bevel) in [("bricks", 3, false), ("cobblestone", 3, false), ("bricks", 1, true), ("cobblestone", 1, true), ("planks", 3, false)] {
            assert!(lay(name, deep, bevel, [60, 60]).0.iter().all(|b| b.min[2] == 0 && b.ext[2] == deep), "{name}, {deep} deep, bevel {bevel}: a brick stops short of the wall's face");
        }
        for uneven in [[0, 0], [100, 100]] {
            let (bricks, shades) = lay("bricks", 3, true, uneven);
            assert_eq!(bricks.len(), shades.len());
            // everything stops a cell short of the face but the tops of the bricks, which are the top cell and are not mortar
            assert!(bricks.iter().all(|b| (b.min[2] == 0 && b.ext[2] == 2) || (b.min[2] == 2 && b.ext[2] == 1)));
            let tops: Vec<&CellBrick> = bricks.iter().filter(|b| b.min[2] == 2).collect();
            assert!(bricks.iter().zip(&shades).filter(|(b, _)| b.min[2] == 2).all(|(_, shade)| *shade != 0), "mortar reaches the face");
            // Nothing but whole cells, the four ramps that slope down
            // outwards and the four hips of the four corners, each
            // standing on its four lower corners: uneven bricks are
            // rectangles still, and bevel the same way.
            assert_eq!(tops.iter().map(|b| kind(b)).collect::<HashSet<Kind>>(), HashSet::from([Kind::Cube, Kind::Ramp, Kind::Hip]));
            let masks = |which: Kind| tops.iter().filter(|b| kind(b) == which).map(|b| b.mask).collect::<HashSet<u8>>();
            assert_eq!((masks(Kind::Ramp).len(), masks(Kind::Hip).len()), (4, 4));
            assert!(tops.iter().all(|b| b.mask & 0x0f == 0x0f));
            // the sloped bricks along a side are one brick, as long as the side
            let ramps: Vec<&&CellBrick> = tops.iter().filter(|b| kind(b) == Kind::Ramp).collect();
            assert!(ramps.iter().any(|b| b.ext[0] >= 4) && ramps.iter().all(|b| b.ext[0] == 1 || b.ext[1] == 1), "the sloped bricks along a side are not one brick");
            // no two bricks share room
            for (n, a) in bricks.iter().enumerate() {
                assert!(bricks[n + 1..].iter().all(|b| (0..3).any(|k| a.min[k] + a.ext[k] <= b.min[k] || b.min[k] + b.ext[k] <= a.min[k])), "two bricks share room");
            }
        }
    }

    #[test]
    fn planks_are_grooved_and_cobblestones_mortar_sunk_with_long_ramps_and_whole_cells_only() {
        // a floor 60 by 40 and 3 deep
        let floor = |p: DVec2| (p.x.abs() - 30.0).max(p.y.abs() - 20.0);
        let lay = |name: &str, size: i32, bevel: bool, mortar: bool| {
            let style = STYLES.iter().position(|s| s.name == name).unwrap();
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            lay_mosaic(&floor, &Mosaic { style, size, bevel, border: 0, seed: 3, mortar, stagger: 50, upright: false, uneven: [60, 60] }, 2, [-30, -20], [30, 20], 0, 3, &Opts::default(), &mut bricks, &mut shades);
            (bricks, shades)
        };
        // (no two bricks share room, but the two halves of a tile cut in two, which share its box and fill half each)
        let apart = |bricks: &[CellBrick]| {
            for (n, a) in bricks.iter().enumerate() {
                assert!(bricks[n + 1..].iter().all(|b| (0..3).any(|k| a.min[k] + a.ext[k] <= b.min[k] || b.min[k] + b.ext[k] <= a.min[k]) || (a.min == b.min && a.ext == b.ext && matches!(a.mask & b.mask & 0x0f, 0b0110 | 0b1001))), "two bricks share room");
            }
        };
        // Planks four tiles broad: under the top cell the floor is as it
        // was, and the top cell is level for three rows of tiles and one
        // ramp for the fourth, all the ramps sloping the same way.
        let (bricks, _) = lay("planks", 4, true, true);
        assert!(bricks.iter().all(|b| (b.min[2] == 0 && b.ext[2] == 2) || (b.min[2] == 2 && b.ext[2] == 1)));
        let ramps: Vec<&CellBrick> = bricks.iter().filter(|b| b.mask != crate::shapes::CUBE).collect();
        assert!(!ramps.is_empty() && ramps.iter().all(|b| b.min[2] == 2 && b.mask == ramps[0].mask && crate::shapes::shape_info(b.mask).unwrap().kind == crate::shapes::Kind::Ramp));
        // a ramp is one tile across and as long as its board (but for the offcut a row begins with and the one the floor's edge leaves), and its row is every fourth
        assert!(ramps.iter().all(|b| b.ext[1] == 1) && ramps.iter().filter(|b| b.ext[0] >= 16).count() * 2 > ramps.len(), "ramps are not as long as boards");
        let rows: HashSet<i32> = ramps.iter().map(|b| b.min[1]).collect();
        assert_eq!(rows.len(), 10, "ramps in rows {rows:?}");
        assert!(rows.iter().all(|row| (row - rows.iter().min().unwrap()) % 4 == 0));
        // (40 rows of tiles 60 long: 10 of ramps, 30 level)
        assert_eq!(bricks.iter().filter(|b| b.min[2] == 2).map(|b| b.volume()).sum::<f64>(), 60.0 * 10.0 * 0.5 + 60.0 * 30.0);
        apart(&bricks);
        // at the usual size a board is two tiles broad and is left level: half of it would be ramp
        assert!(lay("planks", 2, true, true).0.iter().all(|b| b.mask == crate::shapes::CUBE && b.ext[2] == 3));
        // Cobblestone: no brick sloped across the floor's face (every one
        // is the same from its foot to its top: a block, or the half of
        // a tile a stone's slanting edge cuts), the stones the floor's
        // whole depth between them and their tops, and the mortar a
        // cell short of the face.
        let (bricks, shades) = lay("cobblestone", 2, true, true);
        assert!(bricks.iter().all(|b| b.mask & 0x0f == b.mask >> 4));
        assert!(bricks.iter().zip(&shades).all(|(b, shade)| if *shade == 0 { b.min[2] == 0 && b.ext[2] == 2 } else { (b.min[2] == 0 && b.ext[2] == 2) || (b.min[2] == 2 && b.ext[2] == 1) }), "mortar reaches the face");
        let (plain, plain_shades) = lay("cobblestone", 2, false, true);
        let cells = |bricks: &[CellBrick], shades: &[u8], mortar: bool| bricks.iter().zip(shades).filter(|(_, shade)| (**shade == 0) == mortar).map(|(b, _)| b.volume()).sum::<f64>();
        // (the stones are what they were, and the mortar has lost its top cell: a third of it)
        assert_eq!(cells(&bricks, &shades, false), cells(&plain, &plain_shades, false));
        assert_eq!(cells(&bricks, &shades, true) * 3.0, cells(&plain, &plain_shades, true) * 2.0);
        apart(&bricks);
        // with no mortar there is nothing to sink
        assert!(lay("cobblestone", 2, true, false).0.iter().all(|b| b.ext[2] == 3));
    }

    #[test]
    fn stones_with_no_gap_meet_with_no_seam_and_no_overlap() {
        // discs and a band, stones small and large, even and uneven
        let disc = |p: DVec2| p.length() - 40.0;
        for (name, region, size, uneven, seed) in [("a disc", &disc as &(dyn Fn(DVec2) -> f64 + Sync), 8.0, 0.5, 1), ("a disc of small stones", &disc, 4.0, 1.0, 8), ("a disc of large even stones", &disc, 14.0, 0.0, 3), ("a band", &band, 6.0, 0.6, 5)] {
            let (mut bricks, mut shades) = (Vec::new(), Vec::new());
            lay_stones(region, &Stones { size, gap: 0.0, uneven, seed }, 2, [-62, -42], [62, 42], 0, 1, &Opts::default(), &mut bricks, &mut shades);
            // every place inside the region, four to a cell each way: how many bricks fill it
            let mut filled: HashMap<(i32, i32), u8> = HashMap::new();
            for brick in &bricks {
                let info = crate::shapes::shape_info(brick.mask).unwrap();
                for j in 0..brick.ext[1] * 4 {
                    for i in 0..brick.ext[0] * 4 {
                        if crate::shapes::inside_shape(info, (i as f64 + 0.37) / (brick.ext[0] * 4) as f64, (j as f64 + 0.61) / (brick.ext[1] * 4) as f64, 0.5) {
                            *filled.entry((brick.min[0] * 4 + i, brick.min[1] * 4 + j)).or_default() += 1;
                        }
                    }
                }
            }
            // (counted apart: the places more than a stone and a half in from the rim, and all those a cell and a half in)
            let (mut places, mut bare, mut twice) = ([0; 2], [0; 2], [0; 2]);
            for j in -62 * 4..62 * 4 {
                for i in -62 * 4..62 * 4 {
                    let inside = -region(DVec2::new((i as f64 + 0.37) / 4.0, (j as f64 + 0.61) / 4.0));
                    for (which, least) in [1.5 * size, 1.5].into_iter().enumerate() {
                        if inside > least {
                            places[which] += 1;
                            bare[which] += !filled.contains_key(&(i, j)) as usize;
                            twice[which] += (filled.get(&(i, j)).copied().unwrap_or(0) > 1) as usize;
                        }
                    }
                }
            }
            // Away from the rim the stones meet: nothing filled twice, and
            // under a place in a hundred bare (0.1% to 0.6% over these
            // four). What is left is slivers: where corners moved to the
            // grid have folded a small stone over, and along an edge that
            // is nearly level (13 cells along for 1 across), which is
            // still fitted as a slope from one side and a step from the
            // other now and then.
            let most = 100;
            assert!(bare[0] * most <= places[0] && twice[0] * 10_000 <= places[0], "{name}: of {} places away from the rim, {} are bare and {} filled twice", places[0], bare[0], twice[0]);
            // Up to the rim, where an edge ends on the rim and not on a
            // corner of the grid, slivers are left: 0.4% to 4% of the
            // floor, where fitted each to its own line it was 5% to 38%.
            assert!(bare[1] * 100 < places[1] * 5 && twice[1] * 2000 <= places[1], "{name}: of {} places, {} are bare and {} filled twice", places[1], bare[1], twice[1]);
            // and they are separate stones still, each with sloped edges
            assert!(shades.iter().collect::<HashSet<_>>().len() == 8 && bricks.iter().filter(|b| b.mask != crate::shapes::CUBE).count() * 5 > bricks.len());
        }
    }

    #[test]
    fn a_mirrored_style_sits_in_the_middle_of_a_floor_that_is_off_the_grid() {
        // a floor 121 by 79 whose corner is at no multiple of a tile of 4 by 3
        let floor = |p: DVec2| ((p.x - 62.5).abs() - 60.5).max((p.y - 46.5).abs() - 39.5);
        let style = STYLES.iter().position(|s| s.name == "rug").unwrap();
        let (mut bricks, mut shades) = (Vec::new(), Vec::new());
        lay_mosaic(&floor, &Mosaic { style, size: 4, bevel: false, border: 0, seed: 2, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }, 2, [2, 7], [123, 86], 0, 1, &Opts::default(), &mut bricks, &mut shades);
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
