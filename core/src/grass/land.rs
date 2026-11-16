//! A landscape for a lawn, composed from a seed: touches of the brush set
//! down with intent, and the plants that belong on the ground they make.
//!
//! A lawn's ground is whatever its touches make it ([`crate::tree::ground`]),
//! so a landscape is a list of touches, the same touches a hand would
//! make with the brush: nothing here is a new kind of ground, and what it
//! makes can be changed afterwards one touch at a time. Noise was the
//! other way to make ground at random (a sum of octaves of
//! [value noise](https://en.wikipedia.org/wiki/Value_noise) is the usual
//! one), and was not taken: it would need a grid of heights kept in the
//! form, where a form keeps a few numbers a person can read, and noise
//! has no intent. A hill it makes is not a hill anyone put there, and it
//! puts a pond above a cliff above a pond only by luck.
//!
//! What is composed, each scaled to the patch (`u` below is the square
//! root of the patch's area, its width if it were square):
//!
//! - **The fall.** A table of raised ground with a sharp edge, a hollow
//!   on it by its rim, and a hollow in the low ground under that rim. The
//!   upper hollow is a raised pond, its stream goes out through the rim
//!   and over the table's side, which is a cliff, and lands by the lower
//!   pond ([`super::Waters`]). It is placed first and everything else
//!   keeps clear of it.
//! - **A high side and a low side.** One broad push beyond the patch's
//!   edge on the table's side tilts the whole patch a few cells.
//! - **A ridge.** Two or three overlapping hills on the high side, of
//!   differing sizes, some steep enough for bare soil.
//! - **Mounds**, one for every 6,000 cells of the patch (one to seven),
//!   each set where it is furthest from what is there already.
//! - **Ponds.** One for every 9,000 cells and one more, two at the least
//!   (the fall's) and six at the most. The others are by turns a tarn, a
//!   hill with a hollow in its top whose stream runs down to lower
//!   water, and a mere, a hollow in the low ground.
//! - **A river.** A chain of small hollows from the fall's lower pond
//!   to the low edge of the patch, which is one winding water at the
//!   lawn's own level.
//!
//! Then the ground is made ([`crate::spec::lawn_ground`], a millisecond
//! or two) and looked at: if no fall of [`TALL`] cells or more is there,
//! the fall is composed again with a higher table, another way round or
//! smaller, up to [`TRIES`] times, each from the same seed and the number
//! of the try, so that the same seed always ends at the same landscape.
//! A patch too small or too narrow for any of them (under about 28 cells
//! across, which is an outline of 20 and its reach of 4 to each side) is
//! given the try that came nearest, and [`Landscape::fall`] says there
//! is none.

use super::Ground;
use crate::spec::{Spec, SHARP};
use crate::tree::Kind;
use glam::{DVec2, DVec3};
use std::f64::consts::TAU;

/// How many cells the water must come down, from the level it falls from
/// to the level it lands at, for a landscape to have its fall.
pub const TALL: i32 = 6;

/// How many times the fall is composed before the last is kept.
pub const TRIES: u32 = 10;

/// The most plants a landscape is given, and the most bricks they are
/// reckoned to be between them (see [`planting`]).
pub const MOST_PLANTS: usize = 40;
pub const MOST_BRICKS: f64 = 260_000.0;

/// Numbers from a seed, the same on every machine: SplitMix64
/// (<https://prng.di.unimi.it/splitmix64.c>, public domain). One run of
/// numbers and not a number for each place, as a lawn's blades have:
/// what is composed here is a short list made in one order, on one
/// thread, and nothing has to be the same when another part changes.
struct Numbers(u64);

impl Numbers {
    fn new(seed: u64, which: u64) -> Numbers {
        Numbers(seed ^ which.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5EED_1A2D)
    }

    /// The next number, from 0 up to but not 1.
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }

    fn whole(&mut self) -> u64 {
        (self.next() * (1u64 << 32) as f64) as u64
    }
}

/// The patch a landscape is composed on: which cells the lawn covers.
struct Patch {
    lo: [i32; 2],
    n: [usize; 2],
    covered: Vec<bool>,
    /// how many cells it covers, and the middle of them
    cells: f64,
    middle: DVec2,
}

impl Patch {
    fn of(ground: &Ground) -> Patch {
        let (lo, hi) = ground.cells();
        let n = [(hi[0] - lo[0]) as usize, (hi[1] - lo[1]) as usize];
        let covered: Vec<bool> = (0..n[0] * n[1]).map(|c| ground.at(lo[0] + (c % n[0]) as i32, lo[1] + (c / n[0]) as i32).is_some_and(|place| place.covered)).collect();
        let (mut cells, mut sum) = (0.0, DVec2::ZERO);
        for c in (0..covered.len()).filter(|c| covered[*c]) {
            cells += 1.0;
            sum += DVec2::new(lo[0] as f64 + (c % n[0]) as f64 + 0.5, lo[1] as f64 + (c / n[0]) as f64 + 0.5);
        }
        Patch { lo, n, covered, cells, middle: if cells > 0.0 { sum / cells } else { DVec2::ZERO } }
    }

    fn covers(&self, p: DVec2) -> bool {
        let (i, j) = (p.x.floor() as i32 - self.lo[0], p.y.floor() as i32 - self.lo[1]);
        i >= 0 && j >= 0 && (i as usize) < self.n[0] && (j as usize) < self.n[1] && self.covered[j as usize * self.n[0] + i as usize]
    }

    /// Whether the lawn covers a round place: its middle and eight places on its rim.
    fn holds(&self, p: DVec2, r: f64) -> bool {
        self.covers(p) && (0..8).all(|k| self.covers(p + DVec2::from_angle(TAU * k as f64 / 8.0) * r))
    }

    /// How far the patch reaches each way along `way` from its middle: the least and the most.
    fn reach(&self, way: DVec2) -> (f64, f64) {
        let mut span = (f64::MAX, f64::MIN);
        for c in (0..self.covered.len()).filter(|c| self.covered[*c]) {
            let s = (DVec2::new(self.lo[0] as f64 + (c % self.n[0]) as f64 + 0.5, self.lo[1] as f64 + (c / self.n[0]) as f64 + 0.5) - self.middle).dot(way);
            span = (span.0.min(s), span.1.max(s));
        }
        span
    }
}

/// One touch of the brush, before it is written as a form keeps it.
#[derive(Clone, Copy)]
struct Touch {
    at: DVec2,
    range: f64,
    amount: f64,
    sharp: bool,
}

/// A landscape composed for a lawn (see [`compose`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Landscape {
    /// its touches, as the lawn's form keeps them (`Spec::pulls`)
    pub pulls: Vec<(DVec3, DVec3)>,
    /// how many ponds were dug: the fall's two, and the tarns and meres
    pub ponds: usize,
    /// how far the water falls, in cells, if there is a fall of [`TALL`] or more
    pub fall: Option<i32>,
    /// which try this was, from 0
    pub tries: u32,
}

/// What a lawn's settings must be for a landscape to be what was
/// composed: raised ponds on, water standing somewhere below the lawn,
/// and cliffs.
pub fn settings(lawn: &Spec) -> Spec {
    let mut lawn = lawn.clone();
    lawn.tube = true;
    lawn.tree.baubles = lawn.tree.baubles.max(2);
    if lawn.tree.count <= 0 {
        lawn.tree.count = 3;
    }
    lawn
}

/// How far the water of a ground's falls comes down, from the highest
/// level any falls from to the lowest any lands at: `None` if no stream
/// drops two cells at a step anywhere.
pub fn fall_of(ground: &Ground) -> Option<i32> {
    let (lo, hi) = ground.cells();
    let mut span: Option<(i32, i32)> = None;
    for y in lo[1]..hi[1] {
        for x in lo[0]..hi[0] {
            let Some(place) = ground.at(x, y) else { continue };
            if let (Some(from), Some(level)) = (place.falls_from, place.water) {
                if from - level >= 2 {
                    span = Some(span.map_or((from, level), |(top, foot)| (top.max(from), foot.min(level))));
                }
            }
        }
    }
    span.map(|(top, foot)| top - foot)
}

/// The landscape `seed` gives the lawn `lawn`: its touches, in place of
/// any the lawn has. The lawn's outline, where it stands and how far
/// below it water stands are the lawn's; its touches and the rest of its
/// settings are not read. The same on any machine and any number of
/// threads: the ground it looks at is ([`Ground`]), and the rest is one
/// run of numbers.
pub fn compose(lawn: &Spec, seed: u64) -> Landscape {
    let lawn = settings(lawn);
    let flat = Spec { pulls: Vec::new(), ..lawn.clone() }.snapped();
    let none = Landscape { pulls: Vec::new(), ponds: 0, fall: None, tries: 0 };
    let Some(ground) = crate::spec::lawn_ground(&flat) else { return none };
    let patch = Patch::of(&ground);
    let Some(floor) = flat.points.first().map(|p| p.z) else { return none };
    if patch.cells < 1.0 {
        return none;
    }
    let water = flat.tree.count.max(1) as f64;
    let mut best: Option<Landscape> = None;
    for tries in 0..TRIES {
        let (touches, ponds) = laid_out(&patch, water, seed, tries);
        // (as a form keeps a touch: where, on the lawn's own level, and its range, what it does and its amount, in whole and half cells)
        let half = |v: f64| (v * 2.0).round() / 2.0;
        let pulls: Vec<(DVec3, DVec3)> = touches.iter().map(|touch| (DVec3::new(touch.at.x.floor() + 0.5, touch.at.y.floor() + 0.5, floor), DVec3::new(half(touch.range).max(crate::tree::LEAST_RANGE), if touch.sharp { SHARP } else { 0.0 }, half(touch.amount)))).collect();
        let fall = crate::spec::lawn_ground(&Spec { pulls: pulls.clone(), ..flat.clone() }).as_ref().and_then(fall_of);
        let made = Landscape { pulls, ponds, fall: fall.filter(|tall| *tall >= TALL), tries };
        if made.fall.is_some() {
            return made;
        }
        // (kept if it is the tallest so far, to have something to give a patch no fall fits on)
        if best.as_ref().is_none_or(|was| fall.unwrap_or(0) > was.fall.unwrap_or(0)) || best.is_none() {
            best = Some(Landscape { fall: None, ..made });
        }
    }
    best.unwrap_or(none)
}

/// The touches of try `tries` at a landscape for `patch`, and how many ponds they dig.
fn laid_out(patch: &Patch, water: f64, seed: u64, tries: u32) -> (Vec<Touch>, usize) {
    let u = patch.cells.sqrt();
    // (the fall from numbers of its own, one run for each try; the rest from one run whatever the try, so that a second try moves the fall and leaves the hills)
    let mut fall = Numbers::new(seed, 1 + tries as u64);
    let mut rest = Numbers::new(seed, 0);
    let mut touches: Vec<Touch> = Vec::new();
    // Which way is uphill: any way at first, and after two tries with a
    // higher table, round by a little over a third of a turn each try,
    // which goes through every side of a patch that has room one way only.
    let turn = TAU * Numbers::new(seed, 0x51DE).next() + (tries.saturating_sub(2) as f64) * 2.4;
    let up = DVec2::from_angle(turn);
    let across = DVec2::new(-up.y, up.x);
    let (low, high) = patch.reach(up);
    let (left, right) = patch.reach(across);
    // The fall: smaller on later tries, to fit where the first did not.
    let shrink = 1.0 - 0.07 * tries.saturating_sub(4) as f64;
    let table = (0.2 * u).clamp(11.0, 40.0) * shrink;
    let lower = (0.17 * u).clamp(9.0, 30.0) * shrink;
    let upper = (0.36 * table).clamp(5.5, 11.0);
    let band = (0.2 * table).max(2.0);
    let tall = (0.7 * table).clamp(10.0, 30.0) + 2.0 * tries.min(3) as f64;
    // along the way uphill, from the low end: the lower pond, a little flat, the table's edge, and the whole of the table, which has the upper pond on it by that edge
    let long = lower * 1.6 + 3.0 + 2.0 * table + 2.0;
    let slack = (high - low - 4.0 - long).max(0.0);
    let foot = low + 2.0 + lower + slack * fall.between(0.3, 1.0);
    let edge = foot + 0.6 * lower + 3.0;
    let (at_table, at_upper) = (edge + table, edge + band + 1.5 + upper);
    // (to one side or the other of the middle, where the table and both ponds are on the lawn: a few places are tried)
    let room = ((right - left) / 2.0 - table.max(lower) - 3.0).max(0.0);
    let aside = (0..8).map(|_| (left + right) / 2.0 + room * fall.between(-1.0, 1.0)).find(|aside| patch.holds(patch.middle + up * foot + across * *aside, lower * 0.8) && patch.holds(patch.middle + up * at_table + across * *aside, table)).unwrap_or((left + right) / 2.0);
    let on = |along: f64, aside: f64| patch.middle + up * along + across * aside;
    let (pool, tarn) = (on(foot, aside), on(at_upper, aside));
    // the tilt first, under everything: a broad hill beyond the high edge
    touches.push(Touch { at: on(high + 0.2 * (high - low), (left + right) / 2.0), range: 1.2 * (high - low), amount: (0.05 * u).clamp(3.0, 8.0), sharp: false });
    touches.push(Touch { at: on(at_table, aside), range: table, amount: tall, sharp: true });
    touches.push(Touch { at: tarn, range: upper, amount: -(water + 3.0 + fall.between(0.0, 2.0)), sharp: false });
    // (deep for its width: water stands where the hollow is more than a cell under the level, and a shallow bowl is a wide ring of sand round a puddle)
    touches.push(Touch { at: pool, range: lower, amount: -(water + 8.0 + fall.between(0.0, 4.0)), sharp: false });
    let mut ponds = 2;
    // What is there already, as round places to keep clear of: the table, the pool, and the way between them.
    let mut taken: Vec<(DVec2, f64)> = vec![(on(at_table, aside), table), (pool, lower), (on(edge, aside), 6.0)];
    // the furthest of a dozen places from everything there, for something `range` across; `None` if none of them is on the lawn
    let place = |numbers: &mut Numbers, taken: &[(DVec2, f64)], range: f64, lowest: f64, highest: f64| {
        (0..12)
            .map(|_| on(numbers.between(lowest, highest), numbers.between(left, right)))
            .filter(|p| patch.holds(*p, range * 0.5))
            .map(|p| (taken.iter().map(|(at, r)| p.distance(*at) - r - 0.8 * range).fold(f64::MAX, f64::min), p))
            .filter(|(clear, _)| *clear > 1.0)
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p)
    };
    // the river, from the pool down to the low edge: one small hollow every four and a half cells along a winding line
    let (mut along, phase, swing) = (foot - 0.6 * lower, rest.between(0.0, TAU), (0.04 * u).min(6.0));
    let mut river = 0;
    while along > low + 5.0 && touches.len() < 150 {
        let p = on(along, aside + swing * ((foot - along) * TAU / 40.0 + phase).sin() * ((foot - along) / 12.0).min(1.0));
        if !patch.holds(p, 6.0) {
            break;
        }
        touches.push(Touch { at: p, range: 9.0, amount: -(water + 3.0), sharp: false });
        taken.push((p, 7.0));
        along -= 5.0;
        river += 1;
    }
    let _ = river;
    // the ridge: two or three hills on the high side, to the other side of the middle from the table
    let side = if aside > (left + right) / 2.0 { -1.0 } else { 1.0 };
    let hills = 2 + (rest.next() * 2.0) as usize;
    for hill in 0..hills {
        let range = rest.between(0.14, 0.22) * u;
        let at = on(low + (high - low) * rest.between(0.5, 0.9), (left + right) / 2.0 + side * (right - left) * (0.12 + 0.13 * hill as f64) + rest.between(-0.05, 0.05) * u);
        if taken[..2].iter().all(|(p, r)| at.distance(*p) > r + 0.6 * range) && patch.covers(at) {
            touches.push(Touch { at, range, amount: range * rest.between(0.3, 0.5), sharp: false });
            taken.push((at, 0.6 * range));
        }
    }
    // the other ponds: by turns a tarn on a hill of its own and a mere in the low ground
    let wanted = ((patch.cells / 9000.0).round() as usize + 1).clamp(2, 6);
    for pond in 2..wanted {
        if pond % 2 == 0 {
            let range = (0.16 * u).clamp(14.0, 34.0) * rest.between(0.85, 1.1);
            let Some(at) = place(&mut rest, &taken, range, low + 0.25 * (high - low), high) else { continue };
            let hollow = 0.42 * range;
            // (the hill is 0.376 of its height lower at the hollow's rim than at its middle: the hollow is dug that much deeper, and as deep again as a pond need be)
            touches.push(Touch { at, range, amount: 0.5 * range, sharp: false });
            touches.push(Touch { at, range: hollow, amount: -(0.376 * 0.5 * range + water + 3.5), sharp: false });
            taken.push((at, range));
        } else {
            let range = (0.13 * u).clamp(9.0, 26.0) * rest.between(0.8, 1.2);
            let Some(at) = place(&mut rest, &taken, range, low, low + 0.55 * (high - low)) else { continue };
            touches.push(Touch { at, range, amount: -(water + rest.between(7.0, 11.0)), sharp: false });
            taken.push((at, range));
        }
        ponds += 1;
    }
    // the mounds, of differing sizes, wherever there is most room left
    for _ in 0..((patch.cells / 6000.0).round() as usize).clamp(1, 7) {
        let range = (rest.between(0.08, 0.16) * u).max(7.0);
        let Some(at) = place(&mut rest, &taken, range, low, high) else { continue };
        touches.push(Touch { at, range, amount: range * rest.between(0.28, 0.48), sharp: false });
        taken.push((at, 0.7 * range));
    }
    (touches, ponds)
}

/// One plant of a landscape: which, where it stands on the lawn, how
/// large, and which of its kind it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planted {
    pub kind: Kind,
    /// the cell of the lawn its foot is in, and the top of the soil there
    pub cell: [i32; 2],
    pub top: i32,
    pub height: f64,
    pub spread: f64,
    pub seed: u64,
}

/// About how many bricks a plant of a kind is at its usual size
/// ([`crate::tree::Tree::usual_size`]): from docs/performance.md where it
/// has the kind, and built once for the others. A plant's bricks go by
/// its surface, so one of another height is reckoned at these times the
/// square of the heights' ratio. Only to keep a landscape's plants
/// within [`MOST_BRICKS`]: nothing is built to find out.
fn usual_bricks(kind: Kind) -> f64 {
    match kind {
        Kind::Oak => 40_000.0,
        Kind::Beech => 48_000.0,
        Kind::Pine => 30_000.0,
        Kind::ScotsPine => 30_000.0,
        Kind::Birch => 22_000.0,
        Kind::Willow => 45_000.0,
        Kind::Cherry => 40_000.0,
        Kind::Boxwood => 8_500.0,
        Kind::Hydrangea => 17_000.0,
        Kind::Peony => 3_800.0,
        Kind::Sunflower => 9_000.0,
        Kind::Tulip => 5_900.0,
        Kind::Daisy => 4_000.0,
        Kind::Daffodil => 4_000.0,
        _ => 10_000.0,
    }
}

/// The plants that belong on a lawn's ground, for a landscape: trees,
/// bushes and flowers of several kinds and sizes, each where its kind
/// grows. `density` is how thickly, 0 for none to 100.
///
/// Where each grows. Nothing stands in water, on a cliff, on ground that
/// rises more than 0.45 for each cell along, within three cells of the
/// lawn's edge, or within ten cells of falling water, which a tree at
/// its foot would hide. A tree within seven cells of water and no more than
/// three above it is a willow. On ground over half as high as the
/// lawn's highest it is a pine, a Scots pine or a birch; on the rest, a
/// birch or a cherry, an oak too where the tree is 60 tall or more and
/// a beech from 85. Trees are tried at places thrown
/// over the lawn and kept if their crown would overlap no other tree's
/// by more than a fifth of the two spreads, so crowns touch and trunks
/// never do. Bushes and flowers stand at the edges of clearings: a
/// little way outside a tree's crown, on the side away from the other
/// trees, and a few by the water.
///
/// How many. A tree's height is 0.3 to 0.5 of `u`, the square root of
/// the lawn's area, between 40 and 130 cells, so that trees are in scale
/// with the patch: a lawn 180 by 120 has trees 44 to 74 tall. At a
/// density of 100 a tree is tried for every 1,400 cells of lawn, and
/// about as many bushes and half again as many flowers; fewer in
/// proportion below. Never more than [`MOST_PLANTS`], nor more bricks
/// than [`MOST_BRICKS`] as [`usual_bricks`] reckons them: the plants
/// last found are left out.
pub fn planting(ground: &Ground, seed: u64, density: i32) -> Vec<Planted> {
    let density = density.clamp(0, 100) as f64 / 100.0;
    let patch = Patch::of(ground);
    if density <= 0.0 || patch.cells < 1.0 {
        return Vec::new();
    }
    let u = patch.cells.sqrt();
    let mut numbers = Numbers::new(seed, 0xF011A6E);
    let (lo, hi) = ground.cells();
    let at = |p: DVec2| ground.at(p.x.floor() as i32, p.y.floor() as i32);
    let highest = (lo[1]..hi[1]).flat_map(|y| (lo[0]..hi[0]).map(move |x| (x, y))).filter_map(|(x, y)| ground.at(x, y)).filter(|place| place.covered).map(|place| place.height).fold(1.0, f64::max);
    // whether a plant may stand at a place, and whether water is near it
    // (and not within ten cells of falling water, on any side: a fall is to be seen, and a tree at its foot hides it)
    let falls: Vec<DVec2> = (lo[1]..hi[1]).flat_map(|y| (lo[0]..hi[0]).map(move |x| (x, y))).filter(|(x, y)| ground.at(*x, *y).is_some_and(|place| place.falls_from.zip(place.water).is_some_and(|(from, level)| from - level >= 2))).map(|(x, y)| DVec2::new(x as f64 + 0.5, y as f64 + 0.5)).collect();
    let stands = |p: DVec2| {
        let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
        at(p).is_some_and(|place| place.covered && place.water.is_none() && place.cliff == 0 && !place.stream) && ground.slope(x, y).is_some_and(|slope| slope.length() <= 0.45) && patch.holds(p, 3.0) && falls.iter().all(|fall| fall.distance(p) > 10.0)
    };
    let by_water = |p: DVec2| {
        let here = at(p).map_or(0.0, |place| place.height);
        (0..16).any(|k| (3..=7).step_by(2).any(|far| at(p + DVec2::from_angle(TAU * k as f64 / 16.0) * far as f64).is_some_and(|place| place.water.is_some_and(|level| here - (level as f64) <= 3.0 + 1.6))))
    };
    let usual = |kind: Kind| crate::tree::Tree::usual_size(kind);
    let mut plants: Vec<(Planted, DVec2)> = Vec::new();
    // the trees
    let tries = (density * patch.cells / 1400.0 * 3.0).ceil() as usize;
    let wanted = ((density * patch.cells / 1400.0).round() as usize).clamp(1, 18);
    let mut trees = 0;
    for _ in 0..tries {
        if trees >= wanted {
            break;
        }
        let p = DVec2::new(numbers.between(lo[0] as f64, hi[0] as f64), numbers.between(lo[1] as f64, hi[1] as f64));
        let (pick, size) = (numbers.next(), numbers.between(0.3, 0.5) * u);
        if !stands(p) {
            continue;
        }
        let high = at(p).map_or(0.0, |place| place.height) > 0.5 * highest && highest > 6.0;
        // (An oak or a beech only where it will be large: their leaves are in masses whose size goes with the tree's, and under 60 tall an oak is a trunk with a few tufts, and a beech under 85. A birch, a cherry, a pine and a willow are themselves at 40.)
        let kind = if by_water(p) {
            Kind::Willow
        } else if high {
            [Kind::Pine, Kind::ScotsPine, Kind::Birch, Kind::Pine][(pick * 4.0) as usize % 4]
        } else if size >= 85.0 {
            [Kind::Oak, Kind::Beech, Kind::Birch, Kind::Cherry, Kind::Oak][(pick * 5.0) as usize % 5]
        } else if size >= 60.0 {
            [Kind::Oak, Kind::Birch, Kind::Cherry, Kind::Birch][(pick * 4.0) as usize % 4]
        } else {
            [Kind::Birch, Kind::Cherry, Kind::Birch, Kind::Cherry][(pick * 4.0) as usize % 4]
        };
        let (tall, wide) = usual(kind);
        // (a willow is as wide as it is tall and a cherry nearly: they are kept lower, to be no wider than a pine twice their height)
        let squat = if matches!(kind, Kind::Willow | Kind::Cherry) { 0.75 } else { 1.0 };
        let height = (size * squat).clamp(40.0, 130.0).round();
        let spread = (wide * height / tall).round().max(4.0);
        // (nor may a tree's crown reach over a fall)
        if plants.iter().any(|(other, q)| p.distance(*q) < 0.8 * (spread + other.spread)) || falls.iter().any(|fall| fall.distance(p) < spread + 6.0) {
            continue;
        }
        let Some(place) = at(p) else { continue };
        plants.push((Planted { kind, cell: [p.x.floor() as i32, p.y.floor() as i32], top: place.top, height, spread, seed: numbers.whole() }, p));
        trees += 1;
    }
    // bushes and flowers at the edges of the clearings: outside a tree's crown, away from the trees nearest it
    let small = [Kind::Boxwood, Kind::Hydrangea, Kind::Boxwood, Kind::Tulip, Kind::Daisy, Kind::Daffodil, Kind::Peony, Kind::Sunflower, Kind::Tulip, Kind::Daisy];
    let wanted = ((density * (2.5 * trees as f64 + 2.0)).round() as usize).min(MOST_PLANTS.saturating_sub(plants.len()));
    let mut found = 0;
    for n in 0..wanted * 6 {
        if found >= wanted {
            break;
        }
        let kind = small[n % small.len()];
        let (tall, wide) = usual(kind);
        // (In scale with the trees: a bush a third to a half as tall as the trees are in the middle of their range, and a flower a quarter to a third. At their usual sizes a tulip is 64 tall, as tall as the trees of a lawn 180 by 120, and the first landscape had tulips standing among the oaks like street lamps.)
        let size = 0.4 * u * if matches!(kind, Kind::Boxwood | Kind::Hydrangea) { numbers.between(0.3, 0.5) } else { numbers.between(0.16, 0.26) };
        let height = size.clamp(9.0, tall).round();
        let spread = (wide * height / tall).round().max(2.0);
        let p = if trees > 0 && numbers.next() < 0.8 {
            let (tree, q) = plants[(numbers.next() * trees as f64) as usize % trees];
            q + DVec2::from_angle(numbers.between(0.0, TAU)) * (tree.spread * numbers.between(0.9, 1.3) + spread)
        } else {
            DVec2::new(numbers.between(lo[0] as f64, hi[0] as f64), numbers.between(lo[1] as f64, hi[1] as f64))
        };
        if !stands(p) || plants.iter().any(|(other, q)| p.distance(*q) < if other.spread > 16.0 { 0.75 * other.spread + spread } else { other.spread + spread + 1.0 }) {
            continue;
        }
        let Some(place) = at(p) else { continue };
        plants.push((Planted { kind, cell: [p.x.floor() as i32, p.y.floor() as i32], top: place.top, height, spread, seed: numbers.whole() }, p));
        found += 1;
    }
    // within the bounds: as many as fit, in the order they were found
    let mut bricks = 0.0;
    plants
        .into_iter()
        .map(|(plant, _)| plant)
        .take(MOST_PLANTS)
        .take_while(|plant| {
            bricks += usual_bricks(plant.kind) * (plant.height / usual(plant.kind).0).powi(2);
            bricks <= MOST_BRICKS
        })
        .collect()
}
