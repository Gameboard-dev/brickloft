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
    /// a tall smooth-barked tree with a dense domed crown
    Beech,
    /// a dense rounded shrub
    Boxwood,
    /// the same shrub clipped to a wall of any length
    Hedge,
    /// a clump of stems each with one many-petalled bloom
    Peony,
    Sunflower,
    Tulip,
    /// a rounded shrub with large domed heads of small flowers
    Hydrangea,
    /// a low mound of trumpet flowers that trails
    Petunia,
    /// a stand of bulrushes or of plumed reeds
    Reeds,
    /// a patch or strip of grass over ground that can be raised
    Grass,
    /// a slender white-barked tree with fine hanging twigs
    Birch,
    /// a weeping willow
    Willow,
    /// a date palm: a stout rough trunk and a ball of stiff fronds
    DatePalm,
    /// a Scots pine: a bare trunk and a flat crown of plates
    ScotsPine,
    /// an upright cypress or juniper
    Cypress,
    /// a rosette of sword leaves
    Yucca,
    Daisy,
    Daffodil,
    /// a bed of mixed flowers
    Bed,
    /// a Japanese flowering cherry in blossom
    Cherry,
}

/// The kinds on offer, with the names they are written and read by.
pub const KINDS: [(Kind, &str); 23] = [(Kind::Pine, "pine"), (Kind::Palm, "palm"), (Kind::Oak, "oak"), (Kind::Beech, "beech"), (Kind::Boxwood, "boxwood"), (Kind::Hedge, "hedge"), (Kind::Peony, "peony"), (Kind::Sunflower, "sunflower"), (Kind::Tulip, "tulip"), (Kind::Hydrangea, "hydrangea"), (Kind::Petunia, "petunia"), (Kind::Reeds, "reeds"), (Kind::Grass, "grass"), (Kind::Birch, "birch"), (Kind::Willow, "willow"), (Kind::DatePalm, "datepalm"), (Kind::ScotsPine, "scotspine"), (Kind::Cypress, "cypress"), (Kind::Yucca, "yucca"), (Kind::Daisy, "daisy"), (Kind::Daffodil, "daffodil"), (Kind::Bed, "bed"), (Kind::Cherry, "cherry")];

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
            Kind::Oak => (60, 5, 50),
            Kind::Beech => (65, 6, 30),
            Kind::Boxwood => (60, 6, 40),
            Kind::Hedge => (60, 5, 25),
            Kind::Peony => (60, 3, 75),
            Kind::Sunflower => (60, 3, 40),
            Kind::Tulip => (60, 5, 30),
            Kind::Hydrangea => (60, 9, 30),
            Kind::Petunia => (60, 26, 40),
            Kind::Reeds => (60, 24, 35),
            // (its count is how far below it water stands)
            Kind::Grass => (70, 3, 30),
            Kind::Birch => (60, 8, 35),
            Kind::Willow => (60, 5, 45),
            Kind::DatePalm => (60, 34, 10),
            Kind::ScotsPine => (60, 5, 55),
            Kind::Cypress => (60, 1, 15),
            Kind::Yucca => (55, 34, 25),
            Kind::Daisy => (50, 7, 30),
            Kind::Daffodil => (50, 5, 30),
            Kind::Bed => (50, 14, 40),
            Kind::Cherry => (60, 7, 40),
        };
        Tree { kind, seed: 1, fullness, count, bend, extras: false }
    }

    /// The fewest and most of whatever `count` counts, for a kind.
    pub fn count_range(kind: Kind) -> (i32, i32) {
        match kind {
            Kind::Pine => (4, 40),
            Kind::Palm => (5, 40),
            Kind::Oak => (2, 7),
            Kind::Beech => (3, 9),
            Kind::Boxwood => (2, 12),
            Kind::Hedge => (1, 40),
            Kind::Peony => (1, 9),
            Kind::Sunflower | Kind::Tulip => (1, 12),
            Kind::Hydrangea => (1, 24),
            Kind::Petunia => (3, 80),
            Kind::Reeds => (1, 120),
            // (grass has nothing to count)
            Kind::Cypress => (1, 1),
            Kind::Grass => (0, 40),
            Kind::Birch | Kind::Willow | Kind::ScotsPine | Kind::Cherry => (3, 9),
            Kind::DatePalm => (8, 60),
            Kind::Yucca => (8, 80),
            Kind::Daisy | Kind::Daffodil => (1, 20),
            Kind::Bed => (2, 40),
        }
    }

    /// The same settings, each within what it may be: a tree read from a
    /// file or sent by another program may say anything.
    pub fn clamped(self) -> Tree {
        let (least, most) = Tree::count_range(self.kind);
        Tree { fullness: self.fullness.clamp(0, 100), count: self.count.clamp(least, most), bend: self.bend.clamp(0, 100), ..self }
    }

    /// The least a plant of a kind may be tall, in cells. Grass is short;
    /// under a dozen cells nothing else has room for a stem with anything on it.
    pub fn least_height(kind: Kind) -> f64 {
        if kind == Kind::Grass { 3.0 } else { LEAST_HEIGHT }
    }

    /// Whether a kind lies along a line the form's points give (see [`Lay`]).
    pub fn lies_along(kind: Kind) -> bool {
        matches!(kind, Kind::Hedge | Kind::Reeds | Kind::Grass)
    }

    /// The height and spread a tree of a kind starts at, in cells. Large,
    /// because a tree's detail is a cell or two across at any size: a
    /// small tree has room for its outline and nothing more.
    pub fn usual_size(kind: Kind) -> (f64, f64) {
        match kind {
            Kind::Pine => (140.0, 36.0),
            Kind::Palm => (120.0, 44.0),
            Kind::Oak => (110.0, 62.0),
            // (a beech is about three quarters as wide as it is tall)
            Kind::Beech => (140.0, 52.0),
            Kind::Boxwood => (40.0, 21.0),
            // a hedge's spread is half its thickness; its length is apart (see [`Model::with_length`])
            Kind::Hedge => (36.0, 12.0),
            // a peony's spread is half the width of a full-blown bloom
            Kind::Peony => (56.0, 10.0),
            // a flower's spread is its head's: half the width of a
            // sunflower's head, petals and all, and of a tulip's cup
            Kind::Sunflower => (90.0, 17.0),
            Kind::Tulip => (64.0, 6.0),
            Kind::Hydrangea => (46.0, 28.0),
            // (a petunia is a hand high and several across)
            Kind::Petunia => (20.0, 26.0),
            // a stand of reeds' spread is half its width; its length is apart, as a hedge's is
            Kind::Reeds => (84.0, 10.0),
            // grass's height is its blades', and its spread how far it reaches to each side of its line
            // (short and thick: a lawn. Longer and thinner it is rough grass.)
            Kind::Grass => (5.0, 20.0),
            Kind::Birch => (130.0, 34.0),
            Kind::Willow => (100.0, 58.0),
            Kind::DatePalm => (120.0, 40.0),
            Kind::ScotsPine => (130.0, 44.0),
            Kind::Cypress => (110.0, 11.0),
            Kind::Yucca => (34.0, 24.0),
            // a daisy's and a daffodil's spread is half the width of its flower
            Kind::Daisy => (40.0, 6.0),
            Kind::Daffodil => (46.0, 6.0),
            // a bed's is how far the bed reaches
            Kind::Bed => (56.0, 36.0),
            // (wider than it is tall)
            Kind::Cherry => (84.0, 56.0),
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
/// How many times its own thickness a hedge is long when it is given no length.
pub const HEDGE_LENGTH: f64 = 3.0;

pub const LEAST_HEIGHT: f64 = 12.0;
pub const LEAST_SPREAD: f64 = 3.0;

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

// Grass and its pond: the bed under the water, which reeds' heads share; the water; sand; and stones.
pub const BED: u8 = LIMB;
pub const WATER: u8 = TOP;
pub const SAND: u8 = EXTRA;
pub const STONE: u8 = STAR;

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
    Cone { a: DVec3, ba: DVec3, ra: f64, rb: f64, l2: f64, il2: f64, a2: f64 },
    /// An ellipsoid. `leafy` ones are leaves, lit from above (see [`Model::shade`]).
    Blob { c: DVec3, inv: DVec3, least: f64, leafy: bool },
    /// A length of a palm's trunk: eight-sided in plan, from height
    /// `a.z` to `b.z`, its middle going from `a` to `b` and its size from
    /// `ra` to `rb` (the distance from the middle to a face). The section
    /// is taken level and not square to the lean, which keeps four of the
    /// eight faces on the grid's own planes whatever the lean; `slack`
    /// makes up for the lean, so the value is never more than the distance.
    /// `cut` is where its four slanting faces are, as the sum of the two
    /// distances out along x and y at which they lie: for a regular
    /// octagon the size times the root of 2, and for a trunk on the
    /// lattice the whole number nearest that (see [`Grown::trunk`]). 0
    /// for regular.
    Column { a: DVec3, b: DVec3, ra: f64, rb: f64, slack: f64, cut: f64 },
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
        Shape::Cone { a, ba, ra, rb, l2, il2: 1.0 / l2, a2: l2 - rr * rr }
    }

    fn distance(&self, p: DVec3) -> f64 {
        match *self {
            Shape::Cone { a, ba, ra, rb, l2, il2, a2 } => {
                let (pa, rr) = (p - a, ra - rb);
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
            Shape::Column { a, b, ra, rb, slack, cut } => {
                let t = ((p.z - a.z) / (b.z - a.z)).clamp(0.0, 1.0);
                let q = (p.truncate() - a.truncate().lerp(b.truncate(), t)).abs();
                let size = ra + (rb - ra) * t;
                let slant = if cut > 0.0 { (q.x + q.y - cut) * FRAC_1_SQRT_2 } else { (q.x + q.y) * FRAC_1_SQRT_2 - size };
                let out = (q.x.max(q.y) - size).max(slant);
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

    /// A tuft of leaves: a ball of them, `radii` to each side of `c`.
    ///
    /// Built as a smaller ball with many small ones standing out of it all
    /// over, each a cluster of leaves two or three cells across, and the
    /// gaps between them the hollows. A smooth ball of bricks is a
    /// boulder. The first roughening was smooth noise added to the ball's
    /// distance, which looked much the same and cost four times as much:
    /// noise steepens a distance function, so every value had to be more
    /// than halved to stay an underestimate, and the fitter then sampled
    /// twice as deep a shell of cells, working the noise out at each. A
    /// union of balls is an exact distance with nothing to make up for.
    fn leaves(&mut self, c: DVec3, radii: DVec3) {
        let least = radii.min_element();
        self.blob(c, (radii - 1.5).max(DVec3::splat(1.0)), LEAF, true);
        if least < 2.6 {
            return; // too small to have clusters: the ball itself
        }
        // enough clusters to cover three quarters of it
        let area = 4.0 * PI * (radii.x * radii.y + radii.x * radii.z + radii.y * radii.z) / 3.0;
        for _ in 0..(area * 0.75 / (PI * 2.3 * 2.3)).round() as usize {
            let (way, r) = (self.any_way(), self.between(1.6, 3.0).min(least * 0.75));
            self.blob(c + way * (radii - 0.9), DVec3::new(r, r, r * 0.85), LEAF, true);
        }
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
///
/// One cell, where a path's segments are filed four cells to a bucket.
/// A bough of a pine is side shoots two cells apart, and a bucket four
/// cells wide lists fifty of them: measured, a pine then spent 350
/// nanoseconds a sample and three seconds a build, all of it comparing
/// with parts that were not the nearest. A bucket of one cell lists the
/// three or four that could be. A tree too large for that many buckets
/// gets larger ones.
const BUCKET: f64 = 1.0;
const MAX_BUCKETS: f64 = 2_000_000.0;

/// How near a bucket a part must come to be filed in it. A sample whose
/// bucket lists nothing is at least this far from the tree, which is all
/// the fitter asks of a far sample: over 0.87 of a cell, a cell's half
/// diagonal, it does not look closer.
const NEAR: f64 = 0.9;

/// How many buckets, each way, to one of the coarse grid that says how far
/// the empty buckets are from the tree.
const COARSE: usize = 4;

/// The patches of light and shade on leaves repeat every this many of
/// their own lengths, each way: their numbers are a table.
const LATTICE: usize = 32;

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
    /// one over `bucket`
    per_bucket: f64,
    dims: [usize; 3],
    /// the size of the coarse grid `clear` is on
    coarse: [usize; 3],
    /// bucket b holds `items[start[b]..start[b + 1]]`
    start: Vec<u32>,
    items: Vec<u32>,
    /// for each bucket of a grid `COARSE` times coarser, a distance every point of it is at least from every part not listed in its bucket
    clear: Vec<f32>,
    /// the numbers the patches of light on leaves are blended from (see [`LATTICE`])
    lattice: Vec<f32>,
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
fn noise(lattice: &[f32], p: DVec3) -> f64 {
    let f = p.floor();
    let (x, y, z) = (f.x as i64, f.y as i64, f.z as i64);
    let t = p - f;
    let t = t * t * (3.0 - 2.0 * t);
    let wrap = |v: i64| v.rem_euclid(LATTICE as i64) as usize;
    let at = |i: i64, j: i64, k: i64| lattice[wrap(x + i) + LATTICE * (wrap(y + j) + LATTICE * wrap(z + k))] as f64;
    let along = |j: i64, k: i64| at(0, j, k) + (at(1, j, k) - at(0, j, k)) * t.x;
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
        Model::with_length(tree, base, height, spread, spread * HEDGE_LENGTH)
    }

    /// [`Model::new`], for a plant that has a length as well: a hedge
    /// reaches `long` to each side of its middle along x. Other kinds take
    /// no notice of it.
    pub fn with_length(tree: &Tree, base: DVec3, height: f64, spread: f64, long: f64) -> Model {
        Model::laid(tree, base, height, spread, &Lay::straight(long))
    }

    /// [`Model::new`], for a plant that lies along a line: a hedge, a
    /// stand of reeds or a patch of grass (see [`Lay`]). Other kinds take
    /// no notice of the line.
    pub fn laid(tree: &Tree, base: DVec3, height: f64, spread: f64, lay: &Lay) -> Model {
        let tree = tree.clamped();
        let (height, spread) = (height.max(Tree::least_height(tree.kind)), spread.max(LEAST_SPREAD));
        // each kind has numbers of its own from one seed
        // (by name and not by its place in the list, so that a kind
        // added to the list does not change the plants the others grow)
        let salt = 0x7EE5_0001u64 + match tree.kind {
            Kind::Pine => 0,
            Kind::Palm => 1,
            Kind::Oak => 2,
            Kind::Boxwood => 3,
            Kind::Sunflower => 5,
            Kind::Tulip => 6,
            Kind::Beech => 7,
            Kind::Hedge => 8,
            Kind::Peony => 9,
            Kind::Hydrangea => 10,
            Kind::Petunia => 11,
            Kind::Reeds => 12,
            Kind::Grass => 13,
            Kind::Birch => 14,
            Kind::Willow => 15,
            Kind::DatePalm => 16,
            Kind::ScotsPine => 17,
            Kind::Cypress => 18,
            Kind::Yucca => 19,
            Kind::Daisy => 20,
            Kind::Daffodil => 21,
            Kind::Bed => 22,
            Kind::Cherry => 23,
        };
        let mut grown = Grown { parts: Vec::new(), limbs: Vec::new(), forks: Vec::new(), numbers: Numbers(tree.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt) };
        match tree.kind {
            Kind::Pine => pine(&mut grown, &tree, height, spread),
            Kind::Palm => palm(&mut grown, &tree, height, spread, false),
            Kind::DatePalm => palm(&mut grown, &tree, height, spread, true),
            Kind::Birch => broadleaf(&mut grown, &tree, height, spread, &BIRCH, false),
            Kind::Willow => broadleaf(&mut grown, &tree, height, spread, &WILLOW, false),
            Kind::ScotsPine => broadleaf(&mut grown, &tree, height, spread, &SCOTS, false),
            Kind::Cypress => cypress(&mut grown, &tree, height, spread),
            Kind::Yucca => yucca(&mut grown, &tree, height, spread),
            Kind::Daisy => clump(&mut grown, &tree, height, spread, daisy_at),
            Kind::Daffodil => clump(&mut grown, &tree, height, spread, daffodil_at),
            Kind::Bed => bed(&mut grown, &tree, height, spread),
            Kind::Oak => broadleaf(&mut grown, &tree, height, spread, &OAK, false),
            Kind::Cherry => broadleaf(&mut grown, &tree, height, spread, &CHERRY, lay.closed),
            Kind::Beech => broadleaf(&mut grown, &tree, height, spread, &BEECH, false),
            Kind::Hedge | Kind::Reeds => {
                // grown straight, then laid along its line (see [`Grown::lay_along`])
                let run = Run::new(lay);
                let half = run.long() / 2.0;
                let closed = lay.closed && lay.run.len() > 2;
                if tree.kind == Kind::Hedge {
                    hedge(&mut grown, &tree, height, spread, half, closed);
                    grown.lay_along(0, 0, &run, half);
                    // At a sharp corner the straight hedge laid round it
                    // opens on the outside like a hinge. A round post of
                    // the same leaves stands in each corner and closes it.
                    if lay.sharp {
                        let corners = if closed { 0..run.points.len() - 1 } else { 1..run.points.len() - 1 };
                        let mass = (spread * 0.3).clamp(2.6, height * 0.3);
                        for at in corners.map(|i| run.points[i]) {
                            grown.blob(at + DVec3::Z * height * 0.5, DVec3::new(spread - mass, spread - mass, height * 0.5 - mass * 0.5), LEAF_DARK, true);
                            let rows = ((height * 0.9) / (mass * 1.15)).ceil() as usize;
                            for row in 0..=rows {
                                for k in 0..10 {
                                    let way = TAU * (k as f64 + 0.5 * (row % 2) as f64) / 10.0;
                                    let r = mass * grown.between(0.8, 1.1);
                                    let out = if row == rows { (spread - r - 1.7) * (k as f64 / 10.0) } else { spread - r - 1.7 };
                                    grown.leaves(at + DVec3::new(way.cos(), way.sin(), 0.0) * out.max(0.0) + DVec3::Z * (height * 0.1 + r * 0.4 + (height * 0.9 - r * 1.2) * row as f64 / rows as f64), DVec3::new(r, r, r * 0.8));
                                }
                            }
                        }
                    }
                } else if closed {
                    reeds(&mut grown, &tree, height, spread, half, Some(&run));
                } else {
                    reeds(&mut grown, &tree, height, spread, half, None);
                    grown.lay_along(0, 0, &run, half);
                }
            }
            Kind::Grass => grass(&mut grown, &tree, height, spread, lay),
            Kind::Peony => peony(&mut grown, &tree, height, spread),
            Kind::Hydrangea => hydrangea(&mut grown, &tree, height, spread),
            Kind::Petunia => petunia(&mut grown, &tree, height, spread),
            Kind::Boxwood => boxwood(&mut grown, &tree, height, spread),
            Kind::Sunflower => sunflower(&mut grown, &tree, height, spread),
            Kind::Tulip => tulip(&mut grown, &tree, height, spread),
        }
        // grown standing on the origin; moved to where it stands, on the
        // floor of the cell its point is in
        let floor = base.z.floor();
        let stand = DVec3::new(base.x, base.y, floor);
        // (grass dug below the floor it lies on reaches that far under it)
        let dug = if tree.kind == Kind::Grass { grown.parts.iter().map(|part| part.shape.bounds().0.z).fold(0.0, f64::min) } else { 0.0 };
        // (the soil of a flat lawn hangs a little under its level, and is cut off there as it always was)
        let dug = if dug < -1.0 { dug.floor() } else { 0.0 };
        let ground = floor + dug;
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
        let (mut lo, mut hi) = (DVec3::MAX, DVec3::MIN);
        for part in &grown.parts {
            let (a, b) = part.shape.bounds();
            (lo, hi) = (lo.min(a), hi.max(b));
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
                let reach = NEAR;
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
        // How clear of the tree the empty buckets are, worked out on a
        // grid `COARSE` times coarser: a coarse bucket k away from every
        // one with anything in it is (k - 1) coarse buckets clear of every
        // part. The fitter passes over a whole block of cells whose middle
        // is 7 cells clear, which is most of the box round a tree. On the
        // buckets themselves this took longer than growing the tree.
        let coarse = dims.map(|d| d.div_ceil(COARSE));
        let wide = |x: usize, y: usize, z: usize| x + coarse[0] * (y + coarse[1] * z);
        let mut steps = vec![u8::MAX; coarse[0] * coarse[1] * coarse[2]];
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let b = index(x, y, z);
                    if start[b + 1] > start[b] {
                        steps[wide(x / COARSE, y / COARSE, z / COARSE)] = 0;
                    }
                }
            }
        }
        let far = (8.0 / (bucket * COARSE as f64)).ceil() as u8 + 1;
        for step in 1..=far {
            let before = steps.clone();
            for z in 0..coarse[2] {
                for y in 0..coarse[1] {
                    for x in 0..coarse[0] {
                        if before[wide(x, y, z)] != u8::MAX {
                            continue;
                        }
                        let beside = (z.saturating_sub(1)..=(z + 1).min(coarse[2] - 1)).any(|k| (y.saturating_sub(1)..=(y + 1).min(coarse[1] - 1)).any(|j| (x.saturating_sub(1)..=(x + 1).min(coarse[0] - 1)).any(|i| before[wide(i, j, k)] == step - 1)));
                        if beside {
                            steps[wide(x, y, z)] = step;
                        }
                    }
                }
            }
        }
        let clear = steps.iter().map(|s| NEAR.max(((*s).clamp(1, far + 1) - 1) as f64 * bucket * COARSE as f64) as f32).collect();
        let seed = tree.seed as u32 ^ 0x5EED;
        let lattice = (0..LATTICE * LATTICE * LATTICE).map(|n| hash((n % LATTICE) as i32, (n / LATTICE % LATTICE) as i32, (n / (LATTICE * LATTICE)) as i32, seed) as f32).collect();
        Model { kind: tree.kind, parts: grown.parts, limbs: grown.limbs, forks: grown.forks, ground, lo, hi, origin, bucket, per_bucket: 1.0 / bucket, dims, coarse, start, items, clear, lattice }
    }

    /// The box the tree fills.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        (self.lo, self.hi)
    }

    /// How many parts the tree is the union of.
    pub fn parts(&self) -> usize {
        self.parts.len()
    }

    /// Negative inside the tree, positive outside; near its surface, no
    /// more than the distance to it in cells. Also the part `p` is deepest
    /// in or nearest to, if any is near.
    ///
    /// With `settle`, the first part found that `p` is well inside ends
    /// the search: the answer is then that part's depth and not the
    /// deepest, which is still no more than the distance, and it is all
    /// the fitter asks of a point inside. Half of a fit's samples are
    /// inside, most of them in several parts at once.
    fn nearest(&self, p: DVec3, settle: bool) -> (f64, Option<usize>) {
        // (no rounding down: within the grid these are not negative, and
        // turning one into a whole number drops its fraction. Rounding
        // down is a call into the maths library on a processor not known
        // to have the instruction for it, and three calls a sample were a
        // third of a fit's time.)
        let c = (p - self.origin) * self.per_bucket;
        let under = self.ground - p.z;
        if c.min_element() < 0.0 || c.x >= self.dims[0] as f64 || c.y >= self.dims[1] as f64 || c.z >= self.dims[2] as f64 {
            // outside the grid, which reaches `NEAR` past every part
            let out = (self.origin - p).max(p - (self.origin + DVec3::new(self.dims[0] as f64, self.dims[1] as f64, self.dims[2] as f64) * self.bucket)).max_element();
            return ((out + NEAR).max(under), None);
        }
        let b = c.x as usize + self.dims[0] * (c.y as usize + self.dims[1] * c.z as usize);
        let (mut best, mut which) = (f64::INFINITY, None);
        for &n in &self.items[self.start[b] as usize..self.start[b + 1] as usize] {
            let d = self.parts[n as usize].shape.distance(p);
            if d < best {
                (best, which) = (d, Some(n as usize));
                if settle && d < -NEAR {
                    break;
                }
            }
        }
        if best > NEAR {
            // nothing listed is near: how clear the bucket is of everything else
            let wide = c.x as usize / COARSE + self.coarse[0] * (c.y as usize / COARSE + self.coarse[1] * (c.z as usize / COARSE));
            best = best.min(self.clear[wide] as f64);
        }
        (best.max(under), which)
    }

    /// Negative inside the tree, positive outside; near its surface, no
    /// more than the distance to it in cells.
    pub fn eval(&self, p: DVec3) -> f64 {
        self.nearest(p, true).0
    }

    /// The colour number of the tree at `p`: that of the part `p` is
    /// deepest in, with the light and shade a part has within itself.
    pub fn shade(&self, p: DVec3) -> u8 {
        let Some(n) = self.nearest(p, false).1 else { return TRUNK };
        let part = &self.parts[n];
        match (self.kind, part.shape, part.shade) {
            // The top of a pine's boughs: needles with open air a cell
            // and a half above them. A paler green in the natural sets
            // and white in the snowy ones, where it is the snow lying.
            (Kind::Pine, _, LEAF | LEAF_DARK | LEAF_LIGHT) if self.eval(p + DVec3::Z * 1.6) > 0.0 && self.eval(p + DVec3::Z * 2.6) > 0.0 => TOP,
            // Leaves are light where the sky is over them and dark on
            // the underside of a mass, with the boundaries uneven. By
            // the open air above and below, and not by where in its own
            // ball of leaves a brick is: that lit every ball like a
            // lamp, and a mass read as the balls it was made of.
            (_, Shape::Blob { leafy: true, .. }, _) => {
                let patch = noise(&self.lattice, p / 7.0 + 20.5);
                let open = |up: f64| self.eval(p + DVec3::Z * up) > 0.0;
                if open(2.5 + 2.0 * patch) && open(5.5) {
                    LEAF_LIGHT
                } else if open(-2.2 + patch) || patch < -0.55 {
                    LEAF_DARK
                } else {
                    LEAF
                }
            }
            // a birch's white bark is marked with black: bands and patches, more of them low down
            // a cherry's bark is banded across with pale lenticels: every third layer of its trunk, broken here and there
            (Kind::Cherry, Shape::Column { .. }, TRUNK) if (p.z.floor() as i64).rem_euclid(3) == 1 && noise(&self.lattice, DVec3::new(p.x * 0.6, p.y * 0.6, p.z * 0.3)) > -0.35 => STAR,
            (Kind::Birch, _, TRUNK | LIMB) if noise(&self.lattice, DVec3::new(p.x * 0.35, p.y * 0.35, p.z * 1.1)) > 0.42 => EXTRA,
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
    pub fn colour(&self, bricks: Vec<CellBrick>, max_ext: i32, threads: crate::par::Threads) -> (Vec<CellBrick>, Vec<u8>) {
        let (mut out, mut shades) = (Vec::with_capacity(bricks.len()), Vec::with_capacity(bricks.len()));
        let mut loose: [Vec<[i32; 3]>; 8] = Default::default();
        let middle = |c: [i32; 3], e: [i32; 3]| DVec3::new(c[0] as f64 + e[0] as f64 / 2.0, c[1] as f64 + e[1] as f64 / 2.0, c[2] as f64 + e[2] as f64 / 2.0);
        // The asking is shared out over threads: each brick's colours
        // depend on nothing but the brick. What is done with the answers
        // is done in the bricks' own order, so the result is the same
        // on any number of threads.
        let asked: Vec<Vec<u8>> = threads.map(bricks.len(), 2048, |i| {
            let b = &bricks[i];
            if b.mask != CUBE || b.ext == [1; 3] {
                return vec![self.shade(middle(b.min, b.ext))];
            }
            let mut found = Vec::with_capacity((b.ext[0] * b.ext[1] * b.ext[2]) as usize);
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        found.push(self.shade(middle([b.min[0] + x, b.min[1] + y, b.min[2] + z], [1; 3])));
                    }
                }
            }
            found
        });
        for (b, found) in bricks.into_iter().zip(asked) {
            if found.iter().all(|f| *f == found[0]) {
                shades.push(found[0]);
                out.push(b);
                continue;
            }
            let mut each = found.into_iter();
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        loose[each.next().unwrap().min(7) as usize].push([b.min[0] + x, b.min[1] + y, b.min[2] + z]);
                    }
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
    let tip = 0.7;
    // (it once swayed a hundredth of its height, which was a step in its trunk every few cells and could not be seen as a lean)
    let (lean_way, lean) = (g.between(0.0, TAU), 0.0 * g.unit());
    let stem = |z: f64| DVec3::new(lean_way.cos(), lean_way.sin(), 0.0) * (lean * (z / height * PI * 1.5).sin()) + DVec3::Z * z;
    let thick = |z: f64| tip + (foot - tip) * (1.0 - z / height).max(0.0).powf(0.85) + foot * 0.4 * (-z / (foot * 1.6)).exp();
    // (straight and eight-sided: see [`Grown::trunk`])
    let trunk = g.trunk(DVec3::ZERO, height, &thick, TRUNK);

    // bare below, boughs from there to just under the leader
    let (low, high) = (height * g.between(0.09, 0.14), height * 0.93);
    let gap = (high - low) / (tiers - 1) as f64;
    let mut tips = Vec::new();
    // how long a tier's boughs are, as a share of the longest: shortening
    // to the top, and the lowest tiers drawn in a little
    let share = |u: f64| ((1.0 - u).powf(0.9) * 0.9 + 0.1) * (0.84 + 0.16 * (u / 0.12).min(1.0));
    let longest = (0..tiers).map(|tier| share(tier as f64 / (tiers - 1) as f64)).fold(0.0, f64::max);
    for tier in 0..tiers {
        // 0 at the lowest tier, 1 at the highest
        let u = tier as f64 / (tiers - 1) as f64;
        // the tiers closer together towards the top, where each year's growth is shorter
        let z = low + (high - low) * (0.55 * u + 0.45 * (1.0 - (1.0 - u).powf(1.7)));
        let reach = spread * share(u) / longest;
        let boughs = ((4.0 + 3.0 * (1.0 - u)) * (0.75 + 0.5 * full)).round().max(3.0) as usize;
        let turn = g.between(0.0, TAU);
        for k in 0..boughs {
            let way = turn + TAU * (k as f64 + g.between(-0.22, 0.22)) / boughs as f64;
            // (the first of a tier at its full length, so that the tree is as wide as asked)
            let (length, at) = (reach * if k == 0 { 1.0 } else { g.between(0.84, 1.0) }, z + g.between(-0.2, 0.2) * gap);
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
    let bare = (0.05 + 0.15 * (1.0 - u)).min(1.0 - 4.0 / length.max(5.0)).max(0.0);
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
fn palm(g: &mut Grown, tree: &Tree, height: f64, spread: f64, date: bool) {
    let (full, lean) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let fronds = tree.count.max(3) as usize;
    // the upper fronds stand above the top of the trunk by about a third of their length
    let top = (height - spread * 0.34).max(height * 0.45);
    // A date palm, *Phoenix dactylifera*, is the same plant in other
    // proportions: a trunk half as thick again, upright, rough all the
    // way up with the stumps of cut fronds; three times the fronds, stiff
    // and nearly straight, set all round into a ball, the lowest hanging;
    // and narrow leaflets held up in a V.
    // https://en.wikipedia.org/wiki/Date_palm
    let slim = (height * 0.021).max(1.6) * if date { 1.55 } else { 1.0 };
    // The lean: steepest at the foot, gone by the top. The slope at
    // height t (0 to 1) is `steep * (1 - t)^1.7`, and the stem's place is
    // the sum of the slopes below it.
    let (way, steep) = (g.between(0.0, TAU), lean * if date { 0.3 } else { 0.85 });
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
        // (the last may be cut short by the top of the trunk, and widens by as much less)
        let (ra, rb) = (thick(z0) - 0.3, thick(z1) - 0.3 + if date { 1.3 } else { 0.65 } * (z1 - z0) / ring);
        let slack = 1.0 / (1.0 + run * run + ((rb - ra) / (z1 - z0).max(1e-9)).powi(2)).sqrt();
        // its top cell is the scar: a ring of another colour
        let scar = (z1 - 1.0).max(z0);
        let at = a.lerp(b, (scar - z0) / (z1 - z0).max(1e-9));
        let r_at = ra + (rb - ra) * (scar - z0) / (z1 - z0).max(1e-9);
        if scar > z0 {
            g.parts.push(Part { shape: Shape::Column { a, b: at, ra, rb: r_at, slack, cut: 0.0 }, shade: TRUNK });
        }
        g.parts.push(Part { shape: Shape::Column { a: at, b, ra: r_at, rb, slack, cut: 0.0 }, shade: LIMB });
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
        let leaves_at = if date { 0.1 + 2.1 * age } else { 0.12 + 1.32 * age.powf(0.85) } + g.between(-0.07, 0.07);
        let arch = (0.75 + 0.95 * age) * g.between(0.85, 1.15) * if date { 0.3 } else { 1.0 };
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
        let long = length * 0.27 * (0.65 + 0.7 * full) * if date { 0.6 } else { 1.0 };
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
                let hang = if date { -0.25 } else { 0.15 + 0.75 * age };
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
    if tree.extras && date {
        // dates: bunches hanging on stalks from under the crown
        for k in 0..6 {
            let way = k as f64 * GOLDEN_ANGLE;
            let out = DVec3::new(way.cos(), way.sin(), 0.0);
            let (from, to) = (heart - DVec3::Z * slim, heart + out * slim * 3.0 - DVec3::Z * slim * 3.5);
            g.cone(from, to, 1.0, 0.9, LIMB);
            for n in 0..7 {
                let off = g.any_way();
                g.blob(to + DVec3::new(off.x, off.y, off.z.abs() * -1.5) * slim * 0.7 - DVec3::Z * n as f64 * 0.4, DVec3::splat(1.3), EXTRA, false);
            }
        }
    } else if tree.extras {
        // coconuts, in a cluster under the crown round the trunk
        let nut = (slim * 1.0).max(2.2);
        let nuts = 6 + (g.unit() * 4.0) as usize;
        for k in 0..nuts {
            let way = k as f64 * GOLDEN_ANGLE;
            let at = heart + DVec3::new(way.cos(), way.sin(), 0.0) * (slim + nut * 0.9) - DVec3::Z * (slim * 1.6 + nut * 1.1 * (k % 3) as f64);
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

/// How a broadleaf tree grows: the numbers that make the one rule of
/// forking an oak or a beech (see [`broadleaf`]).
struct Habit {
    /// the trunk's radius, as a share of the tree's height
    girth: f64,
    /// between what shares of the height the trunk first divides
    bole: (f64, f64),
    /// a mass of leaves, as a share of the spread: at no fullness, and what full adds
    mass: (f64, f64),
    /// whether the trunk carries on up through the crown as the thickest limb
    leader: bool,
    /// between what angles from upright the limbs leave the trunk, in radians
    polar: (f64, f64),
    /// how hard a heavy limb is drawn outwards and down, and a thin one up
    out: f64,
    sag: f64,
    up: f64,
    /// how far a limb wanders at full crookedness
    wander: f64,
    /// how flat a tuft of leaves is: its height over its width
    flat: f64,
    /// how far up the crown its widest part is
    heart: f64,
    /// branches thinner than this carry leaves along them, not only at their ends
    leafy: f64,
    /// how far the roots run out, against an oak's
    roots: f64,
    /// 0 for stout crooked twigs, 1 for many slender straight ones (see [`twigs`])
    fine: f64,
    /// how far its branch ends hang: 0 for none, 1 for a willow's curtains to near the ground
    weep: f64,
    /// how far the smaller tufts of a mass stand above and below its middle: little, for leaves in flat layers
    spray: f64,
    /// in blossom: clouds of small clusters along its branches, where another tree has masses of leaves at their ends (see [`CHERRY`])
    bloom: bool,
}

/// An oak: low fork, wide heavy limbs, leaves in separate masses.
const OAK: Habit = Habit { girth: 0.055, bole: (0.17, 0.23), mass: (0.058, 0.045), leader: false, polar: (0.78, 1.22), out: 0.36, sag: 0.1, up: 0.15, wander: 0.44, flat: 0.78, heart: 0.15, leafy: 1.5, roots: 1.0, fine: 0.0, weep: 0.0, spray: 0.5, bloom: false };

/// A Japanese flowering cherry in blossom, *Prunus serrulata* or the
/// Yoshino cherry: a short dark trunk banded across with pale lenticels,
/// forking low into wide limbs that sweep a little upwards, and a broad
/// airy crown wider than the tree is tall. It flowers before its leaves,
/// so what is seen is blossom along the bare dark branches, in clouds.
/// https://en.wikipedia.org/wiki/Prunus_serrulata
/// https://en.wikipedia.org/wiki/Cherry_blossom
/// The oak's rule of forking, with limbs that leave the trunk wide and
/// turn up a little, and with blossom on every branch under five and a
/// half cells thick and not only at the ends, which is what puts it along the
/// branches.
const CHERRY: Habit = Habit { girth: 0.042, bole: (0.15, 0.2), mass: (0.065, 0.04), leader: false, polar: (1.0, 1.4), out: 0.46, sag: 0.0, up: 0.1, wander: 0.3, flat: 0.6, heart: 0.18, leafy: 5.5, roots: 0.8, fine: 0.5, weep: 0.0, spray: 0.42, bloom: true };

/// A beech, *Fagus sylvatica*: a smooth grey trunk that carries on up
/// through a dense, oval crown about three quarters as wide as the tree
/// is tall, with limbs that climb and then sweep out into flat sprays of
/// slender twigs.
/// https://en.wikipedia.org/wiki/Fagus_sylvatica
/// https://plants.ces.ncsu.edu/plants/fagus-sylvatica/common-name/european-beech
/// It is the oak's rule with other numbers, which is the reason the rule
/// has numbers: a leading stem, limbs that wander less, flatter tufts of
/// leaves on more of the branches (so the crown closes), and small roots.
const BEECH: Habit = Habit { girth: 0.04, bole: (0.22, 0.28), mass: (0.09, 0.055), leader: true, polar: (0.75, 1.1), out: 0.24, sag: 0.0, up: 0.1, wander: 0.24, flat: 0.42, heart: 0.34, leafy: 3.0, roots: 0.55, fine: 1.0, weep: 0.0, spray: 0.12, bloom: false };

/// A silver birch, *Betula pendula*: a slender white trunk marked with
/// black, carrying on to the top of a narrow, open crown of thin
/// climbing limbs whose ends hang.
/// https://en.wikipedia.org/wiki/Betula_pendula
const BIRCH: Habit = Habit { girth: 0.019, bole: (0.26, 0.34), mass: (0.12, 0.06), leader: true, polar: (0.4, 0.75), out: 0.12, sag: 0.0, up: 0.18, wander: 0.22, flat: 0.8, heart: 0.4, leafy: 2.6, roots: 0.4, fine: 1.0, weep: 0.3, spray: 0.5, bloom: false };

/// A weeping willow, *Salix babylonica*: a short stout trunk, limbs that
/// climb and spread into a wide dome, and from every branch end a
/// curtain of long thin shoots hanging straight down, some to the ground.
/// https://en.wikipedia.org/wiki/Salix_babylonica
const WILLOW: Habit = Habit { girth: 0.05, bole: (0.2, 0.26), mass: (0.06, 0.035), leader: false, polar: (0.5, 0.95), out: 0.3, sag: 0.0, up: 0.16, wander: 0.36, flat: 0.7, heart: 0.45, leafy: 1.8, roots: 0.9, fine: 1.0, weep: 1.0, spray: 0.4, bloom: false };

/// A Scots pine, *Pinus sylvestris*, grown old: a tall bare trunk, its
/// lower boughs long shed, and at the top a flat, broken crown of
/// needles in plates on a few crooked level limbs. Its upper bark is
/// orange, which the colours give its limbs.
/// https://en.wikipedia.org/wiki/Pinus_sylvestris
/// It is grown by the forking rule and not the pine's tiers: an old
/// Scots pine has lost the tiers a young one had, and what is left is a
/// few limbs that fork.
const SCOTS: Habit = Habit { girth: 0.034, bole: (0.52, 0.6), mass: (0.12, 0.06), leader: true, polar: (1.0, 1.45), out: 0.3, sag: 0.04, up: 0.02, wander: 0.5, flat: 0.34, heart: 0.5, leafy: 1.3, roots: 0.5, fine: 0.0, weep: 0.0, spray: 0.1, bloom: false };

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
fn broadleaf(g: &mut Grown, tree: &Tree, height: f64, spread: f64, habit: &Habit, fallen: bool) {
    let (dense, gnarl) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let limbs = tree.count.max(2) as usize;
    let girth = (height * habit.girth).max(2.4);
    // the height the trunk divides at: for an oak low, a fifth to a quarter of the way up
    let bole = height * g.between(habit.bole.0, habit.bole.1);
    // how big a mass of leaves is
    let mass = (spread * (habit.mass.0 + habit.mass.1 * dense)).max(2.6);

    // the trunk, a little crooked, widening to its foot
    // (straight and eight-sided: see [`Grown::trunk`]. It was a little
    // crooked, by half its thickness at most, which in bricks was a
    // trunk stepped all the way up and not one seen to be crooked; an
    // oak's crookedness is in its limbs.)
    let _ = g.between(0.0, TAU);
    let stem = |z: f64| DVec3::Z * z;
    // (widening to its foot by up to a third, over twice its own thickness of height)
    // A tree that keeps its stem through its crown narrows all the way up
    // it, and is at its fork a little over three quarters of what it is
    // near the ground; one that divides there into limbs is a column to
    // its fork. What it is at the fork is what its limbs share.
    // (a slender one, a birch, by more)
    let neck = if habit.leader { girth * if habit.girth < 0.03 { 0.7 } else { 0.78 } } else { girth };
    let trunk = g.trunk(DVec3::ZERO, bole, &|z: f64| (girth + (neck - girth) * (z / bole).min(1.0)) * (1.0 + 0.36 * habit.roots * (-z / (girth * 2.2)).exp()), TRUNK);
    // and narrowing under its fork to what its limbs are together, so that they do not leave a shoulder
    g.cone(DVec3::Z * (bole - neck * 0.4), DVec3::Z * (bole + neck * 0.7), neck * 0.85, neck * 0.6, TRUNK);
    // the roots where they leave the trunk: buttresses running down into the ground
    let roots = 5 + (g.unit() * 3.0) as usize;
    let turn = g.between(0.0, TAU);
    for k in 0..roots {
        let way = turn + TAU * (k as f64 + g.between(-0.25, 0.25)) / roots as f64;
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        // A buttress is a thin ridge running down the flare of the
        // trunk and out along the ground a little way. They were stout
        // cones reaching two and a half trunks out, ending half way up
        // a trunk that had hardly widened: a wide skirt with a post
        // standing in it. A slender tree has none.
        let far = girth * g.between(1.55, 1.95) * habit.roots;
        // (and no higher than half way up to its fork)
        let high = (girth * g.between(2.4, 3.4) * habit.roots).min(bole * 0.5);
        if habit.roots >= 0.7 {
            g.cone(out * girth * 0.72 + DVec3::Z * high, out * far - DVec3::Z * girth * 0.2, girth * 0.3, girth * 0.24, TRUNK);
        }
    }

    // The crown's outline: a dome, widest a fifth of the way up it and
    // reaching a little below where the trunk divides, as the lowest
    // limbs of an oak in the open do. Branch ends stop short of it by
    // the leaves they carry.
    let fork = stem(bole);
    let (out, over) = ((spread - mass * 1.4).max(spread * 0.5), (height - mass * 1.3).max(height * 0.6));
    let heart = DVec3::new(fork.x, fork.y, bole + (over - bole) * habit.heart);
    let dome = DVec3::new(out, out, over - heart.z);
    let under = heart.z - bole * 0.8;
    // where a point is in the dome: 0 at its heart, 1 on its outline
    let within = |p: DVec3| {
        let q = p - heart;
        DVec3::new(q.x / dome.x, q.y / dome.y, q.z / if q.z < 0.0 { under } else { dome.z })
    };

    // The branches are grown twice from the same numbers. The first
    // time at no particular size, to find how far they reach; the second
    // at the size that fills the dome, and held within it: a branch near
    // the outline turns along it. Growing once and stretching the result
    // to fit was tried first, and one long limb then set the size of the
    // whole crown, with the rest of it huddled round the trunk.
    let (first, first_fork, numbers) = (g.limbs.len(), g.forks.len(), g.numbers.0);
    let mut ends: Vec<(DVec3, DVec3, f64)> = Vec::new();
    let mut scale = 1.0;
    for sized in [false, true] {
        g.limbs.truncate(first);
        g.forks.truncate(first_fork);
        g.numbers.0 = numbers;
        ends.clear();
        // the limbs the trunk divides into, sharing its cross-section
        let mut shares: Vec<f64> = (0..limbs).map(|_| g.between(0.7, 1.3)).collect();
        if habit.leader {
            // (the stem carries on as the thickest of them: of a slender tree, a birch, as most of it, its limbs being thin things off a stem)
            shares[0] *= if habit.girth < 0.03 { 5.0 } else { 2.4 };
        }
        let total: f64 = shares.iter().sum();
        let turn = g.between(0.0, TAU);
        let mut waiting = Vec::new();
        let mut radii = Vec::new();
        for (k, share) in shares.iter().enumerate() {
            let radius = neck * (share / total).sqrt();
            radii.push(radius);
            let way = turn + TAU * (k as f64 + g.between(-0.2, 0.2)) / limbs as f64;
            // striking out at wide angles, 45 to 70 degrees off upright;
            // and where there are four or more, one of them nearer
            // upright, to fill the top of the crown
            let polar = if k == 0 && habit.leader { g.between(0.02, 0.12) } else if k == 0 && limbs >= 4 { g.between(0.15, 0.4) } else { g.between(habit.polar.0, habit.polar.1) };
            waiting.push(Shoot { from: fork, way: DVec3::new(way.cos() * polar.sin(), way.sin() * polar.sin(), polar.cos()), radius, parent: trunk, level: 1, roll: g.between(0.0, TAU) });
        }
        g.forks.push((neck, radii));

        while let Some(shoot) = waiting.pop() {
            let (mut at, mut way, mut radius, mut roll) = (shoot.from, shoot.way, shoot.radius, shoot.roll);
            let (mut path, mut thick) = (vec![at], vec![radius]);
            // (it is recorded once grown, and its children are told then which limb it is)
            let mut children = Vec::new();
            loop {
                // one length between forks, in two steps so that it can bend
                let run = scale * 6.2 * radius.powf(0.62) * g.between(0.8, 1.25);
                for _ in 0..2 {
                    let outward = DVec3::new(at.x - fork.x, at.y - fork.y, 0.0).normalize_or_zero();
                    let heavy = (radius / girth).min(1.0);
                    // heavy limbs spread and sag; thin ones turn up
                    let mut pull = outward * (habit.out * heavy) - DVec3::Z * (habit.sag * heavy) + DVec3::Z * (habit.up * (1.0 - heavy).powi(2));
                    let wander = g.any_way() * (habit.wander * gnarl);
                    if sized {
                        // near the outline, turned back along it
                        let q = within(at);
                        let near = q.length();
                        if near > 0.7 {
                            pull -= (q / dome).normalize_or_zero() * ((near - 0.7) * 3.5);
                        }
                    }
                    way = (way + wander + pull).normalize();
                    // no limb of an oak dives
                    if way.z < -0.15 {
                        way = DVec3::new(way.x, way.y, -0.15).normalize();
                    }
                    at += way * run * 0.5;
                    path.push(at);
                    thick.push(radius);
                }
                if radius < TWIG {
                    ends.push((at, way, 1.0));
                    break;
                }
                // the outer branches carry leaves along them too
                if radius < habit.leafy {
                    ends.push((at, way, 0.7));
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
                let chance = g.unit();
                if side_way.z < -0.05 && chance < 0.85 {
                    hinge = -hinge;
                    side_way = turned(way, hinge, angle);
                }
                // the limb itself turns the other way, by less the more of it carries on
                way = turned(way, hinge, -angle * (1.0 - share) * 0.9);
                if aside >= 0.72 {
                    children.push(Shoot { from: at, way: side_way, radius: aside, parent: 0, level: shoot.level + 1, roll: g.between(0.0, TAU) });
                } else {
                    // too thin to follow: a tuft of leaves on the limb here
                    ends.push((at, side_way, 0.7));
                }
                radius = carries;
            }
            let me = g.limb_record(path, thick, Some(shoot.parent), shoot.level);
            for mut child in children {
                child.parent = me;
                waiting.push(child);
            }
        }
        if !sized {
            // the size at which all but the furthest seventh of the ends are within nine tenths of the dome's width
            let mut far: Vec<f64> = ends.iter().map(|e| (e.0 - fork).truncate().length()).collect();
            far.sort_by(f64::total_cmp);
            scale = 0.9 * out / far[(far.len() * 6 / 7).min(far.len() - 1)].max(1e-9);
        }
    }
    // What the holding in leaves over is taken up by stretching: the
    // furthest end to the dome's width and the highest to its top.
    let reach = ends.iter().map(|e| (e.0 - fork).truncate().length()).fold(1e-9, f64::max);
    let rise = ends.iter().map(|e| e.0.z - bole).fold(1e-9, f64::max);
    let (wide, tall) = (out / reach, (over - bole) / rise);
    let place = |p: DVec3| DVec3::new(fork.x + (p.x - fork.x) * wide, fork.y + (p.y - fork.y) * wide, bole + (p.z - bole) * tall);
    for n in first..g.limbs.len() {
        let path: Vec<DVec3> = g.limbs[n].path.iter().map(|p| place(*p)).collect();
        let radius = g.limbs[n].radius.clone();
        let level = g.limbs[n].level;
        for i in 1..path.len() {
            g.cone(path[i - 1], path[i], radius[i - 1], radius[i], // (a slender tree's limbs keep its trunk's bark while they are over a cell and a half thick: a birch is white well up into its crown)
            if (level == 1 && radius[i] > girth * 0.42) || (habit.girth < 0.03 && radius[i] > 1.02) { TRUNK } else { LIMB });
        }
        g.limbs[n].path = path;
    }

    // The leaves. A branch that ends is carried on as a spray of
    // twigs too thin to build, wandering on and up for three lengths,
    // and at each length there is a tuft: one flattened ball of leaves
    // and two smaller beside it. A branch's tufts run into each other
    // and into its neighbours', which makes the masses; no mass is one
    // ball, which in bricks is a clipped bush.
    let hold = |c: DVec3| {
        let (room, far) = ((spread - 1.0).max(0.0), (c - fork).truncate().length());
        let back = (c - fork).truncate() * (room / far.max(1e-9)).min(1.0);
        DVec3::new(fork.x + back.x, fork.y + back.y, c.z.clamp(bole * 0.6, height - 1.0))
    };
    for (end, way, size) in ends {
        let mut at = place(end);
        let mut way = DVec3::new(way.x * wide, way.y * wide, way.z * tall).normalize_or_zero();
        if tree.extras {
            // bare: the branch carried on in twigs, where in leaf it ends in tufts (see [`twigs`])
            // (a fork deeper on a tall tree, which has the room for it)
            let deep = if size >= 1.0 { 2 + habit.fine as u8 + (height > 150.0) as u8 } else { 1 };
            twigs(g, at, way, mass * (1.25 + 0.2 * habit.fine), deep, habit.fine, gnarl, &hold);
            continue;
        }
        if habit.bloom {
            // Blossom: a loose cloud of small clusters round this place
            // on the branch, each its own ball two to three cells
            // through and most a cell or two from the next, so that sky
            // and the dark branch show between them. One ball of
            // clusters, as an oak's tuft is, would be a pink oak. Of
            // every twenty clusters nine are the middle pink, five
            // white, four the deep pink, one a dark bud and one a young
            // bronze leaf.
            let clusters = ((9.0 + 16.0 * dense) * size) as usize;
            for _ in 0..clusters {
                let (off, far, throw) = (g.any_way(), g.unit().sqrt() * mass * 2.1, g.unit());
                let r = if throw >= 0.9 { g.between(1.2, 1.6) } else { g.between(1.6, 2.7) };
                let c = hold(at + DVec3::new(off.x, off.y, off.z * 0.45) * far + DVec3::Z * mass * 0.25);
                let shade = if throw < 0.45 { LEAF } else if throw < 0.7 { LEAF_LIGHT } else if throw < 0.9 { LEAF_DARK } else if throw < 0.95 { EXTRA } else { TOP };
                g.blob(DVec3::new(c.x, c.y, c.z.min(height - r)), DVec3::new(r, r, r * 0.8), shade, false);
            }
            continue;
        }
        if habit.weep > 0.0 && size >= 1.0 {
            // Hanging shoots: from the branch end a few thin strands of
            // leaves, out a little and then straight down. A birch's are
            // a few cells long; a willow's reach most of the way to the
            // ground, each its own length, which is its curtain.
            let drop = if habit.weep >= 1.0 { (at.z - height * 0.06).max(4.0) } else { mass * 2.5 * habit.weep / 0.3 };
            for _ in 0..(2.0 + 3.0 * dense * habit.weep.min(1.0) + 1.0) as usize {
                let off = g.any_way();
                let from = hold(at + DVec3::new(off.x, off.y, 0.3) * mass * 1.2);
                let long = drop * g.between(0.45, 0.97);
                let knee = from + DVec3::new(off.x, off.y, 0.0) * mass * 0.4 - DVec3::Z * long * 0.3;
                let lean = g.any_way();
                g.cone(from, knee, 1.15, 1.1, LEAF);
                g.cone(knee, knee + DVec3::new(lean.x * 0.08, lean.y * 0.08, -1.0) * long * 0.7, 1.1, 0.85, LEAF_LIGHT);
            }
        }
        for tuft in 0..if size >= 1.0 { 3 } else { 1 } {
            let r = mass * size * g.between(0.85, 1.35) * (1.0 - 0.14 * tuft as f64) * if habit.weep >= 1.0 { 0.7 } else { 1.0 };
            // (each ball kept inside the tree's size, clusters and all: the
            // spray wanders, and the spread asked for is the spread)
            let held = |c: DVec3, r: f64| {
                let (room, far) = ((spread - r - 1.7).max(0.0), (c - fork).truncate().length());
                let back = (c - fork).truncate() * (room / far.max(1e-9)).min(1.0);
                DVec3::new(fork.x + back.x, fork.y + back.y, c.z.min(height - r - 1.7))
            };
            at = held(at, r);
            g.leaves(held(at + DVec3::Z * r * 0.2, r), DVec3::new(r, r, r * habit.flat));
            for _ in 0..2 {
                let (off, small) = (g.any_way(), r * g.between(0.55, 0.8));
                g.leaves(held(at + DVec3::new(off.x, off.y, off.z * habit.spray) * r * 0.9, small), DVec3::new(small, small, small * 0.72 * (habit.flat / 0.78).min(1.0)));
            }
            way = (way + g.any_way() * 0.6 + DVec3::Z * 0.2).normalize();
            way.z = way.z.max(-0.1) * (habit.spray / 0.5).min(1.0);
            at += way * r * 1.15;
        }
    }
    if fallen && habit.bloom && !tree.extras {
        // fallen petals: small flat drifts on the ground under the crown, thickest under its middle
        for _ in 0..(spread * spread * 0.035) as usize {
            let (way, far) = (g.between(0.0, TAU), g.unit().powf(0.7) * (spread - 2.0));
            let (r, pale) = (g.between(1.2, 2.0), g.unit() < 0.4);
            g.blob(DVec3::new(fork.x + way.cos() * far, fork.y + way.sin() * far, 0.35), DVec3::new(r, r, 0.75), if pale { LEAF_LIGHT } else { LEAF }, false);
        }
    }
}

/// Berries on a tuft of leaves at `at`, `r` across: a few small balls
/// standing just proud of its outside.
///
/// A box has no berries (its fruit is a small brown capsule); these are a
/// holly's or a cotoneaster's, asked for on a clipped bush because a hedge
/// in berry is what is wanted of it in a build.
/// https://www.rhs.org.uk/plants/hollies
fn berries(g: &mut Grown, at: DVec3, r: f64, from: DVec3) {
    for _ in 0..2 {
        let out = ((at - from).normalize_or_zero() + g.any_way() * 0.7).normalize_or_zero();
        g.blob(at + out * (r + 0.3), DVec3::splat(1.25), EXTRA, false);
    }
}

/// A hedge: box clipped to a wall, as long as it is asked to be.
///
/// The same plant as the boxwood and the same tufts of leaves, held to
/// another outline: a long box with its top a little narrower than its
/// foot (the "batter" a hedge is clipped to, so that light reaches its
/// base) and its edges rounded.
/// https://www.rhs.org.uk/plants/types/hedges/pruning-hedges
///
/// It is a kind of its own and not a setting of the boxwood. A bush is
/// round and has a number of stems; a hedge has a length, and is a row of
/// plants, each with its own few stems showing at the foot. One kind with
/// both would have had a length that did nothing to a round bush.
///
/// The tufts are set on the top and the sides in rows, each row half a
/// step along from the last, and pushed in by a random amount the
/// looseness sets: at none it is freshly clipped, at full it has grown
/// out. The inside is filled, so no light shows through it.
fn hedge(g: &mut Grown, tree: &Tree, height: f64, spread: f64, long: f64, closed: bool) {
    let (dense, loose) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    // (`closed`: it comes round to its own start, and has no ends)
    let long = long.max(2.0);
    let mass = (spread * (0.26 + 0.1 * dense)).clamp(2.6, height * 0.3);
    let foot = height * 0.1;
    // half its width at a height: narrower towards the top
    let half = |z: f64| (spread - mass * 0.6) * (1.0 - 0.14 * (z / height));
    // the inside: a row of large masses along its middle
    let inner = (spread - mass).max(2.0);
    let fill = ((2.0 * long / inner).ceil() as usize).max(1);
    for n in 0..=fill {
        // (stopping short of the two ends, which the tufts close)
        let reach = if closed { long } else { (long - inner - mass * 0.4).max(0.0) };
        let x = -reach + 2.0 * reach * n as f64 / fill as f64;
        g.blob(DVec3::new(x, 0.0, foot + (height - foot) * 0.5), DVec3::new(inner, inner, ((height - foot) * 0.5 - mass * 0.4).max(2.0)), LEAF_DARK, true);
    }
    // the plants it is a row of: each a few stems from the ground
    let plants = tree.count.max(1) as usize;
    let trunk = g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![1.2, 1.2], None, 0);
    for n in 0..plants {
        let x = -long + 2.0 * long * (n as f64 + 0.5) / plants as f64;
        for _ in 0..2 + (g.unit() * 2.0) as usize {
            let from = DVec3::new(x + g.between(-0.2, 0.2) * spread, g.between(-0.35, 0.35) * spread, 0.0);
            let to = from + DVec3::new(g.between(-0.3, 0.3) * spread, g.between(-0.2, 0.2) * spread, foot + mass);
            let thick = g.between(1.0, 1.5);
            let bent = from.lerp(to, 0.5) + g.any_way() * 0.8;
            g.limb(vec![from, bent, to], vec![thick, thick * 0.9, thick * 0.8], Some(trunk), 1, TRUNK);
        }
    }
    // the tufts: rows along the top and down each side
    let step = mass * 1.15;
    let along = ((2.0 * long / step).ceil() as usize).max(1);
    // (`outward` is the way the face it is on looks: it is set back from the face by its own size, and by more the looser the hedge)
    let tuft = |g: &mut Grown, at: DVec3, outward: DVec3| {
        let r = mass * g.between(0.8, 1.15);
        let at = at - outward * (r * 0.75 + loose * g.between(0.0, 0.45) * mass) + g.any_way() * 0.25 * mass * (0.3 + loose);
        // (kept inside the hedge's size, its clusters of leaves and all)
        let (far, wide) = (if closed { long } else { (long - r - 1.7).max(0.0) }, (spread - r - 1.7).max(0.0));
        let at = DVec3::new(at.x.clamp(-far, far), at.y.clamp(-wide, wide), at.z.clamp(foot + r * 0.4, height - r * 0.8));
        g.leaves(at, DVec3::new(r, r, r * 0.8));
        // (as many to an area on a thin hedge, whose tufts are small and many, as on a thick one)
        if tree.extras && g.unit() < 0.35 * (mass / 4.5).powi(2).min(1.0) {
            berries(g, at, r * 0.8, at - outward * mass);
        }
    };
    let across = ((2.0 * half(height) / step).ceil() as usize).max(1);
    let up = (((height - foot) / step).ceil() as usize).max(1);
    for i in 0..=along {
        for j in 0..=across {
            let x = -long + (2.0 * long) * (i as f64 + 0.5 * (j % 2) as f64) / along as f64;
            tuft(g, DVec3::new(x.min(long), -half(height) + 2.0 * half(height) * j as f64 / across as f64, height), DVec3::Z);
        }
        for k in 0..up {
            let z = foot + mass * 0.6 + (height - foot - mass * 0.6) * k as f64 / up as f64;
            let x = (-long + (2.0 * long) * (i as f64 + 0.5 * (k % 2) as f64) / along as f64).min(long);
            for side in [-1.0, 1.0] {
                tuft(g, DVec3::new(x, side * (half(z) + mass * 0.3), z), DVec3::Y * side);
            }
        }
    }
    // and its two ends, if it has them
    for end in if closed { vec![] } else { vec![-1.0, 1.0] } {
        for j in 0..=across {
            for k in 0..up {
                let z = foot + mass * 0.6 + (height - foot - mass * 0.6) * k as f64 / up as f64;
                tuft(g, DVec3::new(end * long, -half(z) + 2.0 * half(z) * j as f64 / across as f64, z), DVec3::X * end);
            }
        }
    }
}

/// A peony, *Paeonia lactiflora*, of the double kind, or a clump of them.
///
/// A double peony's bloom is many petals in rounds: a few broad guard
/// petals outside, and inside them round after round of narrower ones,
/// the innermost made over from stamens. It opens from a hard round bud,
/// through a soft ball, to a full-blown bowl with its guard petals laid
/// back and, in a semi-double, a heart of yellow stamens showing.
/// https://americanpeonysociety.org/?p=35
/// https://en.wikipedia.org/wiki/Paeonia_lactiflora
///
/// The petals are set by the golden angle, the innermost first, which
/// puts no petal behind another in any round (see [`vogel`]); rings of
/// petals, the other way, stack them in spokes. Each petal is two ribs
/// side by side that leave the heart leaning out by an angle of its own
/// and curl back in along their length. Opening is those two numbers:
/// in the bud every petal stands nearly upright and curls over hard, so
/// the bloom is a ball; full blown, the outer petals lean right out and
/// lie nearly straight, and each round inside leans and uncurls less.
/// So one setting takes the same petals from bud to bowl.
///
/// The plant is a clump of stems from one crown, leaning apart, each with
/// a few leaves of three lobed leaflets and one bloom at its top.
fn peony(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, open) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let stems = tree.count.max(1) as usize;
    let turn = g.between(0.0, TAU);
    for plant in 0..stems {
        // the first upright, the rest leaning out from the crown, a golden angle apart
        let way = turn + plant as f64 * GOLDEN_ANGLE;
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        let lean = if plant == 0 { 0.06 } else { g.between(0.22, 0.5) };
        let tall = height * if plant == 0 { 1.0 } else { g.between(0.72, 0.95) };
        let bloom = spread * if plant == 0 { 1.0 } else { g.between(0.8, 0.98) };
        let top = tall - bloom * (0.55 + 0.25 * (1.0 - open));
        let steps = 7;
        let stem = |t: f64| out * (1.5 * (plant.min(1) as f64) + lean * top * (t + 0.6 * t * t) / 1.6) + DVec3::Z * top * t;
        let thick = (tall * 0.028).clamp(1.2, 1.8);
        let limb = g.limb((0..=steps).map(|k| stem(k as f64 / steps as f64)).collect(), vec![thick; steps + 1], None, 0, TRUNK);
        // the leaves: each a stalk with three lobed leaflets
        let leaves = 3 + (tall / 30.0) as usize;
        let around = g.between(0.0, TAU);
        for n in 0..leaves {
            let t = 0.22 + 0.55 * n as f64 / (leaves - 1).max(1) as f64;
            let at = around + n as f64 * GOLDEN_ANGLE;
            let (to, side) = (DVec3::new(at.cos(), at.sin(), 0.0), DVec3::new(-at.sin(), at.cos(), 0.0));
            let long = bloom * (1.1 - 0.4 * t) * (0.7 + 0.5 * full);
            let stalk = stem(t) + (to * 0.85 + DVec3::Z * 0.5) * long * 0.45;
            g.cone(stem(t), stalk, 1.0, 0.9, LIMB);
            for leaflet in [-1.0, 0.0, 1.0] {
                let along = (to + side * leaflet * 0.9 - DVec3::Z * 0.2).normalize();
                blade(g, stalk, along, along.cross(DVec3::Z).normalize_or_zero(), long * if leaflet == 0.0 { 0.8 } else { 0.62 }, 1, 0.42, 0.18);
            }
            g.limbs.push(Limb { path: vec![stem(t), stalk], radius: vec![1.0, 0.9], parent: Some(limb), level: 1 });
        }
        // the bloom
        let axis = (stem(1.0) - stem(0.88)).normalize();
        let (u, v) = (across(axis), axis.cross(across(axis)));
        let heart = stem(1.0);
        g.cone(heart - axis * 1.0, heart + axis * bloom * 0.12, thick, bloom * 0.3, LIMB);
        // the stamens, which only an open bloom shows
        for k in 1..=((bloom * 0.22 / 1.2).powi(2)) as usize {
            let (far, at) = vogel(k, 1.2);
            g.blob(heart + (u * at.cos() + v * at.sin()) * far + axis * bloom * 0.3, DVec3::splat(1.2), STAR, false);
        }
        let petals = (22.0 + 22.0 * full + bloom) as usize;
        for k in 0..petals {
            // 0 for the innermost, 1 for the guard petals
            let t = (k as f64 + 0.5) / petals as f64;
            let at = k as f64 * GOLDEN_ANGLE;
            let (to, round) = (u * at.cos() + v * at.sin(), v * at.cos() - u * at.sin());
            // how far it leans out where it leaves the heart, and how hard it curls back in
            let leans = 0.25 + 0.5 * t + open * (0.2 + 0.95 * t.powf(1.3));
            let curl = (1.0 - open * (0.35 + 0.6 * t)) * (1.5 + 0.9 * t);
            let long = bloom * (0.46 + 0.44 * t) * (0.86 + 0.14 * open);
            let wide = 0.6 + 1.3 * t;
            for hand in [-1.0, 1.0] {
                let mut place = heart + axis * bloom * (0.18 + 0.1 * (1.0 - t)) + to * bloom * (0.05 + 0.2 * t) + round * hand * wide * 0.5;
                for step in 0..3 {
                    let angle: f64 = leans - curl * (step as f64 + 0.5) / 3.0;
                    let next = place + (axis * angle.cos() + to * angle.sin() + round * hand * 0.12 * (1.0 - step as f64)).normalize() * long / 3.0;
                    g.cone(place, next, 1.25 - 0.1 * step as f64, 1.15 - 0.1 * step as f64, if t < 0.5 { EXTRA } else { TOP });
                    place = next;
                }
            }
        }
    }
}

/// The twigs of a bare tree at the end of a branch: forks of thin shoots,
/// `deep` forks of them, each shorter than the last.
///
/// A tree in leaf ends its branches where they are a cell thick, with
/// the leaves hiding the stop. Bare, that stop is a stump. The twigs
/// carry the branching on: they cannot thin as real twigs do, a cell
/// being the least that is built, so they are told from the branches by
/// their number and their shortness and not by their thickness. `fine`
/// is how slender the tree's habit is: a beech's twigs are many, straight
/// and close-angled, an oak's fewer, stouter and crooked.
#[allow(clippy::too_many_arguments)]
fn twigs(g: &mut Grown, from: DVec3, way: DVec3, long: f64, deep: u8, fine: f64, crooked: f64, hold: &dyn Fn(DVec3) -> DVec3) {
    let mut waiting = vec![(from, way, long, deep)];
    while let Some((at, way, long, left)) = waiting.pop() {
        let way = (way + g.any_way() * (0.15 + 0.5 * crooked) + DVec3::Z * 0.12).normalize();
        let end = hold(at + way * long * g.between(0.8, 1.2));
        let thick = 0.95 - 0.1 * fine;
        g.cone(at, end, thick, thick - 0.1, LIMB);
        if left > 0 {
            let hinge = turned(across(way), way, g.between(0.0, TAU));
            let angle = g.between(0.4, 0.6) + 0.35 * (1.0 - fine);
            waiting.push((end, turned(way, hinge, angle), long * 0.78, left - 1));
            waiting.push((end, turned(way, hinge, -angle * g.between(0.5, 1.0)), long * 0.78, left - 1));
            // (a beech carries its leader on between the two as well)
            if fine > 0.5 && left > 1 {
                waiting.push((end, way, long * 0.85, left - 2));
            }
        }
    }
}

/// A hydrangea: a mophead, *Hydrangea macrophylla*.
///
/// A rounded shrub about as wide as it is tall, of broad leaves, carrying
/// at the ends of its shoots large domed heads of flowers. A head is some
/// hundreds of florets, each of four petal-like sepals, and is blue on
/// acid ground and pink on chalk.
/// https://plants.ces.ncsu.edu/plants/hydrangea-macrophylla/
///
/// The shrub is the boxwood's (stems by forks, tufts of leaves over a
/// dome) with larger, fewer tufts, for a leaf a hand long. The heads
/// stand on the upper part of the dome, a golden angle apart. A head is
/// not a ball: its florets are set over a cap by Vogel's model (see
/// [`vogel`]), and each floret is four small balls in a cross about a
/// middle of another colour, lying on the cap. Four cells across is the
/// least a cross can be, so a head has a few dozen florets where the
/// plant's has hundreds; what is kept is that the head is a dome of
/// separate four-parted flowers. The florets of a head are of two colour
/// numbers, mixed, which the sets paint as two tones of one colour or as
/// the blue running to pink of a bush on middling ground.
fn hydrangea(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (dense, loose) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let heads = tree.count.max(1) as usize;
    let mass = (spread * (0.26 + 0.1 * dense)).clamp(2.8, height * 0.3);
    let head = (spread * 0.3).min(height * 0.28).max(3.0);
    let heart = DVec3::Z * height * 0.48;
    let dome = DVec3::new((spread - mass * 0.9).max(2.0), (spread - mass * 0.9).max(2.0), (height * 0.5 - head * 0.7).max(2.0));
    // a few stems from the ground, to show at its foot
    let trunk = g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![1.2, 1.2], None, 0);
    let turn = g.between(0.0, TAU);
    for k in 0..5 {
        let way = turn + TAU * k as f64 / 5.0 + g.between(-0.3, 0.3);
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        let (from, to) = (out * spread * g.between(0.08, 0.2), out * spread * g.between(0.3, 0.5) + DVec3::Z * height * 0.4);
        let bent = from.lerp(to, 0.5) + g.any_way() * 1.2;
        g.limb(vec![from, bent, to], vec![1.4, 1.2, 1.0], Some(trunk), 1, TRUNK);
    }
    // the leaves: tufts over the dome, the lowest left out
    let tufts = ((0.8 + 0.7 * dense) * 4.0 * dome.x * dome.z.max(dome.x) / (mass * mass)).ceil() as usize;
    for n in 0..tufts {
        let up = 1.0 - 2.0 * (n as f64 + 0.5) / tufts as f64;
        if up < -0.7 {
            continue;
        }
        let (level, way) = ((1.0 - up * up).sqrt(), n as f64 * GOLDEN_ANGLE);
        let at = heart + DVec3::new(level * way.cos(), level * way.sin(), up) * dome * (1.0 - loose * g.between(0.0, 0.3));
        let r = mass * g.between(0.75, 1.15);
        g.leaves(DVec3::new(at.x, at.y, at.z.max(r * 0.8)), DVec3::new(r, r, r * 0.7));
    }
    // the heads: over the upper two thirds of the dome
    for n in 0..heads {
        let up = 1.0 - 1.25 * (n as f64 + 0.5) / heads as f64;
        let (level, way) = ((1.0 - up * up).sqrt(), n as f64 * GOLDEN_ANGLE + turn);
        let normal = (DVec3::new(level * way.cos(), level * way.sin(), up) + DVec3::Z * 0.35).normalize();
        let size = head * g.between(0.8, 1.1);
        let middle = heart + DVec3::new(level * way.cos(), level * way.sin(), up) * dome + normal * size * 0.25;
        let (u, v) = (across(normal), normal.cross(across(normal)));
        // a green cushion under the florets, so that no light shows through the head
        g.blob(middle - normal * size * 0.15, DVec3::splat(size * 0.62), LEAF_DARK, false);
        let apart = 3.3;
        for k in 1..=((size / apart).powi(2) * 2.4) as usize {
            let (far, at) = vogel(k, apart * 0.62);
            if far > size {
                break;
            }
            // on a cap: the further out, the further round the dome of the head
            let round = far / size * 1.25;
            let out = u * at.cos() + v * at.sin();
            let face = (normal * round.cos() + out * round.sin()).normalize();
            let floret = middle + face * size * 0.78;
            let (a, b) = (across(face), face.cross(across(face)));
            let twist = g.between(0.0, TAU);
            let shade = if g.unit() < 0.5 { TOP } else { EXTRA };
            for petal in 0..4 {
                let to = twist + TAU * petal as f64 / 4.0;
                g.blob(floret + (a * to.cos() + b * to.sin()) * 1.15, DVec3::splat(1.0), shade, false);
            }
            g.blob(floret + face * 0.55, DVec3::splat(0.75), STAR, false);
        }
    }
}

/// A petunia: a garden petunia of the spreading kind, *Petunia x hybrida*.
///
/// A low mound of small leaves, a hand high and several across, whose
/// stems lie along the ground with their tips turned up, covered in
/// funnel-shaped flowers of five joined lobes, often with a darker throat.
/// https://plants.ces.ncsu.edu/plants/petunia-x-hybrida/
///
/// The mound is tufts of leaves over a low dome, and the trailing stems
/// run out from under it along the ground, as far as the trailing setting
/// says, each ending in a tuft and a flower. The flowers stand all over
/// the mound a golden angle apart, facing outwards and up. A flower is a
/// short green tube, five lobes round its mouth (each a short round rib
/// leaning back, so that the face is a shallow funnel and not a flat
/// star) and a throat of another colour number sunk in the middle.
fn petunia(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (dense, trail) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let flowers = tree.count.max(1) as usize;
    let mass = (spread * 0.16).clamp(2.6, height * 0.4);
    let bloom = (spread * 0.14).clamp(3.0, 5.5);
    // the mound keeps to the middle; the more it trails, the smaller the mound and the longer the stems
    let mound = spread * (1.0 - 0.45 * trail);
    let heart = DVec3::Z * height * 0.15;
    let dome = DVec3::new((mound - mass).max(2.0), (mound - mass).max(2.0), (height * 0.85 - mass * 0.7 - bloom * 0.5).max(2.0));
    let trunk = g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![1.0, 1.0], None, 0);
    let tufts = ((1.0 + 0.8 * dense) * 2.0 * dome.x * dome.x / (mass * mass)).ceil() as usize;
    for n in 0..tufts {
        let up = 1.0 - (n as f64 + 0.5) / tufts as f64;
        let (level, way) = ((1.0 - up * up).sqrt(), n as f64 * GOLDEN_ANGLE);
        let at = heart + DVec3::new(level * way.cos(), level * way.sin(), up) * dome;
        let r = mass * g.between(0.75, 1.15);
        g.leaves(DVec3::new(at.x, at.y, at.z.max(r * 0.55)), DVec3::new(r, r, r * 0.6));
    }
    // a flower at `at`, facing `face`
    let flower = |g: &mut Grown, at: DVec3, face: DVec3| {
        let (a, b) = (across(face), face.cross(across(face)));
        let size = bloom * g.between(0.85, 1.1);
        g.cone(at - face * size * 0.5, at, 1.0, 1.5, LIMB);
        let twist = g.between(0.0, TAU);
        for lobe in 0..5 {
            let to = twist + TAU * lobe as f64 / 5.0;
            let out = a * to.cos() + b * to.sin();
            g.cone(at + out * 1.0, at + out * size + face * size * 0.3, 1.25, 1.1, TOP);
        }
        g.blob(at + face * 0.3, DVec3::splat(1.3), EXTRA, false);
    };
    // the trailing stems, and how many of the flowers they carry
    let stems = (3.0 + 9.0 * trail) as usize;
    let turn = g.between(0.0, TAU);
    let mut left = flowers;
    for k in 0..stems.min(flowers) {
        let way = turn + k as f64 * GOLDEN_ANGLE;
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        let far = mound * 0.6 + (spread - mound * 0.6 - bloom) * g.between(0.6, 1.0);
        let (from, low, tip) = (out * mound * 0.4 + DVec3::Z * 1.2, out * far * 0.7 + DVec3::Z * 1.0, out * far + DVec3::Z * (1.5 + mass * 0.6));
        g.limb(vec![from, low, tip], vec![1.0, 0.95, 0.9], Some(trunk), 1, TRUNK);
        g.leaves(low + DVec3::Z * mass * 0.3, DVec3::new(mass * 0.8, mass * 0.8, mass * 0.5));
        g.leaves(tip, DVec3::new(mass * 0.75, mass * 0.75, mass * 0.5));
        flower(g, tip + (out * 0.5 + DVec3::Z).normalize() * mass * 0.6, (out * 0.7 + DVec3::Z).normalize());
        left -= 1;
    }
    for n in 0..left {
        let up = 1.0 - 0.92 * (n as f64 + 0.5) / left as f64;
        let (level, way) = ((1.0 - up * up).sqrt(), n as f64 * GOLDEN_ANGLE + turn);
        let normal = DVec3::new(level * way.cos(), level * way.sin(), up);
        let at = heart + normal * (dome + mass * 0.75);
        flower(g, DVec3::new(at.x, at.y, at.z.max(bloom * 0.6)), (normal + DVec3::Z * 0.5).normalize());
    }
}

/// Reeds: a stand of bulrushes (cattails), *Typha latifolia*, or with
/// feathery plumes, of common reed, *Phragmites australis*.
///
/// Tall unbranched stems, close together, with long narrow leaves that
/// stand nearly upright and flop at their tips. A bulrush carries the
/// brown sausage it is known by, a thumb thick and a hand long, with a
/// thinner spike above it; a common reed carries a loose plume that
/// hangs to one side.
/// https://en.wikipedia.org/wiki/Typha_latifolia
/// https://en.wikipedia.org/wiki/Phragmites_australis
///
/// The bulrush is what a stand starts as, because its head is the one
/// that can be told at this size: a plume is a soft thing, and in bricks
/// a soft thing is a rough lump. The plume is there to be chosen.
///
/// The stems stand in an oval, as long as the stand is asked to be and
/// as wide as its spread, scattered evenly by throwing each a golden
/// angle and a square root further out than the last (see [`vogel`]),
/// stretched to the oval. All lean the same way by an amount of their
/// own, as reeds do in a wind.
fn reeds(g: &mut Grown, tree: &Tree, height: f64, spread: f64, long: f64, within: Option<&Run>) {
    let long = long.max(2.0);
    let (full, sway) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let stems = tree.count.max(1) as usize;
    let _ = spread;
    let wind = g.between(0.0, TAU);
    let downwind = DVec3::new(wind.cos(), wind.sin(), 0.0);
    for n in 0..stems {
        let (far, at) = vogel(n + 1, 1.0 / (stems as f64).sqrt());
        let mut foot = DVec3::new(far * at.cos() * (long - 2.0).max(1.0), far * at.sin() * (spread - 2.0).max(1.0), 0.0);
        // Inside a closed line they fill it: each stem thrown into the
        // line's box until it lands inside (a few dozen tries at most,
        // and a stem that never does is left out).
        if let Some(run) = within {
            let (lo, hi) = run.points.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            let landed = (0..40).map(|_| lo + DVec3::new(g.unit(), g.unit(), 0.0) * (hi - lo)).find(|p| run.holds(*p));
            match landed {
                Some(p) => foot = p,
                None => continue,
            }
        }
        let tall = height * g.between(0.7, 1.0);
        let lean = sway * g.between(0.05, 0.3);
        let own = (downwind + g.any_way() * 0.5).normalize_or_zero();
        let stem = |t: f64| foot + DVec3::new(own.x, own.y, 0.0) * lean * tall * t * t + DVec3::Z * tall * t;
        let steps = 6;
        let limb = g.limb((0..=steps).map(|k| stem(0.86 * k as f64 / steps as f64)).collect(), vec![1.0; steps + 1], None, 0, TRUNK);
        // the head
        let (low, high) = (stem(0.72), stem(0.86));
        if tree.extras {
            // a plume: soft shoots hanging to the lee
            for k in 0..6 {
                let from = low.lerp(stem(0.98), k as f64 / 5.0);
                let hang = (downwind * 0.9 + g.any_way() * 0.5 + DVec3::Z * (0.5 - 0.15 * k as f64)).normalize();
                g.cone(from, from + hang * tall * 0.07, 1.2, 0.85, TOP);
            }
            g.cone(low, stem(0.98), 1.0, 0.8, TOP);
        } else {
            // the sausage, and the thin spike that stands out of its top
            g.cone(low, high, 1.9, 1.9, EXTRA);
            g.cone(high, stem(1.0), 0.9, 0.8, STAR);
        }
        // the leaves: from low on the stem, nearly upright, flopping at the tip
        let leaves = 1 + (full * 2.6) as usize;
        for k in 0..leaves {
            let way = at + k as f64 * GOLDEN_ANGLE;
            let out = DVec3::new(way.cos(), way.sin(), 0.0);
            let reach = tall * g.between(0.45, 0.8);
            let mut place = stem(0.03 + 0.1 * k as f64);
            let mut path = vec![place];
            for step in 0..5 {
                let polar: f64 = 0.12 + (0.5 + 0.9 * sway) * ((step as f64 + 0.5) / 5.0).powi(3) * 1.6;
                let next = place + (DVec3::Z * polar.cos() + out * polar.sin()) * reach / 5.0;
                g.cone(place, next, 1.0 - 0.03 * step as f64, 0.97 - 0.03 * step as f64, if k % 2 == 0 { LEAF } else { LEAF_DARK });
                place = next;
                path.push(place);
            }
            let radius = vec![1.0; path.len()];
            g.limbs.push(Limb { path, radius, parent: Some(limb), level: 1 });
        }
    }
}

/// Where a hedge, a stand of reeds or a patch of grass lies: the line it
/// runs along, and for grass the places its ground is raised.
///
/// Everything is given from the plant's own foot, as its parts are.
#[derive(Clone, Debug, Default)]
pub struct Lay {
    /// the line it runs along, as points on the ground; two at least
    pub run: Vec<DVec3>,
    /// whether the line comes back to where it began
    pub closed: bool,
    /// whether the line is straight between its points, with corners at them
    pub sharp: bool,
    /// grass: how far below the level it lies on water stands in any hollow dug deeper than that; `None` for no water
    pub water: Option<f64>,
    /// grass: how thickly reeds stand round the water, 0 to 1
    pub reeds: f64,
    /// grass: how many levels its water stands at, 1 or 2 (see [`Waters`]); anything under 2 is one
    pub levels: u8,
    /// grass: what has been done to its ground, in order (see [`Sculpt`])
    pub mounds: Vec<Sculpt>,
}

/// One touch of a brush on the ground under grass: where, how wide, and
/// what it does. The ground is the floor with every touch applied to it
/// in turn (see [`ground`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sculpt {
    /// where on the ground, from the plant's foot
    pub at: DVec3,
    /// how far out from there it reaches
    pub range: f64,
    /// Pushing: how far the ground is raised at its middle, or lowered if
    /// negative. Flattening: the height the ground is brought to.
    pub amount: f64,
    /// bring the ground to a level, where otherwise it is pushed up or down
    pub flatten: bool,
}

/// The least a brush reaches: under this a push is a spike.
pub const LEAST_RANGE: f64 = 4.0;

/// How high the ground under grass is at `p`, over the floor it lies on.
///
/// The touches are applied in the order they were made, as strokes of a
/// brush are, so a later one works on the ground the earlier ones left.
///
/// A push adds a smooth hill (or takes a hollow out of ground that has
/// been raised): `amount * (1 + cos(pi * d / range)) / 2` at the distance
/// `d` from its middle, nothing beyond its range. That is level at the
/// middle and level again at the edge, so hills run into one another and
/// into the flat with no crease.
/// https://en.wikipedia.org/wiki/Raised_cosine_distribution
///
/// Flattening moves the ground towards one height: all the way within
/// half its range, and by less from there out to the edge, on the same
/// curve. A terrace cut into a hillside then has a level top and a bank
/// that runs smoothly back into the hill.
///
/// A push down may take the ground below the floor the patch lies on:
/// that is how a pond is dug (see [`grass`]). The patch then reaches as
/// far below its own level as it was dug, and wants ground that deep
/// under it, or to be raised by as much.
pub fn ground(touches: &[Sculpt], p: DVec3) -> f64 {
    let mut height: f64 = 0.0;
    for touch in touches {
        let range = touch.range.max(LEAST_RANGE);
        let d = touch.at.truncate().distance(p.truncate());
        if d >= range {
            continue;
        }
        if touch.flatten {
            let share = if d < range * 0.5 { 1.0 } else { 0.5 * (1.0 + (PI * (d - range * 0.5) / (range * 0.5)).cos()) };
            height += (touch.amount - height) * share;
        } else {
            height += touch.amount * 0.5 * (1.0 + (PI * d / range).cos());
        }
    }
    height
}

impl Lay {
    /// Straight along x, `long` to each side of the foot.
    pub fn straight(long: f64) -> Lay {
        Lay { run: vec![-DVec3::X * long, DVec3::X * long], closed: false, sharp: false, water: None, reeds: 0.0, levels: 1, mounds: Vec::new() }
    }
}

/// A line along the ground with a way of finding the place so far along it.
struct Run {
    points: Vec<DVec3>,
    /// how far along the line each point is
    along: Vec<f64>,
}

impl Run {
    fn new(lay: &Lay) -> Run {
        let mut points: Vec<DVec3> = lay.run.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect();
        if lay.closed && points.len() > 2 {
            points.push(points[0]);
        }
        // (a point on top of the last has no direction, and is left out)
        points.dedup_by(|b, a| a.distance(*b) < 1e-6);
        if points.len() < 2 {
            points = vec![DVec3::ZERO, DVec3::X];
        }
        let mut along = vec![0.0];
        for i in 1..points.len() {
            along.push(along[i - 1] + points[i].distance(points[i - 1]));
        }
        Run { points, along }
    }

    fn long(&self) -> f64 {
        *self.along.last().unwrap()
    }

    /// The place `s` along the line, and the way the line is heading
    /// there. Before its start and past its end it carries straight on.
    fn place(&self, s: f64) -> (DVec3, DVec3) {
        let i = self.along.partition_point(|a| *a <= s).clamp(1, self.points.len() - 1);
        let heading = (self.points[i] - self.points[i - 1]).normalize();
        (self.points[i - 1] + heading * (s - self.along[i - 1]), heading)
    }

    /// Where a point of a plant grown straight along x, `half` to each
    /// side of its middle, is once the plant is laid along this line: as
    /// far along the line as it was along x, and as far to the side of it.
    fn laid(&self, p: DVec3, half: f64) -> DVec3 {
        let (at, heading) = self.place(p.x + half);
        at + DVec3::new(-heading.y, heading.x, 0.0) * p.y + DVec3::Z * p.z
    }

    /// How far a point of the ground is from the line.
    fn distance(&self, p: DVec3) -> f64 {
        self.points.windows(2).map(|w| {
            let ab = w[1] - w[0];
            let t = ((p - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            (w[0] + ab * t).truncate().distance(p.truncate())
        }).fold(f64::INFINITY, f64::min)
    }

    /// Whether a point of the ground is inside the line, taken as an
    /// outline (the even-odd rule, as `sdf::Fill` has it).
    fn holds(&self, p: DVec3) -> bool {
        let mut inside = false;
        let mut a = *self.points.last().unwrap();
        for &b in &self.points {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
            a = b;
        }
        inside
    }
}

impl Grown {
    /// Lay everything grown since `parts` and `limbs` along `run` (see
    /// [`Run::laid`]).
    ///
    /// A hedge is grown straight and then laid along its line, where it
    /// could have been grown along the line from the start. Grown straight,
    /// its rows of tufts, its stems and its filling are each a few plain
    /// loops; and a hedge laid along a straight line is, part for part, the
    /// hedge there was before it could bend. A ball stays a ball and a
    /// branch a branch: each is moved, and none is sheared, so on a bend
    /// the tufts on the outside stand a little apart and those on the
    /// inside crowd, as leaves do.
    fn lay_along(&mut self, parts: usize, limbs: usize, run: &Run, half: f64) {
        for part in &mut self.parts[parts..] {
            part.shape = match part.shape {
                Shape::Cone { a, ba, ra, rb, .. } => Shape::cone(run.laid(a, half), run.laid(a + ba, half), ra, rb),
                Shape::Blob { c, inv, least, leafy } => Shape::Blob { c: run.laid(c, half), inv, least, leafy },
                other => other,
            };
        }
        for limb in &mut self.limbs[limbs..] {
            limb.path.iter_mut().for_each(|p| *p = run.laid(*p, half));
        }
    }
}

/// Grass: a patch or a strip of it, over ground that can be raised.
///
/// It covers the ground within its spread of the line it is laid along,
/// and, where the line is closed, everything the line encloses: a strip
/// along a path, or a lawn of any outline.
///
/// It is a surface and not a solid. Under the blades is a skin of soil
/// about two cells thick, in a colour of its own, so that a patch set on a
/// floor leaves no gap under it and a mound is closed on top; under a
/// mound there is nothing. Blades with nothing under them were tried in
/// the mind only: over a floor of another colour the floor shows between
/// every tuft, and a raised patch would be blades standing in the air.
///
/// The ground is shaped as terrain is in a game that has a tool for it:
/// by touches of a brush, each with a range and an amount, that push the
/// ground up, push it down or flatten it (see [`ground`]). Dug below the
/// water's level, a hollow is a pond: going down a bank the grass thins
/// and stops at the water's edge; the soil is sand from a little above
/// the water to a little below it, and the pond's bed further down,
/// with a stone on it here and there; water fills the hollow to the
/// level, flat on top; and reeds stand in the shallows and on the bank.
/// The stones and reeds are placed by numbers of their own for each spot
/// of ground, and not from the run the blades are drawn from, so that
/// digging a pond leaves the lawn round it as it was. The soil and
/// the blades follow the ground, and the blades stay upright on it.
///
/// The blades are in tufts, on a grid shaken so that no rows show, two to
/// four to a tuft, each of its own height and lean and of one of three
/// greens; all lean a little one way, as grass does. A blade is two
/// lengths of round rib a cell or so thick, the thinnest that is built.
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

/// How far under a stream's water its bed is cut. The soil is balls that
/// stand a cell and a half over the ground they are laid on (see
/// [`grass`]), so a bed a cell and a half down, which was tried, was soil
/// to the water's very surface with water showing in the gaps.
const BED_UNDER: f64 = 2.8;

/// What water a place is beside: its level, and when that water is a
/// stream's, how far the place is from the middle of it.
type Wet = (f64, Option<f64>);

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

fn grass(g: &mut Grown, tree: &Tree, height: f64, spread: f64, lay: &Lay) {
    let (thick, lean) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let run = Run::new(lay);
    let covered = |p: DVec3| run.distance(p) <= spread || (lay.closed && run.holds(p));
    let sculpted = |p: DVec3| ground(&lay.mounds, p);
    let seed = tree.seed as u32 ^ 0x9A55;
    let (lo, hi) = run.points.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p - spread), hi.max(*p + spread)));
    // (the grid the water is found on is the grid the soil is laid on)
    let step = 2.4;
    let waters = lay.water.map(|lower| Waters::found(lay, lower, &covered, &sculpted, lo, hi, step, seed));
    let lifted = |p: DVec3| waters.as_ref().map_or(sculpted(p), |w| w.ground(p, sculpted(p)));
    let water = |p: DVec3| -> Option<Wet> { waters.as_ref().map(|w| w.level(p, sculpted(p))) };
    g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![1.0, 1.0], None, 0);
    // the soil: a skin of flat balls, close enough to run together
    let across = |low: f64, high: f64, step: f64| (0..=((high - low) / step).ceil() as usize).map(move |i| low + i as f64 * step);
    for y in across(lo.y, hi.y, step) {
        for x in across(lo.x, hi.x, step) {
            let p = DVec3::new(x, y, 0.0);
            if covered(p) {
                // (on a slope, soil down to the ground beside it as well, so that the skin has no tear in it)
                let here = lifted(p);
                let lowest = [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y].iter().map(|way| lifted(p + *way * step)).fold(here, f64::min);
                // by the water it is sand, and under it the bed of the pond
                // (a stream's bed is sand and stones all through: it runs too fast for mud to settle)
                let wet = water(p);
                let skin = match wet {
                    Some((level, None)) if here <= level - 1.5 => BED,
                    Some((level, _)) if here <= level + 1.2 => SAND,
                    _ => TRUNK,
                };
                let mut z = here;
                while z > lowest - 1.0 {
                    g.blob(p + DVec3::Z * (z + 0.45), DVec3::new(2.3, 2.3, 1.15), skin, false);
                    z -= 1.6;
                }
                // (a stream's water only in its channel, and not wherever the ground beside it happens to lie lower)
                if let Some((level, _)) = wet.filter(|(level, by)| here + 1.0 < *level && by.is_none_or(|far| far < 3.0)) {
                    // The water: a column from the bed to the level, flat
                    // on top, wide enough to run into its neighbours.
                    // Eight-sided and upright, as a trunk is, so that it
                    // is boxes the fitter can join into few.
                    g.parts.push(Part { shape: Shape::Column { a: p + DVec3::Z * (here + 0.8), b: p + DVec3::Z * level, ra: 1.75, rb: 1.75, slack: 1.0, cut: 0.0 }, shade: WATER });
                    // and here and there a stone on the bed
                    let chance = hash(x.round() as i32, y.round() as i32, 3, seed);
                    if here <= level - 1.5 && chance < 0.06 {
                        let r = 1.4 + 18.0 * chance;
                        g.blob(p + DVec3::Z * (here + 1.0 + r * 0.4), DVec3::new(r, r * 0.85, r * 0.7), STONE, false);
                    }
                }
            }
        }
    }
    // the blades
    let wind = g.between(0.0, TAU);
    let downwind = DVec3::new(wind.cos(), wind.sin(), 0.0);
    let gap = 3.4 - 1.9 * thick;
    for y in across(lo.y, hi.y, gap) {
        for x in across(lo.x, hi.x, gap) {
            let p = DVec3::new(x + g.between(-0.45, 0.45) * gap, y + g.between(-0.45, 0.45) * gap, 0.0);
            let blades = 2 + (g.unit() * 2.4) as usize;
            // (the numbers are drawn whether or not the tuft is kept, so that widening a patch leaves the tufts it had as they were)
            let drawn: Vec<(DVec3, DVec3, f64, f64)> = (0..blades).map(|_| (g.any_way(), g.any_way(), g.between(0.5, 1.0), g.unit())).collect();
            if !covered(p) {
                continue;
            }
            let here = lifted(p);
            if let Some((level, by)) = water(p) {
                // Reeds in the shallows and on the bank, by a throw of
                // their own for each place, so that the same seed has
                // them in the same places whatever else changes.
                let (throw, thin) = (hash(x.round() as i32, y.round() as i32, 5, seed), hash(x.round() as i32, y.round() as i32, 6, seed));
                // (a third as many along a stream as round a pond)
                if (here - level).abs() < 2.6 && throw < lay.reeds * if by.is_some() { 0.05 } else { 0.14 } {
                    let tall = 14.0 + 12.0 * thin + height;
                    let (foot, top) = (p + DVec3::Z * (here + 0.8), p + DVec3::new(drawn[0].0.x, drawn[0].0.y, 0.0) * 1.5 + DVec3::Z * (here + tall));
                    g.cone(foot, top, 1.0, 0.9, LEAF_DARK);
                    g.cone(foot.lerp(top, 0.66), foot.lerp(top, 0.86), 1.45, 1.45, BED);
                    let out = DVec3::new(drawn[0].1.x, drawn[0].1.y, 0.0).normalize_or_zero();
                    g.cone(foot, foot + DVec3::Z * tall * 0.4 + out * 1.5, 1.0, 0.95, LEAF);
                    g.cone(foot + DVec3::Z * tall * 0.4 + out * 1.5, foot + DVec3::Z * tall * 0.6 + out * 6.0, 0.95, 0.8, LEAF);
                }
                // the grass thins out towards the water and stops at its edge
                if here <= level + 0.3 || thin > (here - level - 0.3) / 1.7 {
                    continue;
                }
            }
            let foot = p + DVec3::Z * (here + 1.0);
            for (first, second, long, tone) in drawn {
                let up = (DVec3::Z + downwind * 0.45 * lean + first * 0.3).normalize();
                let over = (up + downwind * 0.5 * lean + second * 0.25).normalize();
                let (half, tip) = (foot + up * height * long * 0.55, foot + up * height * long * 0.55 + over * height * long * 0.45);
                let shade = if tone < 0.5 { LEAF } else if tone < 0.78 { LEAF_DARK } else { LEAF_LIGHT };
                g.cone(foot, half, 0.95, 0.85, shade);
                g.cone(half, tip, 0.85, 0.62, shade);
            }
        }
    }
}

impl Grown {
    /// Move everything grown since `parts` and `limbs` by `by`: a plant
    /// grown at the foot of a bed, set down in its place in it.
    fn shift(&mut self, parts: usize, limbs: usize, by: DVec3) {
        for part in &mut self.parts[parts..] {
            match &mut part.shape {
                Shape::Cone { a, .. } => *a += by,
                Shape::Blob { c, .. } => *c += by,
                Shape::Column { a, b, .. } => {
                    *a += by;
                    *b += by;
                }
            }
        }
        for limb in &mut self.limbs[limbs..] {
            limb.path.iter_mut().for_each(|p| *p += by);
        }
    }

    /// A trunk standing straight up from `foot` to the height `top`, as
    /// thick at each height as `thick` says: eight-sided, in lengths each
    /// of one size all the way up it.
    ///
    /// A round trunk fitted cell by cell is ramps and steps that change
    /// from one layer to the next, because a circle crosses every cell
    /// differently. An octagon whose four straight faces lie on the
    /// grid's own planes and whose four slanting ones run through cell
    /// corners is the same in every layer of a length, so each face is
    /// one tall brick and each corner one tall ramp, with a step only
    /// where the trunk changes size. Its size is therefore rounded to
    /// what puts its faces there: so many cells and a half from its
    /// middle, which stands in the middle of a cell. Below a cell and a
    /// half it is too thin to be eight-sided and is round as before.
    fn trunk(&mut self, foot: DVec3, top: f64, thick: &dyn Fn(f64) -> f64, shade: u8) -> usize {
        let size = |z: f64| (thick(z) - 0.5).round() + 0.5;
        // the lengths of one size, from the ground up
        let mut lengths: Vec<(f64, f64, f64)> = Vec::new();
        let mut from = 0.0;
        while from < top {
            let now = size(from);
            let mut to = (from + 1.0).min(top);
            while to < top && size(to) == now {
                to = (to + 1.0).min(top);
            }
            lengths.push((from, to, now));
            from = to;
        }
        let (mut path, mut radius) = (vec![foot], vec![thick(0.0)]);
        for (i, &(from, to, now)) in lengths.iter().enumerate() {
            // Where the next length is narrower, the top of this one
            // slopes in to it, two cells up for each cell in: a face of
            // long ramps. Left square, each change of size was a ledge
            // round the trunk, and a trunk that widens to its foot was a
            // post standing on a plinth.
            let next = lengths.get(i + 1).map_or(now, |l| l.2);
            let slope = if next < now && next >= 1.5 { (2.0 * (now - next)).min(to - from) } else { 0.0 };
            let (a, m, b) = (foot + DVec3::Z * from, foot + DVec3::Z * (to - slope), foot + DVec3::Z * to);
            if now >= 1.5 {
                if to - slope > from {
                    self.parts.push(Part { shape: Shape::Column { a, b: m, ra: now, rb: now, slack: 1.0, cut: (now * std::f64::consts::SQRT_2).round() }, shade });
                }
                if slope > 0.0 {
                    self.parts.push(Part { shape: Shape::Column { a: m, b, ra: now, rb: next, slack: 1.0 / (1.0 + ((now - next) / slope).powi(2)).sqrt(), cut: 0.0 }, shade });
                }
            } else {
                self.cone(a, b, thick(from).max(0.7), thick(to).max(0.7), shade);
            }
            path.push(b);
            radius.push(thick(to));
        }
        self.limb_record(path, radius, None, 0)
    }
}

/// A cypress: the upright Italian cypress, *Cupressus sempervirens*, or
/// a column juniper.
///
/// A narrow spire many times as tall as it is wide, closed to the ground,
/// of sprays that sweep upwards against the stem.
/// https://en.wikipedia.org/wiki/Cupressus_sempervirens
///
/// Tufts are set up a spindle by the golden angle, each a ball of leaves
/// stretched upwards, so the outline is ribbed with upward strokes where
/// a boxwood's is lumpy all ways. The spindle is widest a third of the
/// way up and comes to a point.
fn cypress(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (dense, loose) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let mass = (spread * 0.42).clamp(2.6, 7.0);
    let stem = (height * 0.02).clamp(1.0, 2.5);
    g.limb(vec![DVec3::ZERO, DVec3::Z * height * 0.5], vec![stem, stem * 0.7], None, 0, TRUNK);
    // half its width at the share `t` of its height
    let wide = |t: f64| spread * (t / 0.1).min(1.0).powf(0.6) * (1.0 - ((t - 0.3).max(0.0) / 0.7).powf(1.8)).max(0.0).powf(0.75);
    let foot = height * 0.05;
    let tufts = ((0.8 + 0.8 * dense) * 2.6 * spread * height / (mass * mass)).ceil() as usize;
    for n in 0..tufts {
        let t = (n as f64 + 0.5) / tufts as f64;
        let way = n as f64 * GOLDEN_ANGLE;
        let r = mass * g.between(0.75, 1.1) * (0.55 + 0.45 * (wide(t) / spread));
        let out = (wide(t) - r * 0.8).max(0.0) * (1.0 - loose * g.between(0.0, 0.35));
        let at = DVec3::new(way.cos() * out, way.sin() * out, foot + (height - foot - r * 1.5) * t + r * 0.8);
        g.leaves(at, DVec3::new(r, r, r * 1.7));
    }
    // filled up the middle, so that no light shows through it
    let fill = (height / (spread * 0.8)).ceil() as usize;
    for n in 0..fill {
        let t = (n as f64 + 0.5) / fill as f64;
        let r = (wide(t) - mass * 0.5).max(1.0);
        g.blob(DVec3::Z * (foot + (height - foot) * t), DVec3::new(r, r, (height / fill as f64).max(r)), LEAF_DARK, true);
    }
    g.cone(DVec3::Z * (height - mass * 2.0), DVec3::Z * (height - 1.0), mass * 0.7, 1.0, LEAF_LIGHT);
}

/// A yucca: a rosette of sword leaves, *Yucca filamentosa* or an agave.
///
/// Stiff pointed blades from one crown, each a golden angle round from
/// the last. The youngest stand upright in the middle and each older one
/// leans further out, to level and below it at the outside. In flower it
/// sends up one tall stalk of hanging cream bells.
/// https://en.wikipedia.org/wiki/Yucca_filamentosa
///
/// A blade is two ribs side by side at its root that meet at its tip, as
/// a sunflower's petal is, and straight but for the bend asked for.
fn yucca(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, bend) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let blades = tree.count.max(3) as usize;
    let trunk = g.limb(vec![DVec3::ZERO, DVec3::Z * height * 0.12], vec![2.2, 1.8], None, 0, TRUNK);
    let heart = DVec3::Z * height * 0.1;
    // (in flower, the blades are the lower half of it and the stalk the rest)
    let tall = if tree.extras { height * 0.5 } else { height };
    for n in 0..blades {
        let age = (n as f64 + 0.5) / blades as f64;
        let way = n as f64 * GOLDEN_ANGLE;
        let (out, side) = (DVec3::new(way.cos(), way.sin(), 0.0), DVec3::new(-way.sin(), way.cos(), 0.0));
        let polar: f64 = 0.1 + 1.6 * age.powf(0.8) + g.between(-0.08, 0.08);
        // as long as reaches the height standing up and the spread lying out
        let long = (tall - heart.z) * (1.0 - age) + spread * age;
        let wide = (0.8 + 1.2 * full) * (0.6 + 0.4 * age);
        let steps = 4;
        let mut at = heart + out * 0.8;
        let mut path = vec![at];
        for k in 0..steps {
            let lean = (polar + bend * 0.9 * ((k as f64 + 0.5) / steps as f64).powi(2)).min(2.4);
            let next = at + (DVec3::Z * lean.cos() + out * lean.sin()) * long / steps as f64;
            let narrow = |k: usize| wide * (1.0 - k as f64 / steps as f64);
            for hand in [-1.0, 1.0] {
                g.cone(at + side * hand * narrow(k), next + side * hand * narrow(k + 1), 1.1, if k + 1 == steps { 0.7 } else { 1.05 }, if n % 3 == 0 { LEAF_LIGHT } else if n % 3 == 1 { LEAF } else { LEAF_DARK });
            }
            at = next;
            path.push(at);
        }
        let radius = vec![1.0; path.len()];
        g.limbs.push(Limb { path, radius, parent: Some(trunk), level: 1 });
    }
    if tree.extras {
        let top = DVec3::Z * height;
        g.cone(heart, top, 1.3, 0.9, LIMB);
        let bells = (height * 0.5 / 2.2) as usize;
        for n in 0..bells {
            let t = n as f64 / bells as f64;
            let way = n as f64 * GOLDEN_ANGLE;
            let at = heart.lerp(top, 0.5 + 0.5 * t) + DVec3::new(way.cos(), way.sin(), 0.0) * (1.6 + 3.2 * (1.0 - t));
            g.blob(at - DVec3::Z * 0.6, DVec3::new(1.5, 1.5, 1.9), TOP, false);
        }
    }
}

/// One daisy, *Bellis* or an ox-eye: a thin stem and a flat head facing
/// the sky, a button of yellow in a single ring of white rays.
/// https://en.wikipedia.org/wiki/Leucanthemum_vulgare
///
/// A sunflower's head, by the same model of seeds, made small and
/// simple: one row of rays, laid nearly flat, and a disc a third of the
/// head's width. Its leaves are low on the stem.
fn daisy_at(g: &mut Grown, foot: DVec3, tall: f64, head: f64, full: f64) {
    let facing = g.between(0.0, TAU);
    let tilt = g.between(0.1, 0.5);
    let face = DVec3::new(facing.cos(), facing.sin(), 0.0);
    let normal = (DVec3::Z * tilt.cos() + face * tilt.sin()).normalize();
    let lean = g.between(0.03, 0.12) * tall;
    let stem = |t: f64| foot + face * lean * t * t + DVec3::Z * (tall - 1.5) * t;
    let limb = g.limb((0..=5).map(|k| stem(k as f64 / 5.0)).collect(), vec![1.0; 6], None, 0, TRUNK);
    for n in 0..3 {
        let way = facing + 1.0 + n as f64 * GOLDEN_ANGLE;
        let (out, across) = (DVec3::new(way.cos(), way.sin(), 0.0), DVec3::new(-way.sin(), way.cos(), 0.0));
        let from = stem(0.08 + 0.12 * n as f64);
        blade(g, from, (out + DVec3::Z * 0.5).normalize(), across, head * (1.0 + 0.6 * full), 1, 0.35, 0.3);
        g.limbs.push(Limb { path: vec![from, from + out], radius: vec![1.0, 1.0], parent: Some(limb), level: 1 });
    }
    let middle = stem(1.0);
    let (u, v) = (across(normal), normal.cross(across(normal)));
    let disc = (head * 0.36).max(1.8);
    g.cone(middle - normal * 1.2, middle, 1.0, disc * 0.8, LIMB);
    for k in 1..=((disc / 1.3).powi(2)).max(3.0) as usize {
        let (far, at) = vogel(k, 1.3);
        g.blob(middle + (u * at.cos() + v * at.sin()) * far.min(disc - 0.6) + normal * (0.9 - 0.4 * far / disc), DVec3::splat(1.25), EXTRA, false);
    }
    let rays = if head < 6.0 { 13 } else { 21 };
    for n in 0..rays {
        let at = TAU * n as f64 / rays as f64;
        let (out, round) = (u * at.cos() + v * at.sin(), v * at.cos() - u * at.sin());
        let tip = middle + out * head * g.between(0.92, 1.0) - normal * g.between(0.0, 0.12) * head;
        for hand in [-0.5, 0.5] {
            g.cone(middle + out * disc * 0.8 + round * hand, tip, 1.05, 0.8, TOP);
        }
    }
}

/// One daffodil, *Narcissus pseudonarcissus*: six petals laid out flat
/// in a star, and standing out of their middle a trumpet as long as they
/// are, flared at its mouth. The flower faces sideways on a bent neck.
/// https://en.wikipedia.org/wiki/Narcissus_pseudonarcissus
///
/// The trumpet is a tube and is built as one: a ring of ribs round an
/// empty middle, leaning outwards at the mouth, with a heart of another
/// colour seen down it. A ball there would be a daffodil's colour and
/// not its shape. Its leaves are a tulip's, narrower and upright.
fn daffodil_at(g: &mut Grown, foot: DVec3, tall: f64, head: f64, full: f64) {
    let facing = g.between(0.0, TAU);
    let face = (DVec3::new(facing.cos(), facing.sin(), 0.0) + DVec3::Z * g.between(0.0, 0.35)).normalize();
    let top = tall - head * 0.6;
    let stem = |t: f64| foot + DVec3::Z * top * t;
    let limb = g.limb(vec![stem(0.0), stem(0.5), stem(1.0)], vec![1.1; 3], None, 0, TRUNK);
    // the neck, bent over to face the way it looks
    let middle = stem(1.0) + face * 2.0 + DVec3::Z * 0.5;
    g.cone(stem(1.0), middle, 1.1, 1.2, LIMB);
    let (u, v) = (across(face), face.cross(across(face)));
    let twist = g.between(0.0, TAU);
    for n in 0..6 {
        let at = twist + TAU * n as f64 / 6.0;
        let (out, round) = (u * at.cos() + v * at.sin(), v * at.cos() - u * at.sin());
        let (half, tip) = (middle + out * head * 0.55 + face * 0.4, middle + out * head - face * 0.3);
        for hand in [-0.9, 0.9] {
            g.cone(middle + round * hand * 0.4, half + round * hand, 1.1, 1.15, TOP);
            g.cone(half + round * hand, tip, 1.15, 0.8, TOP);
        }
    }
    let (bore, long) = ((head * 0.34).max(1.9), head * 0.95);
    for n in 0..10 {
        let at = TAU * n as f64 / 10.0;
        let out = u * at.cos() + v * at.sin();
        g.cone(middle + out * bore + face * 0.6, middle + out * bore * 1.05 + face * long * 0.75, 1.05, 1.05, EXTRA);
        g.cone(middle + out * bore * 1.05 + face * long * 0.75, middle + out * bore * 1.5 + face * long, 1.05, 0.9, EXTRA);
    }
    g.blob(middle + face * 0.9, DVec3::splat((bore * 0.6).max(1.1)), STAR, false);
    let leaves = 2 + (full * 2.5) as usize;
    for n in 0..leaves {
        let way = facing + 2.0 + n as f64 * GOLDEN_ANGLE;
        let out = DVec3::new(way.cos(), way.sin(), 0.0);
        let reach = tall * g.between(0.6, 0.85);
        let mut at = foot + out * 1.2;
        let mut path = vec![at];
        for k in 0..5 {
            let polar: f64 = 0.08 + 0.5 * ((k as f64 + 0.5) / 5.0).powi(3);
            let next = at + (DVec3::Z * polar.cos() + out * polar.sin()) * reach / 5.0;
            g.cone(at, next, 1.15 - 0.04 * k as f64, 1.1 - 0.04 * k as f64, if n % 2 == 0 { LEAF } else { LEAF_DARK });
            at = next;
            path.push(at);
        }
        let radius = vec![1.1; path.len()];
        g.limbs.push(Limb { path, radius, parent: Some(limb), level: 1 });
    }
}

/// A clump of daisies or of daffodils: so many from one seed, standing
/// apart as a bed of flowers does (see [`standing`]).
fn clump(g: &mut Grown, tree: &Tree, height: f64, spread: f64, one: fn(&mut Grown, DVec3, f64, f64, f64)) {
    let full = tree.fullness as f64 / 100.0;
    for plant in 0..tree.count.max(1) as usize {
        let foot = standing(g, plant, spread * 1.7);
        let tall = height * if plant == 0 { 1.0 } else { g.between(0.65, 0.95) };
        let head = spread * if plant == 0 { 1.0 } else { g.between(0.8, 1.0) };
        one(g, foot, tall, head, full);
    }
}

/// A bed of mixed flowers from one seed: tulips, daffodils, daisies and
/// peonies in turn, scattered evenly over a round bed (see [`vogel`]),
/// the tall kinds and the short among one another.
///
/// Each flower is grown as it is alone, at the foot of the bed, and set
/// down in its place. They share the bed's colours part for part, petals
/// with petals and hearts with hearts, so a bed is of a few colours that
/// go together and not of every colour each kind comes in.
fn bed(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, loose) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let flowers = tree.count.max(1) as usize;
    g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![1.0, 1.0], None, 0);
    for n in 0..flowers {
        let (far, way) = vogel(n + 1, (spread - 4.0).max(2.0) / (flowers as f64).sqrt());
        let at = DVec3::new(way.cos(), way.sin(), 0.0) * far + DVec3::new(g.between(-1.0, 1.0), g.between(-1.0, 1.0), 0.0) * loose * 3.0;
        let (parts, limbs) = (g.parts.len(), g.limbs.len());
        let tall = height * g.between(0.8, 1.0);
        let one = Tree { count: 1, ..*tree };
        match (n + (g.unit() * 1.3) as usize) % 4 {
            0 => tulip(g, &one, tall, (tall * 0.095).max(4.0)),
            1 => daffodil_at(g, DVec3::ZERO, tall * 0.72, (tall * 0.11).max(4.5), full),
            2 => daisy_at(g, DVec3::ZERO, tall * 0.6, (tall * 0.11).max(4.5), full),
            _ => peony(g, &Tree { bend: 80, ..one }, tall * 0.86, (tall * 0.16).max(6.0)),
        }
        g.shift(parts, limbs, at);
    }
}

/// Where the `k`-th seed of a flower head is, by Vogel's model: how far
/// from the middle, and at what angle round it.
///
/// Each seed is a golden angle on from the last and `c * sqrt(k)` out,
/// which packs the seeds evenly and gives the crossing spirals of a
/// sunflower's head (Vogel 1979, "A better way to construct the sunflower
/// head"). Rings of seeds, the obvious alternative, leave gaps between
/// the rings and line the seeds up in spokes.
/// https://en.wikipedia.org/wiki/Fermat%27s_spiral#The_golden_ratio_and_the_golden_angle
pub fn vogel(k: usize, c: f64) -> (f64, f64) {
    (c * (k as f64).sqrt(), k as f64 * GOLDEN_ANGLE)
}

/// Where each plant of a clump stands, the first in the middle and the
/// rest round it `apart` from one another, by the same model: a bed of
/// flowers is itself a head of seeds.
fn standing(g: &mut Grown, plant: usize, apart: f64) -> DVec3 {
    let (far, way) = vogel(plant, apart);
    let way = way + g.between(-0.3, 0.3);
    DVec3::new(way.cos(), way.sin(), 0.0) * far * if plant == 0 { 0.0 } else { g.between(0.85, 1.1) }
}

/// A boxwood: a dense rounded evergreen shrub, *Buxus sempervirens*.
///
/// Several stems from the ground, forking early and often, under a
/// closed canopy of small leaves about as wide as it is tall.
/// https://en.wikipedia.org/wiki/Buxus_sempervirens
///
/// The stems are grown as an oak's limbs are, by forks, three forks deep.
/// The canopy is not left to the ends of the branches, as an oak's is: a
/// boxwood has no sky in it. Tufts of leaves are set all over a dome, a
/// golden angle apart so that they cover it evenly, each pushed in or out
/// a little, and the lowest fifth of the dome is left open so that the
/// stems show at the foot. That is the lumpy, layered outline of a shrub
/// that has been left to grow, where one smooth ball is a clipped one.
fn boxwood(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (dense, loose) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let stems = tree.count.max(1) as usize;
    let mass = (spread * (0.2 + 0.1 * dense)).clamp(2.6, height * 0.3);
    let stem = (height * 0.03).clamp(1.0, 1.7);
    // the dome the leaves lie on, its middle half way up
    let heart = DVec3::Z * height * 0.5;
    let dome = DVec3::new((spread - mass).max(2.0), (spread - mass).max(2.0), (height * 0.5 - mass * 0.8).max(2.0));
    let turn = g.between(0.0, TAU);
    let trunk = g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![stem, stem], None, 0);
    for k in 0..stems {
        let way = turn + TAU * (k as f64 + g.between(-0.3, 0.3)) / stems as f64;
        let polar = g.between(0.2, 0.85);
        // (each from its own place in the ground, a little apart: a bush has no trunk)
        let apart = spread * g.between(0.08, 0.3);
        let mut waiting = vec![(DVec3::new(way.cos(), way.sin(), 0.0) * apart, DVec3::new(way.cos() * polar.sin(), way.sin() * polar.sin(), polar.cos()), stem, 0u8)];
        while let Some((from, mut way, radius, depth)) = waiting.pop() {
            let run = height * 0.2 * g.between(0.75, 1.2);
            let mut path = vec![from];
            for _ in 0..2 {
                way = (way + g.any_way() * (0.15 + 0.3 * loose) + DVec3::Z * 0.12).normalize();
                let mut next = *path.last().unwrap() + way * run * 0.5;
                // held inside the canopy
                let q = (next - heart) / dome;
                if q.length() > 0.85 {
                    next = heart + q * (0.85 / q.length()) * dome;
                }
                next.z = next.z.max(0.5);
                path.push(next);
            }
            let end = *path.last().unwrap();
            let thin = (radius * 0.78).max(0.85);
            g.limb(path, vec![radius, (radius + thin) / 2.0, thin], Some(trunk), depth + 1, if depth == 0 { TRUNK } else { LIMB });
            if depth < 2 {
                let hinge = turned(across(way), way, g.between(0.0, TAU));
                let angle = g.between(0.45, 0.85);
                g.forks.push((radius, vec![thin, thin]));
                waiting.push((end, turned(way, hinge, angle), thin, depth + 1));
                waiting.push((end, turned(way, hinge, -angle), thin, depth + 1));
            } else {
                g.leaves(end, DVec3::new(mass, mass, mass * 0.8));
            }
        }
    }
    // the canopy: tufts all over the dome, evenly, the lowest left out
    let tufts = ((0.9 + 0.9 * dense) * 4.0 * dome.x * dome.z.max(dome.x) / (mass * mass)).ceil() as usize;
    for n in 0..tufts {
        let up = 1.0 - 2.0 * (n as f64 + 0.5) / tufts as f64;
        if up < -0.72 {
            continue;
        }
        let (level, way) = ((1.0 - up * up).sqrt(), n as f64 * GOLDEN_ANGLE);
        let at = heart + DVec3::new(level * way.cos(), level * way.sin(), up) * dome * (1.0 - loose * g.between(0.0, 0.3));
        let r = mass * g.between(0.75, 1.2);
        g.leaves(DVec3::new(at.x, at.y, at.z.max(r * 0.8)), DVec3::new(r, r, r * 0.8));
        if tree.extras && g.unit() < 0.35 {
            berries(g, DVec3::new(at.x, at.y, at.z.max(r * 0.8)), r * 0.8, heart);
        }
    }
}

/// A leaf blade as a fan of round ribs from `from`: the middle rib along
/// `along` and `long`, the ribs to each side shorter and swung out in the
/// blade's own plane, across `side`. Five ribs make the heart of a
/// sunflower's leaf, three close together the strap of a tulip's.
#[allow(clippy::too_many_arguments)]
fn blade(g: &mut Grown, from: DVec3, along: DVec3, side: DVec3, long: f64, ribs: i32, swing: f64, droop: f64) {
    for n in -ribs..=ribs {
        let angle = swing * n as f64;
        let reach = long * (1.0 - 0.42 * (n as f64 / ribs.max(1) as f64).abs());
        let way = (along * angle.cos() + side * angle.sin()).normalize();
        let (half, end) = (from + way * reach * 0.55, from + way * reach - DVec3::Z * droop * reach);
        let shade = if n == 0 { LEAF_LIGHT } else if n.abs() == ribs { LEAF_DARK } else { LEAF };
        g.cone(from, half, 1.1, 1.25, shade);
        g.cone(half, end, 1.25, 0.8, shade);
    }
}

/// A sunflower, *Helianthus annuus*, or a clump of them.
///
/// The head is built by rule, as the plant builds it. The seeds of the
/// disc are set by Vogel's model (see [`vogel`]), each a small ball, the
/// inner ones one colour and the outer ring another, on a disc that
/// bulges a little in the middle. Round it are the ray florets, the
/// petals: 13, 21 or 34 of them by the size of the head, as a real
/// head's are Fibonacci numbers, with a second row behind and between
/// the first. A head faces to one side and nods, more as it ripens.
/// https://en.wikipedia.org/wiki/Common_sunflower
///
/// The stem is stout and nearly straight, and carries broad heart-shaped
/// leaves that grow smaller up it, each a golden angle round from the
/// last (see [`blade`]).
///
/// A flower is large here, 90 cells tall with a head 34 across. A seed
/// has to be two cells through to be built at all, and a head needs
/// some dozens of seeds before its spirals can be seen, so the head sets
/// the size and the rest is in proportion.
fn sunflower(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, nod) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    // the heads of a clump face one way, give or take: grown heads all face east
    let east = g.between(0.0, TAU);
    for plant in 0..tree.count.max(1) as usize {
        let foot = standing(g, plant, spread * 1.25);
        let size = if plant == 0 { 1.0 } else { g.between(0.72, 0.95) };
        let (tall, head) = (height * if plant == 0 { 1.0 } else { g.between(0.6, 0.92) }, spread * size);
        // which way it faces, and the stem up to it: leaning a little that way, bending over at the top
        let facing = east + g.between(-0.6, 0.6);
        let face = DVec3::new(facing.cos(), facing.sin(), 0.0);
        let side = DVec3::new(-facing.sin(), facing.cos(), 0.0);
        let over = 0.75 + 0.75 * nod + g.between(-0.1, 0.1);
        let normal = (face * over.sin() + DVec3::Z * over.cos()).normalize();
        let thick = (tall * 0.02).clamp(1.3, 2.4);
        let top = tall - head * (over.sin() * 0.9).max(0.3) - 1.0;
        let steps = 9;
        let lean = g.between(0.02, 0.08) * tall;
        let stem = |t: f64| foot + face * (lean * t * t + head * 0.35 * ((t - 0.85) / 0.15).max(0.0).powi(2)) + DVec3::Z * top * t;
        let path: Vec<DVec3> = (0..=steps).map(|k| stem(k as f64 / steps as f64)).collect();
        let middle = *path.last().unwrap() + normal * 1.0;
        let limb = g.limb(path, (0..=steps).map(|k| thick * (1.0 - 0.25 * k as f64 / steps as f64)).collect(), None, 0, TRUNK);
        // the leaves, from a fifth of the way up to four fifths
        let leaves = 4 + (tall / 22.0) as usize;
        let turn = g.between(0.0, TAU);
        for n in 0..leaves {
            let t = 0.2 + 0.6 * n as f64 / (leaves - 1).max(1) as f64;
            let way = turn + n as f64 * GOLDEN_ANGLE;
            let (out, across) = (DVec3::new(way.cos(), way.sin(), 0.0), DVec3::new(-way.sin(), way.cos(), 0.0));
            let long = head * (0.95 - 0.45 * t) * (0.75 + 0.5 * full);
            let stalk = stem(t) + (out * 0.9 + DVec3::Z * 0.45) * long * 0.3;
            g.cone(stem(t), stalk, 1.0, 0.9, LIMB);
            blade(g, stalk, (out - DVec3::Z * 0.25).normalize(), across, long, 2, 0.5, 0.25);
            g.limbs.push(Limb { path: vec![stem(t), stalk], radius: vec![1.0, 0.9], parent: Some(limb), level: 1 });
        }
        // the head: its back, the disc of seeds, and the petals round it
        let (u, v) = (side, normal.cross(side).normalize());
        let disc = head * 0.5;
        // (its back is a plate of the same pattern, coarser, and not one
        // solid: a ball or cone as wide as the disc is as deep as it is
        // wide, and swallowed the seeds)
        g.cone(*g.limbs[limb].path.last().unwrap(), middle - normal * 1.2, thick * 0.8, thick * 1.5, LIMB);
        for k in 1..=((disc / 2.0).powi(2)) as usize {
            let (far, at) = vogel(k, 2.0);
            g.blob(middle + (u * at.cos() + v * at.sin()) * far.min(disc - 1.0) - normal * 1.5, DVec3::splat(1.7), LIMB, false);
        }
        let apart = 1.3;
        for k in 1..=((disc / apart).powi(2)) as usize {
            let (far, at) = vogel(k, apart);
            // (domed: a head is a shallow cushion, highest in the middle)
            let bulge = disc * 0.34 * (1.0 - (far / disc).powi(2));
            let seed = middle + (u * at.cos() + v * at.sin()) * far + normal * bulge;
            g.blob(seed, DVec3::splat(1.3), if far < disc * 0.6 { STAR } else { EXTRA }, false);
        }
        let petals = if head < 12.0 { 13 } else if head < 26.0 { 21 } else { 34 };
        for row in 0..2 {
            for n in 0..petals {
                let at = TAU * (n as f64 + 0.5 * row as f64) / petals as f64;
                let (out, round) = (u * at.cos() + v * at.sin(), v * at.cos() - u * at.sin());
                let long = (head - disc) * (1.0 - 0.14 * row as f64) * g.between(0.88, 1.05) * (0.8 + 0.3 * full);
                let from = middle + out * disc * 0.92 - normal * (0.3 + 1.1 * row as f64);
                // Cupped forward where it leaves the disc and laid back
                // at its tip, so the ring of petals is a shallow bowl
                // with a rolled rim and not a flat plate. Two ribs side by
                // side at its root, meeting at its tip: a petal is flat.
                let half = from + out * long * 0.5 + normal * long * (0.2 - 0.06 * row as f64);
                let tip = from + out * long * 0.96 - normal * long * g.between(0.12, 0.3);
                for hand in [-0.75, 0.75] {
                    g.cone(from + round * hand, half + round * hand * 0.6, 1.15, 1.05, TOP);
                    g.cone(half + round * hand * 0.6, tip, 1.05, 0.75, TOP);
                }
            }
        }
    }
}

/// A tulip, or a clump of them.
///
/// A tulip is a cup of six petals (tepals) in two rings of three, the
/// inner three standing in the gaps of the outer, on a bare stem, with a
/// few broad strap-shaped leaves from the ground that clasp the stem and
/// arch away from it. The cup is taller than it is wide.
/// https://en.wikipedia.org/wiki/Tulip
///
/// Each petal is three ribs side by side, bowed out from the base of the
/// cup and in again at the lip, the middle rib the longest, so the petal
/// comes to a point. The outer ring is one colour number and the inner
/// another, so the cup can be painted in two tones or flamed.
fn tulip(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let (full, open) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    for plant in 0..tree.count.max(1) as usize {
        let foot = standing(g, plant, spread * 2.7);
        let tall = height * if plant == 0 { 1.0 } else { g.between(0.68, 0.96) };
        let cup = spread * if plant == 0 { 1.0 } else { g.between(0.8, 0.98) };
        let deep = cup * 2.5;
        let leaning = g.between(0.0, TAU);
        let lean = DVec3::new(leaning.cos(), leaning.sin(), 0.0);
        let sway = g.between(0.02, 0.1) * tall;
        let top = (tall - deep).max(tall * 0.4);
        let stem = |t: f64| foot + lean * sway * t * t + DVec3::Z * top * t;
        let steps = 6;
        let thick = (tall * 0.02).clamp(1.2, 1.9);
        let limb = g.limb((0..=steps).map(|k| stem(k as f64 / steps as f64)).collect(), vec![thick; steps + 1], None, 0, TRUNK);
        // the cup: its axis carries on from the stem
        let axis = (stem(1.0) - stem(0.9)).normalize();
        let (u, v) = (across(axis), axis.cross(across(axis)));
        let base = stem(1.0);
        g.blob(base + axis * cup * 0.35, DVec3::splat(cup * 0.5), TOP, false);
        let turn = g.between(0.0, TAU);
        // how far out a rib is, and how far up the cup, at five places along it
        let bow = [0.3, 0.88, 1.0, 0.92, 0.62 + 0.5 * open];
        let rise = [0.0, 0.2, 0.5, 0.8, 1.0];
        for ring in 0..2 {
            for n in 0..3 {
                let round = turn + TAU * (n as f64 + 0.5 * ring as f64) / 3.0;
                let reach = cup * (1.0 - 0.14 * ring as f64);
                for rib in -1i32..=1 {
                    let at = round + 0.42 * rib as f64;
                    let out = u * at.cos() + v * at.sin();
                    // the side ribs stop short of the lip
                    let ends = if rib == 0 { 4 } else { 3 };
                    for k in 0..ends {
                        let place = |k: usize| base + out * reach * bow[k] + axis * deep * rise[k] * if rib == 0 { 1.0 } else { 0.94 };
                        g.cone(place(k), place(k + 1), 1.3, if k + 1 == ends { 0.85 } else { 1.3 }, if ring == 0 { TOP } else { EXTRA });
                    }
                }
            }
        }
        // the leaves: two or three from the ground, clasping the stem, arching away
        let leaves = 2 + (g.unit() * 1.6) as usize;
        let turn = g.between(0.0, TAU);
        for n in 0..leaves {
            let way = turn + n as f64 * (TAU / leaves as f64) + g.between(-0.4, 0.4);
            let (out, across) = (DVec3::new(way.cos(), way.sin(), 0.0), DVec3::new(-way.sin(), way.cos(), 0.0));
            let long = tall * g.between(0.5, 0.68) * (0.8 + 0.4 * full);
            let wide = (cup * 0.38).clamp(1.2, 3.4);
            let mut record = vec![foot + out * thick];
            for rib in -1i32..=1 {
                let mut at = foot + out * thick + across * wide * rib as f64;
                let reach = long * if rib == 0 { 1.0 } else { 0.86 };
                for k in 0..6i32 {
                    // nearly upright at the ground, turned out by its tip
                    let polar: f64 = 0.12 + 1.15 * ((k as f64 + 0.5) / 6.0).powf(1.5);
                    let next = at + (DVec3::Z * polar.cos() + out * polar.sin() - across * 0.12 * rib as f64).normalize() * reach / 6.0;
                    let thin = |k: i32| 1.35 - 0.1 * k as f64;
                    g.cone(at, next, thin(k), thin(k + 1), if rib == 0 { LEAF_LIGHT } else if n % 2 == 0 { LEAF } else { LEAF_DARK });
                    at = next;
                    if rib == 0 {
                        record.push(at);
                    }
                }
            }
            let radius = vec![1.3; record.len()];
            g.limbs.push(Limb { path: record, radius, parent: Some(limb), level: 1 });
        }
    }
}

/// Where the middle of a palm's trunk is at each height, for tests.
#[cfg(test)]
fn level_middle(model: &Model, z: f64) -> Option<glam::DVec2> {
    let trunk = &model.limbs[0];
    trunk.path.windows(2).find(|w| w[0].z <= z && z <= w[1].z).map(|w| w[0].lerp(w[1], (z - w[0].z) / (w[1].z - w[0].z).max(1e-9)).truncate())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{build, Form, Spec};

    fn spec(kind: Kind, seed: u64) -> Spec {
        let (height, spread) = Tree::usual_size(kind);
        Spec { form: Form::Tree, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: spread, rise: Some(height), grain: 0, tree: Tree { seed, ..Tree::of(kind) }, ..Spec::default() }
    }

    /// Where a build's time goes, printed: run with `--nocapture`. Not a
    /// check of anything; the bound is `a_tree_builds_in_under_a_second`.
    #[test]
    #[ignore = "a measurement, not a check: run it by name with --nocapture"]
    #[cfg(not(debug_assertions))]
    fn where_the_time_goes() {
        use crate::par::Threads;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Mutex;
        use std::time::{Duration, Instant};
        let _ = Duration::ZERO;
        for (kind, name) in KINDS {
            let spec = spec(kind, 1);
            let start = Instant::now();
            let model = Model::new(&spec.tree, spec.points[0], spec.rise.unwrap(), spec.radius);
            let grown = start.elapsed();
            let (lo, hi) = model.bounds();
            let (min, max) = ((lo - 1.0).floor().to_array().map(|v| v as i32), (hi + 1.0).ceil().to_array().map(|v| v as i32));
            // every 16th sample is kept, to be asked again on their own
            let (calls, kept) = (AtomicU64::new(0), Mutex::new(Vec::new()));
            let near = AtomicU64::new(0);
            let field = |p: DVec3| {
                // (a sample inside a cell is at eighths; a block's middle and a cell's corner are at wholes and halves)
                if (p.x * 4.0).fract().abs() == 0.5 {
                    near.fetch_add(1, Ordering::Relaxed);
                }
                if calls.fetch_add(1, Ordering::Relaxed) % 16 == 0 {
                    kept.lock().unwrap().push(p);
                }
                model.eval(p)
            };
            let start = Instant::now();
            let fit = fit_volume::fit_volume(&field, min, max, &fit_volume::Opts::default());
            let fitted = start.elapsed();
            let start = Instant::now();
            let (bricks, _) = model.colour(fit.bricks, 80, Threads::ONE);
            let coloured = start.elapsed();
            let (kept, calls) = (kept.into_inner().unwrap(), calls.into_inner());
            let again = (0..3).map(|_| { let at = Instant::now(); let sum: f64 = kept.iter().map(|p| model.eval(*p).min(1.0)).sum(); std::hint::black_box(sum); at.elapsed() }).min().unwrap();
            let timed: Duration = again * 16;
            let listed: usize = kept.iter().map(|p| {
                let c = ((*p - model.origin) / model.bucket).floor();
                if c.min_element() < 0.0 || c.x >= model.dims[0] as f64 || c.y >= model.dims[1] as f64 || c.z >= model.dims[2] as f64 { return 0 }
                let b = c.x as usize + model.dims[0] * (c.y as usize + model.dims[1] * c.z as usize);
                (model.start[b + 1] - model.start[b]) as usize
            }).sum();
            println!("{name}: a sample's bucket lists {:.1} parts", listed as f64 / kept.len() as f64);
            let timed_on = |threads: Threads| (0..3).map(|_| { let at = Instant::now(); crate::spec::build_with(&spec, true, threads).unwrap(); at.elapsed() }).min().unwrap();
            let (whole, shared) = (timed_on(Threads::ONE), timed_on(Threads::available()));
            println!("{name}: a whole build on {} threads {shared:?}; {} of its {} samples are the 64 inside a cell the surface crosses", Threads::available().count(), near.load(Ordering::Relaxed), calls);
            let occupied = (0..model.start.len() - 1).filter(|b| model.start[b + 1] > model.start[*b]).count();
            println!("{name}: {} buckets, {occupied} with parts, {:.1} parts in each of those", model.start.len() - 1, model.items.len() as f64 / occupied as f64);
            println!("{name}: {} parts, {} bricks; grown in {grown:?}, fitted in {fitted:?} of which the field's {} samples took about {timed:?}, coloured in {coloured:?}; a whole build {whole:?}", model.parts(), bricks.len(), calls);
        }
    }

    fn model(kind: Kind, seed: u64) -> Model {
        let spec = spec(kind, seed);
        Model::new(&spec.tree, spec.points[0], spec.rise.unwrap(), spec.radius)
    }

    /// A smaller tree of each kind, for the tests that build bricks.
    fn small(kind: Kind, seed: u64) -> Spec {
        let (height, spread) = Tree::usual_size(kind);
        Spec { radius: (spread * 0.6).round(), rise: Some((height * 0.6).round()), ..spec(kind, seed) }
    }

    fn is_leaf(shade: u8) -> bool {
        matches!(shade, LEAF | LEAF_DARK | LEAF_LIGHT | TOP)
    }

    #[test]
    fn one_seed_is_one_tree_and_another_seed_another() {
        for (kind, name) in KINDS {
            let (a, again, other) = (build(&small(kind, 5)).unwrap(), build(&small(kind, 5)).unwrap(), build(&small(kind, 6)).unwrap());
            assert!(a.bricks.len() > 400, "{name}: only {} bricks", a.bricks.len());
            assert_eq!((&a.bricks, &a.shades), (&again.bricks, &again.shades), "{name}: one seed gave two trees");
            assert_ne!(a.bricks, other.bricks, "{name}: two seeds gave one tree");
            // and each kind has numbers of its own: the same seed is not the same shape in another kind
            let trunk = |kind| model(kind, 5).limbs[0].path.clone();
            assert_ne!(trunk(Kind::Pine), trunk(Kind::Oak));
        }
    }

    #[test]
    fn no_two_bricks_share_a_cell_and_each_has_its_colour() {
        for (kind, name) in KINDS {
            // (with its extras, but a broadleaf tree's, which is to be bare)
            let built = build(&Spec { tree: Tree { extras: !matches!(kind, Kind::Oak | Kind::Beech | Kind::Birch | Kind::Willow | Kind::ScotsPine | Kind::Cherry), ..small(kind, 2).tree }, ..small(kind, 2) }).unwrap();
            let mut seen = std::collections::HashSet::new();
            for b in &built.bricks {
                for z in 0..b.ext[2] {
                    for y in 0..b.ext[1] {
                        for x in 0..b.ext[0] {
                            assert!(seen.insert((b.min[0] + x, b.min[1] + y, b.min[2] + z)), "{name}: two bricks in one cell");
                        }
                    }
                }
            }
            assert_eq!(built.shades.len(), built.bricks.len(), "{name}: a brick has no colour number");
            assert!(built.shades.iter().all(|s| *s < 8));
            let count = |wanted: fn(u8) -> bool| built.shades.iter().filter(|s| wanted(**s)).count();
            let (bark, leaves) = (count(|s| s == TRUNK), count(is_leaf));
            // (a flower's stem and leaves are few bricks beside a tree's)
            assert!((bark > 20 || matches!(kind, Kind::Cypress | Kind::Yucca)) && leaves > 150, "{name}: {bark} bricks of bark or stem and {leaves} of leaves");
            // nothing below the ground it stands on, and it stands on it
            assert_eq!(built.bricks.iter().map(|b| b.min[2]).min(), Some(0), "{name}");
            // its extras are there when asked for
            if matches!(kind, Kind::Pine | Kind::Palm | Kind::Boxwood | Kind::Hedge | Kind::Reeds) {
                // (reeds' extras are plumes in place of the brown heads, which are the colour the others' extras are)
                if kind == Kind::Reeds {
                    assert!(!built.shades.contains(&EXTRA) && build(&small(kind, 2)).unwrap().shades.contains(&EXTRA));
                    continue;
                }
                assert!(count(|s| s == EXTRA) > 3, "{name}: no extras");
                let plain = build(&Spec { tree: Tree { extras: false, ..small(kind, 2).tree }, ..small(kind, 2) }).unwrap();
                assert!(!plain.shades.contains(&EXTRA), "{name}: extras not asked for");
            }
        }
    }

    /// A box cut by the fitter across bark and leaves is cut apart again:
    /// no brick of bark reaches out into the needles.
    #[test]
    fn a_brick_is_of_one_colour_all_through() {
        let spec = small(Kind::Pine, 3);
        let built = build(&spec).unwrap();
        let tree = Model::new(&spec.tree, spec.points[0], spec.rise.unwrap(), spec.radius);
        let mut boxes = 0;
        for (b, shade) in built.bricks.iter().zip(&built.shades).filter(|(b, _)| b.mask == CUBE && b.ext != [1; 3]) {
            boxes += 1;
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        let middle = DVec3::new((b.min[0] + x) as f64 + 0.5, (b.min[1] + y) as f64 + 0.5, (b.min[2] + z) as f64 + 0.5);
                        assert_eq!(tree.shade(middle), *shade, "a box at {:?} of {:?} is {shade} and holds another colour", b.min, b.ext);
                    }
                }
            }
        }
        assert!(boxes > 100, "only {boxes} boxes looked at");
    }

    #[test]
    fn a_pine_has_tiers_that_shorten_to_a_single_leader() {
        for seed in 1..4 {
            let tree = model(Kind::Pine, seed);
            let (height, spread) = Tree::usual_size(Kind::Pine);
            let trunk = &tree.limbs[0];
            assert!(trunk.parent.is_none() && trunk.level == 0);
            let top = trunk.path.last().unwrap();
            assert!((top.z - height).abs() < 1e-9, "the stem ends at {} of {height}", top.z);
            // every other limb is a bough springing from the stem, and none reaches as high as the stem does
            let boughs: Vec<&Limb> = tree.limbs[1..].iter().collect();
            assert!(boughs.len() > 60, "only {} boughs", boughs.len());
            assert!(boughs.iter().all(|b| b.parent == Some(0) && b.level == 1));
            assert!(boughs.iter().all(|b| b.path.iter().all(|p| p.z < top.z - 3.0)), "a bough stands as high as the leader");
            // how far each reaches from the stem, by where it leaves it
            let reach = |b: &Limb| (*b.path.last().unwrap() - b.path[0]).truncate().length();
            let mean = |low: f64, high: f64| {
                let these: Vec<f64> = boughs.iter().filter(|b| b.path[0].z >= low * height && b.path[0].z < high * height).map(|b| reach(b)).collect();
                assert!(these.len() >= 4, "only {} boughs from {low} to {high}", these.len());
                these.iter().sum::<f64>() / these.len() as f64
            };
            let (low, middle, high, highest) = (mean(0.12, 0.35), mean(0.35, 0.6), mean(0.6, 0.8), mean(0.8, 0.95));
            assert!(low > middle * 1.08 && middle > high * 1.2 && high > highest * 1.3, "mean reach {low:.1}, {middle:.1}, {high:.1}, {highest:.1} going up");
            // the longest reaches as far as the tree is asked to spread, about
            let longest = boughs.iter().map(|b| reach(b)).fold(0.0, f64::max);
            assert!(longest > spread * 0.8 && longest <= spread, "longest bough {longest:.1} of {spread}");
            // the lowest hang and the highest climb
            let slope = |b: &Limb| (b.path[2].z - b.path[0].z) / (b.path[2] - b.path[0]).truncate().length();
            let slopes = |low: f64, high: f64| boughs.iter().filter(|b| b.path[0].z >= low * height && b.path[0].z < high * height).map(|b| slope(b)).sum::<f64>();
            assert!(slopes(0.0, 0.3) < 0.0 && slopes(0.8, 1.0) > 0.0);
            // bare stem below the lowest bough
            let lowest = boughs.iter().map(|b| b.path[0].z).fold(f64::MAX, f64::min);
            assert!(lowest > height * 0.06, "boughs from {lowest}");
            // and the top is a point: a cell under the tip, the tree is under five cells across
            let under = DVec3::new(top.x, top.y, top.z - 1.0);
            assert!(tree.eval(under) < 0.0);
            assert!((0..8).all(|k| tree.eval(under + DVec3::new((k as f64 * TAU / 8.0).cos(), (k as f64 * TAU / 8.0).sin(), 0.0) * 2.5) > 0.0), "the top is blunt");
        }
    }

    /// The distance along `way` from `from` at which the tree's surface is crossed.
    fn surface(tree: &Model, from: DVec3, way: DVec3) -> f64 {
        let (mut low, mut high) = (0.0, 40.0);
        assert!(tree.eval(from) < 0.0 && tree.eval(from + way * high) > 0.0);
        for _ in 0..40 {
            let middle = (low + high) / 2.0;
            if tree.eval(from + way * middle) < 0.0 { low = middle } else { high = middle }
        }
        low
    }

    #[test]
    fn a_palm_is_one_unbranched_eight_sided_trunk_and_a_crown_of_fronds() {
        for seed in 1..4 {
            let settings = Tree { seed, ..Tree::of(Kind::Palm) };
            let (height, spread) = Tree::usual_size(Kind::Palm);
            let tree = Model::new(&settings, DVec3::new(0.5, 0.5, 0.5), height, spread);
            let trunk = &tree.limbs[0];
            let top = *trunk.path.last().unwrap();
            assert!(trunk.path.windows(2).all(|w| w[1].z > w[0].z), "the trunk does not climb all the way");
            // every other limb is a frond, and every frond springs from the crown
            let fronds = &tree.limbs[1..];
            assert_eq!(fronds.len(), settings.count as usize);
            for frond in fronds {
                assert_eq!((frond.parent, frond.level), (Some(0), 1));
                assert!(frond.path[0].distance(top) < height * 0.06, "a frond springs {} from the top of the trunk", frond.path[0].distance(top));
                // as long as asked, about; and its tip hangs lower than its highest point
                let long: f64 = frond.path.windows(2).map(|w| w[0].distance(w[1])).sum();
                assert!(long > spread * 0.4 && long < spread * 1.1, "a frond {long:.1} long of {spread}");
                let highest = frond.path.iter().map(|p| p.z).fold(f64::MIN, f64::max);
                assert!(frond.path.last().unwrap().z < highest - 0.5 || frond.path[1].z > frond.path[0].z + 2.0, "a frond neither arches nor stands");
            }
            // set round the trunk by the golden angle: no two neighbours in age on the same side
            let way = |f: &Limb| (f.path[2] - f.path[0]).truncate().normalize();
            assert!(fronds.windows(2).all(|w| way(&w[0]).dot(way(&w[1])) < 0.2));
            // the oldest hang below the crown and the youngest stand above it
            assert!(fronds.last().unwrap().path.last().unwrap().z < top.z - spread * 0.3);
            assert!(fronds[0].path.last().unwrap().z > top.z + spread * 0.25);
            // The trunk in section, half way up: as far to a face along x
            // and along the diagonal, and further to the corner between.
            let z = (top.z * 0.5).floor() + 0.4;
            let middle = level_middle(&tree, z).unwrap().extend(z);
            let to = |degrees: f64| surface(&tree, middle, DVec3::new(degrees.to_radians().cos(), degrees.to_radians().sin(), 0.0));
            let (face, diagonal, corner) = (to(0.0), to(45.0), to(22.5));
            assert!((face - diagonal).abs() < 0.02 * face, "faces at {face} and {diagonal}");
            assert!((corner / face - 1.0824).abs() < 0.01, "corner at {corner}, face at {face}: not an octagon");
            // swollen at the foot, and slender: under a twelfth of its height through
            let foot = surface(&tree, DVec3::new(0.5, 0.5, 1.5), DVec3::X);
            assert!(foot > face * 1.4, "foot {foot}, half way up {face}");
            assert!(face * 2.0 < height / 12.0);
            // ringed: both colours of trunk up one side of it
            let side: Vec<u8> = (2..(top.z * 0.8) as i32).map(|k| {
                let z = k as f64 + 0.5;
                let middle = level_middle(&tree, z).unwrap().extend(z);
                tree.shade(middle + DVec3::X * (surface(&tree, middle, DVec3::X) - 0.4))
            }).collect();
            let rings = side.windows(2).filter(|w| w[0] == TRUNK && w[1] == LIMB).count();
            assert!(rings >= 8, "{rings} rings up the trunk");
        }
        // and the lean is the lean asked for
        let top_of = |bend: i32| {
            let tree = Model::new(&Tree { bend, ..Tree::of(Kind::Palm) }, DVec3::new(0.5, 0.5, 0.5), 120.0, 44.0);
            (*tree.limbs[0].path.last().unwrap() - tree.limbs[0].path[0]).truncate().length()
        };
        assert!(top_of(0) < 3.0 && top_of(100) > 25.0, "{} and {}", top_of(0), top_of(100));
    }

    #[test]
    fn an_oak_divides_by_da_vincis_rule_into_a_wide_crown_of_separate_masses() {
        for seed in 1..5 {
            let tree = model(Kind::Oak, seed);
            let (height, spread) = Tree::usual_size(Kind::Oak);
            // the squares of the radii a limb divides into sum to the square of its own
            assert!(tree.forks.len() > 25, "only {} forks", tree.forks.len());
            for (limb, into) in &tree.forks {
                let sum: f64 = into.iter().map(|r| r * r).sum();
                assert!((sum - limb * limb).abs() < 1e-9 * limb * limb, "{limb} divides into {into:?}");
                assert!(into.len() >= 2);
            }
            // and the limbs really are those radii: each springs no thicker than what it springs from
            for limb in &tree.limbs[1..] {
                let from = &tree.limbs[limb.parent.unwrap()];
                assert!(limb.radius[0] <= from.radius.iter().cloned().fold(0.0, f64::max) + 1e-9);
                assert_eq!(limb.level, from.level + 1);
                // it starts on the limb it springs from
                assert!(from.path.iter().any(|p| p.distance(limb.path[0]) < 1e-6), "a limb starts in the air");
                // and thins or holds along its length, never thickens
                assert!(limb.radius.windows(2).all(|w| w[1] <= w[0] + 1e-9));
            }
            // a short trunk: it divides in the lowest quarter
            let fork = tree.limbs[0].path.last().unwrap().z;
            assert!(fork < height * 0.26 && fork > height * 0.15, "the trunk divides at {fork}");
            // limbs fork repeatedly: some branch is four forks from the trunk
            assert!(tree.limbs.iter().map(|l| l.level).max().unwrap() >= 3 && tree.limbs.len() > 15, "{} limbs", tree.limbs.len());
            // the crown is as wide as asked and as tall, and wider than it is tall
            let (lo, hi) = tree.bounds();
            let wide = (hi.x - lo.x).max(hi.y - lo.y);
            assert!(wide > spread * 1.6 && wide <= spread * 2.0 + 1e-6, "{wide} wide of {}", spread * 2.0);
            assert!(hi.z > height * 0.85 && hi.z <= height + 1e-6, "{} tall of {height}", hi.z);
            // Leaves in separate masses with sky between: seen from the
            // side, the leaves cover well under the share of the crown's
            // box that one ball of leaves would (which is pi / 4), and
            // there are gaps right through the crown.
            let leaf = |p: DVec3| tree.eval(p) < 0.0 && is_leaf(tree.shade(p));
            let (mut covered, mut all, mut holes) = (0, 0, 0);
            let low = fork + 10.0;
            let mut z = low;
            while z < hi.z {
                let mut row = Vec::new();
                let mut x = lo.x;
                while x < hi.x {
                    let mut y = lo.y;
                    let mut any = false;
                    while y < hi.y && !any {
                        any = leaf(DVec3::new(x, y, z));
                        y += 1.5;
                    }
                    row.push(any);
                    x += 2.0;
                }
                all += row.len();
                covered += row.iter().filter(|c| **c).count();
                // sky with leaves to both sides of it on this level
                let (first, last) = (row.iter().position(|c| *c), row.iter().rposition(|c| *c));
                if let (Some(first), Some(last)) = (first, last) {
                    holes += row[first..=last].iter().filter(|c| !**c).count();
                }
                z += 2.0;
            }
            let share = covered as f64 / all as f64;
            assert!(share > 0.3 && share < 0.68, "leaves cover {share:.2} of the crown's box from the side");
            assert!(holes > 20, "only {holes} gaps through the crown");
        }
    }

    /// Settings beyond what the sliders allow are brought within it, and a
    /// tree too large is refused before it is grown.
    #[test]
    fn hostile_settings_build_a_tree_or_are_refused() {
        let wild = Tree { kind: Kind::Oak, seed: u64::MAX, fullness: i32::MAX, count: i32::MIN, bend: -7, extras: true };
        assert_eq!(wild.clamped(), Tree { kind: Kind::Oak, seed: u64::MAX, fullness: 100, count: 2, bend: 0, extras: true });
        let built = build(&Spec { tree: wild, radius: 20.0, rise: Some(40.0), ..spec(Kind::Oak, 1) }).unwrap();
        assert!(!built.bricks.is_empty());
        // the smallest there is: too small to be much, and still built
        for (kind, name) in KINDS {
            let tiny = build(&Spec { radius: 0.5, rise: Some(0.5), ..spec(kind, 1) }).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!tiny.bricks.is_empty(), "{name}");
        }
        // (at single microbricks, asked for by name; left to itself it is built at a larger size)
        let vast = build(&Spec { grain: 1, radius: 900.0, rise: Some(900.0), ..spec(Kind::Pine, 1) });
        assert!(vast.is_err_and(|e| e.contains("too large")));
        let at = std::time::Instant::now();
        let vast = build(&Spec { grain: 0, radius: 900.0, rise: Some(900.0), ..spec(Kind::Pine, 1) }).unwrap();
        println!("a pine 900 tall: grain {}, {} bricks, {:?}", vast.grain, vast.bricks.len(), at.elapsed());
        assert!(vast.grain > 1);
    }

    /// The field is what the fitter is told it is: never more than the
    /// distance to the surface, so that the cells it passes over as far
    /// from the tree are. Checked against all the parts, one by one.
    #[test]
    fn the_bucket_grid_agrees_with_asking_every_part() {
        for (kind, name) in KINDS {
            let spec = small(kind, 4);
            let tree = Model::new(&spec.tree, spec.points[0], spec.rise.unwrap(), spec.radius);
            let (lo, hi) = tree.bounds();
            let mut numbers = Numbers(99);
            let (mut near, mut inside) = (0, 0);
            for _ in 0..40_000 {
                let p = lo - 6.0 + DVec3::new(numbers.unit(), numbers.unit(), numbers.unit()) * (hi - lo + 12.0);
                let (nearest, exact) = tree.parts.iter().map(|part| (part, part.shape.distance(p))).fold((&tree.parts[0], f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a });
                let exact = exact.max(tree.ground - p.z);
                let got = tree.eval(p);
                assert_eq!(got < 0.0, exact < 0.0, "{name}: {p} is on the other side");
                // Near the tree, never less than the least any part
                // gives: a bucket lists every part that comes near it.
                // (to within the single precision the far distances are kept in)
                assert!(got >= exact.min(NEAR) - 1e-6, "{name}: {got} at {p}, where a part gives {exact}");
                // And never further than the tree is. Only a round
                // branch's distance is exact away from its surface (a
                // flattened ball of leaves and a length of a palm's trunk
                // give less than theirs), so only there can it be checked.
                if matches!(nearest.shape, Shape::Cone { .. }) && exact >= 0.0 {
                    assert!(got <= exact + 1e-9, "{name}: {got} at {p}, which is {exact} from the tree");
                    near += (exact <= NEAR) as usize;
                }
                inside += (exact < 0.0) as usize;
                // and far from the tree it is known to be far: the fitter passes over such blocks
                // (by more than 7, half the diagonal of a block; 24 from it
                // any way is 14 from it along some axis, three coarse buckets)
                if exact > 24.0 && p.cmpge(lo).all() && p.cmple(hi).all() {
                    assert!(got > 7.0, "{name}: {got} at {p}, which is {exact} from the tree");
                }
            }
            assert!((near > 40 || kind == Kind::Oak) && inside > 150, "{name}: {near} near, {inside} inside");
        }
    }

    /// Release only: an unoptimised build is many times slower and says
    /// nothing about the time a player waits.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_tree_a_hundred_tall_builds_in_about_a_second() {
        for (kind, name) in KINDS {
            // (a bush or a flower at its own size, which is under a hundred)
            let (height, spread) = Tree::usual_size(kind);
            let tall = height.min(100.0);
            let spec = Spec { radius: (spread * tall / height).round(), rise: Some(tall), ..spec(kind, 1) };
            // (on one thread: with more it is quicker, by how many the machine has)
            let least = (0..4).map(|_| { let at = std::time::Instant::now(); crate::spec::build_with(&spec, true, crate::par::Threads::ONE).unwrap(); at.elapsed() }).min().unwrap();
            println!("{name}, {tall} tall: {least:?}");
            // (The slowest kind takes about 0.6 s alone. The bound is four
            // times that because the other tests run beside this one: at
            // 1.2 s it failed in a full run on a busy machine and passed
            // by itself. The faults it is for cost more: before the parts
            // were filed in buckets a pine took 2.9 s.)
            assert!(least.as_secs_f64() < 2.5, "{name} took {least:?}");
        }
    }

    /// The bricks of one colour number of a plant built at its usual size, as their middles.
    fn bricks_of(built: &crate::spec::Built, wanted: &[u8]) -> Vec<DVec3> {
        built.bricks.iter().zip(&built.shades).filter(|(_, s)| wanted.contains(s)).map(|(b, _)| DVec3::new(b.min[0] as f64 + b.ext[0] as f64 / 2.0, b.min[1] as f64 + b.ext[1] as f64 / 2.0, b.min[2] as f64 + b.ext[2] as f64 / 2.0)).collect()
    }

    #[test]
    fn seeds_of_a_head_are_a_golden_angle_apart_and_evenly_packed() {
        // each a golden angle on from the last, and as far out as the root of its number
        for k in 1..200 {
            let ((far, at), (next_far, next_at)) = (vogel(k, 1.3), vogel(k + 1, 1.3));
            assert!((next_at - at - GOLDEN_ANGLE).abs() < 1e-9);
            assert!((far - 1.3 * (k as f64).sqrt()).abs() < 1e-12 && next_far > far);
        }
        assert!((GOLDEN_ANGLE.to_degrees() - 137.5078).abs() < 1e-3);
        // evenly packed: no two of the first 200 seeds nearer than the spacing, and none with a gap of twice it round it
        let seeds: Vec<DVec3> = (1..=200).map(|k| vogel(k, 1.3)).map(|(far, at)| DVec3::new(far * at.cos(), far * at.sin(), 0.0)).collect();
        for (n, seed) in seeds.iter().enumerate().take(150) {
            let nearest = seeds.iter().enumerate().filter(|(m, _)| *m != n).map(|(_, other)| other.distance(*seed)).fold(f64::MAX, f64::min);
            assert!(nearest > 1.3 * 0.9 && nearest < 1.3 * 2.4, "seed {n} has its nearest neighbour {nearest} away");
        }
    }

    #[test]
    fn a_sunflower_is_a_stem_with_leaves_and_a_head_of_seeds_ringed_with_petals() {
        for seed in 1..4 {
            let one = Spec { tree: Tree { seed, count: 1, ..Tree::of(Kind::Sunflower) }, ..spec(Kind::Sunflower, seed) };
            let tree = Model::new(&one.tree, one.points[0], one.rise.unwrap(), one.radius);
            let (height, head) = Tree::usual_size(Kind::Sunflower);
            // one stem, climbing all the way, with leaves springing from it
            let stems: Vec<&Limb> = tree.limbs.iter().filter(|l| l.level == 0).collect();
            assert_eq!(stems.len(), 1);
            assert!(stems[0].path.windows(2).all(|w| w[1].z > w[0].z));
            let leaves: Vec<&Limb> = tree.limbs.iter().filter(|l| l.level == 1).collect();
            assert!(leaves.len() >= 4 && leaves.iter().all(|l| l.parent == Some(0)));
            assert!(leaves.iter().all(|l| l.path[0].z > height * 0.1 && l.path[0].z < height * 0.8), "a leaf at the foot or in the head");
            // the head: seeds in the middle, the outer seeds round them, the petals round those
            let built = build(&one).unwrap();
            let middle = *stems[0].path.last().unwrap();
            let far = |shade: u8| {
                let these = bricks_of(&built, &[shade]);
                assert!(these.len() > 20, "only {} bricks of colour {shade}", these.len());
                (these.iter().map(|p| p.distance(middle)).sum::<f64>() / these.len() as f64, these.iter().map(|p| p.distance(middle)).fold(0.0, f64::max))
            };
            let (heart, outer, petals) = (far(STAR), far(EXTRA), far(TOP));
            assert!(heart.0 < outer.0 && outer.0 < petals.0, "mean distances {heart:?}, {outer:?}, {petals:?}");
            assert!(heart.1 < head * 0.45 && petals.1 > head * 0.8 && petals.1 < head * 1.2, "{heart:?}, {petals:?}, head {head}");
            // it stands as tall as asked, about, and its head is the widest thing on it
            let (lo, hi) = tree.bounds();
            assert!(hi.z > height * 0.85 && hi.z < height * 1.1, "{} tall of {height}", hi.z);
            assert!(lo.z >= 0.0);
        }
        // a clump is that many flowers, each its own height, standing apart
        let clump = model(Kind::Sunflower, 2);
        let stems: Vec<&Limb> = clump.limbs.iter().filter(|l| l.level == 0).collect();
        assert_eq!(stems.len(), Tree::of(Kind::Sunflower).count as usize);
        let tops: Vec<f64> = stems.iter().map(|l| l.path.last().unwrap().z).collect();
        assert!(tops.windows(2).all(|w| (w[0] - w[1]).abs() > 0.5), "{tops:?}");
        assert!(stems.windows(2).all(|w| w[0].path[0].distance(w[1].path[0]) > 10.0));
    }

    #[test]
    fn a_tulip_is_a_cup_of_two_rings_of_petals_on_a_bare_stem_with_leaves_from_the_ground() {
        for seed in 1..4 {
            let one = Spec { tree: Tree { seed, count: 1, ..Tree::of(Kind::Tulip) }, ..spec(Kind::Tulip, seed) };
            let tree = Model::new(&one.tree, one.points[0], one.rise.unwrap(), one.radius);
            let (height, cup) = Tree::usual_size(Kind::Tulip);
            let stem = &tree.limbs[0];
            let top = stem.path.last().unwrap().z;
            // two or three leaves, all from the ground, none reaching the cup
            let leaves: Vec<&Limb> = tree.limbs.iter().filter(|l| l.level == 1).collect();
            assert!((2..=3).contains(&leaves.len()), "{} leaves", leaves.len());
            assert!(leaves.iter().all(|l| l.path[0].z < 1.0 && l.path.iter().all(|p| p.z < top)), "a leaf springs from the stem or reaches the cup");
            // and they arch away: a leaf's tip is further out than it is at half its length
            assert!(leaves.iter().all(|l| l.path.last().unwrap().truncate().length() > 2.0 * l.path[l.path.len() / 2].truncate().length() * 0.6));
            // the cup: both rings of petals, all above the stem, taller than wide
            let built = build(&one).unwrap();
            let (outer, inner) = (bricks_of(&built, &[TOP]), bricks_of(&built, &[EXTRA]));
            assert!(outer.len() > 80 && inner.len() > 30, "{} and {} bricks of petal", outer.len(), inner.len());
            let all: Vec<DVec3> = outer.iter().chain(&inner).copied().collect();
            let (lo, hi) = all.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            assert!(lo.z > top - 2.0, "petals at {} under the top of the stem at {top}", lo.z);
            let (tall, wide) = (hi.z - lo.z, (hi.x - lo.x).max(hi.y - lo.y));
            assert!(tall > wide * 1.05 && tall < wide * 1.6 && wide > cup * 1.6, "a cup {tall:.1} tall and {wide:.1} wide");
            // the stem is bare between the leaves' foot and the cup: no petal or leaf colour beside it half way up... and the whole is as tall as asked
            assert!((hi.z - height).abs() < 4.0, "{} tall of {height}", hi.z);
        }
        assert_eq!(model(Kind::Tulip, 1).limbs.iter().filter(|l| l.level == 0).count(), Tree::of(Kind::Tulip).count as usize);
    }

    #[test]
    fn a_boxwood_is_a_closed_mound_on_several_stems_and_a_rose_is_arching_canes_in_flower() {
        for seed in 1..4 {
            // the boxwood: as many stems as asked, apart at the ground
            let tree = model(Kind::Boxwood, seed);
            let (height, spread) = Tree::usual_size(Kind::Boxwood);
            let stems: Vec<&Limb> = tree.limbs.iter().filter(|l| l.level == 1).collect();
            assert_eq!(stems.len(), Tree::of(Kind::Boxwood).count as usize);
            assert!(stems.iter().all(|l| l.path[0].z == 0.0));
            assert!(stems.iter().any(|a| stems.iter().any(|b| a.path[0].distance(b.path[0]) > spread * 0.15)), "the stems are one trunk");
            let (lo, hi) = tree.bounds();
            assert!(hi.z > height * 0.85 && hi.z <= height + 2.0 && (hi.x - lo.x) > spread * 1.6 && (hi.x - lo.x) <= spread * 2.0 + 4.0, "{lo} to {hi}");
            // closed: from the side, above a third of its height, leaves nearly all the way across
            let leaf = |p: DVec3| tree.eval(p) < 0.0 && is_leaf(tree.shade(p));
            let (mut covered, mut all) = (0, 0);
            let mut z = height * 0.35;
            while z < height * 0.8 {
                let mut x = -spread * 0.6;
                while x < spread * 0.6 {
                    all += 1;
                    covered += (0..(spread as i32 * 2)).any(|y| leaf(DVec3::new(x, y as f64 - spread, z))) as usize;
                    x += 2.0;
                }
                z += 2.0;
            }
            assert!(covered as f64 > all as f64 * 0.93, "leaves cover {covered} of {all} of the middle of the bush");

        }
    }

    /// The box the bricks of some colour numbers fill.
    fn extent(built: &crate::spec::Built, wanted: &[u8]) -> (DVec3, DVec3) {
        bricks_of(built, wanted).iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
    }

    #[test]
    fn a_beech_keeps_its_stem_and_a_bare_tree_is_the_same_tree_in_twigs() {
        for seed in 1..4 {
            let tree = model(Kind::Beech, seed);
            let (height, spread) = Tree::usual_size(Kind::Beech);
            // the thickest limb is the stem carried on: it starts nearly upright and climbs most of the crown
            let leader = tree.limbs[1..].iter().filter(|l| l.level == 1).max_by(|a, b| a.radius[0].total_cmp(&b.radius[0])).unwrap();
            let first = (leader.path[1] - leader.path[0]).normalize();
            assert!(first.z > 0.9, "the leader leaves the trunk at {first}");
            assert!(leader.path.last().unwrap().z > height * 0.55, "the leader ends at {}", leader.path.last().unwrap().z);
            for (limb, into) in &tree.forks {
                assert!((into.iter().map(|r| r * r).sum::<f64>() - limb * limb).abs() < 1e-9 * limb * limb);
            }
            // taller than wide, where an oak is wider than tall; and slimmer in the trunk
            let (lo, hi) = tree.bounds();
            assert!((hi.x - lo.x).max(hi.y - lo.y) < hi.z * 0.85 && hi.z > height * 0.85 && (hi.x - lo.x) <= spread * 2.0 + 1e-6, "{lo} to {hi}");
            let oak = model(Kind::Oak, seed);
            let (olo, ohi) = oak.bounds();
            assert!((ohi.x - olo.x).max(ohi.y - olo.y) > ohi.z);
            assert!(tree.limbs[0].radius.last().unwrap() / height < oak.limbs[0].radius.last().unwrap() / ohi.z);
        }
        for kind in [Kind::Oak, Kind::Beech] {
            let leafy = spec(kind, 2);
            let bare = Spec { tree: Tree { extras: true, ..leafy.tree }, ..leafy.clone() };
            let (in_leaf, stripped) = (model(kind, 2), Model::new(&bare.tree, bare.points[0], bare.rise.unwrap(), bare.radius));
            // the same tree: limb for limb
            assert_eq!(in_leaf.limbs.len(), stripped.limbs.len());
            assert!(in_leaf.limbs.iter().zip(&stripped.limbs).all(|(a, b)| a.path == b.path && a.radius == b.radius), "{kind:?}: bare, it is another tree");
            // no leaf on it, and twigs where the leaves were: thin ends reaching as far up and out as the leaves did
            let built = build(&bare).unwrap();
            assert!(!built.shades.iter().any(|s| is_leaf(*s)), "{kind:?}: a bare tree has leaves");
            let ((lo, hi), (was_lo, was_hi)) = (stripped.bounds(), in_leaf.bounds());
            assert!(hi.z > was_hi.z * 0.9 && (hi.x - lo.x) > (was_hi.x - was_lo.x) * 0.8, "{kind:?}: bare it is {lo} to {hi}, in leaf {was_lo} to {was_hi}");
            // many more ends than branches: each branch end forks at least twice more
            let tips = stripped.parts.iter().filter(|p| matches!(p.shape, Shape::Cone { ra, .. } if ra < 0.96) && p.shade == LIMB).count();
            let ends = in_leaf.limbs.len();
            assert!(tips > ends * 3, "{kind:?}: {tips} twigs on {ends} limbs");
        }
        // a beech has more and finer twigs than an oak
        let twigs = |kind: Kind| { let s = spec(kind, 2); Model::new(&Tree { extras: true, ..s.tree }, s.points[0], s.rise.unwrap(), s.radius).parts.iter().filter(|p| matches!(p.shape, Shape::Cone { ra, .. } if ra < 0.96)).count() };
        assert!(twigs(Kind::Beech) > twigs(Kind::Oak));
    }

    #[test]
    fn a_hedge_is_as_long_as_asked_closed_and_level_and_has_berries_when_asked() {
        let (height, spread) = Tree::usual_size(Kind::Hedge);
        let grown = |long: f64, extras: bool| {
            let s = Spec { stretch: DVec3::new(long / spread, 1.0, 1.0), tree: Tree { extras, ..Tree::of(Kind::Hedge) }, ..spec(Kind::Hedge, 1) };
            (build(&s).unwrap(), Model::with_length(&s.tree, s.points[0], height, spread, long))
        };
        let ((short, short_model), (long, long_model)) = (grown(30.0, false), grown(90.0, true));
        for (model, half) in [(&short_model, 30.0), (&long_model, 90.0)] {
            let (lo, hi) = model.bounds();
            assert!((hi.x - lo.x) > 2.0 * half - 6.0 && (hi.x - lo.x) <= 2.0 * half + 4.0, "{} long of {}", hi.x - lo.x, 2.0 * half);
            assert!((hi.y - lo.y) > 2.0 * spread - 8.0 && (hi.y - lo.y) <= 2.0 * spread + 4.0 && hi.z > height - 4.0 && hi.z <= height + 2.0, "{lo} to {hi}");
            // closed and level: from above, leaves all along its middle, at nearly one height
            let mut tops = Vec::new();
            let mut x = -half + 8.0;
            while x < half - 8.0 {
                let top = (0..(height as i32 + 4)).rev().find(|z| model.eval(DVec3::new(x + 0.5, 0.5, *z as f64 + 0.5)) < 0.0);
                tops.push(top.expect("a gap in the top of the hedge") as f64);
                x += 3.0;
            }
            let (least, most) = (tops.iter().cloned().fold(f64::MAX, f64::min), tops.iter().cloned().fold(0.0, f64::max));
            assert!(most - least <= height * 0.25 && least > height * 0.7, "the top runs from {least} to {most}");
        }
        // the same plant, longer: more stems, the same thickness
        assert!(long_model.limbs.len() == short_model.limbs.len() || long_model.limbs.len() > 3);
        assert!(long.bricks.len() > short.bricks.len() * 2);
        // berries when asked for, on the outside, and none otherwise
        assert!(!short.shades.contains(&EXTRA));
        let berries = bricks_of(&long, &[EXTRA]);
        assert!(berries.len() > 40, "{} bricks of berry", berries.len());
        assert!(berries.iter().all(|b| b.y.abs() > spread * 0.45 || b.z > height * 0.6 || b.x.abs() > 80.0), "a berry inside the hedge");
        // and a boxwood has them the same way
        let bush = |extras: bool| build(&Spec { tree: Tree { extras, ..Tree::of(Kind::Boxwood) }, ..spec(Kind::Boxwood, 1) }).unwrap();
        assert!(bush(true).shades.iter().filter(|s| **s == EXTRA).count() > 30 && !bush(false).shades.contains(&EXTRA));
    }

    #[test]
    fn a_peony_opens_from_a_ball_to_a_bowl_of_many_petals() {
        let one = |open: i32| Spec { tree: Tree { count: 1, bend: open, ..Tree::of(Kind::Peony) }, ..spec(Kind::Peony, 3) };
        let (height, spread) = Tree::usual_size(Kind::Peony);
        let (bud, blown) = (build(&one(0)).unwrap(), build(&one(100)).unwrap());
        let size = |built: &crate::spec::Built| {
            let (lo, hi) = extent(built, &[TOP, EXTRA]);
            ((hi.x - lo.x).max(hi.y - lo.y), hi.z - lo.z)
        };
        let ((bud_wide, bud_tall), (wide, tall)) = (size(&bud), size(&blown));
        // a bud is a ball, as tall as wide and well under the full bloom's width; full blown it is a bowl, wider than tall and as wide as asked
        assert!(bud_wide < wide * 0.75 && (bud_tall / bud_wide - 1.0).abs() < 0.35, "a bud {bud_wide:.1} wide and {bud_tall:.1} tall");
        assert!(wide > tall * 1.3 && wide > spread * 1.5 && wide < spread * 2.5, "full blown {wide:.1} wide and {tall:.1} tall, for {}", spread * 2.0);
        // both rings of petals, inner inside outer, and stamens at the heart of the open one
        let middle = |built: &crate::spec::Built, shade: u8| { let these = bricks_of(built, &[shade]); let (lo, hi) = extent(built, &[TOP, EXTRA]); let c = (lo + hi) / 2.0; these.iter().map(|p| (*p - c).truncate().length()).sum::<f64>() / these.len().max(1) as f64 };
        assert!(middle(&blown, EXTRA) < middle(&blown, TOP) && middle(&blown, STAR) < middle(&blown, EXTRA), "inner {} outer {} heart {}", middle(&blown, EXTRA), middle(&blown, TOP), middle(&blown, STAR));
        assert!(bricks_of(&blown, &[TOP]).len() > 150 && bricks_of(&blown, &[EXTRA]).len() > 60);
        // many petals: more with fullness
        let petals = |full: i32| Model::new(&Tree { count: 1, fullness: full, ..Tree::of(Kind::Peony) }, DVec3::splat(0.5), height, spread).parts.iter().filter(|p| p.shade == TOP || p.shade == EXTRA).count() / 6;
        assert!(petals(0) >= 30 && petals(100) > petals(0) + 15, "{} and {} petals", petals(0), petals(100));
        // a clump is that many stems, each with leaves, and stands as tall as asked
        let clump = model(Kind::Peony, 1);
        assert_eq!(clump.limbs.iter().filter(|l| l.level == 0).count(), Tree::of(Kind::Peony).count as usize);
        assert!(clump.limbs.iter().filter(|l| l.level == 1).count() >= 9);
        assert!(clump.bounds().1.z > height * 0.85 && clump.bounds().1.z < height * 1.15);
        // the rose bush that was: read as a peony of the same seed
        let old = crate::report::parse_spec("form = tree\ntree = rose,7,60,9,50,1\n").unwrap();
        assert_eq!(old.tree, Tree { seed: 7, ..Tree::of(Kind::Peony) });
    }

    #[test]
    fn a_hydrangea_has_domed_heads_of_florets_petunias_trail_and_reeds_stand_in_a_bank() {
        // hydrangea: leaves, and heads of two colours of floret with middles, high on the bush
        let (height, spread) = Tree::usual_size(Kind::Hydrangea);
        let bush = build(&spec(Kind::Hydrangea, 1)).unwrap();
        let (one, other, middles, leaves) = (bricks_of(&bush, &[TOP]), bricks_of(&bush, &[EXTRA]), bricks_of(&bush, &[STAR]), bricks_of(&bush, &[LEAF, LEAF_DARK, LEAF_LIGHT]));
        assert!(one.len() > 150 && other.len() > 150 && middles.len() > 40 && leaves.len() > 2000, "{} {} {} {}", one.len(), other.len(), middles.len(), leaves.len());
        let mean = |these: &[DVec3]| these.iter().map(|p| p.z).sum::<f64>() / these.len() as f64;
        assert!(mean(&one) > mean(&leaves) && mean(&one) > height * 0.45);
        // a head is a dome of separate florets and not a ball: its surface is in pieces of both colours, side by side
        let near = one.iter().filter(|a| other.iter().any(|b| a.distance(*b) < 3.0)).count();
        assert!(near > one.len() / 3, "only {near} of {} bricks of one colour are beside the other", one.len());
        // more heads asked for, more flower
        let heads = |count: i32| bricks_of(&build(&Spec { tree: Tree { count, ..Tree::of(Kind::Hydrangea) }, ..spec(Kind::Hydrangea, 1) }).unwrap(), &[TOP, EXTRA, STAR]).len();
        assert!(heads(18) > heads(4) * 2);
        let (lo, hi) = model(Kind::Hydrangea, 1).bounds();
        assert!(hi.z <= height + 3.0 && hi.z > height * 0.8 && (hi.x - lo.x) <= spread * 2.0 + 8.0 && (hi.x - lo.x) > spread * 1.5, "{lo} to {hi}");

        // petunias: low and wide, a throat in every flower, and trailing stems when asked
        let (height, spread) = Tree::usual_size(Kind::Petunia);
        let grown = |trail: i32| { let s = Spec { tree: Tree { bend: trail, ..Tree::of(Kind::Petunia) }, ..spec(Kind::Petunia, 1) }; (build(&s).unwrap(), Model::new(&s.tree, s.points[0], height, spread)) };
        let ((mound, mound_model), (trailing, trailing_model)) = (grown(0), grown(100));
        for model in [&mound_model, &trailing_model] {
            let (lo, hi) = model.bounds();
            assert!(hi.z <= height + 4.0 && (hi.x - lo.x) > hi.z * 1.8 && (hi.x - lo.x) <= spread * 2.0 + 6.0, "{lo} to {hi}");
        }
        assert!(trailing_model.limbs.len() > mound_model.limbs.len() + 5, "{} stems trailing, {} not", trailing_model.limbs.len(), mound_model.limbs.len());
        for built in [&mound, &trailing] {
            let (lobes, throats) = (bricks_of(built, &[TOP]), bricks_of(built, &[EXTRA]));
            assert!(lobes.len() > 300 && throats.len() >= Tree::of(Kind::Petunia).count as usize / 2, "{} of lobe, {} of throat", lobes.len(), throats.len());
            // a throat is in the middle of its flower: every one has lobes close round it
            assert!(throats.iter().all(|t| lobes.iter().filter(|l| l.distance(*t) < 5.0).count() >= 3));
        }
        // trailing, it lies lower: the mound is smaller and the rest is along the ground
        let high = |built: &crate::spec::Built| { let these = bricks_of(built, &[LEAF, LEAF_DARK, LEAF_LIGHT]); these.iter().map(|p| p.z).sum::<f64>() / these.len() as f64 };
        assert!(high(&trailing) < high(&mound), "{} trailing, {} not", high(&trailing), high(&mound));

        // reeds: as many unbranched stems as asked, in a bank as long as asked, a brown head and a spike on each
        let (height, spread) = Tree::usual_size(Kind::Reeds);
        let stand = |long: f64, extras: bool| { let s = Spec { stretch: DVec3::new(long / spread, 1.0, 1.0), tree: Tree { extras, ..Tree::of(Kind::Reeds) }, ..spec(Kind::Reeds, 1) }; (build(&s).unwrap(), Model::with_length(&s.tree, s.points[0], height, spread, long)) };
        let (bulrushes, bank) = stand(60.0, false);
        let stems: Vec<&Limb> = bank.limbs.iter().filter(|l| l.level == 0).collect();
        assert_eq!(stems.len(), Tree::of(Kind::Reeds).count as usize);
        assert!(stems.iter().all(|l| l.path.windows(2).all(|w| w[1].z > w[0].z) && l.path[0].z == 0.0));
        let feet = stems.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), l| (lo.min(l.path[0]), hi.max(l.path[0])));
        assert!((feet.1.x - feet.0.x) > 80.0 && (feet.1.y - feet.0.y) < 2.0 * spread && (feet.1.y - feet.0.y) > spread * 0.8, "the stems stand from {} to {}", feet.0, feet.1);
        let (heads, spikes, leaves) = (bricks_of(&bulrushes, &[EXTRA]), bricks_of(&bulrushes, &[STAR]), bricks_of(&bulrushes, &[LEAF, LEAF_DARK, LEAF_LIGHT, TOP]));
        assert!(heads.len() >= stems.len() * 3 && spikes.len() >= stems.len() && leaves.len() > 300, "{} of head, {} of spike, {} of leaf", heads.len(), spikes.len(), leaves.len());
        let mean = |these: &[DVec3]| these.iter().map(|p| p.z).sum::<f64>() / these.len() as f64;
        assert!(mean(&heads) > height * 0.5 && mean(&spikes) > mean(&heads) && mean(&leaves) < mean(&heads));
        // the leaves spring low on the stems and flop outwards at their tips
        let blades: Vec<&Limb> = bank.limbs.iter().filter(|l| l.level == 1).collect();
        assert!(blades.len() >= stems.len() && blades.iter().all(|l| l.path[0].z < height * 0.3));
        // plumes in place of the brown heads when asked for
        let (plumed, _) = stand(60.0, true);
        assert!(!plumed.shades.contains(&EXTRA) && bricks_of(&plumed, &[TOP]).len() > stems.len() * 3);
        assert!(bank.bounds().1.z > height * 0.8 && bank.bounds().1.z <= height + 2.0);
    }

    /// A hedge or grass through `points`, as a spec.
    fn along(kind: Kind, points: &[(f64, f64)], spread: f64, closed: bool) -> Spec {
        Spec { points: points.iter().map(|(x, y)| DVec3::new(x + 0.5, y + 0.5, 0.5)).collect(), radius: spread, closed, stretch: DVec3::ONE, ..spec(kind, 1) }
    }

    /// The tree a spec describes, as its solid, by way of a build: how tall it stands over a place on the ground.
    fn top_at(built: &crate::spec::Built, x: f64, y: f64) -> Option<i32> {
        built.bricks.iter().filter(|b| (b.min[0] as f64) <= x && x < (b.min[0] + b.ext[0]) as f64 && (b.min[1] as f64) <= y && y < (b.min[1] + b.ext[1]) as f64).map(|b| b.min[2] + b.ext[2]).max()
    }

    #[test]
    fn a_hedge_follows_its_line_round_bends_and_its_thickness_is_its_own() {
        let (height, _) = Tree::usual_size(Kind::Hedge);
        // an L: along x, then up y
        let bent = build(&along(Kind::Hedge, &[(-60.0, 0.0), (0.0, 0.0), (55.0, 10.0), (60.0, 70.0)], 10.0, false)).unwrap();
        // it stands, to nearly its full height, at each of its points and between them; and not across the corner it goes round
        for (x, y) in [(-55.0, 0.0), (-30.0, 0.0), (0.0, 0.0), (30.0, 3.0), (60.0, 40.0), (60.0, 64.0)] {
            let top = top_at(&bent, x, y).unwrap_or_else(|| panic!("no hedge at {x}, {y}"));
            assert!(top as f64 > height * 0.75 && top as f64 <= height + 2.0, "{top} tall at {x}, {y}");
        }
        assert!(top_at(&bent, -20.0, 50.0).is_none() && top_at(&bent, 20.0, 45.0).is_none(), "the hedge cuts the corner");
        // as thick on the bend's far leg as on its near one: about twice its spread, measured across each
        let across_x = (-25..25).filter(|y| top_at(&bent, -30.5, *y as f64 + 0.5).is_some()).count();
        let across_y = (35..85).filter(|x| top_at(&bent, *x as f64 + 0.5, 45.5).is_some()).count();
        assert!((16..=22).contains(&across_x) && (16..=22).contains(&across_y), "{across_x} and {across_y} across");

        // Thickness and length are apart. Thicker, it is no longer; made thin again it is the hedge it was.
        let line = [(-80.0, 0.0), (0.0, 0.0), (80.0, 0.0)];
        let size = |spread: f64| {
            let built = build(&along(Kind::Hedge, &line, spread, false)).unwrap();
            let (lo, hi) = extent(&built, &[TRUNK, LEAF, LEAF_DARK, LEAF_LIGHT, LIMB]);
            (hi.x - lo.x, hi.y - lo.y, built.bricks)
        };
        let (thin, thick, again) = (size(10.0), size(40.0), size(10.0));
        assert!((thin.0 - 160.0).abs() < 8.0 && (thick.0 - 160.0).abs() < 8.0, "{} long thin, {} long thick, on a line of 160", thin.0, thick.0);
        assert!(thin.1 < 22.0 && thick.1 > 70.0 && thick.1 < 82.0, "{} and {} thick", thin.1, thick.1);
        assert_eq!(thin.2, again.2);
        // laid along a straight line it is the hedge a length alone gave
        let straight = Model::laid(&Tree::of(Kind::Hedge), DVec3::splat(0.5), height, 12.0, &Lay::straight(40.0));
        let old = Model::with_length(&Tree::of(Kind::Hedge), DVec3::splat(0.5), height, 12.0, 40.0);
        assert_eq!(straight.parts.len(), old.parts.len());
        assert!(straight.parts.iter().zip(&old.parts).all(|(a, b)| a.shape.bounds().0.distance(b.shape.bounds().0) < 1e-6));

        // a closed line is a ring with no ends: hedge all the way round, and none in the middle
        let ring = build(&along(Kind::Hedge, &[(-50.0, -40.0), (50.0, -40.0), (50.0, 40.0), (-50.0, 40.0)], 9.0, true)).unwrap();
        let open = build(&along(Kind::Hedge, &[(-50.0, -40.0), (50.0, -40.0), (50.0, 40.0), (-50.0, 40.0)], 9.0, false)).unwrap();
        assert!(top_at(&ring, 0.0, 0.0).is_none());
        // (the side between the last point and the first is there only when it is closed)
        let west = |built: &crate::spec::Built| (-20..20).filter(|y| (-66..-40).any(|x| top_at(built, x as f64 + 0.5, *y as f64 + 0.5).is_some())).count();
        assert!(west(&ring) == 40 && west(&open) < 10, "{} of 40 closed, {} open", west(&ring), west(&open));

        // reeds stand along a line the same way
        let bank = Model::laid(&Tree::of(Kind::Reeds), DVec3::splat(0.5), 84.0, 8.0, &Lay { run: vec![DVec3::ZERO, DVec3::new(60.0, 0.0, 0.0), DVec3::new(60.0, 60.0, 0.0)], closed: false, sharp: false, water: None, reeds: 0.0, levels: 1, mounds: vec![] });
        let feet: Vec<DVec3> = bank.limbs.iter().filter(|l| l.level == 0).map(|l| l.path[0]).collect();
        assert!(feet.iter().any(|f| f.y > 40.0 && f.x > 50.0) && feet.iter().any(|f| f.x < 20.0 && f.y.abs() < 9.0) && !feet.iter().any(|f| f.x < 40.0 && f.y > 20.0), "the reeds do not follow the bank");
    }

    #[test]
    fn grass_is_a_skin_of_soil_and_blades_that_added_points_raise() {
        let (height, spread) = Tree::usual_size(Kind::Grass);
        let strip = along(Kind::Grass, &[(-30.0, 0.0), (0.0, 0.0), (30.0, 0.0)], spread, false);
        let flat = build(&strip).unwrap();
        // thin: nowhere taller than its blades and the soil under them
        let (lo, hi) = extent(&flat, &[TRUNK, LEAF, LEAF_DARK, LEAF_LIGHT]);
        assert!(hi.z <= height + 3.0 && lo.z >= 0.0, "{} to {} high", lo.z, hi.z);
        // soil under all of it, with no gap: over the whole patch the ground is covered, and it is the soil's colour at the bottom
        let (mut covered, mut all) = (0, 0);
        for x in (-28..28).step_by(3) {
            for y in (-18..18).step_by(3) {
                all += 1;
                covered += top_at(&flat, x as f64 + 0.5, y as f64 + 0.5).is_some() as usize;
            }
        }
        assert_eq!(covered, all, "bare ground showing through the grass");
        let soil = bricks_of(&flat, &[TRUNK]);
        assert!(soil.len() > 100 && soil.iter().all(|b| b.z < 3.0), "{} bricks of soil", soil.len());
        // three greens, and blades of different heights
        for shade in [LEAF, LEAF_DARK, LEAF_LIGHT] {
            assert!(bricks_of(&flat, &[shade]).len() > 200, "few bricks of green {shade}");
        }
        let tops: std::collections::HashSet<i32> = (-25..25).filter_map(|x| top_at(&flat, x as f64 + 0.5, 0.5)).collect();
        assert!(tops.len() >= 4, "the blades are all one height: {tops:?}");
        // thicker growth is more blades
        let blades = |full: i32| build(&Spec { tree: Tree { fullness: full, ..strip.tree }, ..strip.clone() }).unwrap().bricks.len();
        assert!(blades(100) > blades(0) * 3 / 2);

        // a point added and lifted raises the ground round it, by about as much, and leaves the far end alone
        let mound = build(&Spec { pulls: vec![(DVec3::new(10.5, 2.5, 0.5), DVec3::new(16.0, 0.0, 12.0))], ..strip.clone() }).unwrap();
        let (at_it, far) = (top_at(&mound, 10.5, 2.5).unwrap(), top_at(&mound, -25.5, 2.5).unwrap());
        assert!(at_it as f64 > 12.0 && at_it as f64 <= 12.0 + height + 3.0 && far as f64 <= height + 3.0, "{at_it} high at the mound, {far} far from it");
        // a surface: under the mound there is nothing
        assert!(!mound.bricks.iter().any(|b| b.min[0] <= 10 && 10 < b.min[0] + b.ext[0] && b.min[1] <= 2 && 2 < b.min[1] + b.ext[1] && b.min[2] <= 4 && 4 < b.min[2] + b.ext[2]), "the mound is solid");
        // the soil is its skin still: soil bricks up at the mound's height
        assert!(bricks_of(&mound, &[TRUNK]).iter().any(|b| b.z > 9.0));
        // pushed down, the ground stays on the floor it stands on
        let dip = build(&Spec { pulls: vec![(DVec3::new(10.5, 2.5, 0.5), DVec3::new(16.0, 0.0, -12.0))], ..strip.clone() }).unwrap();
        // (pushed down in flat ground it digs below the floor, and the soil goes down with it)
        let lowest = dip.bricks.iter().map(|b| b.min[2]).min().unwrap();
        assert!(lowest <= -10 && lowest >= -15, "a hollow dug 12 deep reaches {lowest}");
        // (water stands in it, 3 below the lawn)
        assert!(top_at(&dip, 10.5, 2.5).unwrap() == -3 && top_at(&dip, -25.5, 2.5).unwrap() >= 1);

        // The ground as a brush shapes it. A wide push is a hill, not a
        // spike: half way out it is half as high, and at its edge it is gone.
        let hill = Sculpt { at: DVec3::ZERO, range: 30.0, amount: 10.0, flatten: false };
        let at = |touches: &[Sculpt], x: f64| ground(touches, DVec3::new(x, 0.0, 0.0));
        assert!((at(&[hill], 0.0) - 10.0).abs() < 1e-9 && (at(&[hill], 15.0) - 5.0).abs() < 1e-9 && at(&[hill], 30.0) == 0.0 && at(&[hill], 29.0) < 0.1);
        // level at its top and at its edge: no crease where it meets the flat
        assert!(at(&[hill], 0.0) - at(&[hill], 1.0) < 0.05 && at(&[hill], 29.0) - at(&[hill], 30.0) < 0.05);
        // two pushes add, and a push down takes a hollow out of raised ground and no more
        let beside = Sculpt { at: DVec3::new(20.0, 0.0, 0.0), ..hill };
        assert!(at(&[hill, beside], 10.0) > at(&[hill], 10.0) + 4.0);
        let hollow = Sculpt { at: DVec3::ZERO, range: 10.0, amount: -4.0, flatten: false };
        assert!((at(&[hill, hollow], 0.0) - 6.0).abs() < 1e-9 && at(&[hollow], 0.0) == -4.0);
        // flattening brings the ground to its level within half its range, and leaves it alone beyond it
        let terrace = Sculpt { at: DVec3::new(12.0, 0.0, 0.0), range: 12.0, amount: 3.0, flatten: true };
        for x in [7.0, 12.0, 17.0] {
            assert!((at(&[hill, terrace], x) - 3.0).abs() < 1e-9, "{} at {x}", at(&[hill, terrace], x));
        }
        assert_eq!(at(&[hill, terrace], -5.0), at(&[hill], -5.0));
        // strokes are in order: a hill pushed up after the flattening stands on the terrace
        assert!(at(&[terrace, hill], 12.0) > 3.0);
        // and built: a flattened top is level, where the hill it was cut from was not
        let top = |pulls: Vec<(DVec3, DVec3)>| { let built = build(&Spec { pulls, ..strip.clone() }).unwrap(); bricks_of(&built, &[TRUNK]).iter().filter(|b| (b.x - 0.5).abs() < 6.0 && b.y.abs() < 6.0).map(|b| b.z).fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(z), hi.max(z))) };
        let (raised, levelled) = (top(vec![(DVec3::new(8.5, 0.5, 0.5), DVec3::new(24.0, 0.0, 12.0))]), top(vec![(DVec3::new(8.5, 0.5, 0.5), DVec3::new(24.0, 0.0, 12.0)), (DVec3::new(0.5, 0.5, 0.5), DVec3::new(20.0, 16.0, 6.0))]));
        assert!(raised.1 - raised.0 > 2.5 && levelled.1 - levelled.0 <= 2.0 && (levelled.0 - 6.0).abs() < 2.5, "raised {raised:?}, flattened {levelled:?}");

        // a closed line is an outline, filled; open, it is a strip along the line
        let corners = [(-40.0, -40.0), (40.0, -40.0), (40.0, 40.0), (-40.0, 40.0)];
        let (lawn, strips) = (build(&along(Kind::Grass, &corners, 6.0, true)).unwrap(), build(&along(Kind::Grass, &corners, 6.0, false)).unwrap());
        assert!(top_at(&lawn, 0.5, 0.5).is_some() && top_at(&strips, 0.5, 0.5).is_none());
    }

    #[test]
    fn a_trunk_is_eight_sided_on_the_lattice_and_built_of_tall_bricks() {
        // an oak's trunk between its roots and its fork: every layer the same, so few bricks, each many cells tall
        let spec = spec(Kind::Oak, 2);
        let (built, tree) = (build(&spec).unwrap(), model(Kind::Oak, 2));
        let fork = tree.limbs[0].path.last().unwrap().z;
        let (low, high) = ((fork * 0.55).ceil() as i32, (fork * 0.9).floor() as i32);
        let trunk: Vec<&CellBrick> = built.bricks.iter().filter(|b| b.min[2] < high && b.min[2] + b.ext[2] > low && b.min[0].abs() < 12 && b.min[1].abs() < 12).collect();
        assert!(trunk.len() <= 40 && trunk.iter().all(|b| b.ext[2] >= (high - low).min(4)), "{} bricks in the trunk from {low} to {high}, the shortest {:?} tall", trunk.len(), trunk.iter().map(|b| b.ext[2]).min());
        // its section there: faces on lattice planes (as far each way along x as along y, a whole number of cells), and the corners cut
        let z = (low + high) as f64 / 2.0 + 0.25;
        let reach = |way: DVec3| surface(&tree, DVec3::new(0.5, 0.5, z), way);
        let (east, north, corner) = (reach(DVec3::X), reach(DVec3::Y), reach(DVec3::new(1.0, 1.0, 0.0).normalize()));
        assert!((east - north).abs() < 1e-6 && ((east + 0.5).fract() - 0.5).abs() > 0.49, "faces {east} and {north} from the middle of its cell");
        assert!(corner < east * 1.2, "the corner is {corner} out and a face {east}: not cut");
    }

    #[test]
    fn each_new_tree_has_its_own_habit() {
        // a willow weeps: its lowest leaves hang far below its lowest branch end, near the ground
        let willow = build(&spec(Kind::Willow, 1)).unwrap();
        let (height, spread) = Tree::usual_size(Kind::Willow);
        let leaves = bricks_of(&willow, &[LEAF, LEAF_DARK, LEAF_LIGHT]);
        let lowest = leaves.iter().map(|p| p.z).fold(f64::MAX, f64::min);
        let ends = model(Kind::Willow, 1).limbs[1..].iter().map(|l| l.path.last().unwrap().z).fold(f64::MAX, f64::min);
        assert!(lowest < height * 0.2 && lowest < ends - height * 0.15, "a willow's leaves reach down to {lowest}; its lowest branch ends at {ends}");
        // and they hang in strands: bricks of leaf many cells tall and one or two wide
        assert!(willow.bricks.iter().zip(&willow.shades).filter(|(b, s)| is_leaf(**s) && b.ext[2] >= 6 && b.ext[0] <= 2 && b.ext[1] <= 2).count() > 60);
        assert!(extent(&willow, &[LEAF, LEAF_LIGHT]).1.x <= spread + 1.0);
        // far more of them than an oak has bricks of that shape by chance
        let strands = |built: &crate::spec::Built| built.bricks.iter().zip(&built.shades).filter(|(b, s)| is_leaf(**s) && b.ext[2] >= 6 && b.ext[0] <= 2 && b.ext[1] <= 2).count();
        let oak = build(&spec(Kind::Oak, 1)).unwrap();
        assert!(strands(&willow) > strands(&oak) * 2, "{} on a willow, {} on an oak", strands(&willow), strands(&oak));

        // a birch: slender, its stem carried on, its bark marked
        let birch = model(Kind::Birch, 1);
        let (height, _) = Tree::usual_size(Kind::Birch);
        assert!(birch.limbs[0].radius.last().unwrap() / height < 0.03);
        let (lo, hi) = birch.bounds();
        assert!((hi.x - lo.x).max(hi.y - lo.y) < hi.z * 0.62, "a birch {lo} to {hi}");
        let built = build(&spec(Kind::Birch, 1)).unwrap();
        let (marks, bark) = (bricks_of(&built, &[EXTRA]).len(), bricks_of(&built, &[TRUNK]).len());
        assert!(marks > 5 && bark > 20, "{marks} marks on {bark} bricks of bark");

        // a Scots pine: bare for the lower half, and its needles in flat plates
        let scots = build(&spec(Kind::ScotsPine, 1)).unwrap();
        let (height, _) = Tree::usual_size(Kind::ScotsPine);
        let needles = bricks_of(&scots, &[LEAF, LEAF_DARK, LEAF_LIGHT]);
        assert!(needles.iter().all(|p| p.z > height * 0.4), "needles at {}", needles.iter().map(|p| p.z).fold(f64::MAX, f64::min));
        let tufts = model(Kind::ScotsPine, 1);
        let flat = tufts.parts.iter().filter_map(|p| match p.shape { Shape::Blob { inv, leafy: true, .. } if 1.0 / inv.x > 4.0 => Some(inv.x / inv.z), _ => None }).fold(0.0, f64::max);
        assert!(flat < 0.5, "a plate of needles is {flat} as high as wide");

        // a date palm: a thicker trunk than a coconut's, upright, more fronds, set all round
        let (date, coconut) = (model(Kind::DatePalm, 1), model(Kind::Palm, 1));
        assert!(date.limbs[0].radius[3] > coconut.limbs[0].radius[3] * 1.3);
        let top = |m: &Model| (*m.limbs[0].path.last().unwrap() - m.limbs[0].path[0]).truncate().length();
        assert!(top(&date) < top(&coconut) * 0.5);
        assert_eq!(date.limbs.len() - 1, Tree::of(Kind::DatePalm).count as usize);
        // straighter: a frond's tip is nearly as far from its root as it is long
        let straight = |m: &Model| m.limbs[1..].iter().map(|f| f.path[0].distance(*f.path.last().unwrap()) / f.path.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>()).sum::<f64>() / (m.limbs.len() - 1) as f64;
        assert!(straight(&date) > 0.97 && straight(&date) > straight(&coconut) + 0.04, "{} and {}", straight(&date), straight(&coconut));
        let dated = build(&Spec { tree: Tree { extras: true, ..Tree::of(Kind::DatePalm) }, ..spec(Kind::DatePalm, 1) }).unwrap();
        assert!(bricks_of(&dated, &[EXTRA]).len() > 20);
    }

    #[test]
    fn the_new_bushes_and_flowers_have_their_parts() {
        // a cypress: many times as tall as wide, closed, pointed
        let cypress = model(Kind::Cypress, 1);
        let (height, spread) = Tree::usual_size(Kind::Cypress);
        let (lo, hi) = cypress.bounds();
        assert!(hi.z > height * 0.9 && (hi.x - lo.x) <= spread * 2.0 + 6.0 && hi.z > (hi.x - lo.x) * 3.5, "{lo} to {hi}");
        assert!(cypress.eval(DVec3::new(0.5 + spread * 0.6, 0.5, height * 0.3)) < 0.0 && cypress.eval(DVec3::new(0.5 + spread * 0.6, 0.5, height * 0.95)) > 0.0, "not a spire");

        // a yucca: as many blades as asked, the first upright and the last lying out; in flower, bells high over them
        let yucca = model(Kind::Yucca, 1);
        let blades: Vec<&Limb> = yucca.limbs.iter().filter(|l| l.level == 1).collect();
        assert_eq!(blades.len(), Tree::of(Kind::Yucca).count as usize);
        let climb = |l: &Limb| (l.path.last().unwrap().z - l.path[0].z) / l.path[0].distance(*l.path.last().unwrap());
        assert!(climb(blades[0]) > 0.9 && climb(blades[blades.len() - 1]) < 0.2, "{} and {}", climb(blades[0]), climb(blades[blades.len() - 1]));
        let flowering = build(&Spec { tree: Tree { extras: true, ..Tree::of(Kind::Yucca) }, ..spec(Kind::Yucca, 1) }).unwrap();
        let (bells, leaves) = (bricks_of(&flowering, &[TOP]), bricks_of(&flowering, &[LEAF, LEAF_DARK, LEAF_LIGHT]));
        let mean = |these: &[DVec3]| these.iter().map(|p| p.z).sum::<f64>() / these.len() as f64;
        assert!(bells.len() > 30 && mean(&bells) > mean(&leaves) * 1.8);

        // a daisy: a yellow middle in white rays, further out; a daffodil: a trumpet standing out in front of its petals, with a hollow down it
        let one = |kind: Kind| Spec { tree: Tree { count: 1, ..Tree::of(kind) }, ..spec(kind, 2) };
        let daisy = build(&one(Kind::Daisy)).unwrap();
        let (rays, disc) = (bricks_of(&daisy, &[TOP]), bricks_of(&daisy, &[EXTRA]));
        let middle = disc.iter().fold(DVec3::ZERO, |a, p| a + *p) / disc.len() as f64;
        let far = |these: &[DVec3]| these.iter().map(|p| p.distance(middle)).sum::<f64>() / these.len() as f64;
        assert!(rays.len() > 30 && disc.len() >= 4 && far(&rays) > far(&disc) * 1.8, "{} rays, {} of disc", rays.len(), disc.len());
        let daffodil = Model::new(&one(Kind::Daffodil).tree, DVec3::splat(0.5), 46.0, 6.0);
        let heart = daffodil.parts.iter().find(|p| p.shade == STAR).map(|p| p.shape.bounds()).map(|(a, b)| (a + b) / 2.0).unwrap();
        let trumpet: Vec<DVec3> = daffodil.parts.iter().filter(|p| p.shade == EXTRA).map(|p| { let (a, b) = p.shape.bounds(); (a + b) / 2.0 }).collect();
        let mouth = trumpet.iter().fold(DVec3::ZERO, |a, p| a + *p) / trumpet.len() as f64;
        // a tube: its ribs stand round its axis and none is on it; it is as long as the petals are
        let axis = (mouth - heart).normalize();
        assert!(trumpet.len() == 20 && trumpet.iter().all(|p| ((*p - heart) - axis * (*p - heart).dot(axis)).length() > 1.5), "the trumpet is not a tube");
        assert!((mouth - heart).length() > 2.0 && axis.z.abs() < 0.5, "the trumpet looks {axis}");
        assert!(daffodil.eval(heart + axis * 4.5) > 0.0, "the trumpet's mouth is stopped");

        // a bed: as many flowers as asked, of more than one kind and height, spread over the bed
        let bed = model(Kind::Bed, 1);
        let (_, spread) = Tree::usual_size(Kind::Bed);
        let stems: Vec<&Limb> = bed.limbs.iter().filter(|l| l.level == 0 && l.path.len() > 2).collect();
        assert_eq!(stems.len(), Tree::of(Kind::Bed).count as usize);
        let tops: std::collections::BTreeSet<i32> = stems.iter().map(|l| l.path.last().unwrap().z as i32).collect();
        assert!(tops.len() >= 5 && stems.iter().any(|l| l.path[0].truncate().length() > spread * 0.6) && stems.iter().all(|l| l.path[0].truncate().length() < spread + 2.0), "{tops:?}");
        assert_ne!(build(&spec(Kind::Bed, 1)).unwrap().bricks, build(&spec(Kind::Bed, 2)).unwrap().bricks);
    }

    #[test]
    fn a_hedge_turns_a_sharp_corner_and_reeds_fill_a_closed_line() {
        let corner = [(-50.0, -30.0), (40.0, -30.0), (40.0, 40.0)];
        let sharp = build(&Spec { protrude: true, ..along(Kind::Hedge, &corner, 9.0, false) }).unwrap();
        let smooth = build(&along(Kind::Hedge, &corner, 9.0, false)).unwrap();
        // at the corner itself, and just outside it where a curve cuts across: hedge only when it is sharp
        assert!(top_at(&sharp, 40.5, -30.5).is_some() && top_at(&sharp, 44.5, -34.5).is_some(), "the corner is open");
        // a curve through the same points swings wide of the first leg on its way into the turn; the straight one keeps to its line
        let south = |built: &crate::spec::Built| built.bricks.iter().map(|b| b.min[1]).min().unwrap();
        assert!(south(&sharp) >= -40 && south(&smooth) < south(&sharp) - 2, "{} sharp, {} smooth", south(&sharp), south(&smooth));
        // straight right up to it: full height two cells short of the corner on each leg, and nothing bulging inside the turn
        assert!(top_at(&sharp, 20.5, -30.5).unwrap() > 24 && top_at(&sharp, 40.5, -10.5).unwrap() > 24 && top_at(&sharp, 24.5, -14.5).is_none());
        // level over the corner as along the legs
        assert!((top_at(&sharp, 40.5, -30.5).unwrap() - top_at(&sharp, 0.5, -30.5).unwrap()).abs() <= 4);

        let pond = [(-30.0, -20.0), (30.0, -25.0), (35.0, 20.0), (-20.0, 25.0)];
        let lay = |closed: bool| Lay { run: pond.iter().map(|(x, y)| DVec3::new(*x, *y, 0.0)).collect::<Vec<_>>(), closed, sharp: true, water: None, reeds: 0.0, levels: 1, mounds: vec![] };
        let feet = |closed: bool| -> Vec<DVec3> { Model::laid(&Tree { count: 80, ..Tree::of(Kind::Reeds) }, DVec3::splat(0.5), 70.0, 8.0, &lay(closed)).limbs.iter().filter(|l| l.level == 0).map(|l| l.path[0] - DVec3::new(0.5, 0.5, 0.0)).collect() };
        let (filled, along_it) = (feet(true), feet(false));
        let inner = |feet: &[DVec3]| feet.iter().filter(|f| f.x.abs() < 12.0 && f.y.abs() < 8.0).count();
        assert!(filled.len() >= 70 && inner(&filled) >= 5 && inner(&along_it) == 0, "{} stems, {} in the middle; {} along the line", filled.len(), inner(&filled), inner(&along_it));
        assert!(filled.iter().all(|f| f.x > -31.0 && f.x < 36.0 && f.y > -26.0 && f.y < 26.0));
    }

    /// One thread or many, the same bricks in the same order with the same colours.
    #[test]
    fn a_plant_is_the_same_bricks_on_any_number_of_threads() {
        use crate::par::Threads;
        for kind in [Kind::Oak, Kind::Willow, Kind::Grass, Kind::Bed] {
            let spec = small(kind, 3);
            let (one, many) = (crate::spec::build_with(&spec, true, Threads::ONE).unwrap(), crate::spec::build_with(&spec, true, Threads::forced(5)).unwrap());
            assert_eq!((&one.bricks, &one.shades), (&many.bricks, &many.shades), "{kind:?}");
        }
    }

    /// A lawn 100 by 80, timed: run by name with `--ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    #[cfg(not(debug_assertions))]
    fn how_long_a_lawn_takes() {
        use crate::par::Threads;
        let lawn = Spec { closed: true, ..along(Kind::Grass, &[(-50.0, -40.0), (50.0, -40.0), (50.0, 40.0), (-50.0, 40.0)], 4.0, true) };
        let lawn = Spec { rise: Some(5.0), pulls: (0..40).map(|n| (DVec3::new(-40.0 + 2.0 * n as f64, (n as f64 * 0.7).sin() * 25.0, 0.5), DVec3::new(18.0, 0.0, 6.0))).collect(), ..lawn };
        for threads in [Threads::ONE, Threads::available()] {
            let least = (0..3).map(|_| { let at = std::time::Instant::now(); let built = crate::spec::build_with(&lawn, true, threads).unwrap(); (at.elapsed(), built.bricks.len()) }).min().unwrap();
            println!("a lawn 100 by 80 with 40 touches on {} threads: {:?}, {} bricks", threads.count(), least.0, least.1);
        }
    }

    #[test]
    fn a_hollow_dug_below_the_waters_level_is_a_pond() {
        let lawn = Spec { closed: true, depth: 6, ..along(Kind::Grass, &[(-40.0, -30.0), (40.0, -30.0), (40.0, 30.0), (-40.0, 30.0)], 4.0, true) };
        let level = -(lawn.tree.count as f64);
        // a hollow 10 deep, 22 wide: far deeper than the water stands (3 below the lawn)
        let dig = |amount: f64| Spec { pulls: vec![(DVec3::new(0.5, 0.5, 0.5), DVec3::new(22.0, 0.0, amount))], ..lawn.clone() };
        let pond = build(&dig(-10.0)).unwrap();
        let water = bricks_of(&pond, &[WATER]);
        assert!(water.len() > 5, "no water in a hollow 10 deep");
        // flat on top, at the level, and nowhere above it; under it, down to the bed
        let top = pond.bricks.iter().zip(&pond.shades).filter(|(_, s)| **s == WATER).map(|(b, _)| b.min[2] + b.ext[2]).max().unwrap();
        assert_eq!(top as f64, level, "the water's top");
        for x in [-6.0, 0.0, 5.0] {
            let over = pond.bricks.iter().zip(&pond.shades).filter(|(b, _)| b.min[0] as f64 <= x && x < (b.min[0] + b.ext[0]) as f64 && b.min[1] <= 0 && 0 < b.min[1] + b.ext[1]).max_by_key(|(b, _)| b.min[2] + b.ext[2]).unwrap();
            assert_eq!((*over.1, over.0.min[2] + over.0.ext[2]), (WATER, top), "over the middle of the pond at {x}");
        }
        // in few large bricks
        let cells: i32 = pond.bricks.iter().zip(&pond.shades).filter(|(_, s)| **s == WATER).map(|(b, _)| b.ext[0] * b.ext[1] * b.ext[2]).sum();
        assert!(cells / water.len() as i32 >= 10, "{cells} cells of water in {} bricks", water.len());
        // the bank in order going down: grass, then sand, then the bed; and stones only on the bed
        let mean = |shades: &[u8]| { let these = bricks_of(&pond, shades); assert!(these.len() > 3, "no bricks of {shades:?}"); these.iter().map(|p| p.truncate().length()).sum::<f64>() / these.len() as f64 };
        let (bed, sand) = (mean(&[BED]), mean(&[SAND]));
        assert!(bed < sand && sand < 22.0, "the bed is {bed:.1} from the middle and the sand {sand:.1}");
        let blades = bricks_of(&build(&Spec { depth: 1, ..dig(-10.0) }).unwrap(), &[LEAF, LEAF_DARK, LEAF_LIGHT]);
        let near = blades.iter().filter(|p| p.truncate().length() < sand - 3.0).count();
        assert!(blades.len() > 500);
        assert_eq!(near, 0, "grass growing in the water");
        // (a large one may stand out of shallow water)
        assert!(bricks_of(&pond, &[STONE]).iter().all(|p| p.z < level + 3.0));
        // reeds round the margin, more when asked for more, and none when asked for none
        let reeds = |depth: i32| build(&Spec { depth, ..dig(-10.0) }).unwrap().bricks.iter().filter(|b| b.ext[2] >= 3 && b.min[2] + b.ext[2] > 12).count();
        assert!(reeds(1) == 0 && reeds(4) > 0 && reeds(11) > reeds(4) * 2, "{} {} {}", reeds(1), reeds(4), reeds(11));
        // no water in a hollow that stops above the level, on a hill, or when water is switched off
        assert!(bricks_of(&build(&dig(-2.0)).unwrap(), &[WATER, SAND, BED]).is_empty() || bricks_of(&build(&dig(-2.0)).unwrap(), &[WATER]).is_empty());
        assert!(bricks_of(&build(&dig(8.0)).unwrap(), &[WATER]).is_empty());
        assert!(bricks_of(&build(&Spec { tree: Tree { count: 0, ..lawn.tree }, ..dig(-10.0) }).unwrap(), &[WATER]).is_empty());
        // Digging the pond leaves the lawn far from it blade for blade as it was. (From its own level up: a patch with a pond in it is cut off at the pond's depth and not at its own level, so the underside of its soil, most of a cell, is built all over it where a flat lawn's is cut away.)
        let flat = build(&lawn).unwrap();
        // (as the cells it fills: the bricks are joined up a little differently, the fit having begun from another corner)
        let far = |built: &crate::spec::Built| { let mut cells = std::collections::BTreeSet::new(); for b in built.bricks.iter().filter(|b| b.min[0] > 28 && b.min[2] >= 2 && b.mask == CUBE) { for x in 0..b.ext[0] { for y in 0..b.ext[1] { for z in 0..b.ext[2] { cells.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]); } } } } cells };
        let (was, now) = (far(&flat), far(&pond));
        assert!(was.len() > 300 && was.symmetric_difference(&now).count() * 50 < was.len(), "{} of {} cells differ", was.symmetric_difference(&now).count(), was.len());
    }

    /// How far a trunk reaches from its middle at a height: the least and the most of eight ways round.
    fn reach(tree: &Model, z: f64) -> (f64, f64) {
        // (stepping out to the first place outside it: a willow's strands hang round its trunk, and are not it)
        let out = |way: DVec3| (1..400).map(|n| n as f64 * 0.05).find(|far| tree.eval(DVec3::new(0.5, 0.5, z) + way * *far) > 0.0).unwrap();
        (0..8).map(|k| k as f64 * TAU / 8.0).map(|a| out(DVec3::new(a.cos(), a.sin(), 0.0))).fold((f64::MAX, f64::MIN), |(lo, hi), r| (lo.min(r), hi.max(r)))
    }

    #[test]
    fn a_trunk_widens_to_its_foot_with_no_ledge_and_is_no_wider_than_its_limbs_make_it() {
        for kind in [Kind::Willow, Kind::Oak, Kind::Beech, Kind::Birch, Kind::ScotsPine, Kind::Cherry] {
            for seed in 1..=3 {
                let tree = model(kind, seed);
                let fork = tree.limbs[0].path.last().unwrap().z;
                let heights: Vec<f64> = (1..(fork * 0.8) as usize).map(|k| k as f64 + 0.25).collect();
                let reaches: Vec<(f64, f64)> = heights.iter().map(|z| reach(&tree, *z)).collect();
                // going up, from one cell to the next it never draws in by a whole cell: its sizes run into one another on slopes, and its roots into it
                for (n, pair) in reaches.windows(2).enumerate() {
                    assert!(pair[0].0 - pair[1].0 < 0.8 && pair[0].1 - pair[1].1 < 1.3, "{kind:?} {seed}: from {:?} to {:?} between {} and {} up", pair[0], pair[1], heights[n], heights[n + 1]);
                }
                // and its foot is not a plinth: under half as wide again as the trunk half way up
                let (foot, middle) = (reaches[0].0, reaches[reaches.len() / 2].0);
                assert!(foot <= middle * 1.5 + 0.5, "{kind:?} {seed}: {foot} at its foot and {middle} half way up");
                // where it divides, its limbs are together what it is there (da Vinci's rule), and it is there what its bricks are
                let (stem, into) = &tree.forks[0];
                let sum: f64 = into.iter().map(|r| r * r).sum();
                assert!((sum.sqrt() - stem).abs() < 0.05 * stem, "{kind:?} {seed}: limbs of {into:?} from a trunk of {stem}");
                let under = reach(&tree, (fork * 0.8).floor() + 0.25).0;
                assert!((under - stem).abs() <= 0.85, "{kind:?} {seed}: the trunk is {under} under a fork of {stem}");
            }
        }
        // a birch is slender: its trunk under a fortieth of its height through at breast height, and narrower at its fork than at its foot
        let (birch, height) = (model(Kind::Birch, 1), Tree::usual_size(Kind::Birch).0);
        let fork = birch.limbs[0].path.last().unwrap().z;
        assert!(reach(&birch, 8.25).0 * 2.0 <= height / 22.0, "a birch {} through, {height} tall", reach(&birch, 8.25).0 * 2.0);
        assert!(reach(&birch, (fork * 0.8).floor() + 0.25).0 < reach(&birch, 2.25).0);
    }

    #[test]
    fn a_cherry_is_wide_and_low_with_blossom_in_small_clusters_along_its_branches() {
        let blossom = [LEAF, LEAF_DARK, LEAF_LIGHT];
        let cells = |built: &crate::spec::Built, wanted: &[u8]| built.bricks.iter().zip(&built.shades).filter(|(_, s)| wanted.contains(s)).map(|(b, _)| (b.ext[0] * b.ext[1] * b.ext[2]) as f64).sum::<f64>();
        let airy = |built: &crate::spec::Built| { let (lo, hi) = extent(built, &blossom); cells(built, &blossom) / ((hi.x - lo.x) * (hi.y - lo.y) * (hi.z - lo.z)) };
        let oak = airy(&build(&spec(Kind::Oak, 1)).unwrap());
        for seed in 1..=3 {
            let built = build(&spec(Kind::Cherry, seed)).unwrap();
            // the crown: wider than it is tall
            let (lo, hi) = extent(&built, &blossom);
            assert!((hi.x - lo.x).min(hi.y - lo.y) > (hi.z - lo.z) * 1.1, "seed {seed}: a crown {lo} to {hi}");
            // sky between the clusters: far less of the crown filled than an oak's
            assert!(airy(&built) < oak * 0.6, "seed {seed}: {} of its crown filled, and {oak} of an oak's", airy(&built));
            // blossom along the branches and not only at their ends: some of it in the inner half of the crown, in every quarter round the trunk
            let inner = bricks_of(&built, &blossom).iter().filter(|p| p.truncate().length() < (hi.x - lo.x) * 0.22).count();
            assert!(inner * 12 > bricks_of(&built, &blossom).len(), "seed {seed}: {inner} of {} bricks of blossom near the trunk", bricks_of(&built, &blossom).len());
            // pinks and white, a few buds and young leaves, and bands across the trunk
            let count = |shade: u8| bricks_of(&built, &[shade]).len();
            assert!(count(LEAF) > count(LEAF_LIGHT) && count(LEAF_LIGHT) > count(EXTRA) && count(EXTRA) > 3 && count(TOP) > 3, "seed {seed}");
            let fork = model(Kind::Cherry, seed).limbs[0].path.last().unwrap().z;
            assert!(bricks_of(&built, &[STAR]).iter().filter(|p| p.z < fork).count() >= 3 && bricks_of(&built, &[STAR]).iter().all(|p| p.z < fork + 2.0), "seed {seed}: bands");
            // no petals on the ground unless asked for; asked for, they lie on it under the crown
            let low = |built: &crate::spec::Built| bricks_of(built, &blossom).iter().filter(|p| p.z < 3.0).count();
            let fallen = build(&Spec { closed: true, ..spec(Kind::Cherry, seed) }).unwrap();
            assert!(low(&built) == 0 && low(&fallen) > 30, "seed {seed}: {} and {} bricks of petals", low(&built), low(&fallen));
            // bare: the same tree with no blossom on it nor under it
            let bare = build(&Spec { closed: true, tree: Tree { extras: true, ..spec(Kind::Cherry, seed).tree }, ..spec(Kind::Cherry, seed) }).unwrap();
            assert!(bricks_of(&bare, &[LEAF, LEAF_DARK, LEAF_LIGHT, EXTRA, TOP]).is_empty() && bricks_of(&bare, &[LIMB]).len() > 100, "seed {seed}: bare");
        }
    }

    /// A lawn with a hill that has a hollow on it, and a hollow in the flat beside the hill.
    fn two_hollows() -> Spec {
        let lawn = Spec { depth: 4, ..along(Kind::Grass, &[(-60.0, -40.0), (60.0, -40.0), (60.0, 40.0), (-60.0, 40.0)], 4.0, true) };
        let touch = |x: f64, range: f64, amount: f64| (DVec3::new(x + 0.5, 0.5, 0.5), DVec3::new(range, 0.0, amount));
        Spec { pulls: vec![touch(-25.0, 34.0, 16.0), touch(-25.0, 16.0, -14.0), touch(30.0, 20.0, -10.0)], ..lawn }
    }

    /// The heights water's surface stands at over the places from `from` to `to` along x.
    fn water_tops(built: &crate::spec::Built, from: f64, to: f64) -> std::collections::BTreeSet<i32> {
        let mut tops = std::collections::BTreeMap::new();
        for (b, _) in built.bricks.iter().zip(&built.shades).filter(|(b, s)| **s == WATER && (b.min[0] as f64) < to && (b.min[0] + b.ext[0]) as f64 > from) {
            for x in (b.min[0]..b.min[0] + b.ext[0]).filter(|x| (*x as f64 + 1.0) > from && (*x as f64) < to) {
                for y in b.min[1]..b.min[1] + b.ext[1] {
                    let top = tops.entry((x, y)).or_insert(i32::MIN);
                    *top = (*top).max(b.min[2] + b.ext[2]);
                }
            }
        }
        tops.into_values().collect()
    }

    #[test]
    fn two_hollows_at_two_heights_hold_water_at_two_levels_and_a_stream_runs_down_between_them() {
        let (one, two) = (two_hollows(), Spec { tube: true, ..two_hollows() });
        let (flat, stepped) = (build(&one).unwrap(), build(&two).unwrap());
        // one level: water only at the lawn's own, in the lower hollow
        // (a cell of water at a pond's very edge may be a cell lower, under the lip of its bank)
        let level = |tops: &std::collections::BTreeSet<i32>| { let (low, high) = (*tops.iter().next().unwrap(), *tops.iter().next_back().unwrap()); assert!(high - low <= 1, "a pond's water at {tops:?}"); high };
        assert_eq!(level(&water_tops(&flat, -100.0, 100.0)), -3);
        assert!(water_tops(&flat, -100.0, 5.0).is_empty());
        // two: the hollow on the hill holds water too, higher, and each pond is level
        let (upper, lower) = (water_tops(&stepped, -30.0, -20.0), water_tops(&stepped, 26.0, 34.0));
        assert_eq!(level(&lower), -3);
        let high = level(&upper);
        assert!(high >= 0, "the upper pond stands at {high}");
        // the stream: water all the way from one to the other, never rising on the way down, never over its banks
        let along: Vec<i32> = (-18..=22).map(|x| water_tops(&stepped, x as f64 + 0.25, x as f64 + 0.75).into_iter().max().unwrap_or_else(|| panic!("the stream is dry at {x}"))).collect();
        // (taken two cells apart: where it bends, the end of one reach lies beside the start of the next, and which is the higher at one x is by a cell either way)
        assert!(along.windows(3).all(|w| w[2] <= w[0]) && along[0] == high && *along.last().unwrap() == -3, "the stream's water, going down: {along:?}");
        assert!(along.iter().collect::<std::collections::BTreeSet<_>>().len() >= 4, "it falls in {along:?}: no steps");
        let mut worst: f64 = f64::MIN;
        for (b, _) in stepped.bricks.iter().zip(&stepped.shades).filter(|(b, s)| **s == WATER && b.min[0] > -12 && b.min[0] + b.ext[0] < 10) {
            for x in 0..b.ext[0] {
                for y in 0..b.ext[1] {
                    let at = DVec3::new((b.min[0] + x) as f64 + 0.5, (b.min[1] + y) as f64 + 0.5, 0.5);
                    worst = worst.max((b.min[2] + b.ext[2]) as f64 - crate::spec::grass_ground(&two, at));
                }
            }
        }
        assert!(worst <= 1.0, "the stream stands {worst} over the ground it was cut into");
        // narrow: a few cells of water across, where it runs along x
        let across = |x: i32| stepped.bricks.iter().zip(&stepped.shades).filter(|(b, s)| **s == WATER && b.min[0] <= x && x < b.min[0] + b.ext[0]).map(|(b, _)| (b.min[1], b.min[1] + b.ext[1])).fold((i32::MAX, i32::MIN), |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        assert!((2..=9).contains(&(across(0).1 - across(0).0)), "the stream is {:?} across", across(0));
        // its bed is sand, and the same on any number of threads
        assert!(bricks_of(&stepped, &[SAND]).iter().filter(|p| p.x > -10.0 && p.x < 12.0).count() > 20);
        let many = crate::spec::build_with(&two, true, crate::par::Threads::forced(5)).unwrap();
        assert_eq!((&stepped.bricks, &stepped.shades), (&many.bricks, &many.shades));
        // one hollow in a flat lawn: two levels are the same bricks as one
        let single = Spec { pulls: two_hollows().pulls[2..].to_vec(), ..two_hollows() };
        let (a, b) = (build(&single).unwrap(), build(&Spec { tube: true, ..single.clone() }).unwrap());
        assert_eq!((&a.bricks, &a.shades), (&b.bricks, &b.shades));
        // a hollow on a hill with no lower water to run to: a pond up there, and no stream
        let alone = build(&Spec { tube: true, pulls: two_hollows().pulls[..2].to_vec(), ..two_hollows() }).unwrap();
        assert!(!water_tops(&alone, -30.0, -20.0).is_empty() && water_tops(&alone, -8.0, 100.0).is_empty(), "{:?}", water_tops(&alone, -8.0, 100.0));
    }

    #[test]
    fn a_pond_is_a_basin_as_deep_as_it_was_dug_with_water_down_to_its_bed() {
        let lawn = Spec { closed: true, depth: 1, ..along(Kind::Grass, &[(-40.0, -30.0), (40.0, -30.0), (40.0, 30.0), (-40.0, 30.0)], 4.0, true) };
        for deep in [6.0, 10.0, 16.0] {
            let pond = build(&Spec { pulls: vec![(DVec3::new(0.5, 0.5, 0.5), DVec3::new(22.0, 0.0, -deep))], ..lawn.clone() }).unwrap();
            // the cells over the pond's middle, from the top down: which part each is
            let mut column: Vec<(i32, u8)> = pond.bricks.iter().zip(&pond.shades).filter(|(b, _)| b.min[0] <= 0 && 0 < b.min[0] + b.ext[0] && b.min[1] <= 0 && 0 < b.min[1] + b.ext[1]).flat_map(|(b, s)| (b.min[2]..b.min[2] + b.ext[2]).map(|z| (z, *s))).collect();
            column.sort();
            column.reverse();
            let water: Vec<i32> = column.iter().filter(|c| c.1 == WATER).map(|c| c.0).collect();
            // water from the level (3 below the lawn) down to within a cell or two of the dug depth, with nothing else in it
            assert_eq!(column[0], (-4, WATER), "dug {deep}: the top of {column:?}");
            let bottom = *water.last().unwrap();
            assert!(water.len() as i32 == -3 - bottom && (bottom as f64 + deep).abs() <= 2.0, "dug {deep}: water in {water:?}");
            // and under it the bed, at the dug depth: not soil under the surface, nor a slab of water on flat ground
            let under: Vec<(i32, u8)> = column.iter().filter(|c| c.0 < bottom).copied().collect();
            assert!(!under.is_empty() && under.iter().all(|c| matches!(c.1, BED | STONE)) && under.len() <= 4, "dug {deep}: under the water {under:?}");
        }
    }
}
