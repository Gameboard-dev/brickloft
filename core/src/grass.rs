//! Grass: a lawn laid out by rule, as furniture is, with no fitting.
//!
//! A lawn is a height field with things standing on it. The ground has one
//! height at each place, so every brick of it can be written down from
//! that height: the soil as a column under each cell, the water as the
//! cells between the bed and the water's level, a blade as a column a
//! cell wide with a sloped brick for its tip. [`Ground`] is the height
//! field, and what stands where on it; [`Model`] is the bricks.
//!
//! It was first built as a tree is ([`crate::tree`]): soil, water and
//! every blade were parts of one solid, and the general fitter sampled
//! that solid's distance function over every cell of its box. Measured on
//! a lawn 180 cells by 120 with 24 touches of the brush, that was 15.6
//! million samples for 63,500 bricks: 3.4 s on one thread and 0.53 s on
//! sixteen, of which filing the 27,800 parts took 0.13 s, the samples
//! 0.37 s and the colours 0.01 s. Each touch of the brush built all of it
//! again. Laid out, the same lawn is a few milliseconds (see
//! docs/performance.md), because nothing is searched for: a fitter is the
//! tool for a form whose bricks are not known beforehand, and a lawn's
//! are.
//!
//! A shader was the other way to make it faster, sampling the distance
//! function on the graphics card. It would have kept the fitting and all
//! its samples, and GLSL does not fix how its arithmetic rounds
//! (<https://registry.khronos.org/OpenGL/specs/es/3.0/GLSL_ES_Specification_3.00.pdf>,
//! section 4.5.1), so two cards could build different bricks from one
//! form. Not sampling at all is both faster and exact.
//!
//! The lawn is built in square tiles of ground, [`TILE`] cells a side,
//! each from nothing but the ground under it and a little way round it.
//! Tiles are shared out over the processor's cores and put end to end in
//! their own order ([`crate::par`]), so the bricks are the same on any
//! number of threads. And a touch of the brush changes only the bricks of
//! the tiles it reaches, which is what lets the editor give the graphics
//! card those tiles alone.

use crate::par::Threads;
use crate::shapes::{CellBrick, CUBE};
use crate::tree::{ground, hash, Lay, Run, Tree, BED, LEAF, LEAF_DARK, LEAF_LIGHT, LEAST_SPREAD, SAND, STONE, TRUNK, WATER};
use glam::{DVec2, DVec3};
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::OnceLock;

/// How many cells a tile of ground is along each side.
///
/// A touch of the brush is 8 to 50 cells across, so at 32 it reaches four
/// to nine tiles of a lawn's twenty or more, and the editor draws a tile
/// with one call for each brick shape in it. At 16 a touch reaches a
/// smaller share of the bricks and the lawn is four times as many calls
/// to draw; at 64 a lawn 120 across is four tiles and every touch reaches
/// most of them. A flat tile's soil is one brick, so smaller tiles are
/// also more bricks of soil.
pub const TILE: i32 = 32;

/// How far over the ground's own height the top of the soil is. The
/// blades' feet are a cell above the ground and stand a little into the
/// soil, as they always did (soil was balls reaching 1.6 over the ground).
const SOIL_OVER: f64 = 1.6;

/// How thick the skin of soil is, in cells, where the ground is level.
const SKIN: i32 = 2;

/// How far under a stream's water its bed is cut. A bed a cell and a half
/// down, which was tried when the soil was balls, was soil to the water's
/// very surface with water showing in the gaps.
const BED_UNDER: f64 = 2.8;

/// The chance that a cell of a pond's bed has a stone standing on it: one
/// in about a hundred. (It was 6 in 100 of the places of a grid 2.4
/// apart, which is the same number of stones.)
const STONES: f64 = 0.06 / (2.4 * 2.4);

/// The most cells a blade may be tall: its height is the plant's, which
/// may be set far beyond what grass is.
const TALLEST: i32 = 200;

/// What water a place is beside: its level, and when that water is a
/// stream's, how far the place is from the middle of it.
type Wet = (f64, Option<f64>);

/// Where water stands in a patch of grass that has two levels of it, and
/// the streams that run from the upper to the lower.
///
/// The rule. The ground is flooded from the edge of the patch inwards
/// (priority flood: Barnes, Lehman and Mulla 2014,
/// https://arxiv.org/abs/1511.04463), which gives every place the height
/// water would have to reach there before it ran off the patch: its spill
/// height. A hollow is the places lying under their spill height that
/// touch one another. Water stands in a hollow as far below its rim as it
/// stands below the lawn in a pond, so a hollow whose rim is on raised
/// ground holds its water higher than the lawn's ponds do. All such raised
/// hollows share one height, that of the lowest of them, so there are two
/// levels and no more, and none of them can overflow. Asked which pond is
/// at which level, the answer is read off the ground the user sculpted
/// and never typed.
///
/// A stream leaves each raised pond for the nearest water at the lower
/// level by the cheapest way over the ground (Dijkstra's), where a step
/// costs its length and three times as much again for each cell the
/// ground there stands above the upper water: it goes round a rise sooner
/// than through it. Its water starts at the upper level and at each cell
/// of its length is the lower of what it was and the ground there, in
/// whole cells: flat reaches with a step down between them, never a rise.
/// The alternative, water sloping with the ground, cannot be built of
/// bricks with level tops. A way that would have to climb over ground
/// higher than the pond's own rim is no way downhill, and that pond has no
/// stream.
///
/// The flood is on a grid of places 2.4 cells apart, which is the grid
/// the soil was laid on when it was fitted. It is kept, though the soil
/// is now laid cell by cell: on it a lawn saved before has the levels and
/// the streams it had, and a flood of every cell would be six times the
/// work for water whose level is a whole number either way.
struct Waters {
    lo: DVec3,
    step: f64,
    nx: usize,
    ny: usize,
    /// whether a place is in or beside a raised hollow
    high: Vec<bool>,
    upper: f64,
    lower: f64,
    /// the stream's place nearest each cell of the grid, if one is near
    near: Vec<u32>,
    /// the streams, a cell apart along their lengths: where, and how high the water is there
    stream: Vec<(DVec3, f64)>,
}

impl Waters {
    fn cell(&self, p: DVec3) -> Option<usize> {
        let (i, j) = (((p.x - self.lo.x) / self.step).round(), ((p.y - self.lo.y) / self.step).round());
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.nx && (j as usize) < self.ny).then(|| j as usize * self.nx + i as usize)
    }

    /// The stream beside a place: how far its middle, and its water's level there.
    fn beside(&self, p: DVec3) -> Option<(f64, f64)> {
        let k = *self.near.get(self.cell(p)?)? as usize;
        if k >= self.stream.len() {
            return None;
        }
        (k.saturating_sub(5)..(k + 6).min(self.stream.len())).map(|n| ((self.stream[n].0 - p).truncate().length(), self.stream[n].1)).min_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// The ground at a place with the streams' channels cut into it: a
    /// bed [`BED_UNDER`] the water, four and a half cells
    /// wide, and banks that rise to the ground as it was over the next
    /// two and a quarter, or over more where the cut is deep.
    fn ground(&self, p: DVec3, was: f64) -> f64 {
        match self.beside(p) {
            Some((far, level)) if was > level - BED_UNDER => {
                // (where it cuts deep, through the rim of the pond it leaves, the banks lie back further: upright, the soil of the two would close over the water)
                let t = ((far - 2.2) / (1.5 * (was - level + BED_UNDER)).max(2.25)).clamp(0.0, 1.0);
                level - BED_UNDER + (was - level + BED_UNDER) * t * t * (3.0 - 2.0 * t)
            }
            _ => was,
        }
    }

    /// The water a place is beside, given the ground there as it was sculpted.
    fn level(&self, p: DVec3, was: f64) -> Wet {
        let pond = if self.cell(p).is_some_and(|c| self.high[c]) { self.upper } else { self.lower };
        match self.beside(p) {
            // (in a pond, the pond's water)
            Some((far, level)) if far < 5.0 && was + 1.0 >= pond => (level, Some(far)),
            _ => (pond, None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn found(lay: &Lay, lower: f64, covered: &dyn Fn(DVec3) -> bool, lifted: &dyn Fn(DVec3) -> f64, lo: DVec3, hi: DVec3, step: f64, seed: u32) -> Waters {
        use std::{cmp::Reverse, collections::BinaryHeap};
        let mut waters = Waters { lo, step, nx: 0, ny: 0, high: Vec::new(), upper: lower, lower, near: Vec::new(), stream: Vec::new() };
        let (nx, ny) = (((hi.x - lo.x) / step).ceil() as usize + 1, ((hi.y - lo.y) / step).ceil() as usize + 1);
        // (a patch past six hundred cells square keeps one level: the flood is a grid of the whole patch in memory)
        if lay.levels < 2 || nx * ny > 400_000 {
            return waters;
        }
        let at = |c: usize| DVec3::new(lo.x + (c % nx) as f64 * step, lo.y + (c / nx) as f64 * step, 0.0);
        let inside: Vec<bool> = (0..nx * ny).map(|c| covered(at(c))).collect();
        let high: Vec<f64> = (0..nx * ny).map(|c| if inside[c] { lifted(at(c)) } else { 0.0 }).collect();
        let around = |c: usize| {
            let (i, j) = ((c % nx) as i64, (c / nx) as i64);
            [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)].into_iter().map(move |(di, dj)| (i + di, j + dj)).map(move |(i, j)| (i >= 0 && j >= 0 && i < nx as i64 && j < ny as i64).then(|| j as usize * nx + i as usize))
        };
        // (heights as whole numbers, a thousandth of a cell, for the heap to order)
        let key = |h: f64| (h * 1024.0).round() as i64;
        // the flood, from the edge of the patch inwards
        let mut fill = vec![f64::NAN; nx * ny];
        let mut heap = BinaryHeap::new();
        for c in (0..nx * ny).filter(|c| inside[*c]) {
            if around(c).any(|n| n.is_none_or(|n| !inside[n])) {
                fill[c] = high[c];
                heap.push(Reverse((key(high[c]), c)));
            }
        }
        while let Some(Reverse((_, c))) = heap.pop() {
            for n in around(c).flatten() {
                if inside[n] && fill[n].is_nan() {
                    fill[n] = high[n].max(fill[c]);
                    heap.push(Reverse((key(fill[n]), n)));
                }
            }
        }
        // the hollows, and of them the raised ones deep enough to hold water
        let (mut hollow, mut raised) = (vec![u32::MAX; nx * ny], Vec::<(f64, usize)>::new());
        let mut hollows = 0;
        for c in 0..nx * ny {
            if !inside[c] || hollow[c] != u32::MAX || fill[c] - high[c] <= 0.05 {
                continue;
            }
            let (mut stack, mut deep) = (vec![c], 0.0f64);
            hollow[c] = hollows;
            while let Some(c) = stack.pop() {
                deep = deep.max(fill[c] - high[c]);
                for n in around(c).flatten() {
                    if inside[n] && hollow[n] == u32::MAX && fill[n] - high[n] > 0.05 {
                        hollow[n] = hollows;
                        stack.push(n);
                    }
                }
            }
            let own = (fill[c] + lower).floor();
            if deep > 1.0 - lower && own > lower + 1.5 {
                raised.push((own, hollows as usize));
            }
            hollows += 1;
        }
        waters.nx = nx;
        waters.ny = ny;
        waters.high = vec![false; nx * ny];
        waters.near = vec![u32::MAX; nx * ny];
        if raised.is_empty() {
            return waters;
        }
        waters.upper = raised.iter().map(|r| r.0).fold(f64::MAX, f64::min);
        let upper = waters.upper;
        // each raised hollow and its banks, three cells of the grid out but not over its rim and down the far side
        let mut edge: Vec<usize> = (0..nx * ny).filter(|c| hollow[*c] != u32::MAX && raised.iter().any(|r| r.1 == hollow[*c] as usize)).collect();
        for c in &edge {
            waters.high[*c] = true;
        }
        for _ in 0..3 {
            let mut next = Vec::new();
            for c in edge {
                for n in around(c).flatten() {
                    if inside[n] && !waters.high[n] && fill[n] >= fill[c] - 0.05 {
                        waters.high[n] = true;
                        fill[n] = fill[n].min(fill[c] + 0.04);
                        next.push(n);
                    }
                }
            }
            edge = next;
        }
        let pond = |c: usize| inside[c] && !waters.high[c] && high[c] + 1.0 < lower;
        // the streams: at most eight, each a search of the whole grid
        let mut near = vec![f32::MAX; nx * ny];
        for &(_, which) in raised.iter().take(8) {
            let (mut cost, mut from) = (vec![f64::MAX; nx * ny], vec![u32::MAX; nx * ny]);
            let mut heap = BinaryHeap::new();
            let mut rim = f64::MIN;
            for c in (0..nx * ny).filter(|c| hollow[*c] == which as u32) {
                rim = rim.max(fill[c]);
                if high[c] + 1.0 < upper {
                    cost[c] = 0.0;
                    heap.push(Reverse((0i64, c)));
                }
            }
            let mut end = None;
            while let Some(Reverse((k, c))) = heap.pop() {
                if k > key(cost[c]) {
                    continue;
                }
                if pond(c) {
                    end = Some(c);
                    break;
                }
                for n in around(c).flatten().filter(|n| inside[*n]) {
                    let long = if n % nx != c % nx && n / nx != c / nx { step * std::f64::consts::SQRT_2 } else { step };
                    let to = cost[c] + long * (1.0 + 3.0 * (high[n] - upper).max(0.0));
                    if key(to) < key(cost[n]) {
                        cost[n] = to;
                        from[n] = c as u32;
                        heap.push(Reverse((key(to), n)));
                    }
                }
            }
            let Some(mut c) = end else { continue };
            let mut way = vec![at(c)];
            let mut top = high[c];
            while from[c] != u32::MAX {
                c = from[c] as usize;
                top = top.max(high[c]);
                way.push(at(c));
            }
            // no way that does not climb over more than its own rim: no stream
            if top > rim + 1.0 || way.len() < 2 {
                continue;
            }
            way.reverse();
            for _ in 0..2 {
                let was = way.clone();
                for i in 1..way.len() - 1 {
                    way[i] = (was[i - 1] + was[i] * 2.0 + was[i + 1]) / 4.0;
                }
            }
            // along it a cell at a time, wandering to each side by the seed, most in its middle and not at all at its ends
            let lengths: Vec<f64> = std::iter::once(0.0).chain(way.windows(2).scan(0.0, |sum, w| { *sum += (w[1] - w[0]).length(); Some(*sum) })).collect();
            let long = *lengths.last().unwrap();
            let (phase, swing) = (hash(which as i32, 0, 9, seed) * TAU, (long / 10.0).min(2.4));
            let mut leg = 0;
            let mut places: Vec<(DVec3, f64)> = Vec::new();
            for n in 0..=long.floor() as usize {
                let s = (n as f64).min(long);
                while leg + 2 < way.len() && lengths[leg + 1] < s {
                    leg += 1;
                }
                let (a, b) = (way[leg], way[leg + 1]);
                let along = (b - a).normalize_or_zero();
                let across = DVec3::new(-along.y, along.x, 0.0);
                let p = a.lerp(b, ((s - lengths[leg]) / (lengths[leg + 1] - lengths[leg]).max(1e-9)).clamp(0.0, 1.0)) + across * swing * (std::f64::consts::PI * s / long).sin() * (phase + s * TAU / 28.0).sin();
                // (under the lower of its two banks, so that it stands above neither)
                let bank = [-4.4, -2.2, 0.0, 2.2, 4.4].iter().map(|side| lifted(p + across * *side)).fold(f64::MAX, f64::min);
                places.push((p, bank));
            }
            let mut level = upper;
            for (n, &(p, _)) in places.iter().enumerate() {
                // (Still at the pond's own level until it is out through the rim. From there, under its banks here and for three cells on: a cell of water takes the level of the place on the stream nearest it, which may be a little upstream of it.)
                if !waters.cell(p).is_some_and(|c| waters.high[c]) {
                    let bank = places[n..(n + 4).min(places.len())].iter().map(|place| place.1).fold(f64::MAX, f64::min);
                    level = level.min((bank - 0.5).floor()).max(lower);
                }
                let k = waters.stream.len() as u32;
                waters.stream.push((p, level));
                let reach = (16.0 / step).ceil() as i64;
                let (ci, cj) = (((p.x - lo.x) / step).round() as i64, ((p.y - lo.y) / step).round() as i64);
                for j in (cj - reach).max(0)..=(cj + reach).min(ny as i64 - 1) {
                    for i in (ci - reach).max(0)..=(ci + reach).min(nx as i64 - 1) {
                        let c = j as usize * nx + i as usize;
                        let far = (at(c) - p).length() as f32;
                        if far < 16.0 && far < near[c] {
                            near[c] = far;
                            waters.near[c] = k;
                        }
                    }
                }
            }
            // (a gap in the numbering, so that one stream's places are not taken for the next's)
            for _ in 0..12 {
                waters.stream.push((DVec3::splat(f64::MAX / 4.0), lower));
            }
        }
        waters
    }
}

/// What is at one cell of a lawn's ground (see [`Ground::at`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// whether the lawn covers the cell: outside it there is no brick, though the ground still has a height there
    pub covered: bool,
    /// how high the ground is over the floor the lawn lies on, with the streams' channels cut into it
    pub height: f64,
    /// the cell the top of the soil is the floor of: soil fills the cells under it
    pub top: i32,
    /// the top of the water standing here, as a cell's floor; `None` on dry ground
    pub water: Option<i32>,
    /// whether the water here, or nearest here, is a stream's and not a pond's
    pub stream: bool,
    /// which part the soil's skin is here: [`TRUNK`] for soil, [`SAND`] or [`BED`]
    pub skin: u8,
}

/// water stands in the cell
const WET: u8 = 1;
/// the water beside the cell is a stream's
const STREAM: u8 = 2;
/// a reed stands in the cell or over it
const REED: u8 = 4;
/// the lawn covers the cell
const COVERED: u8 = 8;

/// The ground under a patch of grass: for each cell of it, how high it
/// is, what its skin is, and what water stands on it.
///
/// Everything else about a lawn is laid on this, and whatever wants to
/// know the lie of the land asks here: [`Ground::at`] and
/// [`Ground::slope`] read one cell of a table. It is made without the
/// bricks, in a millisecond or two, so a rule that places things by the
/// ground (a cliff where it is steep, a waterfall where a stream steps
/// down) need not build the lawn to find out where.
///
/// Cells are the world's: cell `(x, y)` is the square from `(x, y)` to
/// `(x + 1, y + 1)`, as a brick's are.
pub struct Ground {
    /// where the plant's foot is, on the floor of its cell: the lawn's own places are measured from here
    stand: DVec3,
    /// the first cell of the table each way, and how many
    lo: [i32; 2],
    n: [usize; 2],
    high: Vec<f64>,
    top: Vec<i32>,
    /// the level of the water the place is beside, as a cell's floor; unused where the lawn has no water
    level: Vec<i32>,
    skin: Vec<u8>,
    flags: Vec<u8>,
    /// the size of the stone whose middle is here, 0 for none
    stone: Vec<f32>,
    /// whether the lawn has water at all
    watered: bool,
}

impl Ground {
    /// The ground of the grass `lay` describes, standing at `base` and
    /// reaching `spread` to each side of its line.
    pub fn new(tree: &Tree, base: DVec3, spread: f64, lay: &Lay, threads: Threads) -> Ground {
        let spread = spread.max(LEAST_SPREAD);
        let stand = DVec3::new(base.x, base.y, base.z.floor());
        let run = Run::new(lay);
        let covered = |p: DVec3| run.distance(p) <= spread || (lay.closed && run.holds(p));
        let sculpted = |p: DVec3| ground(&lay.mounds, p);
        let seed = seed_of(tree);
        let (low, high) = run.points.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p - spread), hi.max(*p + spread)));
        let waters = lay.water.map(|lower| Waters::found(lay, lower, &covered, &sculpted, low, high, 2.4, seed));
        // (one cell more each way than the lawn reaches: a column of soil goes down to meet its neighbours, which it must know the height of)
        let lo = [(stand.x + low.x).floor() as i32 - 1, (stand.y + low.y).floor() as i32 - 1];
        let n = [((stand.x + high.x).ceil() as i32 + 1 - lo[0]).max(1) as usize, ((stand.y + high.y).ceil() as i32 + 1 - lo[1]).max(1) as usize];
        let floor = stand.z as i32;
        // (the cell the foot is in: the numbers a cell's stones are thrown by go by where it is from there, so a lawn moved is the same lawn)
        let foot = [stand.x.floor() as i32, stand.y.floor() as i32];
        // Each row on its own: a cell's ground depends on nothing but
        // where it is. Shared out because the touches are many: every
        // cell asks every touch how far off it is.
        let rows: Vec<Vec<(f64, i32, i32, u8, u8, f32)>> = threads.map(n[1], 16, |j| {
            (0..n[0])
                .map(|i| {
                    let (x, y) = (lo[0] + i as i32, lo[1] + j as i32);
                    let p = DVec3::new(x as f64 + 0.5 - stand.x, y as f64 + 0.5 - stand.y, 0.0);
                    let was = sculpted(p);
                    let here = waters.as_ref().map_or(was, |w| w.ground(p, was));
                    let top = floor + (here + SOIL_OVER).round() as i32;
                    let mut flags = if covered(p) { COVERED } else { 0 };
                    let Some((level, by)) = waters.as_ref().map(|w| w.level(p, was)) else { return (here, top, 0, TRUNK, flags, 0.0) };
                    let surface = floor + level.round() as i32;
                    if by.is_some() {
                        flags |= STREAM;
                    }
                    // (a stream's water only in its channel, and not wherever the ground beside it happens to lie lower)
                    let wet = flags & COVERED != 0 && here + 1.0 < level && by.is_none_or(|far| far < 3.0) && top < surface;
                    // By the water it is sand, and under it the bed of the pond.
                    // (A stream's bed is sand and stones all through: it runs too fast for mud to settle.
                    // And the bed is only where water lies on it: the soil's top is a whole cell, so at
                    // the very edge it may come up to the surface, and a rim of mud round the sand is no bank.)
                    let skin = if wet && by.is_none() && here <= level - 1.5 {
                        BED
                    } else if here <= level + 1.2 {
                        SAND
                    } else {
                        TRUNK
                    };
                    if !wet {
                        return (here, top, surface, skin, flags, 0.0);
                    }
                    // and here and there a stone on the bed
                    let chance = hash(x - foot[0], y - foot[1], 3, seed);
                    let stone = if here <= level - 1.5 && chance < STONES { 1.4 + 1.08 * chance / STONES } else { 0.0 };
                    (here, top, surface, skin, flags | WET, stone as f32)
                })
                .collect()
        });
        let cells = n[0] * n[1];
        let mut made = Ground { stand, lo, n, high: Vec::with_capacity(cells), top: Vec::with_capacity(cells), level: Vec::with_capacity(cells), skin: Vec::with_capacity(cells), flags: Vec::with_capacity(cells), stone: Vec::with_capacity(cells), watered: waters.is_some() };
        for (here, top, level, skin, flags, stone) in rows.into_iter().flatten() {
            made.high.push(here);
            made.top.push(top);
            made.level.push(level);
            made.skin.push(skin);
            made.flags.push(flags);
            made.stone.push(stone);
        }
        made
    }

    /// The cells the table covers: the first each way, and one past the last.
    pub fn cells(&self) -> ([i32; 2], [i32; 2]) {
        (self.lo, [self.lo[0] + self.n[0] as i32, self.lo[1] + self.n[1] as i32])
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let (i, j) = (x - self.lo[0], y - self.lo[1]);
        (i >= 0 && j >= 0 && (i as usize) < self.n[0] && (j as usize) < self.n[1]).then(|| j as usize * self.n[0] + i as usize)
    }

    /// What is at cell `(x, y)`; `None` beyond the table, which is a cell past the lawn each way.
    pub fn at(&self, x: i32, y: i32) -> Option<Place> {
        let c = self.index(x, y)?;
        let flags = self.flags[c];
        Some(Place { covered: flags & COVERED != 0, height: self.high[c], top: self.top[c], water: (flags & WET != 0).then_some(self.level[c]), stream: flags & STREAM != 0, skin: self.skin[c] })
    }

    /// How steeply the ground rises at cell `(x, y)`, and which way: the
    /// rise in cells for each cell gone along x and along y, by the
    /// heights of the cells to each side. `None` at the table's edge.
    /// https://en.wikipedia.org/wiki/Finite_difference#Basic_types
    pub fn slope(&self, x: i32, y: i32) -> Option<DVec2> {
        let h = |x: i32, y: i32| self.index(x, y).map(|c| self.high[c]);
        Some(DVec2::new(h(x + 1, y)? - h(x - 1, y)?, h(x, y + 1)? - h(x, y - 1)?) / 2.0)
    }

    /// The level water stands at beside cell `c`, if the lawn has water.
    fn beside(&self, c: usize) -> Option<i32> {
        self.watered.then(|| self.level[c])
    }
}

/// The numbers a lawn's blades, stones and reeds are thrown by.
fn seed_of(tree: &Tree) -> u32 {
    tree.seed as u32 ^ 0x9A55
}

/// A direction, any at all, evenly, from two numbers of 0 to 1.
fn any_way(u: f64, v: f64) -> DVec3 {
    let (z, turn) = (2.0 * u - 1.0, TAU * v);
    let flat = (1.0 - z * z).max(0.0).sqrt();
    DVec3::new(flat * turn.cos(), flat * turn.sin(), z)
}

/// A step of one cell the way `d` mostly points, along x or y.
fn step_of(d: DVec2) -> [i32; 2] {
    if d.x.abs() >= d.y.abs() {
        [if d.x >= 0.0 { 1 } else { -1 }, 0]
    } else {
        [0, if d.y >= 0.0 { 1 } else { -1 }]
    }
}

/// A sloped brick whose top falls away from the side `towards` is on:
/// a box less its top edge on the far side. A blade's tip, pointing the
/// way the blade leans.
fn tip(towards: [i32; 2]) -> u8 {
    // (a corner is the bit x | y << 1 | z << 2: see `crate::shapes`)
    match towards {
        [1, 0] => !(1 << 4 | 1 << 6),
        [-1, 0] => !(1 << 5 | 1 << 7),
        [0, 1] => !(1 << 4 | 1 << 5),
        _ => !(1 << 6 | 1 << 7),
    }
}

/// What every tile of a lawn is laid by, besides the ground.
struct Rule<'a> {
    ground: &'a Ground,
    seed: u32,
    /// how tall the blades are, and how far apart the tufts
    height: f64,
    gap: f64,
    /// how far they lean with the wind, 0 to 1, and which way it blows
    lean: f64,
    downwind: DVec3,
    /// how thickly reeds stand round the water, 0 to 1
    reeds: f64,
    /// the longest a brick may be
    longest: i32,
    /// for each cell with [`REED`], the heights a reed fills over it: from each first to before each second
    stems: &'a HashMap<[i32; 2], Vec<(i32, i32)>>,
}

/// One tuft of blades, or the reed that stands in its place.
struct Tuft {
    /// where it stands, from the lawn's foot
    at: DVec2,
    /// the cell that is in
    cell: [i32; 2],
    /// the tuft's own numbers on the lattice of tufts
    a: i32,
    b: i32,
}

impl Rule<'_> {
    /// The tufts whose feet are in the cells from `lo` to before `hi`, in
    /// the order they are always taken in: by rows of the lattice.
    ///
    /// The tufts are on a grid shaken so that no rows show. Each is
    /// placed by numbers of its own, worked out from where it is on that
    /// grid, and not drawn in turn from one run of numbers as they were:
    /// a run gives a tuft a different place whenever one before it
    /// changes, and a tile could not be laid without laying every tile
    /// before it.
    fn tufts(&self, lo: [i32; 2], hi: [i32; 2]) -> Vec<Tuft> {
        let g = self.ground;
        // (a tuft is shaken by under half a gap each way)
        let span = |k: usize| (((lo[k] as f64 - g.stand[k]) / self.gap - 0.5).floor() as i32, ((hi[k] as f64 - g.stand[k]) / self.gap + 0.5).ceil() as i32);
        let ((a0, a1), (b0, b1)) = (span(0), span(1));
        let mut all = Vec::new();
        for b in b0..=b1 {
            for a in a0..=a1 {
                let at = DVec2::new((a as f64 + 0.9 * (hash(a, b, 10, self.seed) - 0.5)) * self.gap, (b as f64 + 0.9 * (hash(a, b, 11, self.seed) - 0.5)) * self.gap);
                let cell = [(g.stand.x + at.x).floor() as i32, (g.stand.y + at.y).floor() as i32];
                if cell[0] >= lo[0] && cell[0] < hi[0] && cell[1] >= lo[1] && cell[1] < hi[1] {
                    all.push(Tuft { at, cell, a, b });
                }
            }
        }
        all
    }

    /// Whether a reed stands where a tuft would, by the water: in the
    /// shallows and on the bank, by a throw of its own for each place.
    fn reed_at(&self, tuft: &Tuft) -> bool {
        let g = self.ground;
        let Some(c) = g.index(tuft.cell[0], tuft.cell[1]).filter(|c| g.flags[*c] & COVERED != 0) else { return false };
        let Some(level) = g.beside(c) else { return false };
        // (a third as many along a stream as round a pond)
        let thick = self.reeds * if g.flags[c] & STREAM != 0 { 0.05 } else { 0.14 };
        (g.high[c] - (level - g.stand.z as i32) as f64).abs() < 2.6 && hash(tuft.a, tuft.b, 5, self.seed) < thick
    }

    /// The bricks of the reed that stands at a tuft: those it cannot be
    /// without, then those it can.
    ///
    /// A bulrush: a thin stem with a fat brown head two thirds of the way
    /// up it, the stem's tip standing out of the head's top, and one long
    /// leaf from its foot that arches away. The stem is a column a cell
    /// wide that leans by one cell half way up. The head is three cells
    /// across and eight-sided, its four corners sloped bricks standing
    /// upright, and it narrows by a slope at each end.
    ///
    /// Reeds were round cones fitted as a tree's limbs are, two cells
    /// through with a head three through. Laid out, the stem is one cell,
    /// since a head of three stands evenly on a stem of one and cannot on
    /// a stem of two; a bulrush's head is several times its stem's
    /// thickness, so this is the nearer of the two to the plant.
    fn reed(&self, tuft: &Tuft) -> (Vec<(CellBrick, u8)>, Vec<(CellBrick, u8)>) {
        let g = self.ground;
        let (mut needed, mut extra) = (Vec::new(), Vec::new());
        let Some(c) = g.index(tuft.cell[0], tuft.cell[1]) else { return (needed, extra) };
        let (x, y, foot) = (tuft.cell[0], tuft.cell[1], g.top[c]);
        let number = |n: i32| hash(tuft.a, tuft.b, n, self.seed);
        let tall = ((14.0 + 12.0 * number(6) + self.height).round() as i32).max(8);
        let up = |share: f64| foot + (tall as f64 * share).round() as i32;
        let block = |at: [i32; 2], from: i32, to: i32, mask: u8| CellBrick { mask, min: [at[0], at[1], from], ext: [1, 1, to - from] };
        // the stem, leaning one cell (or, one time in four, none)
        let lean = if number(7) < 0.25 { [0, 0] } else { step_of(DVec2::from_angle(TAU * number(8))) };
        let (half, low, high) = (up(0.45), up(0.66), up(0.86));
        let over = [x + lean[0], y + lean[1]];
        if lean == [0, 0] {
            needed.push((block(over, foot, low, CUBE), LEAF_DARK));
        } else {
            needed.push((block([x, y], foot, half, CUBE), LEAF_DARK));
            needed.push((block(over, half - 1, low, CUBE), LEAF_DARK));
        }
        needed.push((block(over, low, high, CUBE), BED));
        needed.push((block(over, high, foot + tall, CUBE), LEAF_DARK));
        // the head round it: its four sides, each sloped in at its two ends, and its four corners
        for side in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
            let at = [over[0] + side[0], over[1] + side[1]];
            // (a slope at each end: the box less its outer edge there, below and above)
            let outer = |z: u8| (0..8u8).filter(|corner| !((corner >> 2 & 1) == z && [corner & 1, corner >> 1 & 1].iter().zip(side).any(|(bit, way)| way != 0 && (*bit == 1) == (way > 0)))).fold(0u8, |mask, corner| mask | 1 << corner);
            needed.push((block(at, low, low + 1, outer(0)), BED));
            needed.push((block(at, low + 1, high - 1, CUBE), BED));
            needed.push((block(at, high - 1, high, outer(1)), BED));
        }
        for corner in [[1, 1], [-1, 1], [1, -1], [-1, -1]] {
            // (upright, less the edge furthest from the stem)
            let mask = (0..8u8).filter(|bit| !(((bit & 1) == 1) == (corner[0] > 0) && ((bit >> 1 & 1) == 1) == (corner[1] > 0))).fold(0u8, |mask, bit| mask | 1 << bit);
            needed.push((block([over[0] + corner[0], over[1] + corner[1]], low + 1, high - 1, mask), BED));
        }
        // the leaf: up beside the stem, then out and up in two steps, to a point
        let out = step_of(DVec2::from_angle(TAU * number(9)));
        let leaf = |far: i32| [x + out[0] * far, y + out[1] * far];
        let (knee, bend, end) = (up(0.4), up(0.52), up(0.6));
        // (from the ground of the cell it stands on, where that is higher than the stem's)
        let beside = g.index(leaf(1)[0], leaf(1)[1]).map_or(foot, |n| g.top[n].max(foot));
        extra.push((block(leaf(1), beside, knee, CUBE), LEAF));
        extra.push((block(leaf(2), knee - 1, bend, CUBE), LEAF));
        extra.push((block(leaf(3), bend - 1, end - 1, CUBE), LEAF));
        extra.push((block(leaf(3), end - 1, end, tip(out)), LEAF));
        (needed, extra)
    }

    /// Every reed of the lawn, and for each cell one stands in or over,
    /// the heights it fills there.
    ///
    /// All of them at once and before any tile, where everything else is
    /// laid tile by tile: a reed is a dozen cells across with its leaf
    /// and belongs to no one tile, and they are few. They are taken in
    /// the tufts' order, and one that would stand in another already
    /// there, or in the bank beside it, is left out: reeds stand a little
    /// apart, as plants do.
    fn reeds(&self) -> (Vec<(CellBrick, u8)>, HashMap<[i32; 2], Vec<(i32, i32)>>) {
        let g = self.ground;
        let (mut bricks, mut stems): (Vec<(CellBrick, u8)>, HashMap<[i32; 2], Vec<(i32, i32)>>) = (Vec::new(), HashMap::new());
        if self.reeds <= 0.0 || !g.watered {
            return (bricks, stems);
        }
        let (lo, hi) = g.cells();
        // whether a brick stands clear of the ground and of every reed so far
        let clear = |b: &CellBrick, stems: &HashMap<[i32; 2], Vec<(i32, i32)>>| b.ext[2] > 0 && g.index(b.min[0], b.min[1]).is_none_or(|c| b.min[2] >= g.top[c]) && stems.get(&[b.min[0], b.min[1]]).is_none_or(|taken| taken.iter().all(|(from, to)| b.min[2] >= *to || b.min[2] + b.ext[2] <= *from));
        for tuft in self.tufts(lo, hi).iter().filter(|tuft| self.reed_at(tuft)) {
            let (needed, extra) = self.reed(tuft);
            if needed.is_empty() || !needed.iter().all(|(b, _)| clear(b, &stems)) {
                continue;
            }
            for (b, shade) in needed.into_iter().chain(extra) {
                if clear(&b, &stems) {
                    stems.entry([b.min[0], b.min[1]]).or_default().push((b.min[2], b.min[2] + b.ext[2]));
                    bricks.push((b, shade));
                }
            }
        }
        (bricks, stems)
    }

    /// The heights from `from` to before `to` over cell `at` that no reed fills.
    fn free_of_reeds(&self, at: [i32; 2], from: i32, to: i32) -> Vec<(i32, i32)> {
        let mut left = vec![(from, to)];
        for &(low, high) in self.stems.get(&at).into_iter().flatten() {
            left = left.into_iter().flat_map(|(a, b)| [(a, b.min(low)), (a.max(high), b)]).filter(|(a, b)| a < b).collect();
        }
        left
    }

    /// The bricks of the tile whose first cell is `lo`: soil and stones,
    /// joined into the largest boxes, and the blades standing on them. And
    /// where water stands in it: over which cell, from which height to
    /// which (see [`pooled`] for why the water is not bricks yet).
    ///
    /// From the ground of its own cells and of those a little way round
    /// (a cell for the soil, three for stones, a blade's reach for
    /// blades) and from nothing else, so a tile is the same bricks
    /// whatever the tiles beside it are, and whichever thread lays it.
    fn tile(&self, lo: [i32; 2]) -> (Vec<(CellBrick, u8)>, Vec<Standing>) {
        let g = self.ground;
        let hi = [lo[0] + TILE, lo[1] + TILE];
        let side = TILE as usize;
        let slot = |x: i32, y: i32| (y - lo[1]) as usize * side + (x - lo[0]) as usize;
        // how high the stones stand over each cell: each is a low dome over the cells round its middle
        let mut stones = vec![i32::MIN; side * side];
        for y in lo[1] - 3..hi[1] + 3 {
            for x in lo[0] - 3..hi[0] + 3 {
                let Some(c) = g.index(x, y).filter(|c| g.stone[*c] > 0.0) else { continue };
                // (In a stream it is a pebble, one cell: a stream's water is a cell deep
                // and a few wide, and a stone the size of a pond's dammed it.)
                let (r, far) = if g.flags[c] & STREAM != 0 { (1.4, 0) } else { (g.stone[c] as f64, 3) };
                for (dx, dy) in (-far..=far).flat_map(|dy| (-far..=far).map(move |dx| (dx, dy))) {
                    let (x, y) = (x + dx, y + dy);
                    // (a little longer one way than the other, and lower than it is wide)
                    let q = (dx as f64 / r).powi(2) + (dy as f64 / (0.85 * r)).powi(2);
                    if q < 1.0 && x >= lo[0] && x < hi[0] && y >= lo[1] && y < hi[1] {
                        let top = g.top[c] + (1.1 * r * (1.0 - q).sqrt() - 0.6).round() as i32;
                        stones[slot(x, y)] = stones[slot(x, y)].max(top);
                    }
                }
            }
        }
        // the soil under each cell, a stone on it, and water over that
        let mut solid: Vec<(CellBrick, u8)> = Vec::new();
        let mut water: Vec<Standing> = Vec::new();
        // where blades may be over each cell: from here up
        let mut floor = vec![i32::MAX; side * side];
        for y in lo[1]..hi[1] {
            for x in lo[0]..hi[0] {
                let Some(c) = g.index(x, y).filter(|c| g.flags[*c] & COVERED != 0) else { continue };
                let top = g.top[c];
                // (on a slope, soil down to the ground beside it as well, so that the skin has no tear in it)
                let lowest = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().filter_map(|(dx, dy)| g.index(x + dx, y + dy)).map(|n| g.top[n]).fold(top, i32::min);
                let under = (top - SKIN).min(lowest - 1);
                solid.push((CellBrick { mask: CUBE, min: [x, y, under], ext: [1, 1, top - under] }, g.skin[c]));
                let wet = g.flags[c] & WET != 0;
                let reeded = g.flags[c] & REED != 0;
                // (a stone only in water, and never taller than three cells whatever it lies on)
                let stone = if wet { stones[slot(x, y)].clamp(top, top + 3) } else { top };
                let surface = if wet { g.level[c].max(stone) } else { stone };
                for (what, from, to) in [(STONE, top, stone), (WATER, stone, surface)] {
                    // (a reed standing here has its own cells, and nothing else is laid in them)
                    for (from, to) in if reeded { self.free_of_reeds([x, y], from, to) } else { vec![(from, to)] }.into_iter().filter(|(from, to)| from < to) {
                        if what == WATER {
                            water.push(([x, y], from, to));
                        } else {
                            solid.push((CellBrick { mask: CUBE, min: [x, y, from], ext: [1, 1, to - from] }, what));
                        }
                    }
                }
                floor[slot(x, y)] = surface;
            }
        }
        let mut bricks = crate::pattern::joined(solid, self.longest.max(1));
        self.blades(lo, &floor, &mut bricks);
        (bricks, water)
    }

    /// The blades standing in the tile whose first cell is `lo`, added to
    /// `bricks`; `floor` is the height blades may be from over each cell
    /// of it, and `i32::MAX` where there is no lawn.
    ///
    /// The blades are in tufts, two to five to a tuft, each on a cell of
    /// its own beside the tuft's middle, each of its own height and lean
    /// and of one of three greens; all lean a little one way, as grass
    /// does. A blade is a column one cell wide, the thinnest that is
    /// built, with a sloped brick for its tip. Where it leans far enough
    /// to pass over the next cell it goes on there, a step sideways, its
    /// two lengths sharing one cell's height so that they meet face to
    /// face.
    ///
    /// When blades were fitted they were round ribs two lengths long and
    /// nearly two cells through, and what the fitter made of each was a
    /// column with sloped bricks about it. The column and its tip are
    /// that written down. A blade is now half as thick, so a tuft has one
    /// more of them, each on its own cell, to cover the ground as before.
    ///
    /// Two blades may want one cell. The first to ask has it, and the
    /// tufts are always asked in the same order, so a cell has the same
    /// blade whichever tile is laying it. A blade stops under another's
    /// tip and never stands on one.
    fn blades(&self, lo: [i32; 2], floor: &[i32], bricks: &mut Vec<(CellBrick, u8)>) {
        let g = self.ground;
        let side = TILE as usize;
        let hi = [lo[0] + TILE, lo[1] + TILE];
        let (lowest, highest) = floor.iter().filter(|f| **f != i32::MAX).fold((i32::MAX, i32::MIN), |(low, high), f| (low.min(*f), high.max(*f)));
        if lowest > highest {
            return;
        }
        let tallest = (self.height.ceil() as i32 + 1).min(TALLEST);
        let deep = (highest - lowest + tallest + 1) as usize;
        // what has each cell over the tile, from `lowest` up: 0 for nothing, a green, or `TIP` for the sloped brick of a tip (and all above one)
        const TIP: u8 = 255;
        let mut held = vec![0u8; side * side * deep];
        let mut tips: Vec<(CellBrick, u8)> = Vec::new();
        // (how far a blade may reach sideways: it leans under 40 degrees from upright)
        let reach = (tallest as f64 * 0.8).ceil() as i32 + 2;
        for tuft in self.tufts([lo[0] - reach, lo[1] - reach], [hi[0] + reach, hi[1] + reach]) {
            let Some(c) = g.index(tuft.cell[0], tuft.cell[1]).filter(|c| g.flags[*c] & COVERED != 0) else { continue };
            let number = |n: i32| hash(tuft.a, tuft.b, n, self.seed);
            let here = g.high[c];
            if let Some(level) = g.beside(c) {
                if self.reed_at(&tuft) {
                    continue;
                }
                // the grass thins out towards the water and stops at its edge
                let level = (level - g.stand.z as i32) as f64;
                if here <= level + 0.3 || number(6) > (here - level - 0.3) / 1.7 {
                    continue;
                }
            }
            for blade in 0..3 + (number(12) * 2.4) as i32 {
                let number = |n: i32| number(16 + blade * 8 + n);
                // (the first on the tuft's own cell, the rest on the cells round it)
                let from = if blade == 0 { tuft.at } else { tuft.at + DVec2::new(number(6) - 0.5, number(7) - 0.5) * 2.6 };
                let cell = [(g.stand.x + from.x).floor() as i32, (g.stand.y + from.y).floor() as i32];
                // (it stands on the ground of its own cell, which is lawn and not under water)
                let Some(base) = g.index(cell[0], cell[1]).filter(|n| g.flags[*n] & (COVERED | WET) == COVERED).map(|n| g.top[n]) else { continue };
                let (first, second, long, tone) = (any_way(number(0), number(1)), any_way(number(2), number(3)), 0.5 + 0.5 * number(4), number(5));
                let up = (DVec3::Z + self.downwind * 0.45 * self.lean + first * 0.3).normalize();
                let over = (up + self.downwind * 0.5 * self.lean + second * 0.25).normalize();
                // its two lengths: how far up each goes, from a foot a little way into the soil
                let (bent, end) = (up.z * self.height * long * 0.55 - (SOIL_OVER - 1.0), over.z.max(0.2) * self.height * long * 0.45);
                let cells = ((bent + end).round() as i32).clamp(1, tallest);
                let shade = if tone < 0.5 { LEAF } else if tone < 0.78 { LEAF_DARK } else { LEAF_LIGHT };
                // the cell its middle is in at each height, going up
                let column = |k: i32| {
                    let z = k as f64 + 0.5;
                    let out = if z <= bent { up.truncate() * ((z + SOIL_OVER - 1.0) / up.z) } else { up.truncate() * ((bent + SOIL_OVER - 1.0) / up.z) + over.truncate() * ((z - bent) / over.z.max(0.2)) };
                    [(g.stand.x + from.x + out.x).floor() as i32, (g.stand.y + from.y + out.y).floor() as i32]
                };
                let towards = step_of((up.truncate() + over.truncate()) * 0.5 + self.downwind.truncate() * 1e-3);
                // its lengths, each over one cell: from which height to which
                let mut k = 0;
                while k < cells {
                    let over_cell = column(k);
                    let mut to = k + 1;
                    while to < cells && column(to) == over_cell {
                        to += 1;
                    }
                    // (a length after the first begins beside the top cell of the one before, to meet it face to face)
                    let from_k = if k == 0 { 0 } else { k - 1 };
                    let last = to == cells;
                    // the tip: the top two fifths of the blade, or what of its last length there is
                    let tip_cells = if last { ((cells as f64 * 0.4).round() as i32).clamp(1, to - from_k) } else { 0 };
                    if over_cell[0] >= lo[0] && over_cell[0] < hi[0] && over_cell[1] >= lo[1] && over_cell[1] < hi[1] {
                        let at = (over_cell[1] - lo[1]) as usize * side + (over_cell[0] - lo[0]) as usize;
                        let reeds = g.index(over_cell[0], over_cell[1]).filter(|n| g.flags[*n] & REED != 0).and_then(|_| self.stems.get(&over_cell));
                        // whether the cell `z` up is free to take, and its place in `held`
                        let free = |z: i32, held: &[u8]| {
                            let within = floor[at] != i32::MAX && z >= floor[at] && z - lowest < deep as i32;
                            (within && held[at * deep + (z - lowest) as usize] == 0 && reeds.is_none_or(|taken| taken.iter().all(|(a, b)| z < *a || z >= *b))).then(|| at * deep + (z - lowest) as usize)
                        };
                        let (low, top) = (base + from_k, base + to);
                        for z in low..top - tip_cells {
                            if let Some(n) = free(z, &held) {
                                held[n] = shade;
                            }
                        }
                        if tip_cells > 0 {
                            if (top - tip_cells..top).all(|z| free(z, &held).is_some()) {
                                // (nothing may be put over a tip: every cell from it up is taken)
                                for z in top - tip_cells..lowest + deep as i32 {
                                    held[at * deep + (z - lowest) as usize] = TIP;
                                }
                                tips.push((CellBrick { mask: tip(towards), min: [over_cell[0], over_cell[1], top - tip_cells], ext: [1, 1, tip_cells] }, shade));
                            } else {
                                for z in top - tip_cells..top {
                                    if let Some(n) = free(z, &held) {
                                        held[n] = shade;
                                    }
                                }
                            }
                        }
                    }
                    k = to;
                }
            }
        }
        // the columns, each run of one green a brick
        for at in 0..side * side {
            let (x, y) = (lo[0] + (at % side) as i32, lo[1] + (at / side) as i32);
            let column = &held[at * deep..(at + 1) * deep];
            let mut k = 0;
            while k < deep {
                let shade = column[k];
                let mut to = k + 1;
                if shade != 0 && shade != TIP {
                    while to < deep && column[to] == shade && (to - k) < self.longest.max(1) as usize {
                        to += 1;
                    }
                    bricks.push((CellBrick { mask: CUBE, min: [x, y, lowest + k as i32], ext: [1, 1, (to - k) as i32] }, shade));
                }
                k = to;
            }
        }
        bricks.extend(tips);
        // Where the bare ground steps up a cell to one side, a sloped
        // brick in the step: sand going down to the water is a bank and
        // not a stair. Only where it is exact and costs nothing to find:
        // the cell on the lower ground is empty (no blade, stone, water or
        // reed), one of the four cells beside it is higher and the one
        // opposite is not. A corner where two slopes meet is left a step:
        // the piece for it is another shape for each way it can face.
        let mut slopes: Vec<(CellBrick, u8)> = Vec::new();
        for at in 0..side * side {
            let (x, y) = (lo[0] + (at % side) as i32, lo[1] + (at / side) as i32);
            let Some(c) = g.index(x, y).filter(|c| g.flags[*c] & (COVERED | WET) == COVERED && floor[at] == g.top[*c]) else { continue };
            let top = g.top[c];
            if held[at * deep + (top - lowest) as usize] != 0 || self.stems.get(&[x, y]).is_some_and(|taken| taken.iter().any(|(a, b)| top >= *a && top < *b)) {
                continue;
            }
            let higher = |way: [i32; 2]| g.index(x + way[0], y + way[1]).is_some_and(|n| g.flags[n] & COVERED != 0 && g.top[n] > top);
            let ways: Vec<[i32; 2]> = [[1, 0], [-1, 0], [0, 1], [0, -1]].into_iter().filter(|way| higher(*way)).collect();
            if let [way] = ways[..] {
                slopes.push((CellBrick { mask: tip(way), min: [x, y, top], ext: [1; 3] }, g.skin[c]));
            }
        }
        bricks.extend(crate::pattern::joined(slopes, self.longest.max(1)));
    }
}

/// Water standing over one cell: the cell, and from which height to before which.
type Standing = ([i32; 2], i32, i32);

/// A lawn's water as bricks: all of it joined into large boxes.
///
/// Greedily, from the surface down: at each cell of water not yet in a
/// box, the box that is a row along x widened along y, or a row along y
/// widened along x, whichever is the larger, and then made as deep as it
/// can be. Finding the fewest boxes is NP-hard, and a greedy pass is the
/// usual answer: https://0fps.net/2012/06/30/meshing-in-a-minecraft-game/
///
/// From the surface down, because a pond is a bowl: each layer of its
/// water is a whole disc, a little smaller than the one above, and a
/// disc is a few wide boxes. From the bottom up, which is the order the
/// fitter's pass for cubes takes (`fit_volume::merge`), the box over the
/// deepest part goes all the way to the surface and every layer above
/// the lowest is left a ring round it, which is many short boxes.
/// Measured on a pond 31 across and 4 deep, 382 cells of water: 58
/// bricks from the bottom up, 33 from the surface down. On one 44 across
/// and 7 deep, 1017 cells: 93 and 50.
///
/// The whole lawn's at once, where soil and blades are joined a tile at a
/// time: a pond lying across the corner where four tiles meet is four
/// quarter bowls if it is joined by tiles, each with its own boxes (the
/// pond 44 across was 117 bricks by tiles where it was 93 whole). The
/// water is a few hundred bricks at most and the editor draws it as a
/// set of its own, so nothing is lost by its bricks belonging to no tile.
fn pooled(water: &[Standing], longest: i32) -> Vec<(CellBrick, u8)> {
    let span = |of: &dyn Fn(&Standing) -> (i32, i32)| water.iter().map(of).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
    let (Some(x), Some(y), Some(z)) = (span(&|w| (w.0[0], w.0[0] + 1)), span(&|w| (w.0[1], w.0[1] + 1)), span(&|w| (w.1, w.2))) else { return Vec::new() };
    let n = [(x.1 - x.0) as usize, (y.1 - y.0) as usize, (z.1 - z.0) as usize];
    let most = longest.max(1) as usize;
    let mut cells = vec![false; n[0] * n[1] * n[2]];
    for &(at, from, to) in water {
        for up in from..to {
            cells[(at[0] - x.0) as usize + n[0] * ((at[1] - y.0) as usize + n[1] * (z.1 - 1 - up) as usize)] = true;
        }
    }
    // (layer `k` is the `k`th down from the surface)
    let mut bricks = Vec::new();
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                // whether every cell of a box is water still to be boxed
                let all = |cells: &[bool], lo: [usize; 3], ext: [usize; 3]| (lo[2]..lo[2] + ext[2]).all(|z| (lo[1]..lo[1] + ext[1]).all(|y| cells[lo[0] + n[0] * (y + n[1] * z)..lo[0] + ext[0] + n[0] * (y + n[1] * z)].iter().all(|c| *c)));
                if !cells[i + n[0] * (j + n[1] * k)] {
                    continue;
                }
                // a row along `first`, then widened along the other of x and y
                let flat = |first: usize| {
                    let (other, mut ext) = (1 - first, [1usize; 3]);
                    for axis in [first, other] {
                        while [i, j, k][axis] + ext[axis] < n[axis] && ext[axis] < most && all(&cells, { let mut lo = [i, j, k]; lo[axis] += ext[axis]; lo }, { let mut one = ext; one[axis] = 1; one }) {
                            ext[axis] += 1;
                        }
                    }
                    ext
                };
                let (along_x, along_y) = (flat(0), flat(1));
                let mut ext = if along_y[0] * along_y[1] > along_x[0] * along_x[1] { along_y } else { along_x };
                while k + ext[2] < n[2] && ext[2] < most && all(&cells, [i, j, k + ext[2]], [ext[0], ext[1], 1]) {
                    ext[2] += 1;
                }
                for z in k..k + ext[2] {
                    for y in j..j + ext[1] {
                        cells[i + n[0] * (y + n[1] * z)..i + ext[0] + n[0] * (y + n[1] * z)].fill(false);
                    }
                }
                bricks.push((CellBrick { mask: CUBE, min: [i as i32 + x.0, j as i32 + y.0, z.1 - (k + ext[2]) as i32], ext: ext.map(|e| e as i32) }, WATER));
            }
        }
    }
    bricks
}

/// How far out from a brick [`Model::distance`] looks: a place further
/// than this from every brick is said to be this far from them.
const REACH: f64 = 2.0;

/// The squares of ground the bricks are filed by are this many cells a side.
const SQUARE: i32 = 8;

/// A lawn as its bricks: laid out by rule, with no fitting, and exactly
/// what it is.
pub struct Model {
    pub bricks: Vec<CellBrick>,
    /// which part each brick is: [`TRUNK`] for soil, [`SAND`], [`BED`]
    /// (the pond's bed and a reed's head), [`STONE`], [`WATER`], and
    /// the three greens of the blades and reeds
    pub shades: Vec<u8>,
    /// the volume of all its bricks, in cells cubed
    pub volume: f64,
    /// the ground it lies on
    pub ground: Ground,
    lo: [i32; 3],
    hi: [i32; 3],
    index: OnceLock<Index>,
}

/// For each square of ground, the bricks that come within [`REACH`] of
/// it: made when a distance is first asked for.
struct Index {
    across: i32,
    squares: Vec<Vec<u32>>,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a lawn of {} bricks", self.bricks.len())
    }
}

impl Model {
    /// The lawn `tree` and `lay` describe, standing at `base`, its
    /// blades `height` tall, reaching `spread` to each side of its line,
    /// in bricks no longer than `longest`.
    pub fn new(tree: &Tree, base: DVec3, height: f64, spread: f64, lay: &Lay, longest: i32, threads: Threads) -> Model {
        let tree = tree.clamped();
        let height = height.max(Tree::least_height(tree.kind));
        let mut ground = Ground::new(&tree, base, spread, lay, threads);
        let seed = seed_of(&tree);
        let wind = TAU * hash(0, 0, 1, seed);
        let none = HashMap::new();
        let (gap, lean, downwind) = (3.4 - 1.9 * tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0, DVec3::new(wind.cos(), wind.sin(), 0.0));
        let (reeds, stems) = Rule { ground: &ground, seed, height, gap, lean, downwind, reeds: lay.reeds, longest, stems: &none }.reeds();
        let marked: Vec<usize> = stems.keys().filter_map(|at| ground.index(at[0], at[1])).collect();
        for c in marked {
            ground.flags[c] |= REED;
        }
        let rule = Rule { ground: &ground, seed, height, gap, lean, downwind, reeds: lay.reeds, longest, stems: &stems };
        // the tiles: those of the world's own grid of them that the lawn reaches
        let (lo, hi) = ground.cells();
        let (first, last) = ([lo[0].div_euclid(TILE), lo[1].div_euclid(TILE)], [(hi[0] - 1).div_euclid(TILE), (hi[1] - 1).div_euclid(TILE)]);
        let across = (last[0] - first[0] + 1) as usize;
        let tiles = threads.map(across * (last[1] - first[1] + 1) as usize, 2, |n| rule.tile([(first[0] + (n % across) as i32) * TILE, (first[1] + (n / across) as i32) * TILE]));
        // (in the tiles' own order, then the water, then the reeds: the same whichever thread laid which)
        let water: Vec<Standing> = tiles.iter().flat_map(|tile| tile.1.iter().copied()).collect();
        let (mut bricks, mut shades) = (Vec::new(), Vec::new());
        for (brick, shade) in tiles.into_iter().flat_map(|tile| tile.0).chain(pooled(&water, longest)).chain(reeds) {
            bricks.push(brick);
            shades.push(shade);
        }
        let volume = bricks.iter().map(|b| b.volume()).sum();
        let corner = |far: bool| [0, 1, 2].map(|k| bricks.iter().map(|b| b.min[k] + if far { b.ext[k] } else { 0 }).reduce(if far { i32::max } else { i32::min }).unwrap_or(0));
        let (lo, hi) = (corner(false), corner(true));
        Model { bricks, shades, volume, ground, lo, hi, index: OnceLock::new() }
    }

    /// The box the lawn's bricks fill.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        (DVec3::new(self.lo[0] as f64, self.lo[1] as f64, self.lo[2] as f64), DVec3::new(self.hi[0] as f64, self.hi[1] as f64, self.hi[2] as f64))
    }

    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            let across = (self.hi[0] - self.lo[0]) / SQUARE + 1;
            let along = (self.hi[1] - self.lo[1]) / SQUARE + 1;
            let mut squares = vec![Vec::new(); (across * along) as usize];
            let reach = REACH.ceil() as i32;
            for (n, b) in self.bricks.iter().enumerate() {
                let span = |k: usize, most: i32| ((b.min[k] - reach - self.lo[k]).div_euclid(SQUARE).clamp(0, most - 1), (b.min[k] + b.ext[k] + reach - self.lo[k]).div_euclid(SQUARE).clamp(0, most - 1));
                let ((x0, x1), (y0, y1)) = (span(0, across), span(1, along));
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        squares[(y * across + x) as usize].push(n as u32);
                    }
                }
            }
            Index { across, squares }
        })
    }

    /// The bricks filed under the square of ground `p` is over.
    fn near(&self, p: DVec3) -> &[u32] {
        let index = self.index();
        let square = |k: usize| ((p[k] - self.lo[k] as f64).floor() as i32).div_euclid(SQUARE);
        index.squares.get((square(1) * index.across + square(0).clamp(0, index.across - 1)).max(0) as usize).map_or(&[], |near| near)
    }

    /// How far `p` is from the lawn, in cells: negative inside a brick of
    /// it. Never more than the true distance, and no more than [`REACH`]
    /// however far off `p` is within the lawn's own box; its sign is
    /// exact. As furniture's is ([`crate::furniture::Model::distance`]),
    /// and for the same use: a form joined into a lawn asks where the
    /// lawn is.
    pub fn distance(&self, p: DVec3) -> f64 {
        let (lo, hi) = self.bounds();
        let q = (p - (lo + hi) / 2.0).abs() - (hi - lo) / 2.0;
        let outside = q.max(DVec3::ZERO).length();
        if outside > 0.0 {
            // (outside its box, at least as far from any brick as from the box)
            return outside;
        }
        self.near(p).iter().map(|n| crate::furniture::brick_distance(&self.bricks[*n as usize], p)).fold(REACH, f64::min)
    }

    /// Which part of the lawn `p` is in: the shade of the brick whose box
    /// holds it, `None` where there is none. What a brick is made of in
    /// the game's file goes by this (water, a blade to walk through, soil
    /// to stand on), whatever colours the parts have been given.
    pub fn part(&self, p: DVec3) -> Option<u8> {
        let holds = |b: &CellBrick| (0..3).all(|k| p[k] >= b.min[k] as f64 && p[k] <= (b.min[k] + b.ext[k]) as f64);
        self.near(p).iter().find(|n| holds(&self.bricks[**n as usize])).map(|n| self.shades[*n as usize])
    }
}

#[cfg(test)]
mod tests;
