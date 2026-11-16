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
//!
//! And what lets a touch cost by the ground it changes and not by the
//! lawn's size: the ground and each tile's bricks are kept from one build
//! to the next (the module `kept`), so a build works out the heights the
//! touch reaches and lays the tiles whose ground is no longer what they
//! were laid from.

use crate::par::Threads;
use crate::shapes::{CellBrick, CUBE};
use crate::tree::{ground, ground_on, hash, Lay, Run, Sculpt, Tree, BED, LEAF, LEAF_DARK, LEAF_LIGHT, LEAST_RANGE, LEAST_SPREAD, SAND, STONE, TRUNK, WATER};

/// The parts a lawn has besides those every plant has
/// ([`crate::tree::TRUNK`] to [`crate::tree::STONE`], which for a lawn are
/// its soil, three greens, the bed of a pond, water, sand and stones).
/// The darker of the two tones of a cliff of bare soil; the other is the soil's own.
pub const CLIFF: u8 = 8;
/// A cliff of rock: most of its beds, the darker beds, and the paler.
pub const ROCK: u8 = 9;
pub const ROCK_DARK: u8 = 10;
pub const ROCK_LIGHT: u8 = 11;
/// The top of a rock: a ledge, weathered or mossy, and snow in winter.
pub const LEDGE: u8 = 12;
/// White water, where a fall lands.
pub const FOAM: u8 = 13;

/// The most [`Lay::cliffs`] may be.
pub const MOST_CLIFFS: i32 = 3;

/// How steep the ground is, as rise over run, where grass gives way to
/// bare soil and where soil gives way to rock, for each setting of
/// [`Lay::cliffs`] from 1: 52 and 68 degrees, 45 and 61, 36 and 53.
///
/// 45 and 61 for the middle setting were chosen by looking, starting
/// from 40 and 60. At 40 a hill 16 high made with a brush 24 in range,
/// which is 46 degrees at its steepest and a hill anyone might push up,
/// was bare soil nearly all over: a mound of earth and no cliff. At 45
/// it keeps its grass but for a narrow band, a hill pushed half as high
/// again has a face of soil, and rock begins on a hill taller than its
/// brush's range or on ground cut with a sharp edge.
const STEEP: [(f64, f64); 3] = [(1.28, 2.48), (1.0, 1.8), (0.73, 1.33)];

/// How far apart, in cells of height, the ledges of a rock face are.
const LEDGES: i32 = 4;
use glam::{DVec2, DVec3};
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::sync::{Arc, OnceLock};

mod kept;
pub mod land;

/// How many cells a tile of ground is along each side.
///
/// A touch of the brush is 8 to 50 cells across, so at 32 it reaches four
/// to nine tiles of a lawn's twenty or more, and the rest keep their
/// bricks. A level tile's soil is one brick, so at 16 level ground is
/// four times as many bricks of soil. At 64 a lawn 180 by 120 is six
/// tiles: too few pieces of work to share among sixteen cores, and every
/// touch changes the bricks of most of them. The editor's own tiles (its
/// `tiles` module) are this size or a whole number of these.
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

/// What water a place is beside.
struct Wet {
    /// the level the water stands at
    level: f64,
    /// when that water is a stream's, how far the place is from the middle of it
    by: Option<f64>,
    /// The level water comes down onto the place from: that of the reach
    /// above, where the place is in a stream's channel and is the first
    /// of a lower reach, and `level` anywhere else.
    over: f64,
    /// how far the place is from where falling water lands, if that is near
    foot: Option<f64>,
    /// whether the place is on or beside falling water: bare rock
    face: bool,
}

/// One place along a stream.
#[derive(Clone, Copy)]
struct Flow {
    at: DVec3,
    /// the level of the water here
    level: f64,
    /// the level of the water at the place before: what comes down here comes from there
    from: f64,
    /// whether a fall lands here: the last place of a run of drops that is three cells or more in all
    foot: bool,
}

/// How far a stream's water reaches to each side of its middle.
///
/// It was 3.0, five or six cells of water across, when the soil was balls
/// on a grid 2.4 apart and a narrower channel closed over. The soil is a
/// column under each cell now, and a channel three cells across stays
/// open: a stream to step over, where the wider one was a canal.
const STREAM_WIDE: f64 = 1.7;

/// How much a stream's water drops from one place to the next before the
/// drop is a fall: bare rock about it, and white water where a run of
/// such drops lands if they are [`LANDS`] in all.
const FALLS: f64 = 2.0;
/// See [`FALLS`].
const LANDS: f64 = 3.0;

/// Where water stands in a patch of grass that has raised ponds, and the
/// streams and falls that run down from them.
///
/// The rule. The ground is flooded from the edge of the patch inwards
/// (priority flood: Barnes, Lehman and Mulla 2014,
/// https://arxiv.org/abs/1511.04463), which gives every place the height
/// water would have to reach there before it ran off the patch: its spill
/// height. A hollow is the places lying under their spill height that
/// touch one another. Water stands in a hollow as far below its rim as it
/// stands below the lawn in a pond, so a hollow whose rim is on raised
/// ground holds its water higher than the lawn's ponds do, each at the
/// level its own rim gives it, and none of them can overflow. Asked which
/// pond is at which level, the answer is read off the ground the user
/// sculpted and never typed.
///
/// (Every raised pond stood at one height, that of the lowest of them, so
/// that there were two levels and no more. A hollow on a much higher
/// hill then held a puddle at the bottom of a deep pit, or nothing.)
///
/// A stream leaves each raised pond for the nearest water that stands
/// lower, the lawn's own or another raised pond's, by the cheapest way
/// over the ground (Dijkstra's), where a step costs its length and three
/// times as much again for each cell the ground there stands above the
/// pond's water: it goes round a rise sooner than through it. A way that
/// would have to climb over ground higher than the pond's own rim is no
/// way downhill, and that pond has no stream.
///
/// The water's level starts at the pond's and at each cell of the
/// stream's length is the lower of what it was and what the ground there
/// allows, in whole cells: flat reaches with a step down between them,
/// never a rise. The alternative, water sloping with the ground, cannot
/// be built of bricks with level tops. On gentle ground the water lies in
/// a channel, half a cell or more under the lower of its banks here and
/// for two cells on. Where the ground falls away by four fifths of a cell
/// or more for each cell along, it lies on the ground: level with the
/// soil to each side of it, so that it is seen from the side and not
/// hidden in a trench down the face. Each place's water reaches up to the
/// level of the place before (`Flow::from`), so the stream is one piece
/// of water from the pond it leaves to the water it reaches: where the
/// ground is a cliff that piece is a sheet standing against the face,
/// which is the waterfall. Nothing else makes one, and so a stream that
/// meets a cliff part way down becomes a fall there and a stream again
/// below it.
///
/// The flood is on a grid of places 2.4 cells apart, which is the grid
/// the soil was laid on when it was fitted. It is kept, though the soil
/// is now laid cell by cell: a flood of every cell would be six times the
/// work for water whose level is a whole number either way.
#[derive(Clone)]
struct Waters {
    lo: DVec3,
    step: f64,
    nx: usize,
    ny: usize,
    /// which raised pond a place is in or beside: one more than its number in `ponds`, and 0 for none
    pond: Vec<u16>,
    /// the level each raised pond's water stands at
    ponds: Vec<f64>,
    lower: f64,
    /// the stream's place nearest each cell of the grid, if one is near
    near: Vec<u32>,
    /// the streams, a cell apart along their lengths
    stream: Vec<Flow>,
    /// What the flood was made from, kept for the next: whether the lawn
    /// covers each place of the grid, and how high the sculpted ground is
    /// there. Empty where there was no flood.
    inside: Vec<bool>,
    lifted: Vec<f64>,
}

impl Waters {
    /// Whether the water stands and runs where `other`'s does: all that
    /// a cell's ground is worked out from, which the samples the flood
    /// was made from are not.
    fn same_water(&self, other: &Waters) -> bool {
        let bits = |v: DVec3| v.to_array().map(f64::to_bits);
        let levels = |levels: &[f64]| levels.iter().map(|level| level.to_bits()).collect::<Vec<u64>>();
        bits(self.lo) == bits(other.lo) && self.step.to_bits() == other.step.to_bits() && (self.nx, self.ny) == (other.nx, other.ny) && self.pond == other.pond && levels(&self.ponds) == levels(&other.ponds) && self.lower.to_bits() == other.lower.to_bits() && self.near == other.near
            && self.stream.len() == other.stream.len() && self.stream.iter().zip(&other.stream).all(|(a, b)| bits(a.at) == bits(b.at) && a.level.to_bits() == b.level.to_bits() && a.from.to_bits() == b.from.to_bits() && a.foot == b.foot)
    }

    /// Water at one level and no more: every pond at `lower`, and no stream.
    fn still(lower: f64, lo: DVec3, step: f64) -> Waters {
        Waters { lo, step, nx: 0, ny: 0, pond: Vec::new(), ponds: Vec::new(), lower, near: Vec::new(), stream: Vec::new(), inside: Vec::new(), lifted: Vec::new() }
    }

    /// Whether a lawn with this lay has a flood made of it, on a grid of
    /// `nx` by `ny` places: only one with raised ponds.
    /// (A patch past six hundred cells square keeps one level: the flood is a grid of the whole patch in memory.)
    fn floods(lay: &Lay, nx: usize, ny: usize) -> bool {
        lay.levels >= 2 && nx * ny <= 400_000
    }

    fn cell(&self, p: DVec3) -> Option<usize> {
        let (i, j) = (((p.x - self.lo.x) / self.step).round(), ((p.y - self.lo.y) / self.step).round());
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.nx && (j as usize) < self.ny).then(|| j as usize * self.nx + i as usize)
    }

    /// The stream beside a place: how far its middle, the place of it
    /// that is nearest, and how far off the nearest foot of a fall is
    /// and the nearest place the water falls at.
    fn beside(&self, p: DVec3) -> Option<(f64, Flow, [Option<f64>; 2])> {
        let k = *self.near.get(self.cell(p)?)? as usize;
        if k >= self.stream.len() {
            return None;
        }
        let about = k.saturating_sub(5)..(k + 6).min(self.stream.len());
        let far = |n: usize| (self.stream[n].at - p).truncate().length();
        let nearest = about.clone().map(|n| (far(n), n)).min_by(|a, b| a.0.total_cmp(&b.0))?;
        let foot = about.clone().filter(|n| self.stream[*n].foot).map(far).min_by(|a, b| a.total_cmp(b));
        let fall = about.filter(|n| self.stream[*n].from - self.stream[*n].level >= FALLS).map(far).min_by(|a, b| a.total_cmp(b));
        Some((nearest.0, self.stream[nearest.1], [foot, fall]))
    }

    /// The ground at a place with the streams' channels cut into it: a
    /// bed [`BED_UNDER`] the water, two cells wide, and banks that rise
    /// to the ground as it was over the next cell and a half, or over
    /// more where the cut is deep.
    fn ground(&self, p: DVec3, was: f64) -> f64 {
        match self.beside(p) {
            Some((far, flow, _)) if was > flow.level - BED_UNDER => {
                let deep = was - flow.level + BED_UNDER;
                let t = ((far - 1.0) / (0.6 * deep).max(1.5)).clamp(0.0, 1.0);
                flow.level - BED_UNDER + deep * t * t * (3.0 - 2.0 * t)
            }
            _ => was,
        }
    }

    /// The water a place is beside, given the ground there as it was sculpted.
    fn level(&self, p: DVec3, was: f64) -> Wet {
        let pond = match self.cell(p).map(|c| self.pond[c]) {
            Some(raised) if raised > 0 => self.ponds[raised as usize - 1],
            _ => self.lower,
        };
        let Some((far, flow, [foot, fall])) = self.beside(p) else { return Wet { level: pond, by: None, over: pond, foot: None, face: false } };
        let foot = foot.filter(|far| *far < 3.6);
        if far < 5.0 && was + 1.0 >= pond {
            let over = if far < STREAM_WIDE { flow.from } else { flow.level };
            // (Rock for six cells round wherever the water falls, the channel's bed and its banks: sand does not lie on a face. Six, because ground within five of a stream and low beside it is sand, and at four a strip of sand was left down each side of the fall.)
            return Wet { level: flow.level, by: Some(far), over, foot, face: fall.is_some_and(|far| far < 6.5) };
        }
        // (in a pond, the pond's water: and where a stream comes down into it, what comes down)
        let lands = flow.level == pond && far < STREAM_WIDE;
        Wet { level: pond, by: None, over: if lands { flow.from } else { pond }, foot: foot.filter(|_| flow.level == pond), face: false }
    }

    /// The water of a lawn with raised ponds, from the grid's samples:
    /// `inside`, whether the lawn covers each place, and `high`, how high
    /// the sculpted ground is at each it covers (0 at the others). `lifted`
    /// is the sculpted ground anywhere, for the banks of a stream.
    #[allow(clippy::too_many_arguments)]
    fn found(lower: f64, inside: &[bool], high: &[f64], lifted: &dyn Fn(DVec3) -> f64, lo: DVec3, step: f64, nx: usize, ny: usize, seed: u32) -> Waters {
        use std::{cmp::Reverse, collections::BinaryHeap};
        let mut waters = Waters::still(lower, lo, step);
        let at = |c: usize| DVec3::new(lo.x + (c % nx) as f64 * step, lo.y + (c / nx) as f64 * step, 0.0);
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
        // the hollows, and of them the raised ones deep enough to hold water: each with the level its water stands at
        let (mut hollow, mut raised) = (vec![u32::MAX; nx * ny], Vec::<(f64, u32)>::new());
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
            // (as many as a place of the grid can be numbered among)
            if deep > 1.0 - lower && own > lower + 1.5 && raised.len() < u16::MAX as usize - 1 {
                raised.push((own, hollows));
            }
            hollows += 1;
        }
        waters.nx = nx;
        waters.ny = ny;
        waters.pond = vec![0; nx * ny];
        waters.near = vec![u32::MAX; nx * ny];
        if raised.is_empty() {
            return waters;
        }
        waters.ponds = raised.iter().map(|pond| pond.0).collect();
        // each raised hollow and its banks, three cells of the grid out but not over its rim and down the far side
        let number: std::collections::HashMap<u32, u16> = raised.iter().enumerate().map(|(n, pond)| (pond.1, n as u16 + 1)).collect();
        let mut edge: Vec<usize> = Vec::new();
        for c in 0..nx * ny {
            if let Some(n) = number.get(&hollow[c]) {
                waters.pond[c] = *n;
                edge.push(c);
            }
        }
        for _ in 0..3 {
            let mut next = Vec::new();
            for c in edge {
                for n in around(c).flatten() {
                    if inside[n] && waters.pond[n] == 0 && fill[n] >= fill[c] - 0.05 {
                        waters.pond[n] = waters.pond[c];
                        fill[n] = fill[n].min(fill[c] + 0.04);
                        next.push(n);
                    }
                }
            }
            edge = next;
        }
        // the level of the water standing at a place of the grid, if any does: a raised pond's in its own hollow, the lawn's anywhere that is no raised pond's
        let ponds = waters.ponds.clone();
        let belongs = waters.pond.clone();
        let standing = |c: usize| {
            if !inside[c] {
                None
            } else if belongs[c] > 0 {
                let level = ponds[belongs[c] as usize - 1];
                (number.get(&hollow[c]) == Some(&belongs[c]) && high[c] + 1.0 < level).then_some(level)
            } else {
                (high[c] + 1.0 < lower).then_some(lower)
            }
        };
        // the streams: at most twelve, each a search of the whole grid
        let mut near = vec![f32::MAX; nx * ny];
        for (pond, &(own, which)) in raised.iter().enumerate().take(12) {
            let mine = pond as u16 + 1;
            let (mut cost, mut from) = (vec![f64::MAX; nx * ny], vec![u32::MAX; nx * ny]);
            let mut heap = BinaryHeap::new();
            let mut rim = f64::MIN;
            for c in (0..nx * ny).filter(|c| hollow[*c] == which) {
                rim = rim.max(fill[c]);
                if high[c] + 1.0 < own {
                    cost[c] = 0.0;
                    heap.push(Reverse((0i64, c)));
                }
            }
            let mut end = None;
            while let Some(Reverse((k, c))) = heap.pop() {
                if k > key(cost[c]) {
                    continue;
                }
                // (water that stands lower, and is not this pond's own)
                if belongs[c] != mine && standing(c).is_some_and(|level| level < own - 0.5) {
                    end = Some(c);
                    break;
                }
                for n in around(c).flatten().filter(|n| inside[*n]) {
                    let long = if n % nx != c % nx && n / nx != c / nx { step * std::f64::consts::SQRT_2 } else { step };
                    let to = cost[c] + long * (1.0 + 3.0 * (high[n] - own).max(0.0));
                    if key(to) < key(cost[n]) {
                        cost[n] = to;
                        from[n] = c as u32;
                        heap.push(Reverse((key(to), n)));
                    }
                }
            }
            let Some(mut c) = end else { continue };
            let reached = standing(c).unwrap_or(lower);
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
                // (the lower of its two banks, so that it stands above neither)
                let bank = [-1.8, -0.9, 0.0, 0.9, 1.8].iter().map(|side| lifted(p + across * *side)).fold(f64::MAX, f64::min);
                places.push((p, bank));
            }
            let first = waters.stream.len();
            let mut level = own;
            let steep: Vec<bool> = (0..places.len()).map(|n| (places[n.saturating_sub(1)].1 - places[(n + 1).min(places.len() - 1)].1) / 2.0 >= 0.8).collect();
            for (n, &(p, bank)) in places.iter().enumerate() {
                let before = level;
                // (Still at the pond's own level until it is out through the rim. From there, on gentle ground under its banks here and for two cells on, as far as the ground stays gentle: a cell of water takes the level of the place on the stream nearest it, which may be a little upstream of it. On steep ground level with the soil beside it: see `Waters`.)
                if !waters.cell(p).is_some_and(|c| waters.pond[c] == mine) {
                    let allowed = if steep[n] { (bank + SOIL_OVER).round() } else { ((n..(n + 3).min(places.len())).take_while(|m| !steep[*m]).map(|m| places[m].1).fold(f64::MAX, f64::min) - 0.5).floor() };
                    level = level.min(allowed).max(reached);
                }
                let k = waters.stream.len() as u32;
                waters.stream.push(Flow { at: p, level, from: before, foot: false });
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
            // Where falls land: the last place of each run of places the
            // water drops at, if one of the drops is a fall and the run
            // comes down far enough in all. (A run and not a single drop:
            // down a face at 60 degrees the water drops two cells, two
            // cells, one cell, and that is one fall landing once.)
            let (mut top, mut fell) = (None, false);
            for n in first..waters.stream.len() {
                let flow = waters.stream[n];
                let drop = flow.from - flow.level;
                if drop >= 1.0 {
                    top.get_or_insert(flow.from);
                    fell |= drop >= FALLS;
                }
                if n + 1 == waters.stream.len() || waters.stream[n + 1].level > flow.level - 1.0 {
                    if let Some(from) = top.take() {
                        waters.stream[n].foot = fell && from - flow.level >= LANDS;
                    }
                    fell = false;
                }
            }
            // (a gap in the numbering, so that one stream's places are not taken for the next's)
            for _ in 0..12 {
                waters.stream.push(Flow { at: DVec3::splat(f64::MAX / 4.0), level: lower, from: lower, foot: false });
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
    /// what the ground's steepness makes of it: 0 for ground grass grows on, 1 for a cliff of bare soil, 2 for rock
    pub cliff: u8,
    /// the height water comes down onto the cell from, as a cell's floor, where a stream drops here: the water in the cell reaches up to it
    pub falls_from: Option<i32>,
}

/// water stands in the cell
const WET: u8 = 1;
/// the water beside the cell is a stream's
const STREAM: u8 = 2;
/// a reed stands in the cell or over it
const REED: u8 = 4;
/// the lawn covers the cell
const COVERED: u8 = 8;
/// the water in the cell is white on top: a fall lands by it
const FOAMS: u8 = 16;
/// the cell is on or beside falling water, and bare rock
const ROCKY: u8 = 32;

/// A number from 0 to 1 that changes slowly from cell to cell: the
/// numbers thrown at the corners of squares seven cells a side, eased
/// between (value noise: https://en.wikipedia.org/wiki/Value_noise). By
/// it the edge of a cliff wanders, where steepness alone would draw it
/// as a ring at one height round every hill.
fn wander(x: i32, y: i32, which: i32, seed: u32) -> f64 {
    const SIDE: i32 = 7;
    let (i, j) = (x.div_euclid(SIDE), y.div_euclid(SIDE));
    let ease = |t: f64| t * t * (3.0 - 2.0 * t);
    let (u, v) = (ease(x.rem_euclid(SIDE) as f64 / SIDE as f64), ease(y.rem_euclid(SIDE) as f64 / SIDE as f64));
    let at = |i: i32, j: i32| hash(i, j, which, seed);
    let (low, high) = (at(i, j) + (at(i + 1, j) - at(i, j)) * u, at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * u);
    low + (high - low) * v
}

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
#[derive(Clone)]
pub struct Ground {
    /// What it was made from, kept so that the next ground can be made
    /// from this one (see [`Ground::after`]); a piece cut out of a ground
    /// for a tile to be laid from has none.
    made: Option<Box<Made>>,
    /// where the plant's foot is, on the floor of its cell: the lawn's own places are measured from here
    stand: DVec3,
    /// the first cell of the table each way, and how many
    lo: [i32; 2],
    n: [usize; 2],
    high: Vec<f64>,
    top: Vec<i32>,
    /// the level of the water the place is beside, as a cell's floor; unused where the lawn has no water
    level: Vec<i32>,
    /// the top of the water in the cell, as a cell's floor: the level, or higher where a stream comes down onto the cell
    over: Vec<i32>,
    skin: Vec<u8>,
    flags: Vec<u8>,
    /// the size of the stone whose middle is here, 0 for none
    stone: Vec<f32>,
    /// what the ground's steepness makes of the cell: 0, 1 for a cliff of soil, 2 for rock
    face: Vec<u8>,
    /// how bare of grass the cell's steepness leaves it: 0 for not at all to 255 for wholly
    bare: Vec<u8>,
    /// whether the lawn has water at all
    watered: bool,
}

/// What a ground was made from, besides the touches: where the lawn is
/// and what water it has. Two grounds of one site differ by their touches
/// alone. Numbers are kept as their bits, so that two sites are the same
/// only where every number of them is the very same number.
#[derive(Clone, PartialEq)]
struct Site {
    base: [u64; 3],
    spread: u64,
    run: Vec<[u64; 3]>,
    closed: bool,
    water: Option<u64>,
    levels: u8,
    cliffs: u8,
    seed: u32,
}

impl Site {
    fn of(base: DVec3, spread: f64, lay: &Lay, seed: u32) -> Site {
        let bits = |v: DVec3| v.to_array().map(f64::to_bits);
        Site { base: bits(base), spread: spread.max(LEAST_SPREAD).to_bits(), run: lay.run.iter().map(|p| bits(*p)).collect(), closed: lay.closed, water: lay.water.map(f64::to_bits), levels: lay.levels, cliffs: lay.cliffs, seed }
    }
}

/// See [`Ground::made`].
#[derive(Clone)]
struct Made {
    site: Site,
    touches: Vec<Sculpt>,
    /// how high the sculpted ground is at each cell, before any stream is cut into it
    was: Vec<f64>,
    waters: Option<Waters>,
    /// the cells marked [`REED`]
    reeded: Vec<u32>,
}

/// Whether two touches are the very same: every number of them, bit for bit.
fn same_touch(a: &Sculpt, b: &Sculpt) -> bool {
    let bits = |t: &Sculpt| (t.at.to_array().map(f64::to_bits), t.range.to_bits(), t.amount.to_bits(), t.flatten, t.sharp);
    bits(a) == bits(b)
}

/// How the touches a ground is wanted for differ from those a kept ground
/// was made with, and so which places have another height.
///
/// A place's height is the touches that reach it, applied in turn
/// ([`ground`]); a touch that does not reach a place does nothing there.
/// Leave out the touches both lists begin with and those both end with,
/// and what is left of each is the touches that differ: a place none of
/// them reaches has the same touches reaching it in the same order as
/// before, and so the same height. A place one does reach is worked out
/// again, from the floor up; or, where touches were only added to the
/// end, as a stroke of the brush adds them, by applying the new ones to
/// the height it had ([`ground_on`]).
struct Change<'a> {
    /// the touches to apply where a touch that differs reaches
    apply: &'a [Sculpt],
    /// whether they are applied to the height the place had, or from the floor
    onto: bool,
    /// the touches that differ, of the old and of the new
    differ: Vec<Sculpt>,
    /// no ground was kept: every place is worked out
    whole: bool,
}

/// The places of a table that are to be worked out again: which, and the box they lie in.
struct Marks {
    marked: Vec<bool>,
    /// the first place of the box each way, and one past the last; empty if they are equal
    lo: [usize; 2],
    hi: [usize; 2],
}

impl<'a> Change<'a> {
    fn whole(now: &'a [Sculpt]) -> Change<'a> {
        Change { apply: now, onto: false, differ: Vec::new(), whole: true }
    }

    fn between(was: &[Sculpt], now: &'a [Sculpt]) -> Change<'a> {
        let head = was.iter().zip(now).take_while(|(a, b)| same_touch(a, b)).count();
        let tail = was[head..].iter().rev().zip(now[head..].iter().rev()).take_while(|(a, b)| same_touch(a, b)).count();
        let differ = was[head..was.len() - tail].iter().chain(&now[head..now.len() - tail]).copied().collect();
        if head == was.len() { Change { apply: &now[head..], onto: true, differ, whole: false } } else { Change { apply: now, onto: false, differ, whole: false } }
    }

    /// The height of the sculpted ground at `p`, which a touch that differs reaches and where it was `kept`.
    fn height(&self, kept: f64, p: DVec3) -> f64 {
        ground_on(if self.onto { kept } else { 0.0 }, self.apply, p)
    }

    /// The places of a table of `n` places each way, `step` apart from
    /// `first`, that a touch which differs may reach: those in the square
    /// round each such touch, a place more each way than its range. More
    /// than it reaches, which costs those places' heights worked out once
    /// more to the number they were, and is a box to mark and not a circle.
    fn marks(&self, n: [usize; 2], first: DVec2, step: f64) -> Marks {
        if self.whole {
            return Marks { marked: vec![true; n[0] * n[1]], lo: [0, 0], hi: n };
        }
        let mut marks = Marks { marked: vec![false; n[0] * n[1]], lo: n, hi: [0, 0] };
        for touch in &self.differ {
            let range = touch.range.max(LEAST_RANGE);
            // (a touch that is no number reaches who knows where: every place)
            if !(touch.at.is_finite() && range.is_finite()) {
                return Marks { marked: vec![true; n[0] * n[1]], lo: [0, 0], hi: n };
            }
            let span = |k: usize| ((((touch.at[k] - range - first[k]) / step).floor() - 1.0).clamp(0.0, n[k] as f64) as usize, (((touch.at[k] + range - first[k]) / step).ceil() + 2.0).clamp(0.0, n[k] as f64) as usize);
            let ((i0, i1), (j0, j1)) = (span(0), span(1));
            if i0 >= i1 || j0 >= j1 {
                continue;
            }
            for j in j0..j1 {
                marks.marked[j * n[0] + i0..j * n[0] + i1].fill(true);
            }
            (marks.lo, marks.hi) = ([marks.lo[0].min(i0), marks.lo[1].min(j0)], [marks.hi[0].max(i1), marks.hi[1].max(j1)]);
        }
        if marks.lo[0] >= marks.hi[0] {
            (marks.lo, marks.hi) = ([0, 0], [0, 0]);
        }
        marks
    }
}

impl Ground {
    /// The ground of the grass `lay` describes, standing at `base` and
    /// reaching `spread` to each side of its line.
    pub fn new(tree: &Tree, base: DVec3, spread: f64, lay: &Lay, threads: Threads) -> Ground {
        Ground::after(None, tree, base, spread, lay, threads).0
    }

    /// [`Ground::new`], made from `kept` if that is a ground of the same
    /// lawn with other touches: only what the touches that differ reach is
    /// worked out, and the rest is `kept`'s. The same ground, to the bit,
    /// as one made from nothing (see [`Change`]). With it, the box of
    /// cells that were worked out, which may have changed: the first each
    /// way and one past the last, the same twice if none were.
    ///
    /// Where the lawn has two levels of water the flood that finds them
    /// is made again whole at every touch, from samples kept as the cells'
    /// heights are: a hollow anywhere may change what stands where. If
    /// the water comes out as it was, the cells are still only those the
    /// touch reaches; if not, every cell is worked out, though only those
    /// the touch reaches have another height to begin from.
    fn after(kept: Option<Ground>, tree: &Tree, base: DVec3, spread: f64, lay: &Lay, threads: Threads) -> (Ground, [[i32; 2]; 2]) {
        let spread = spread.max(LEAST_SPREAD);
        let stand = DVec3::new(base.x, base.y, base.z.floor());
        let run = Run::new(lay);
        let covered = |p: DVec3| run.distance(p) <= spread || (lay.closed && run.holds(p));
        let sculpted = |p: DVec3| ground(&lay.mounds, p);
        let seed = seed_of(tree);
        let site = Site::of(base, spread, lay, seed);
        let (low, high) = run.points.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p - spread), hi.max(*p + spread)));
        // (one cell more each way than the lawn reaches: a column of soil goes down to meet its neighbours, which it must know the height of)
        let lo = [(stand.x + low.x).floor() as i32 - 1, (stand.y + low.y).floor() as i32 - 1];
        let n = [((stand.x + high.x).ceil() as i32 + 1 - lo[0]).max(1) as usize, ((stand.y + high.y).ceil() as i32 + 1 - lo[1]).max(1) as usize];
        let cells = n[0] * n[1];
        // The ground kept, if it is this lawn's: its tables to be written
        // over where the touches differ, and what it was made from.
        let kept = kept.filter(|g| g.made.as_ref().is_some_and(|made| made.site == site && made.was.len() == cells));
        let known = kept.is_some();
        let (mut made, mut was_made) = match kept {
            Some(mut ground) => {
                let was_made = *ground.made.take().unwrap();
                for c in &was_made.reeded {
                    ground.flags[*c as usize] &= !REED;
                }
                (ground, Some(was_made))
            }
            None => (Ground { made: None, stand, lo, n, high: vec![0.0; cells], top: vec![0; cells], level: vec![0; cells], over: vec![0; cells], skin: vec![TRUNK; cells], flags: vec![0; cells], stone: vec![0.0; cells], face: vec![0; cells], bare: vec![0; cells], watered: false }, None),
        };
        let touches_were = was_made.as_mut().map(|m| std::mem::take(&mut m.touches));
        let change = touches_were.as_ref().map_or_else(|| Change::whole(&lay.mounds), |were| Change::between(were, &lay.mounds));
        let mut was = was_made.as_mut().map_or_else(|| vec![0.0; cells], |m| std::mem::take(&mut m.was));
        let mut waters_were = was_made.and_then(|m| m.waters);
        let waters = lay.water.map(|lower| {
            let step = 2.4;
            let (nx, ny) = (((high.x - low.x) / step).ceil() as usize + 1, ((high.y - low.y) / step).ceil() as usize + 1);
            if !Waters::floods(lay, nx, ny) {
                return Waters::still(lower, low, step);
            }
            let at = |c: usize| DVec3::new(low.x + (c % nx) as f64 * step, low.y + (c / nx) as f64 * step, 0.0);
            // the grid's samples: those kept, with the ones the touches that differ reach taken again
            let (inside, lifted) = match waters_were.as_mut().filter(|w| w.inside.len() == nx * ny && w.lifted.len() == nx * ny) {
                Some(were) => {
                    let (inside, mut lifted) = (std::mem::take(&mut were.inside), std::mem::take(&mut were.lifted));
                    let marks = change.marks([nx, ny], low.truncate(), step);
                    for j in marks.lo[1]..marks.hi[1] {
                        for c in (j * nx + marks.lo[0]..j * nx + marks.hi[0]).filter(|c| marks.marked[*c] && inside[*c]) {
                            lifted[c] = change.height(lifted[c], at(c));
                        }
                    }
                    (inside, lifted)
                }
                None => {
                    let inside: Vec<bool> = (0..nx * ny).map(|c| covered(at(c))).collect();
                    let lifted = (0..nx * ny).map(|c| if inside[c] { sculpted(at(c)) } else { 0.0 }).collect();
                    (inside, lifted)
                }
            };
            let mut waters = Waters::found(lower, &inside, &lifted, &sculpted, low, step, nx, ny, seed);
            (waters.inside, waters.lifted) = (inside, lifted);
            waters
        });
        // whether the water is where it was: if not, every cell is worked out again
        let still = known && match (&waters_were, &waters) {
            (None, None) => true,
            (Some(were), Some(now)) => were.same_water(now),
            _ => false,
        };
        let floor = stand.z as i32;
        // (the cell the foot is in: the numbers a cell's stones are thrown by go by where it is from there, so a lawn moved is the same lawn)
        let foot = [stand.x.floor() as i32, stand.y.floor() as i32];
        let touched = change.marks(n, DVec2::new(lo[0] as f64 + 0.5 - stand.x, lo[1] as f64 + 0.5 - stand.y), 1.0);
        let (from, to) = if still { (touched.lo, touched.hi) } else { ([0, 0], n) };
        // Each row on its own: a cell's ground depends on nothing but
        // where it is. Shared out because the touches are many: every
        // cell asks every touch how far off it is. (Sixteen rows of a
        // whole lawn were worth a thread; of a few cells a row, as many
        // more as make up those cells.)
        let least = if to[0] - from[0] == n[0] { 16 } else { (16 * 180 / (to[0] - from[0]).max(1)).max(16) };
        let rows: Vec<Vec<(f64, f64, i32, i32, i32, u8, u8, f32)>> = threads.map(to[1] - from[1], least, |row| {
            let j = from[1] + row;
            (from[0]..to[0])
                .filter(|i| !still || touched.marked[j * n[0] + i])
                .map(|i| {
                    let c = j * n[0] + i;
                    let (x, y) = (lo[0] + i as i32, lo[1] + j as i32);
                    let p = DVec3::new(x as f64 + 0.5 - stand.x, y as f64 + 0.5 - stand.y, 0.0);
                    let was = if touched.marked[c] { change.height(was[c], p) } else { was[c] };
                    let here = waters.as_ref().map_or(was, |w| w.ground(p, was));
                    let top = floor + (here + SOIL_OVER).round() as i32;
                    // (whether the lawn covers a cell goes by its line, which is the site's: as it was, if it was kept)
                    let mut flags = if known { made.flags[c] & COVERED } else if covered(p) { COVERED } else { 0 };
                    let Some(Wet { level, by, over, foot: lands, face }) = waters.as_ref().map(|w| w.level(p, was)) else { return (was, here, top, 0, 0, TRUNK, flags, 0.0) };
                    let surface = floor + level.round() as i32;
                    if by.is_some() {
                        flags |= STREAM;
                    }
                    if face {
                        flags |= ROCKY;
                    }
                    // (a stream's water only in its channel, and not wherever the ground beside it happens to lie lower)
                    let wet = flags & COVERED != 0 && here + 1.0 < level && by.is_none_or(|far| far < STREAM_WIDE) && top < surface;
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
                        return (was, here, top, surface, surface, skin, flags, 0.0);
                    }
                    // (white where a fall lands, most of the water within two cells of it and less further off)
                    if lands.is_some_and(|far| hash(x - foot[0], y - foot[1], 14, seed) < 1.0 - 0.25 * far) {
                        flags |= FOAMS;
                    }
                    // and here and there a stone on the bed
                    let chance = hash(x - foot[0], y - foot[1], 3, seed);
                    let stone = if here <= level - 1.5 && chance < STONES { 1.4 + 1.08 * chance / STONES } else { 0.0 };
                    (was, here, top, surface, surface.max(floor + over.round() as i32), skin, flags | WET, stone as f32)
                })
                .collect()
        });
        // (written where they belong, in the order they were worked out in)
        for (row, cells) in rows.into_iter().enumerate() {
            let j = from[1] + row;
            let places: Vec<usize> = (from[0]..to[0]).filter(|i| !still || touched.marked[j * n[0] + i]).collect();
            for (i, (raw, here, top, level, over, skin, flags, stone)) in places.into_iter().zip(cells) {
                let c = j * n[0] + i;
                (was[c], made.high[c], made.top[c], made.level[c], made.over[c], made.skin[c], made.flags[c], made.stone[c]) = (raw, here, top, level, over, skin, flags, stone);
            }
        }
        // The cliffs: what each cell's steepness makes of it, for the
        // cells worked out. (And for no others, though a cell's steepness
        // goes by the heights of the cells beside it: the box of cells
        // worked out reaches a cell further each way than any touch that
        // differs, see `Change::marks`, so a cell outside it has no cell
        // beside it whose height changed.) By the ground as it was sculpted and not
        // as the streams cut it, or every stream would run between cliffs
        // of its own banks. The steepness is the rise each way over the
        // cells to each side
        // (https://en.wikipedia.org/wiki/Finite_difference#Basic_types),
        // made more or less by up to a quarter by a number that changes
        // slowly over the ground, so that the cliff's edge wanders. Grass
        // thins from three quarters of the steepness at which soil is bare.
        if lay.cliffs > 0 && from[0] < to[0] && from[1] < to[1] {
            let (soil, rock) = STEEP[(lay.cliffs as usize).min(STEEP.len()) - 1];
            let rows: Vec<Vec<(u8, u8, i32)>> = threads.map(to[1] - from[1], 64, |row| {
                let j = from[1] + row;
                (from[0]..to[0])
                    .map(|i| {
                        let c = j * n[0] + i;
                        let (x, y) = (lo[0] + i as i32 - foot[0], lo[1] + j as i32 - foot[1]);
                        let at = |i: usize, j: usize| was[j * n[0] + i];
                        let rise = DVec2::new(at((i + 1).min(n[0] - 1), j) - at(i.saturating_sub(1), j), at(i, (j + 1).min(n[1] - 1)) - at(i, j.saturating_sub(1))) / 2.0;
                        let steep = rise.length() * (0.85 + 0.3 * wander(x, y, 30, seed));
                        // (the top of the soil as the ground's height gives it, which is what it is anywhere but on rock)
                        let top = floor + (made.high[c] + SOIL_OVER).round() as i32;
                        if steep >= rock {
                            // Rock stands in ledges: its top is brought
                            // down to the next of a set of levels LEDGES
                            // apart, which rise and dip slowly across the
                            // ground. A face is then risers a few cells
                            // tall with its beds showing in them, and
                            // treads between; left a cell's step for each
                            // cell, it was a grey slope with no face to
                            // show a bed. Only dry rock that is soil
                            // otherwise: sand and a pond's bed keep the
                            // height the water was found by.
                            let dry = made.skin[c] == TRUNK && made.flags[c] & (WET | ROCKY) == 0;
                            let lift = (LEDGES as f64 * wander(x, y, 32, seed)) as i32;
                            (2, 255, if dry { (top + lift).div_euclid(LEDGES) * LEDGES - lift } else { top })
                        } else if steep >= soil {
                            // (the odd tuft on a cliff of soil: about one place in fourteen keeps its grass)
                            (1, 237, top)
                        } else {
                            (0, (237.0 * ((steep - 0.75 * soil) / (0.25 * soil)).clamp(0.0, 1.0)) as u8, top)
                        }
                    })
                    .collect()
            });
            for (row, cells) in rows.into_iter().enumerate() {
                for (i, (face, bare, top)) in (from[0]..to[0]).zip(cells) {
                    let c = (from[1] + row) * n[0] + i;
                    (made.face[c], made.bare[c], made.top[c]) = (face, bare, top);
                }
            }
        }
        made.watered = waters.is_some();
        made.made = Some(Box::new(Made { site, touches: lay.mounds.clone(), was, waters, reeded: Vec::new() }));
        let redone = [[lo[0] + from[0] as i32, lo[1] + from[1] as i32], [lo[0] + to[0] as i32, lo[1] + to[1] as i32]];
        (made, redone)
    }

    /// The part of the ground over the cells from `lo` to before `hi`, as
    /// a ground of its own: what a tile is laid from, so that a tile's
    /// bricks can depend on nothing outside it (see [`kept`]). Where the
    /// box reaches past the table the piece does not: there is nothing to
    /// say of those cells in either.
    fn cut(&self, lo: [i32; 2], hi: [i32; 2]) -> Ground {
        let (lo, n) = self.window(lo, hi);
        let cells = n[0] * n[1];
        let mut cut = Ground { made: None, stand: self.stand, lo, n, high: Vec::with_capacity(cells), top: Vec::with_capacity(cells), level: Vec::with_capacity(cells), over: Vec::with_capacity(cells), skin: Vec::with_capacity(cells), flags: Vec::with_capacity(cells), stone: Vec::with_capacity(cells), face: Vec::with_capacity(cells), bare: Vec::with_capacity(cells), watered: self.watered };
        for j in 0..n[1] {
            let from = self.index(lo[0], lo[1] + j as i32).unwrap_or(0);
            let row = from..from + n[0];
            cut.high.extend_from_slice(&self.high[row.clone()]);
            cut.top.extend_from_slice(&self.top[row.clone()]);
            cut.level.extend_from_slice(&self.level[row.clone()]);
            cut.over.extend_from_slice(&self.over[row.clone()]);
            cut.face.extend_from_slice(&self.face[row.clone()]);
            cut.bare.extend_from_slice(&self.bare[row.clone()]);
            cut.skin.extend_from_slice(&self.skin[row.clone()]);
            cut.flags.extend_from_slice(&self.flags[row.clone()]);
            cut.stone.extend_from_slice(&self.stone[row]);
        }
        cut
    }

    /// The part of the box from `lo` to before `hi` that the table has: its first cell, and how many each way.
    fn window(&self, lo: [i32; 2], hi: [i32; 2]) -> ([i32; 2], [usize; 2]) {
        let end = self.cells().1;
        let first = [lo[0].clamp(self.lo[0], end[0]), lo[1].clamp(self.lo[1], end[1])];
        (first, [(hi[0].clamp(first[0], end[0]) - first[0]) as usize, (hi[1].clamp(first[1], end[1]) - first[1]) as usize])
    }

    /// Whether `cut` is what [`Ground::cut`] would now cut from `lo` to
    /// before `hi`: every number of it the very same. Nothing is copied
    /// to find out.
    fn same_cut(&self, cut: &Ground, lo: [i32; 2], hi: [i32; 2]) -> bool {
        let (lo, n) = self.window(lo, hi);
        let bits = |v: DVec3| v.to_array().map(f64::to_bits);
        if (cut.lo, cut.n, cut.watered) != (lo, n, self.watered) || bits(cut.stand) != bits(self.stand) {
            return false;
        }
        (0..n[1]).all(|j| {
            let from = self.index(lo[0], lo[1] + j as i32).unwrap_or(0);
            let (row, of_cut) = (from..from + n[0], j * n[0]..(j + 1) * n[0]);
            // (The heights by their bits, every one of a row looked at
            // and the differences gathered, where stopping at the first
            // that differs would be a branch a number: this way the
            // compiler compares several at once, and a tile kept is
            // nearly always the same to its last number anyway.)
            self.top[row.clone()] == cut.top[of_cut.clone()]
                && self.flags[row.clone()] == cut.flags[of_cut.clone()]
                && self.level[row.clone()] == cut.level[of_cut.clone()]
                && self.over[row.clone()] == cut.over[of_cut.clone()]
                && self.face[row.clone()] == cut.face[of_cut.clone()]
                && self.bare[row.clone()] == cut.bare[of_cut.clone()]
                && self.skin[row.clone()] == cut.skin[of_cut.clone()]
                && self.high[row.clone()].iter().zip(&cut.high[of_cut.clone()]).fold(0, |differ, (a, b)| differ | (a.to_bits() ^ b.to_bits())) == 0
                && self.stone[row].iter().zip(&cut.stone[of_cut]).fold(0, |differ, (a, b)| differ | (a.to_bits() ^ b.to_bits())) == 0
        })
    }

    /// How many bytes the tables take, near enough.
    fn bytes(&self) -> usize {
        self.high.len() * 36 + self.made.as_ref().map_or(0, |made| made.was.len() * 8 + made.waters.as_ref().map_or(0, |w| w.inside.len() * 14))
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
        let wet = flags & WET != 0;
        // (rock where the ground is steep enough for it and is soil, and wherever water falls)
        let cliff = if self.skin[c] == BED { 0 } else if flags & ROCKY != 0 { 2 } else if self.skin[c] == TRUNK { self.face[c] } else { 0 };
        Some(Place { covered: flags & COVERED != 0, height: self.high[c], top: self.top[c], water: wet.then_some(self.level[c]), stream: flags & STREAM != 0, skin: self.skin[c], cliff, falls_from: (wet && self.over[c] > self.level[c]).then_some(self.over[c]) })
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
#[derive(Clone, Copy)]
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
#[derive(Clone)]
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
    /// How far a blade may reach sideways: it leans under 40 degrees from upright.
    fn reach(&self) -> i32 {
        ((self.height.ceil() as i32 + 1).min(TALLEST) as f64 * 0.8).ceil() as i32 + 2
    }

    /// Every number a tile is laid by besides its ground and its reeds,
    /// as bits: two rules with the same lay the same tile from the same
    /// ground.
    fn bits(&self) -> [u64; 8] {
        [self.seed as u64, self.height.to_bits(), self.gap.to_bits(), self.lean.to_bits(), self.downwind.x.to_bits(), self.downwind.y.to_bits(), self.reeds.to_bits(), self.longest as u64]
    }

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
        // (none on rock, where a fall is)
        let Some(c) = g.index(tuft.cell[0], tuft.cell[1]).filter(|c| g.flags[*c] & (COVERED | ROCKY) == COVERED && g.bare[*c] < 255) else { return false };
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

    /// The tufts in the cells from `lo` to before `hi` that a reed may
    /// stand at ([`Rule::reed_at`]), in the tufts' order.
    ///
    /// Whether a tuft is one goes by the ground of its own cell and by
    /// nothing else, so when a touch has changed some cells the tufts to
    /// ask again are those of the changed cells, and the rest are what
    /// they were (see `Model::laid`).
    fn candidates(&self, lo: [i32; 2], hi: [i32; 2]) -> Vec<Tuft> {
        if self.reeds <= 0.0 || !self.ground.watered || lo[0] >= hi[0] || lo[1] >= hi[1] {
            return Vec::new();
        }
        self.tufts(lo, hi).into_iter().filter(|tuft| self.reed_at(tuft)).collect()
    }

    /// Every reed of the lawn, from the tufts one may stand at, and for
    /// each cell one stands in or over, the heights it fills there.
    ///
    /// All of them at once and before any tile, where everything else is
    /// laid tile by tile: a reed is a dozen cells across with its leaf
    /// and belongs to no one tile, and they are few. They are taken in
    /// the tufts' order, and one that would stand in another already
    /// there, or in the bank beside it, is left out: reeds stand a little
    /// apart, as plants do.
    fn reeds_of(&self, candidates: &[Tuft]) -> (Vec<(CellBrick, u8)>, HashMap<[i32; 2], Vec<(i32, i32)>>) {
        let g = self.ground;
        let (mut bricks, mut stems): (Vec<(CellBrick, u8)>, HashMap<[i32; 2], Vec<(i32, i32)>>) = (Vec::new(), HashMap::new());
        // whether a brick stands clear of the ground and of every reed so far
        let clear = |b: &CellBrick, stems: &HashMap<[i32; 2], Vec<(i32, i32)>>| b.ext[2] > 0 && g.index(b.min[0], b.min[1]).is_none_or(|c| b.min[2] >= g.top[c]) && stems.get(&[b.min[0], b.min[1]]).is_none_or(|taken| taken.iter().all(|(from, to)| b.min[2] >= *to || b.min[2] + b.ext[2] <= *from));
        for tuft in candidates {
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
                let wet = g.flags[c] & WET != 0;
                match g.at(x, y).map_or(0, |place| place.cliff) {
                    0 => solid.push((CellBrick { mask: CUBE, min: [x, y, under], ext: [1, 1, top - under] }, g.skin[c])),
                    cliff => self.beds([x, y], under, top, cliff == 2, wet, &mut solid),
                }
                let reeded = g.flags[c] & REED != 0;
                // (a stone only in water, and never taller than three cells whatever it lies on)
                let stone = if wet { stones[slot(x, y)].clamp(top, top + 3) } else { top };
                let surface = if wet { g.over[c].max(stone) } else { stone };
                // (white on top where a fall lands, in place of the water's top cell, and not under water that is still coming down)
                let white = if wet && g.flags[c] & FOAMS != 0 && g.over[c] == g.level[c] && surface > stone { surface - 1 } else { surface };
                for (what, from, to) in [(STONE, top, stone), (WATER, stone, white), (FOAM, white, surface)] {
                    // (a reed standing here has its own cells, and nothing else is laid in them)
                    for (from, to) in if reeded { self.free_of_reeds([x, y], from, to) } else { vec![(from, to)] }.into_iter().filter(|(from, to)| from < to) {
                        if what == WATER {
                            water.push(([x, y], from, to));
                        } else {
                            solid.push((CellBrick { mask: CUBE, min: [x, y, from], ext: [1, 1, to - from] }, what));
                        }
                    }
                }
                // (and none over rock, though one may lean that way from the grass beside it)
                floor[slot(x, y)] = if g.at(x, y).is_some_and(|place| place.cliff == 2) { i32::MAX } else { surface };
            }
        }
        let mut bricks = crate::pattern::joined(solid, self.longest.max(1));
        self.blades(lo, &floor, &mut bricks);
        (bricks, water)
    }

    /// The soil or the rock under one cell of a cliff, from `under` to
    /// before `top`, added to `solid`.
    ///
    /// A cliff is in beds, as a cut bank or a rock face is: level layers,
    /// each of one tone, that rise and dip a little across the ground
    /// (by [`wander`]). Rock is in beds three cells thick, mostly of one
    /// grey with darker and paler ones among them, and its top cell is a
    /// ledge, a part of its own: weathered in summer, and what snow lies
    /// on in winter. Soil is in beds two thick, the soil's own tone and
    /// a darker one. Columns side by side are in the same bed at the
    /// same height, so the beds join into long slabs
    /// (`pattern::joined`), which is what makes a face read as rock and
    /// not as grey soil: a face of one tone, a column to a cell, was a
    /// grey hill.
    ///
    /// Rock's beds rise and dip as its ledges do (`LEDGES`, which is two
    /// beds), so a ledge's top is the top of a bed; and a column of rock
    /// goes down to the foot of the bed its foot is in, a cell or so
    /// further into the hill than its skin need go. Every piece of a
    /// column is then a whole bed, or the half bed under a ledge, and is
    /// the same piece in the columns beside it: left to end where the
    /// column happened to, a face was 2.5 cells of rock to a brick (2,225
    /// cells in 874 bricks), and so it is 3.4 (2,635 in 766).
    fn beds(&self, at: [i32; 2], under: i32, top: i32, rock: bool, wet: bool, solid: &mut Vec<(CellBrick, u8)>) {
        let g = self.ground;
        let (thick, which) = if rock { (2, 40) } else { (2, 41) };
        let here = [at[0] - g.stand.x.floor() as i32, at[1] - g.stand.y.floor() as i32];
        let lift = if rock { (LEDGES as f64 * wander(here[0], here[1], 32, self.seed)) as i32 } else { (thick as f64 * 2.0 * wander(here[0], here[1], 31, self.seed)) as i32 };
        // (the ledge on a rock's top, unless water lies on it)
        let ledge = if rock && !wet && top > under { top - 1 } else { top };
        let mut z = if rock { (under + lift).div_euclid(thick) * thick - lift } else { under };
        while z < ledge {
            let bed = (z + lift).div_euclid(thick);
            let end = ((bed + 1) * thick - lift).min(ledge);
            let tone = hash(bed, 0, which, self.seed);
            let shade = match (rock, tone) {
                (true, t) if t < 0.5 => ROCK,
                (true, t) if t < 0.8 => ROCK_DARK,
                (true, _) => ROCK_LIGHT,
                (false, t) if t < 0.45 => CLIFF,
                (false, _) => TRUNK,
            };
            solid.push((CellBrick { mask: CUBE, min: [at[0], at[1], z], ext: [1, 1, end - z] }, shade));
            z = end;
        }
        if ledge < top {
            solid.push((CellBrick { mask: CUBE, min: [at[0], at[1], ledge], ext: [1, 1, top - ledge] }, LEDGE));
        }
    }

    /// The blades standing in the tile whose first cell is `lo`, added to
    /// `bricks`; `floor` is the height blades may be from over each cell
    /// of it, and `i32::MAX` where there is no lawn.
    ///
    /// The blades are in tufts, three to five to a tuft, each on a cell of
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
        let reach = self.reach();
        for tuft in self.tufts([lo[0] - reach, lo[1] - reach], [hi[0] + reach, hi[1] + reach]) {
            let Some(c) = g.index(tuft.cell[0], tuft.cell[1]).filter(|c| g.flags[*c] & COVERED != 0) else { continue };
            let number = |n: i32| hash(tuft.a, tuft.b, n, self.seed);
            let here = g.high[c];
            // (no grass on rock, nor where water falls; and less of it the nearer the ground is to a cliff: a tuft has one number, and stands only on ground less bare than that)
            let hardy = number(13) * 255.0;
            if g.flags[c] & ROCKY != 0 || (g.bare[c] > 0 && hardy < g.bare[c] as f64) {
                continue;
            }
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
                // (it stands on the ground of its own cell, which is lawn and not under water, nor rock)
                let Some(base) = g.index(cell[0], cell[1]).filter(|n| g.flags[*n] & (COVERED | WET | ROCKY) == COVERED && (g.bare[*n] == 0 || hardy >= g.bare[*n] as f64)).map(|n| g.top[n]) else { continue };
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
            // (and not on a cliff: a rock's ledges are square, and a bank of sand on a face is no rock)
            if let ([way], Some(0)) = (&ways[..], g.at(x, y).map(|place| place.cliff)) {
                slopes.push((CellBrick { mask: tip(*way), min: [x, y, top], ext: [1; 3] }, g.skin[c]));
            }
        }
        bricks.extend(crate::pattern::joined(slopes, self.longest.max(1)));
    }
}

/// The volume of some bricks and the box they fill, kept with a tile so
/// that a lawn's can be had from its tiles' without going through its
/// bricks.
#[derive(Clone)]
struct Sums {
    volume: f64,
    /// whether every brick's volume is a whole number of half cells: such volumes add up to the same number in any order
    halves: bool,
    lo: [i32; 3],
    hi: [i32; 3],
}

impl Default for Sums {
    fn default() -> Sums {
        Sums { volume: 0.0, halves: true, lo: [i32::MAX; 3], hi: [i32::MIN; 3] }
    }
}

impl Sums {
    fn of(bricks: &[CellBrick]) -> Sums {
        bricks.iter().fold(Sums::default(), |sums, b| {
            let volume = b.volume();
            Sums { volume: sums.volume + volume, halves: sums.halves && (volume * 2.0).fract() == 0.0, lo: [0, 1, 2].map(|k| sums.lo[k].min(b.min[k])), hi: [0, 1, 2].map(|k| sums.hi[k].max(b.min[k] + b.ext[k])) }
        })
    }

    fn with(&self, other: &Sums) -> Sums {
        Sums { volume: self.volume + other.volume, halves: self.halves && other.halves, lo: [0, 1, 2].map(|k| self.lo[k].min(other.lo[k])), hi: [0, 1, 2].map(|k| self.hi[k].max(other.hi[k])) }
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
    pub ground: Arc<Ground>,
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
    ///
    /// Made from what was kept of the lawn's last build, where there is
    /// one (see [`kept`]): the same bricks in the same order as
    /// [`Model::fresh`] makes from nothing, for less work.
    pub fn new(tree: &Tree, base: DVec3, height: f64, spread: f64, lay: &Lay, longest: i32, threads: Threads) -> Arc<Model> {
        let site = Site::of(base, spread, lay, seed_of(tree));
        kept::with(&site, |lawn| Model::laid(lawn, tree, base, height, spread, lay, longest, threads))
    }

    /// [`Model::new`], from nothing kept and keeping nothing: what a lawn
    /// is, by which the keeping is checked.
    pub fn fresh(tree: &Tree, base: DVec3, height: f64, spread: f64, lay: &Lay, longest: i32, threads: Threads) -> Arc<Model> {
        Model::laid(&mut kept::Lawn::default(), tree, base, height, spread, lay, longest, threads)
    }

    /// The lawn, with `lawn` holding what was kept of its last build
    /// going in and of this one coming out. There is one way to build a
    /// lawn and this is it: from nothing, `lawn` is empty, every cell of
    /// the ground is worked out and every tile laid.
    #[allow(clippy::too_many_arguments)]
    fn laid(lawn: &mut kept::Lawn, tree: &Tree, base: DVec3, height: f64, spread: f64, lay: &Lay, longest: i32, threads: Threads) -> Arc<Model> {
        let tree = tree.clamped();
        let height = height.max(Tree::least_height(tree.kind));
        let seed = seed_of(&tree);
        let wind = TAU * hash(0, 0, 1, seed);
        let none = HashMap::new();
        let (gap, lean, downwind) = (3.4 - 1.9 * tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0, DVec3::new(wind.cos(), wind.sin(), 0.0));
        let site = Site::of(base, spread, lay, seed);
        if lawn.site.as_ref() != Some(&site) {
            *lawn = kept::Lawn::default();
            lawn.site = Some(site.clone());
        }
        // (the rule but for its ground and reeds, which are not yet known: its numbers are)
        let blank = Ground { made: None, stand: DVec3::ZERO, lo: [0; 2], n: [0; 2], high: Vec::new(), top: Vec::new(), level: Vec::new(), over: Vec::new(), skin: Vec::new(), flags: Vec::new(), stone: Vec::new(), face: Vec::new(), bare: Vec::new(), watered: false };
        let sown = Rule { ground: &blank, seed, height, gap, lean, downwind, reeds: lay.reeds, longest, stems: &none }.bits();
        // The very lawn that was built last, as when a lawn built while
        // the mouse was down is built again on letting go, or its parts
        // are asked for to write the game's file: it is that lawn.
        let same_touches = |made: &Made| made.touches.len() == lay.mounds.len() && made.touches.iter().zip(&lay.mounds).all(|(a, b)| same_touch(a, b));
        lawn.laid = 0;
        if let Some((_, model)) = lawn.last.as_ref().filter(|(was, model)| *was == sown && model.ground.made.as_ref().is_some_and(|made| made.site == site && same_touches(made))) {
            return model.clone();
        }
        // (dropped before the ground is taken, which the model has a hold of: with nobody else holding it, the ground is changed where it lies and not copied)
        lawn.last = None;
        let before = lawn.ground.take().map(|ground| Arc::try_unwrap(ground).unwrap_or_else(|held| (*held).clone()));
        let (mut ground, redone) = Ground::after(before, &tree, base, spread, lay, threads);
        // The reeds. The tufts one may stand at are kept, and asked again
        // only in the cells whose ground was worked out again; which of
        // them do stand is settled over the whole lawn each time, in the
        // tufts' order, since each gives way to those before it. They are
        // few: on a lawn 600 cells by 480, asking every tuft was 2.7 ms,
        // and settling the reeds is the work of the reeds there are.
        let bare = Rule { ground: &ground, seed, height, gap, lean, downwind, reeds: lay.reeds, longest, stems: &none };
        let candidates = match lawn.tufts.take().filter(|(was, _)| *was == sown) {
            Some((_, mut were)) => {
                were.retain(|tuft| !(0..2).all(|k| tuft.cell[k] >= redone[0][k] && tuft.cell[k] < redone[1][k]));
                were.extend(bare.candidates(redone[0], redone[1]));
                were.sort_by_key(|tuft| (tuft.b, tuft.a));
                were
            }
            None => {
                let (lo, hi) = ground.cells();
                bare.candidates(lo, hi)
            }
        };
        let (reeds, stems) = bare.reeds_of(&candidates);
        lawn.tufts = Some((sown, candidates));
        let marked: Vec<u32> = stems.keys().filter_map(|at| ground.index(at[0], at[1])).map(|c| c as u32).collect();
        for c in &marked {
            ground.flags[*c as usize] |= REED;
        }
        if let Some(made) = ground.made.as_mut() {
            made.reeded = marked;
        }
        let rule = Rule { ground: &ground, seed, height, gap, lean, downwind, reeds: lay.reeds, longest, stems: &stems };
        // the tiles: those of the world's own grid of them that the lawn reaches
        let (lo, hi) = ground.cells();
        let (first, last) = ([lo[0].div_euclid(TILE), lo[1].div_euclid(TILE)], [(hi[0] - 1).div_euclid(TILE), (hi[1] - 1).div_euclid(TILE)]);
        let across = (last[0] - first[0] + 1) as usize;
        let count = across * (last[1] - first[1] + 1) as usize;
        let place = |n: usize| [first[0] + (n % across) as i32, first[1] + (n / across) as i32];
        // What a tile is laid from: the ground of its own cells and of
        // those as far round it as anything in it reads (a blade's reach,
        // and the two cells a blade may stand from its tuft, and one
        // more), and the reeds over its own cells.
        let margin = rule.reach() + 3;
        let around = |tile: [i32; 2]| ([tile[0] * TILE - margin, tile[1] * TILE - margin], [(tile[0] + 1) * TILE + margin, (tile[1] + 1) * TILE + margin]);
        let mut reeded: HashMap<[i32; 2], kept::Stems> = HashMap::new();
        for (at, taken) in &stems {
            reeded.entry([at[0].div_euclid(TILE), at[1].div_euclid(TILE)]).or_default().push((*at, taken.clone()));
        }
        reeded.values_mut().for_each(|over| over.sort());
        let no_reeds = kept::Stems::new();
        let stems_over = |tile: [i32; 2]| reeded.get(&tile).unwrap_or(&no_reeds);
        // The tiles kept that are no longer what would be laid. (A tile
        // kept takes a few microseconds to compare, so a hundred of them
        // are worth a thread, and a lawn of fewer is gone through here.)
        let kept_tiles = &lawn.tiles;
        let fresh = threads.map(count, 100, |n| {
            let tile = place(n);
            let (from, to) = around(tile);
            kept_tiles.get(&tile).is_some_and(|kept| kept.rule == sown && kept.stems == *stems_over(tile) && ground.same_cut(&kept.cut, from, to))
        });
        let stale: Vec<usize> = (0..count).filter(|n| !fresh[*n]).collect();
        let laid = threads.map(stale.len(), 2, |k| {
            let tile = place(stale[k]);
            let (from, to) = around(tile);
            let cut = ground.cut(from, to);
            let over: HashMap<[i32; 2], Vec<(i32, i32)>> = stems_over(tile).iter().cloned().collect();
            // (from the piece cut out, and from nothing else: what is kept with the tile is all it was laid from)
            let (solid, water) = Rule { ground: &cut, stems: &over, ..rule }.tile([tile[0] * TILE, tile[1] * TILE]);
            let (bricks, shades): (Vec<CellBrick>, Vec<u8>) = solid.into_iter().unzip();
            let sums = Sums::of(&bricks);
            kept::Tile { rule: sown, cut, stems: stems_over(tile).clone(), bricks, shades, water, sums }
        });
        lawn.laid = stale.len();
        for (n, tile) in stale.iter().zip(laid) {
            lawn.tiles.insert(place(*n), tile);
        }
        // (in the tiles' own order, then the water, then the reeds: the same whichever thread laid which, and whichever were kept)
        let tiles: Vec<&kept::Tile> = (0..count).map(|n| &lawn.tiles[&place(n)]).collect();
        let water: Vec<Standing> = tiles.iter().flat_map(|tile| tile.water.iter().copied()).collect();
        // (the water is joined over the whole lawn, so it is joined again only when some of it has changed)
        let pool = match lawn.pool.take().filter(|(was, long, _)| *long == longest && *was == water) {
            Some((_, _, pool)) => pool,
            None => pooled(&water, longest),
        };
        let rest = pool.len() + reeds.len();
        let all = tiles.iter().map(|tile| tile.bricks.len()).sum::<usize>() + rest;
        let (mut bricks, mut shades) = (Vec::with_capacity(all), Vec::with_capacity(all));
        for tile in &tiles {
            bricks.extend_from_slice(&tile.bricks);
            shades.extend_from_slice(&tile.shades);
        }
        for (brick, shade) in pool.iter().chain(&reeds) {
            bricks.push(*brick);
            shades.push(*shade);
        }
        lawn.pool = Some((water, longest, pool));
        // Its volume and its box, from each tile's own, which were added
        // up when the tile was laid: gone through brick by brick, a lawn
        // of 335,000 bricks took 1.6 ms a build for these two alone.
        let tail = Sums::of(&bricks[bricks.len() - rest..]);
        let sums = tiles.iter().map(|tile| &tile.sums).chain([&tail]).fold(Sums::default(), |all, part| all.with(part));
        // (A volume is added up brick by brick in the bricks' order, and
        // adding it up a tile at a time is the same number only while
        // every sum on the way is exact, which it is where each brick's
        // volume is so many halves of a cell: a box, or a box less an
        // edge. Should a lawn come to have a brick that is not, it is
        // added up in order as it always was.)
        let volume = if sums.halves { sums.volume } else { bricks.iter().map(|b| b.volume()).sum() };
        let (lo, hi) = if bricks.is_empty() { ([0; 3], [0; 3]) } else { (sums.lo, sums.hi) };
        let ground = Arc::new(ground);
        let model = Arc::new(Model { bricks, shades, volume, ground: ground.clone(), lo, hi, index: OnceLock::new() });
        lawn.ground = Some(ground);
        lawn.last = Some((sown, model.clone()));
        model
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
