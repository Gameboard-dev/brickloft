//! Trees: a pine, a palm and an oak, each grown by the method that suits it.
//!
//! A tree is one solid, described as a distance function like every other
//! form (see `sdf`), and fitted by the cell-by-cell fitter (`fit_volume`).
//! The solid is the union of a few thousand simple parts: tapered round
//! branches, the eight-sided segments of a palm's trunk, and the rough
//! rounded masses of an oak's leaves. Each part carries a colour number, and
//! a brick takes the number of the part it lies deepest in.
//!
//! What the three have in common is the skeleton: a trunk that puts out
//! limbs, each described by a few numbers, after Weber and Penn, "Creation
//! and rendering of realistic trees" (1995):
//! https://courses.cs.duke.edu/fall02/cps124/resources/p119-weber.pdf
//! Their model is one recursive rule with some forty numbers per level. It
//! is not used whole here, because the three species differ in kind and not
//! only in numbers, and a rule general enough for all three describes none
//! of them closely:
//!
//! - A **pine** (a spruce or fir) has one straight stem that puts out a
//!   whorl of boughs each year. It is built tier by tier (see [`pine`]).
//! - A **palm** has one unbranched stem and a single crown of fronds, set
//!   round it by the golden angle (see [`palm`]).
//! - An **oak** has no leading stem above its short trunk: it divides again
//!   and again, and is the one tree here grown by a recursive rule (see
//!   [`oak`]).
//!
//! Space colonisation (Runions, Lane and Prusinkiewicz 2007) was the other
//! candidate for the oak:
//! https://algorithmicbotany.org/papers/colonization.egwnp2007.pdf
//! It grows branches towards points scattered through the crown, and gives
//! fine irregular broadleaf crowns. It was not taken, for two reasons. Its
//! branches are as thin as its node spacing at any size, where a tree in
//! bricks wants a few heavy limbs a cell or more thick, which a rule that
//! divides a limb's cross-section gives by construction. And every point
//! influences every branch near it, so a small change to a setting grows a
//! different tree, where here a seed is one tree whose height, spread and
//! foliage can be changed while it stays itself.
//!
//! Evaluating the union is the cost. Every part is filed in a grid of
//! buckets (see [`Model::new`]), as `sdf::PolyDist` files a path's segments,
//! so a sample is compared with the handful of parts near it and not with
//! all of them.

use crate::fit_volume;
use crate::pattern::Numbers;
use crate::shapes::{CellBrick, CUBE};
use glam::DVec3;
use std::f64::consts::{FRAC_1_SQRT_2, PI, TAU};

/// The golden angle, in radians: the turn between one leaf and the next
/// that never lines two leaves up, so each has the most light (Vogel 1979).
/// A coconut palm's fronds are set about 137.5 degrees apart.
/// https://en.wikipedia.org/wiki/Golden_angle
pub const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Pine,
    Palm,
    Oak,
}

/// The kinds on offer, with the names they are written and read by.
pub const KINDS: [(Kind, &str); 3] = [(Kind::Pine, "pine"), (Kind::Palm, "palm"), (Kind::Oak, "oak")];

/// How a tree is grown: the settings a form carries for it. Its height is
/// the form's `rise` and its spread the form's `radius`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tree {
    pub kind: Kind,
    /// which tree of its kind: any number, each a different one
    pub seed: u64,
    /// 0 to 100. A pine: how many boughs and how wide their sprays. A palm:
    /// how long its leaflets. An oak: how large its masses of leaves.
    pub fullness: i32,
    /// A pine: how many tiers of boughs. A palm: how many fronds. An oak:
    /// how many limbs its trunk divides into.
    pub count: i32,
    /// 0 to 100. A pine: how far its boughs droop. A palm: how far its
    /// trunk leans. An oak: how crooked its limbs are.
    pub bend: i32,
    /// A pine: a star and baubles. A palm: coconuts. An oak has none.
    pub extras: bool,
}

impl Tree {
    /// The tree of a kind as it starts out.
    pub fn of(kind: Kind) -> Tree {
        let (fullness, count, bend) = match kind {
            Kind::Pine => (60, 15, 50),
            Kind::Palm => (60, 20, 45),
            Kind::Oak => (60, 4, 50),
        };
        Tree { kind, seed: 1, fullness, count, bend, extras: false }
    }

    /// The fewest and most of whatever `count` counts, for a kind.
    pub fn count_range(kind: Kind) -> (i32, i32) {
        match kind {
            Kind::Pine => (4, 40),
            Kind::Palm => (5, 40),
            Kind::Oak => (2, 7),
        }
    }

    /// The same settings, each within what it may be: a tree read from a
    /// file or sent by another program may say anything.
    pub fn clamped(self) -> Tree {
        let (least, most) = Tree::count_range(self.kind);
        Tree { fullness: self.fullness.clamp(0, 100), count: self.count.clamp(least, most), bend: self.bend.clamp(0, 100), ..self }
    }

    /// The height and spread a tree of a kind starts at, in cells. Large,
    /// because a tree's detail is a cell or two across at any size: a
    /// small tree has room for its outline and nothing more.
    pub fn usual_size(kind: Kind) -> (f64, f64) {
        match kind {
            Kind::Pine => (140.0, 36.0),
            Kind::Palm => (120.0, 44.0),
            Kind::Oak => (110.0, 62.0),
        }
    }
}

impl Default for Tree {
    fn default() -> Self {
        Tree::of(Kind::Pine)
    }
}

/// The least a tree may be, in cells: below this there is no room for a
/// trunk with anything on it.
pub const LEAST_HEIGHT: f64 = 12.0;
pub const LEAST_SPREAD: f64 = 4.0;

// The colour numbers. 0 is the trunk, since the first colour of a set is
// the one a pattern's ground has to itself (see `pattern::colour_of`).
pub const TRUNK: u8 = 0;
/// leaves or needles, of the middle tone
pub const LEAF: u8 = 1;
/// leaves in shade: the inside of a bough, the underside of a mass, old fronds
pub const LEAF_DARK: u8 = 2;
/// new growth: the tips of sprays, the tops of masses, young fronds
pub const LEAF_LIGHT: u8 = 3;
/// limbs and boughs, and the rings round a palm's trunk
pub const LIMB: u8 = 4;
/// The top of a pine's boughs, which a snowy set of colours paints white;
/// the midrib of a palm's frond.
pub const TOP: u8 = 5;
/// baubles of one colour; coconuts
pub const EXTRA: u8 = 6;
/// the star, and baubles of another colour; the fibre under a palm's crown
pub const STAR: u8 = 7;

/// One simple solid of the many a tree is the union of.
///
/// A plain enum matched where it is used, and not a trait object: a fit
/// asks for some millions of distances, and a call through a table for each
/// part of each would be a large share of the time.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// A round cone: the solid swept by a ball moving from `a` to `a + ba`
    /// while its radius goes from `ra` to `rb`. Exact distance, after
    /// Quilez: https://iquilezles.org/articles/distfunctions/
    /// A tube with its radius merely blended along it is cheaper and is
    /// not a distance: it claims the surface of a steep taper is further
    /// off than it is, which the fitter's skipping of empty space trusts.
    Cone { a: DVec3, ba: DVec3, ra: f64, rb: f64, l2: f64, a2: f64 },
    /// An ellipsoid. `leafy` ones have the rough surface of a mass of
    /// leaves (see [`Model::rough`]).
    Blob { c: DVec3, inv: DVec3, least: f64, leafy: bool },
    /// A length of a palm's trunk: eight-sided in plan, from height
    /// `a.z` to `b.z`, its middle going from `a` to `b` and its size from
    /// `ra` to `rb` (the distance from the middle to a face). The section
    /// is taken level and not square to the lean, which keeps four of the
    /// eight faces on the grid's own planes whatever the lean; `slack`
    /// makes up for the lean, so the value is never more than the distance.
    Column { a: DVec3, b: DVec3, ra: f64, rb: f64, slack: f64 },
}

#[derive(Clone, Copy, Debug)]
struct Part {
    shape: Shape,
    shade: u8,
}

impl Shape {
    fn cone(a: DVec3, b: DVec3, ra: f64, rb: f64) -> Shape {
        let ba = b - a;
        let (l2, rr) = (ba.length_squared(), ra - rb);
        // one end's ball holds the other's: the whole is that ball
        if l2 <= rr * rr + 1e-9 {
            let (c, r) = if ra >= rb { (a, ra) } else { (b, rb) };
            return Shape::Blob { c, inv: DVec3::splat(1.0 / r), least: r, leafy: false };
        }
        Shape::Cone { a, ba, ra, rb, l2, a2: l2 - rr * rr }
    }

    fn distance(&self, p: DVec3) -> f64 {
        match *self {
            Shape::Cone { a, ba, ra, rb, l2, a2 } => {
                let (pa, rr, il2) = (p - a, ra - rb, 1.0 / l2);
                let y = pa.dot(ba);
                let z = y - l2;
                let x2 = (pa * l2 - ba * y).length_squared();
                let (y2, z2) = (y * y * l2, z * z * l2);
                let k = rr.signum() * rr * rr * x2;
                if z.signum() * a2 * z2 > k {
                    (x2 + z2).sqrt() * il2 - rb
                } else if y.signum() * a2 * y2 < k {
                    (x2 + y2).sqrt() * il2 - ra
                } else {
                    ((x2 * a2 * il2).sqrt() + y * rr) * il2 - ra
                }
            }
            // (as `sdf::Field::Ellipsoid`: exact for a ball, and never more than the distance)
            Shape::Blob { c, inv, least, .. } => (((p - c) * inv).length() - 1.0) * least,
            Shape::Column { a, b, ra, rb, slack } => {
                let t = ((p.z - a.z) / (b.z - a.z)).clamp(0.0, 1.0);
                let q = (p.truncate() - a.truncate().lerp(b.truncate(), t)).abs();
                let out = q.x.max(q.y).max((q.x + q.y) * FRAC_1_SQRT_2) - (ra + (rb - ra) * t);
                (out * slack).max(a.z - p.z).max(p.z - b.z)
            }
        }
    }

    /// A box that holds it.
    fn bounds(&self) -> (DVec3, DVec3) {
        match *self {
            Shape::Cone { a, ba, ra, rb, .. } => ((a - ra).min(a + ba - rb), (a + ra).max(a + ba + rb)),
            Shape::Blob { c, inv, .. } => (c - 1.0 / inv, c + 1.0 / inv),
            Shape::Column { a, b, ra, rb, .. } => {
                // (to its corners, which are further out than its faces)
                let r = ra.max(rb) * 1.09;
                (a.min(b) - DVec3::new(r, r, 0.0), a.max(b) + DVec3::new(r, r, 0.0))
            }
        }
    }
}

/// One limb of a tree's skeleton, for whoever wants to check a tree's
/// structure: the line it follows, and how thick it is at each point of it.
#[derive(Clone, Debug)]
pub struct Limb {
    pub path: Vec<DVec3>,
    pub radius: Vec<f64>,
    /// the limb it springs from; `None` for the trunk
    pub parent: Option<usize>,
    /// 0 for the trunk, 1 for what springs from it, and so on
    pub level: u8,
}

/// What a tree is grown into before it is a solid: its parts and skeleton.
struct Grown {
    parts: Vec<Part>,
    limbs: Vec<Limb>,
    /// each place a limb divides: its radius there, and the radii of what it divides into
    forks: Vec<(f64, Vec<f64>)>,
    numbers: Numbers,
}

impl Grown {
    fn unit(&mut self) -> f64 {
        self.numbers.unit()
    }

    /// From `low` to `high`.
    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    /// A direction, any at all, evenly.
    fn any_way(&mut self) -> DVec3 {
        let (z, turn) = (self.between(-1.0, 1.0), self.between(0.0, TAU));
        let flat = (1.0 - z * z).max(0.0).sqrt();
        DVec3::new(flat * turn.cos(), flat * turn.sin(), z)
    }

    fn cone(&mut self, a: DVec3, b: DVec3, ra: f64, rb: f64, shade: u8) {
        self.parts.push(Part { shape: Shape::cone(a, b, ra, rb), shade });
    }

    fn blob(&mut self, c: DVec3, radii: DVec3, shade: u8, leafy: bool) {
        self.parts.push(Part { shape: Shape::Blob { c, inv: 1.0 / radii, least: radii.min_element(), leafy }, shade });
    }

    /// A limb along `path`, as round cones end to end, and its record.
    fn limb(&mut self, path: Vec<DVec3>, radius: Vec<f64>, parent: Option<usize>, level: u8, shade: u8) -> usize {
        for i in 1..path.len() {
            self.cone(path[i - 1], path[i], radius[i - 1], radius[i], shade);
        }
        self.limbs.push(Limb { path, radius, parent, level });
        self.limbs.len() - 1
    }
}

/// Smallest bucket of the grid the parts are filed in, in cells, and the
/// most buckets it may have (as `sdf::PolyDist`).
const BUCKET: f64 = 4.0;
const MAX_BUCKETS: f64 = 2_000_000.0;

/// How near a bucket a part must come to be filed in it. A sample whose
/// bucket lists nothing is at least this far from the tree, which is all
/// the fitter asks of a far sample: over 0.87 of a cell, a cell's half
/// diagonal, it does not look closer.
const NEAR: f64 = 1.0;

/// How far the surface of a mass of leaves is pushed in and out, in cells,
/// and over what length the pushing changes. A smooth ellipsoid of bricks
/// is a boulder. Pushed about by this much it has the hollows and tufts of
/// foliage, and at two cells' depth over five the fitter can still lay
/// slopes on it.
const ROUGH: f64 = 1.5;
const ROUGH_LENGTH: f64 = 4.5;
/// The pushing steepens the distance function by up to its depth over its
/// length, times the steepest a smoothed step gets (1.5), each way of
/// three. The value is scaled down by that much to stay an underestimate.
const ROUGH_SLACK: f64 = 1.0 / (1.0 + 1.5 * 1.75 * ROUGH / ROUGH_LENGTH);

/// A tree as a solid: its parts, filed for quick lookup.
#[derive(Clone)]
pub struct Model {
    kind: Kind,
    parts: Vec<Part>,
    /// the tree's skeleton (see [`Limb`])
    pub limbs: Vec<Limb>,
    /// each place a limb divides: its radius there, and the radii of what it divides into
    pub forks: Vec<(f64, Vec<f64>)>,
    /// the height of the ground it stands on: nothing of it is below
    ground: f64,
    lo: DVec3,
    hi: DVec3,
    origin: DVec3,
    bucket: f64,
    dims: [usize; 3],
    /// bucket b holds `items[start[b]..start[b + 1]]`
    start: Vec<u32>,
    items: Vec<u32>,
    /// for each bucket, a distance every point of it is at least from every part not listed in it
    clear: Vec<f32>,
    seed: u32,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a {:?} of {} parts", self.kind, self.parts.len())
    }
}

/// A number from 0 up to 1 for each lattice point, the same every time.
fn hash(x: i32, y: i32, z: i32, seed: u32) -> f64 {
    // a few rounds of multiply and shift, as in `pattern::Numbers`
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77) ^ (z as u32).wrapping_mul(0xC2B2_AE3D) ^ seed;
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    (h ^ (h >> 15)) as f64 / 4_294_967_296.0
}

/// Smooth noise from -1 to 1: a number at each lattice point, blended
/// between them ("value noise"). Gradient noise is smoother still and costs
/// three times the arithmetic, for a surface that is then cut into bricks.
/// https://en.wikipedia.org/wiki/Value_noise
fn noise(p: DVec3, seed: u32) -> f64 {
    let f = p.floor();
    let (x, y, z) = (f.x as i32, f.y as i32, f.z as i32);
    let t = p - f;
    let t = t * t * (3.0 - 2.0 * t);
    let at = |i: i32, j: i32, k: i32| hash(x + i, y + j, z + k, seed);
    let along = |j: i32, k: i32| at(0, j, k) + (at(1, j, k) - at(0, j, k)) * t.x;
    let low = along(0, 0) + (along(1, 0) - along(0, 0)) * t.y;
    let high = along(0, 1) + (along(1, 1) - along(0, 1)) * t.y;
    2.0 * (low + (high - low) * t.z) - 1.0
}

impl Model {
    /// Grow the tree `tree` describes, standing at `base`, `height` tall
    /// and reaching `spread` to each side.
    ///
    /// The parts are filed in a uniform grid of buckets, each listing the
    /// parts that come within [`NEAR`] of it, so that a sample reads one
    /// bucket's list (some five parts) and not all of them (some five
    /// thousand). A tree of boxes round the parts would serve as well and
    /// be no faster here: the parts are small and spread evenly through
    /// the crown, which is the case a uniform grid is best at, and a
    /// sample finds its bucket by three multiplications with no descent.
    pub fn new(tree: &Tree, base: DVec3, height: f64, spread: f64) -> Model {
        let tree = tree.clamped();
        let (height, spread) = (height.max(LEAST_HEIGHT), spread.max(LEAST_SPREAD));
        // each kind has numbers of its own from one seed
        let salt = [0x7EE5_0001u64, 0x7EE5_0002, 0x7EE5_0003][tree.kind as usize];
        let mut grown = Grown { parts: Vec::new(), limbs: Vec::new(), forks: Vec::new(), numbers: Numbers(tree.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt) };
        match tree.kind {
            Kind::Pine => pine(&mut grown, &tree, height, spread),
            Kind::Palm => palm(&mut grown, &tree, height, spread),
            Kind::Oak => oak(&mut grown, &tree, height, spread),
        }
        // grown standing on the origin; moved to where it stands, on the
        // floor of the cell its point is in
        let ground = base.z.floor();
        let stand = DVec3::new(base.x, base.y, ground);
        for part in &mut grown.parts {
            match &mut part.shape {
                Shape::Cone { a, .. } => *a += stand,
                Shape::Blob { c, .. } => *c += stand,
                Shape::Column { a, b, .. } => {
                    *a += stand;
                    *b += stand;
                }
            }
        }
        for limb in &mut grown.limbs {
            limb.path.iter_mut().for_each(|p| *p += stand);
        }
        let reach = |part: &Part| NEAR + if matches!(part.shape, Shape::Blob { leafy: true, .. }) { ROUGH + NEAR / ROUGH_SLACK } else { 0.0 };
        let (mut lo, mut hi) = (DVec3::MAX, DVec3::MIN);
        for part in &grown.parts {
            let (a, b) = part.shape.bounds();
            let rough = reach(part) - NEAR;
            (lo, hi) = (lo.min(a - rough), hi.max(b + rough));
        }
        lo.z = lo.z.max(ground);
        let origin = lo - NEAR - 0.01;
        let size = hi + NEAR + 0.01 - origin;
        let bucket = BUCKET.max((size.x * size.y * size.z / MAX_BUCKETS).cbrt());
        let dims = (size / bucket).to_array().map(|d| d.floor() as usize + 1);
        let index = |x: usize, y: usize, z: usize| x + dims[0] * (y + dims[1] * z);
        let half = bucket * 0.5 * 3f64.sqrt();
        // two passes, as `sdf::PolyDist`: count each bucket's parts, then fill
        let mut start = vec![0u32; dims[0] * dims[1] * dims[2] + 1];
        let each = |f: &mut dyn FnMut(usize, u32)| {
            for (n, part) in grown.parts.iter().enumerate() {
                let (a, b) = part.shape.bounds();
                let reach = reach(part);
                let low = ((a - reach - origin) / bucket).floor().max(DVec3::ZERO).to_array().map(|v| v as usize);
                let high = ((b + reach - origin) / bucket).floor().to_array();
                let high = [0, 1, 2].map(|i| (high[i].max(0.0) as usize).min(dims[i] - 1));
                for z in low[2]..=high[2] {
                    for y in low[1]..=high[1] {
                        for x in low[0]..=high[0] {
                            // its box reaches the bucket; does it? A long
                            // slanting branch's box is mostly empty.
                            let middle = origin + (DVec3::new(x as f64, y as f64, z as f64) + 0.5) * bucket;
                            if part.shape.distance(middle) <= half + reach {
                                f(index(x, y, z), n as u32);
                            }
                        }
                    }
                }
            }
        };
        each(&mut |b, _| start[b + 1] += 1);
        for b in 1..start.len() {
            start[b] += start[b - 1];
        }
        let mut items = vec![0u32; *start.last().unwrap() as usize];
        let mut next = start.clone();
        each(&mut |b, n| {
            items[next[b] as usize] = n;
            next[b] += 1;
        });
        // How far each bucket is from the nearest one with anything in
        // it, in buckets, up to 3: a bucket k away from all parts' buckets
        // is (k - 1) buckets clear of every part, and the fitter passes
        // over a block of cells whose middle is 7 cells clear.
        let mut steps = vec![u8::MAX; dims[0] * dims[1] * dims[2]];
        for b in 0..steps.len() {
            if start[b + 1] > start[b] {
                steps[b] = 0;
            }
        }
        for step in 1..=3u8 {
            let before = steps.clone();
            for z in 0..dims[2] {
                for y in 0..dims[1] {
                    for x in 0..dims[0] {
                        if before[index(x, y, z)] != u8::MAX {
                            continue;
                        }
                        let beside = (z.saturating_sub(1)..=(z + 1).min(dims[2] - 1)).any(|k| (y.saturating_sub(1)..=(y + 1).min(dims[1] - 1)).any(|j| (x.saturating_sub(1)..=(x + 1).min(dims[0] - 1)).any(|i| before[index(i, j, k)] == step - 1)));
                        if beside {
                            steps[index(x, y, z)] = step;
                        }
                    }
                }
            }
        }
        let clear = steps.iter().map(|s| (NEAR + (*s).clamp(1, 4) as f64 * bucket - bucket) as f32).collect();
        Model { kind: tree.kind, parts: grown.parts, limbs: grown.limbs, forks: grown.forks, ground, lo, hi, origin, bucket, dims, start, items, clear, seed: tree.seed as u32 ^ 0x5EED }
    }

    /// The box the tree fills.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        (self.lo, self.hi)
    }

    /// How many parts the tree is the union of.
    pub fn parts(&self) -> usize {
        self.parts.len()
    }

    /// How far a mass of leaves is pushed in or out at `p` (see [`ROUGH`]):
    /// two sizes of unevenness, the smaller half the size and half the depth.
    fn rough(&self, p: DVec3) -> f64 {
        ROUGH * (noise(p / ROUGH_LENGTH, self.seed) * 0.7 + noise(p / (ROUGH_LENGTH * 0.45), self.seed ^ 0xABCD) * 0.3)
    }

    /// Negative inside the tree, positive outside; near its surface, no
    /// more than the distance to it in cells. Also the part `p` is deepest
    /// in or nearest to, if any is near.
    fn nearest(&self, p: DVec3) -> (f64, Option<usize>) {
        let c = ((p - self.origin) / self.bucket).floor();
        let under = self.ground - p.z;
        if c.min_element() < 0.0 || c.x >= self.dims[0] as f64 || c.y >= self.dims[1] as f64 || c.z >= self.dims[2] as f64 {
            // outside the grid, which reaches `NEAR` past every part
            let out = (self.origin - p).max(p - (self.origin + DVec3::new(self.dims[0] as f64, self.dims[1] as f64, self.dims[2] as f64) * self.bucket)).max_element();
            return ((out + NEAR).max(under), None);
        }
        let b = c.x as usize + self.dims[0] * (c.y as usize + self.dims[1] * c.z as usize);
        let (mut best, mut which) = (self.clear[b] as f64, None);
        // the leaves' unevenness is the same for every mass at a point, so
        // that two masses run into each other without a seam: worked out
        // once, and only if some mass is near enough for it to matter
        let mut rough = None;
        for &n in &self.items[self.start[b] as usize..self.start[b + 1] as usize] {
            let part = &self.parts[n as usize];
            let mut d = part.shape.distance(p);
            if matches!(part.shape, Shape::Blob { leafy: true, .. }) {
                d = if d > ROUGH + best / ROUGH_SLACK {
                    continue;
                } else if d < -ROUGH - 2.0 {
                    (d + ROUGH) * ROUGH_SLACK
                } else {
                    (d + *rough.get_or_insert_with(|| self.rough(p))) * ROUGH_SLACK
                };
            }
            if d < best {
                (best, which) = (d, Some(n as usize));
            }
        }
        (best.max(under), which)
    }

    /// Negative inside the tree, positive outside; near its surface, no
    /// more than the distance to it in cells.
    pub fn eval(&self, p: DVec3) -> f64 {
        self.nearest(p).0
    }

    /// The colour number of the tree at `p`: that of the part `p` is
    /// deepest in, with the light and shade a part has within itself.
    pub fn shade(&self, p: DVec3) -> u8 {
        let Some(n) = self.nearest(p).1 else { return TRUNK };
        let part = &self.parts[n];
        match (self.kind, part.shape, part.shade) {
            // The top of a pine's boughs: needles with open air a cell
            // and a half above them. A paler green in the natural sets
            // and white in the snowy ones, where it is the snow lying.
            (Kind::Pine, _, LEAF | LEAF_DARK | LEAF_LIGHT) if self.eval(p + DVec3::Z * 1.6) > 0.0 && self.eval(p + DVec3::Z * 2.6) > 0.0 => TOP,
            // A mass of leaves is light on top and dark underneath, with
            // the boundary between them uneven.
            (_, Shape::Blob { c, inv, leafy: true, .. }, _) => {
                let up = (p.z - c.z) * inv.z + 0.55 * noise(p / 6.0, self.seed ^ 0x1234);
                if up > 0.42 {
                    LEAF_LIGHT
                } else if up < -0.3 {
                    LEAF_DARK
                } else {
                    LEAF
                }
            }
            (_, _, shade) => shade,
        }
    }

    /// Give each brick of a fit of this tree its colour number, cutting
    /// apart any box that lies in parts of two colours.
    ///
    /// The fitter joins cubes into the largest boxes it can, knowing
    /// nothing of colour, so one box may run from a bough's bark out into
    /// its needles. Colouring such a box by its middle paints bark green or
    /// needles brown. Every cell of every box is asked its colour instead;
    /// a box of one colour is kept, and the cells of the others are joined
    /// up again colour by colour. Sloped bricks are one cell thick across
    /// their slope and take the colour at their middle.
    pub fn colour(&self, bricks: Vec<CellBrick>, max_ext: i32) -> (Vec<CellBrick>, Vec<u8>) {
        let (mut out, mut shades) = (Vec::with_capacity(bricks.len()), Vec::with_capacity(bricks.len()));
        let mut loose: [Vec<[i32; 3]>; 8] = Default::default();
        let mut found = Vec::new();
        for b in bricks {
            let middle = |c: [i32; 3], e: [i32; 3]| DVec3::new(c[0] as f64 + e[0] as f64 / 2.0, c[1] as f64 + e[1] as f64 / 2.0, c[2] as f64 + e[2] as f64 / 2.0);
            if b.mask != CUBE || b.ext == [1; 3] {
                shades.push(self.shade(middle(b.min, b.ext)));
                out.push(b);
                continue;
            }
            found.clear();
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        let cell = [b.min[0] + x, b.min[1] + y, b.min[2] + z];
                        found.push((cell, self.shade(middle(cell, [1; 3]))));
                    }
                }
            }
            if found.iter().all(|f| f.1 == found[0].1) {
                shades.push(found[0].1);
                out.push(b);
            } else {
                for &(cell, shade) in &found {
                    loose[shade.min(7) as usize].push(cell);
                }
            }
        }
        for (shade, cells) in loose.iter().enumerate().filter(|(_, cells)| !cells.is_empty()) {
            let lo = [0, 1, 2].map(|k| cells.iter().map(|c| c[k]).min().unwrap());
            let n = [0, 1, 2].map(|k| (cells.iter().map(|c| c[k]).max().unwrap() + 1 - lo[k]) as usize);
            let mut grid = vec![0u8; n[0] * n[1] * n[2]];
            for c in cells {
                grid[(c[0] - lo[0]) as usize + n[0] * ((c[1] - lo[1]) as usize + n[1] * (c[2] - lo[2]) as usize)] = CUBE;
            }
            for b in fit_volume::merge(grid, n, max_ext.max(1) as usize) {
                out.push(CellBrick { mask: CUBE, min: [b.min[0] + lo[0], b.min[1] + lo[1], b.min[2] + lo[2]], ext: b.ext });
                shades.push(shade as u8);
            }
        }
        (out, shades)
    }
}

/// `v` turned by `angle` about the direction `axis` (Rodrigues' formula).
/// https://en.wikipedia.org/wiki/Rodrigues%27_rotation_formula
fn turned(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    v * angle.cos() + axis.cross(v) * angle.sin() + axis * axis.dot(v) * (1.0 - angle.cos())
}

/// A direction square to `d`.
fn across(d: DVec3) -> DVec3 {
    let other = if d.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
    d.cross(other).normalize()
}

/// A pine: a spruce or fir, the Christmas tree.
///
/// One straight stem, tapering to a single leader, puts out a whorl of
/// boughs for each year's growth, and the tree's whole look follows from a
/// few facts about those boughs (Norway spruce, *Picea abies*):
/// https://en.wikipedia.org/wiki/Picea_abies
///
/// - They come in tiers with air between, four to six to a tier, and each
///   tier is turned from the one below, so no bough shades the next.
/// - The longest are low down and they shorten to the top, which makes
///   the cone; the lowest of all are a little shorter again. The crown is
///   about half as wide as the tree is tall.
/// - The angle changes with height: the top boughs climb at 40 degrees or
///   more, the middle ones stand level, and the lowest hang below level
///   with their tips turned up again, each bough an S.
///   https://bioresources.cnr.ncsu.edu/resources/crown-architecture-and-structural-development-of-young-norway-spruce-trees-picea-abies-karst-a-basis-for-more-realistic-growth-modelling/
/// - A bough is a flat spray: side shoots to left and right, angled
///   forward, longest a third of the way out, hanging a little, so that a
///   bough seen end on is a shallow roof. Near the trunk the bough is bare.
///
/// Each of these is built as it is said. A cone of leaves on a stick has
/// the outline and none of the rest, and builders of brick spruces say the
/// same: what reads as a spruce is the separate drooping tiers.
/// https://fullplatebuilds.com/2018/04/14/spruce-tutorial/
///
/// The needles are not drawn one by one. A side shoot with its needles is
/// a round cone a little over two cells thick, the least that survives
/// the fit at any angle.
fn pine(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, droop) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let tiers = tree.count.max(2) as usize;
    // the stem: a fortieth of its height thick at the foot, which is
    // stouter than a forest spruce and what a trunk two bricks thick needs
    let foot = (height * 0.024).max(1.8);
    let tip = 0.8;
    let (lean_way, lean) = (g.between(0.0, TAU), height * 0.008 * g.unit());
    let stem = |z: f64| DVec3::new(lean_way.cos(), lean_way.sin(), 0.0) * (lean * (z / height * PI * 1.5).sin()) + DVec3::Z * z;
    let thick = |z: f64| tip + (foot - tip) * (1.0 - z / height).max(0.0).powf(0.85) + foot * 0.4 * (-z / (foot * 1.6)).exp();
    let lengths = (height / 5.0).ceil() as usize;
    let path: Vec<DVec3> = (0..=lengths).map(|i| stem(height * i as f64 / lengths as f64)).collect();
    let radius = path.iter().map(|p| thick(p.z)).collect();
    let trunk = g.limb(path, radius, None, 0, TRUNK);

    // bare below, boughs from there to just under the leader
    let (low, high) = (height * g.between(0.09, 0.14), height * 0.93);
    let gap = (high - low) / (tiers - 1) as f64;
    let mut tips = Vec::new();
    for tier in 0..tiers {
        // 0 at the lowest tier, 1 at the highest
        let u = tier as f64 / (tiers - 1) as f64;
        let z = low + (high - low) * u;
        // shortening to the top, and the lowest tiers drawn in a little
        let reach = spread * ((1.0 - u).powf(0.9) * 0.94 + 0.06) * (0.84 + 0.16 * (u / 0.12).min(1.0));
        let boughs = ((3.6 + 2.6 * (1.0 - u)) * (0.75 + 0.5 * full)).round().max(3.0) as usize;
        let turn = g.between(0.0, TAU);
        for k in 0..boughs {
            let way = turn + TAU * (k as f64 + g.between(-0.22, 0.22)) / boughs as f64;
            let (length, at) = (reach * g.between(0.86, 1.1), z + g.between(-0.2, 0.2) * gap);
            tips.push(bough(g, trunk, stem(at), thick(at), way, length, u, full, droop));
        }
        // and between the tiers, a few shorter boughs: fewer the higher
        // up, and none on a thin tree
        let between = (full * 3.2 * (1.0 - u)).round() as usize;
        for _ in 0..between.min(if tier + 1 < tiers { 9 } else { 0 }) {
            let (way, at) = (g.between(0.0, TAU), z + g.between(0.3, 0.7) * gap);
            let length = reach * g.between(0.4, 0.62);
            bough(g, trunk, stem(at), thick(at), way, length, u, full, droop);
        }
    }
    // The leader: the top of the stem in its own needles, to a point,
    // with two small whorls of climbing shoots under it.
    let top = stem(height);
    g.cone(stem(height * 0.9), top - DVec3::Z * 1.0, 1.7, 1.0, LEAF_LIGHT);
    for (down, length) in [(0.035, 4.0), (0.07, 6.5)] {
        let (turn, at) = (g.between(0.0, TAU), stem(height * (1.0 - down)));
        for k in 0..4 {
            let way = turn + TAU * k as f64 / 4.0;
            let out = DVec3::new(way.cos(), way.sin(), 0.0);
            let length = (length * spread / 36.0f64).clamp(3.0, 9.0);
            g.cone(at, at + (out * 0.75 + DVec3::Z * 0.66) * length, 1.2, 0.85, LEAF_LIGHT);
        }
    }
    if tree.extras {
        // a star of five points standing on the leader, flat and facing a side
        let middle = top + DVec3::Z * 4.0;
        let face = DVec3::new(lean_way.cos(), lean_way.sin(), 0.0);
        let level = face.cross(DVec3::Z);
        g.blob(middle, DVec3::splat(2.3), STAR, false);
        for k in 0..5 {
            let a = PI / 2.0 + TAU * k as f64 / 5.0;
            g.cone(middle, middle + (level * a.cos() + DVec3::Z * a.sin()) * 5.5, 1.5, 0.55, STAR);
        }
        // and baubles hanging under the ends of some boughs, of two colours
        for (n, tip) in tips.iter().enumerate() {
            if g.unit() < 0.3 {
                g.blob(*tip - DVec3::Z * 2.6, DVec3::splat(1.9), if n % 2 == 0 { EXTRA } else { STAR }, false);
            }
        }
    }
}

/// One bough of a pine, from the stem at `from` out the way `way`, `length`
/// long. `u` is how high up the crown it is, 0 to 1. Returns where it ends.
#[allow(clippy::too_many_arguments)]
fn bough(g: &mut Grown, trunk: usize, from: DVec3, stem: f64, way: f64, length: f64, u: f64, full: f64, droop: f64) -> DVec3 {
    let out = DVec3::new(way.cos(), way.sin(), 0.0);
    let side = DVec3::new(-way.sin(), way.cos(), 0.0);
    // The angle above level it leaves the stem at, lowest boughs hanging
    // and highest climbing; then how far it sags on the way out, and how
    // far its tip turns up again.
    let leaves_at = -0.2 + 0.95 * u.powf(1.4) - 0.22 * droop * (1.0 - u);
    let (sag, lift) = (droop * 0.55 * (1.0 - 0.7 * u), 0.5 - 0.2 * u);
    let climb = |s: f64| leaves_at - sag * 2.0 * s + (sag * 2.0 + lift) * s * s * s;
    let steps = ((length / 3.5).ceil() as usize).max(3);
    let step = (length - stem * 0.5) / steps as f64;
    let mut path = vec![from + out * stem * 0.5];
    for i in 0..steps {
        let e = climb((i as f64 + 0.5) / steps as f64) + g.between(-0.05, 0.05);
        path.push(path[i] + (out * e.cos() + DVec3::Z * e.sin()) * step);
    }
    let wood = (length * 0.04).clamp(0.8, 2.0);
    let radius: Vec<f64> = (0..=steps).map(|i| wood + (0.6 - wood) * i as f64 / steps as f64).collect();
    let at = |s: f64| {
        let f = (s * steps as f64).min(steps as f64 - 1e-9);
        let i = f as usize;
        (path[i].lerp(path[i + 1], f - i as f64), (path[i + 1] - path[i]).normalize())
    };
    // bare near the stem, and the lower the bough the more of it is bare:
    // its oldest needles are shaded out
    let bare = (0.1 + 0.16 * (1.0 - u)).min(1.0 - 4.0 / length.max(5.0)).max(0.0);
    // the bough's own needles, dark: it is the inside of the spray
    let own = ((1.0 - bare) * steps as f64).ceil() as usize * 2;
    for i in 0..own {
        let (s0, s1) = (bare + (1.0 - bare) * i as f64 / own as f64, bare + (1.0 - bare) * (i + 1) as f64 / own as f64);
        let shade = if i + 2 >= own { LEAF_LIGHT } else { LEAF_DARK };
        g.cone(at(s0).0, at(s1).0, 1.5 - 0.4 * s0, 1.5 - 0.4 * s1, shade);
    }
    // The side shoots, a pair every 2.3 cells: close enough to make one
    // spray, far enough apart that its edge is toothed.
    let widest = length * 0.36 * (0.7 + 0.6 * full);
    let pairs = (((1.0 - bare) * length / 2.3).floor() as usize).max(1);
    for i in 0..pairs {
        let part = (i as f64 + 0.6) / pairs as f64;
        let (p, forward) = at(bare + (1.0 - bare) * part * 0.97);
        // longest a quarter of the way along, tapering to the tip
        let long = widest * (part / 0.25).min(1.0).powf(0.7) * (1.0 - 0.8 * part) * g.between(0.85, 1.12);
        if long < 2.0 {
            continue;
        }
        for hand in [-1.0, 1.0] {
            let mut d = forward * 0.62 + side * hand * 0.78;
            d.z -= 0.2 + 0.4 * droop;
            let d = d.normalize();
            let (middle, end) = (p + d * long * 0.62, p + d * long - DVec3::Z * (0.08 + 0.1 * droop) * long);
            g.cone(p, middle, 1.2, 1.05, LEAF);
            g.cone(middle, end, 1.05, 0.85, LEAF_LIGHT);
            // on a long shoot, shoots of its own, forward and back in turn
            if long >= 6.0 {
                let sprigs = (long / 2.6) as usize;
                for k in 1..sprigs {
                    let along = k as f64 / sprigs as f64;
                    let q = p.lerp(middle, (along / 0.62).min(1.0)).lerp(end, ((along - 0.62) / 0.38).clamp(0.0, 1.0));
                    let toward = if k % 2 == 0 { forward } else { side * hand * 0.4 - forward * 0.8 };
                    let sprig = (d * 0.55 + toward * 0.8 - DVec3::Z * 0.2).normalize() * (long * 0.3 * (1.0 - 0.6 * along)).clamp(2.0, 4.5);
                    g.cone(q, q + sprig, 1.0, 0.8, if k % 2 == 0 { LEAF_LIGHT } else { LEAF });
                }
            }
        }
    }
    let tip = *path.last().unwrap();
    g.limb(path, radius, Some(trunk), 1, LIMB);
    tip
}

/// A palm: a coconut palm, *Cocos nucifera*.
///
/// One slender stem with no branch on it anywhere, and at its top a single
/// crown of fronds. The figures are a coconut palm's:
/// https://www3.ctahr.hawaii.edu/gsp/doc/Forestry/Little_Skolmen_CFT/CFT_Cocos_nucifera.pdf
///
/// - The trunk is swollen at the foot, nearly twice as thick as higher
///   up, and above that hardly tapers. It leans from the foot and curves
///   back to upright, having grown towards the light. It is ringed with
///   the scars of fallen fronds.
/// - The fronds are a quarter to a third of the tree's height long, each
///   set 137.5 degrees round from the last (see [`GOLDEN_ANGLE`]). The
///   youngest stand nearly upright in the middle, older ones arch out
///   with their tips hanging, and the oldest hang down against the trunk.
/// - A frond is pinnate: a midrib with a row of narrow leaflets down each
///   side, shortest at its two ends, drooping from the rib.
///
/// The trunk is eight-sided, as asked for and as brick palms are built, of
/// short lengths each a little wider at its top than the next is at its
/// foot: that step is the ring.
/// https://fullplatebuilds.com/2018/11/21/palm-tree-tutorial/
///
/// The fronds are placed by rule and not grown by a general branching
/// model, as Weber and Penn do with their fan mode for palms: a palm has
/// one level of structure, and the rule says it directly.
fn palm(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, lean) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let fronds = tree.count.max(3) as usize;
    // the upper fronds stand above the top of the trunk by about a third of their length
    let top = (height - spread * 0.34).max(height * 0.45);
    let slim = (height * 0.021).max(1.6);
    // The lean: steepest at the foot, gone by the top. The slope at
    // height t (0 to 1) is `steep * (1 - t)^1.7`, and the stem's place is
    // the sum of the slopes below it.
    let (way, steep) = (g.between(0.0, TAU), lean * 0.85);
    let (wave, wave_at) = (g.between(-0.012, 0.012) * top * (0.3 + lean), g.between(0.0, TAU));
    let towards = DVec3::new(way.cos(), way.sin(), 0.0);
    let cross = DVec3::new(-way.sin(), way.cos(), 0.0);
    let middle = |z: f64| {
        let t = (z / top).clamp(0.0, 1.0);
        towards * (steep * top * (1.0 - (1.0 - t).powf(2.7)) / 2.7) + cross * (wave * (t * TAU * 0.8 + wave_at).sin() * t) + DVec3::Z * z
    };
    let thick = |z: f64| slim * (1.0 + 0.22 * (1.0 - z / top)) + slim * 0.75 * (-z / (slim * 2.4)).exp();
    // the trunk, ring by ring
    let ring = (slim * 1.5).clamp(3.0, 7.0).round();
    let rings = (top / ring).ceil() as usize;
    let mut path = Vec::new();
    for i in 0..rings {
        let (z0, z1) = (i as f64 * ring, ((i + 1) as f64 * ring).min(top));
        let (a, b) = (middle(z0), middle(z1));
        let run = (b - a).truncate().length() / (z1 - z0).max(1e-9);
        let (ra, rb) = (thick(z0) - 0.3, thick(z1) + 0.35);
        let slack = 1.0 / (1.0 + run * run + ((rb - ra) / (z1 - z0).max(1e-9)).powi(2)).sqrt();
        // its top cell is the scar: a ring of another colour
        let scar = (z1 - 1.0).max(z0);
        let at = a.lerp(b, (scar - z0) / (z1 - z0).max(1e-9));
        let r_at = ra + (rb - ra) * (scar - z0) / (z1 - z0).max(1e-9);
        if scar > z0 {
            g.parts.push(Part { shape: Shape::Column { a, b: at, ra, rb: r_at, slack }, shade: TRUNK });
        }
        g.parts.push(Part { shape: Shape::Column { a: at, b, ra: r_at, rb, slack }, shade: LIMB });
        path.push(a);
    }
    let heart = middle(top);
    path.push(heart);
    let radius = path.iter().map(|p| thick(p.z)).collect();
    let trunk = g.limb_record(path, radius, None, 0);
    // the fibre and old leaf bases under the crown
    g.blob(heart - DVec3::Z * slim * 0.3, DVec3::new(slim * 1.5, slim * 1.5, slim * 1.9), STAR, false);

    let longest = spread;
    for i in 0..fronds {
        // 0 for the youngest, 1 for the oldest
        let age = i as f64 / (fronds - 1).max(1) as f64;
        let way = i as f64 * GOLDEN_ANGLE + g.between(-0.15, 0.15);
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        let side = DVec3::new(-way.sin(), way.cos(), 0.0);
        // The angle from upright it leaves the crown at, and how much
        // further it has turned over by its tip: a young frond stands and
        // arches a little, an old one leaves level and hangs.
        let leaves_at = 0.12 + 1.5 * age.powf(0.85) + g.between(-0.07, 0.07);
        let arch = (0.75 + 1.05 * age) * g.between(0.85, 1.15);
        let length = longest * (0.5 + 0.5 * (age * 3.0).min(1.0)) * g.between(0.9, 1.06);
        let steps = ((length / 3.0).ceil() as usize).max(4);
        let from = heart + out * slim * 0.5 - DVec3::Z * (age * slim * 1.6) + DVec3::Z * slim * 0.8;
        let mut rib = vec![from];
        let mut ways = Vec::new();
        for k in 0..steps {
            let s = (k as f64 + 0.5) / steps as f64;
            let polar = (leaves_at + arch * s.powf(1.7)).min(3.0);
            let d = DVec3::Z * polar.cos() + out * polar.sin();
            ways.push(d);
            rib.push(rib[k] + d * length / steps as f64);
        }
        let radius: Vec<f64> = (0..=steps).map(|k| 1.35 - 0.55 * k as f64 / steps as f64).collect();
        // The leaflets, a pair every three cells from a fifth of the way
        // along: the stalk below that is bare.
        let tone = if age < 0.22 { LEAF_LIGHT } else if age > 0.72 { LEAF_DARK } else { LEAF };
        let long = length * 0.27 * (0.65 + 0.7 * full);
        let pairs = (length * 0.82 / 3.0).floor() as usize;
        for k in 0..pairs {
            let s = 0.18 + 0.82 * (k as f64 + 0.5) / pairs as f64;
            let f = (s * steps as f64).min(steps as f64 - 1e-9);
            let (n, forward) = (f as usize, ways[f as usize]);
            let p = rib[n].lerp(rib[n + 1], f - n as f64);
            // the frond's underside: square to the rib, in the plane it arches in
            let under = side.cross(forward) * -1.0;
            let under = if under.z > 0.0 && forward.z.abs() < 0.95 { -under } else { under };
            // shortest at the two ends of the rib
            let leaflet = long * ((PI * (0.1 + 0.86 * (s - 0.18) / 0.82)).sin().powf(0.6)).max(0.25) * g.between(0.9, 1.1);
            for hand in [-1.0, 1.0] {
                // held out to the side on a young frond, hanging on an old one
                let hang = 0.15 + 0.75 * age;
                let d = (side * hand * (1.0 - 0.5 * hang) + under * hang + forward * 0.38 - DVec3::Z * (0.15 + 0.45 * age)).normalize();
                let bend = (d - DVec3::Z * 0.55).normalize();
                let (elbow, end) = (p + d * leaflet * 0.55, p + d * leaflet * 0.55 + bend * leaflet * 0.45);
                g.cone(p, elbow, 1.1, 1.0, tone);
                g.cone(elbow, end, 1.0, 0.75, if tone == LEAF_DARK { LEAF_DARK } else { LEAF });
            }
        }
        g.limb(rib, radius, Some(trunk), 1, TOP);
    }
    // the spear: the newest frond, still furled, standing in the middle
    g.cone(heart, heart + (DVec3::Z + towards * 0.08) * longest * 0.42, 1.3, 0.7, LEAF_LIGHT);
    if tree.extras {
        // coconuts, in a cluster under the crown round the trunk
        let nut = (slim * 0.85).max(1.9);
        let nuts = 5 + (g.unit() * 4.0) as usize;
        for k in 0..nuts {
            let way = k as f64 * GOLDEN_ANGLE;
            let at = heart + DVec3::new(way.cos(), way.sin(), 0.0) * (slim + nut * 0.7) - DVec3::Z * (slim * 1.2 + nut * 0.9 * (k % 3) as f64);
            g.blob(at, DVec3::splat(nut), EXTRA, false);
        }
    }
}

impl Grown {
    /// The record of a limb whose parts are added some other way.
    fn limb_record(&mut self, path: Vec<DVec3>, radius: Vec<f64>, parent: Option<usize>, level: u8) -> usize {
        self.limbs.push(Limb { path, radius, parent, level });
        self.limbs.len() - 1
    }
}

/// The thinnest an oak's branch is followed to, in cells: thinner than
/// this it would not survive the fit, and it ends in leaves.
const TWIG: f64 = 1.0;

/// A branch of an oak still to be grown: where it starts, which way it is
/// heading, how thick it is, and which limb it springs from.
struct Shoot {
    from: DVec3,
    way: DVec3,
    radius: f64,
    parent: usize,
    level: u8,
    /// the turn of its last fork about its own line: the next is a golden angle on
    roll: f64,
}

/// An oak: an open-grown English or white oak, *Quercus robur* or *alba*.
///
/// A short thick trunk, flared at the foot, divides low down into a few
/// heavy limbs, and from there on there is no leading stem: every limb
/// divides again and again until it is twigs. Grown in the open it is as
/// wide as it is tall or wider, with "large branches striking out at wide
/// angles" and its lowest limbs nearly level.
/// https://en.wikipedia.org/wiki/Quercus_alba
///
/// The rule of growth is one fork, applied over and over:
///
/// - A limb runs a length in proportion to its thickness, wandering as it
///   goes (the gnarl), and then divides in two.
/// - The two share its cross-section: the squares of their radii sum to
///   the square of its own. That is Leonardo da Vinci's rule, and measured
///   trees keep it to within a tenth or two of the exponent 2:
///   https://arxiv.org/abs/1105.2591
///   The share is uneven, anywhere from an even fork to a small side
///   branch off a limb that carries on.
/// - The thinner one turns aside further than the thicker, and each fork
///   is turned by the golden angle about the limb from the fork before, so
///   the crown fills in all round.
/// - A heavy limb is drawn outwards and sags; a thin one turns up to the
///   light. That gives limbs that climb, then spread, with their ends
///   turned up (Weber and Penn's "attraction up").
/// - A branch thinner than a cell ends in a mass of leaves.
///
/// The skeleton is grown at no particular size and then stretched to the
/// height and spread asked for, the limbs keeping their thickness. Growing
/// it to size directly would need the lengths chosen so that the crown
/// came out as wide as asked, which no rule gives: the reach of a crooked
/// limb is known only once it is grown.
///
/// The leaves are in masses at the ends of the branches and nowhere
/// else, so the crown is clumps with sky between them and the limbs show
/// from below, as on the tree. Each mass is a few flattened balls with a
/// rough surface (see [`ROUGH`]).
fn oak(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (dense, gnarl) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let limbs = tree.count.max(2) as usize;
    let girth = (height * 0.047).max(2.4);
    // the height the trunk divides at: low, a fifth to a quarter of the way up
    let bole = height * g.between(0.19, 0.25);
    // how big a mass of leaves is
    let mass = (spread * (0.1 + 0.07 * dense)).max(3.5);

    // the trunk, a little crooked, widening to its foot
    let lengths = ((bole / 5.0).ceil() as usize).max(2);
    let (crook_way, crook) = (g.between(0.0, TAU), girth * 0.5 * gnarl);
    let stem = |z: f64| DVec3::new(crook_way.cos(), crook_way.sin(), 0.0) * (crook * (z / bole * PI).sin()) + DVec3::Z * z;
    let path: Vec<DVec3> = (0..=lengths).map(|i| stem(bole * i as f64 / lengths as f64)).collect();
    let radius: Vec<f64> = path.iter().map(|p| girth * (1.0 + 0.45 * (-p.z / (girth * 1.3)).exp())).collect();
    let trunk = g.limb(path, radius, None, 0, TRUNK);
    // the roots where they leave the trunk: buttresses running down into the ground
    let roots = 5 + (g.unit() * 3.0) as usize;
    let turn = g.between(0.0, TAU);
    for k in 0..roots {
        let way = turn + TAU * (k as f64 + g.between(-0.25, 0.25)) / roots as f64;
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        let far = girth * g.between(1.9, 2.6);
        let high = girth * g.between(1.4, 2.2);
        g.cone(out * girth * 0.4 + DVec3::Z * high, out * far - DVec3::Z * girth * 0.2, girth * 0.62, girth * 0.3, TRUNK);
    }

    // the limbs the trunk divides into, sharing its cross-section
    let fork = stem(bole);
    let shares: Vec<f64> = (0..limbs).map(|_| g.between(0.7, 1.3)).collect();
    let total: f64 = shares.iter().sum();
    let turn = g.between(0.0, TAU);
    let mut waiting = Vec::new();
    let mut radii = Vec::new();
    for (k, share) in shares.iter().enumerate() {
        let radius = girth * (share / total).sqrt();
        radii.push(radius);
        let way = turn + TAU * (k as f64 + g.between(-0.2, 0.2)) / limbs as f64;
        // climbing at first: 30 to 50 degrees off upright
        let polar = g.between(0.52, 0.87);
        waiting.push(Shoot { from: fork, way: DVec3::new(way.cos() * polar.sin(), way.sin() * polar.sin(), polar.cos()), radius, parent: trunk, level: 1, roll: g.between(0.0, TAU) });
    }
    g.forks.push((girth, radii));

    // grow every branch, at no particular size; the ends are kept to hang leaves on
    let first = g.limbs.len();
    let mut ends: Vec<(DVec3, DVec3, f64)> = Vec::new();
    while let Some(shoot) = waiting.pop() {
        let (mut at, mut way, mut radius, mut roll) = (shoot.from, shoot.way, shoot.radius, shoot.roll);
        let (mut path, mut thick) = (vec![at], vec![radius]);
        // (it is recorded once grown, and its children are told then which limb it is)
        let mut children = Vec::new();
        loop {
            // one length between forks, in two steps so that it can bend
            let run = 6.2 * radius.powf(0.62) * g.between(0.8, 1.25);
            for _ in 0..2 {
                let outward = DVec3::new(at.x, at.y, 0.0).normalize_or_zero();
                let heavy = (radius / girth).min(1.0);
                // heavy limbs spread and sag; thin ones turn up
                let pull = outward * (0.32 * heavy) - DVec3::Z * (0.1 * heavy) + DVec3::Z * (0.2 * (1.0 - heavy).powi(2));
                way = (way + g.any_way() * (0.34 * gnarl) + pull).normalize();
                // no limb of an oak dives
                if way.z < -0.12 {
                    way = DVec3::new(way.x, way.y, -0.12).normalize();
                }
                at += way * run * 0.5;
                path.push(at);
                thick.push(radius);
            }
            if radius < TWIG {
                ends.push((at, way, 1.0));
                break;
            }
            // the fork: `share` of the cross-section carries on
            let share = g.between(0.52, 0.86);
            let (carries, aside) = (radius * share.sqrt(), radius * (1.0 - share).sqrt());
            g.forks.push((radius, vec![carries, aside]));
            roll += GOLDEN_ANGLE + g.between(-0.5, 0.5);
            let mut hinge = turned(across(way), way, roll);
            let angle = g.between(0.6, 1.15);
            let mut side_way = turned(way, hinge, angle);
            // a side branch goes out or up, seldom down
            if side_way.z < -0.05 && g.unit() < 0.85 {
                hinge = -hinge;
                side_way = turned(way, hinge, angle);
            }
            // the limb itself turns the other way, by less the more of it carries on
            way = turned(way, hinge, -angle * (1.0 - share) * 0.9);
            if aside >= 0.72 {
                children.push(Shoot { from: at, way: side_way, radius: aside.max(TWIG * 0.8), parent: 0, level: shoot.level + 1, roll: g.between(0.0, TAU) });
            } else {
                // too thin to follow: a small mass of leaves on the limb here
                ends.push((at + side_way * mass * 0.5, side_way, 0.7));
            }
            radius = carries;
        }
        let me = g.limb_record(path, thick, Some(shoot.parent), shoot.level);
        for mut child in children {
            child.parent = me;
            waiting.push(child);
        }
    }

    // Stretch the crown to its size: the furthest branch end to the
    // spread less half a mass of leaves, and the highest to the height
    // less the leaves over it. Sideways from the trunk's line, upwards
    // from the fork.
    let reach = ends.iter().map(|e| e.0.truncate().length()).fold(1e-9, f64::max);
    let rise = ends.iter().map(|e| e.0.z - bole).fold(1e-9, f64::max);
    let (wide, tall) = ((spread - mass * 0.75).max(spread * 0.5) / reach, (height - bole - mass * 0.62).max(height * 0.3) / rise);
    let place = |p: DVec3| DVec3::new(fork.x + (p.x - fork.x) * wide, fork.y + (p.y - fork.y) * wide, bole + (p.z - bole) * tall);
    for n in first..g.limbs.len() {
        let path: Vec<DVec3> = g.limbs[n].path.iter().map(|p| place(*p)).collect();
        let radius = g.limbs[n].radius.clone();
        let level = g.limbs[n].level;
        for i in 1..path.len() {
            g.cone(path[i - 1], path[i], radius[i - 1], radius[i], if level == 1 && radius[i] > girth * 0.42 { TRUNK } else { LIMB });
        }
        g.limbs[n].path = path;
    }

    // The leaves: at each end a mass, of one flattened ball and a few
    // smaller ones round it, set over and beyond the end of the branch.
    for (end, way, size) in ends {
        let size = mass * size * g.between(0.82, 1.2);
        let at = place(end) + DVec3::new(way.x, way.y, 0.0) * size * 0.25 + DVec3::Z * size * 0.22;
        g.blob(at, DVec3::new(size, size, size * 0.66), LEAF, true);
        let more = 2 + (g.unit() * 3.0) as usize;
        for _ in 0..more {
            let off = g.any_way();
            let small = size * g.between(0.5, 0.75);
            g.blob(at + DVec3::new(off.x, off.y, off.z * 0.45) * size * 0.75, DVec3::new(small, small, small * 0.7), LEAF, true);
        }
    }
}

/// Where the middle of a palm's trunk is at each height, for tests.
#[cfg(test)]
fn level_middle(model: &Model, z: f64) -> Option<glam::DVec2> {
    let trunk = &model.limbs[0];
    trunk.path.windows(2).find(|w| w[0].z <= z && z <= w[1].z).map(|w| w[0].lerp(w[1], (z - w[0].z) / (w[1].z - w[0].z).max(1e-9)).truncate())
}
