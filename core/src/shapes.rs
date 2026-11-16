//! The microbrick shapes, as subsets of a box's 8 corners.
//!
//! A corner is a bit index `x | y<<1 | z<<2` (0 = min side, 1 = max side) and
//! a shape is the 8-bit mask of the corners it keeps; the solid is their
//! convex hull. The same mask describes a stretched brick: it says which
//! corners of the brick's bounding box are filled.
//!
//! | kind | corners | poses |
//! |------|---------|-------|
//! | cube | 8 | 1 |
//! | ramp | 6, the missing two share an edge | 12 |
//! | tet  | 4, one corner and its three neighbours | 8 |
//! | cut  | 7, the cube with a corner tet removed | 8 |
//! | hip  | 5, one face and one corner of the face opposite | 24 |
//! | half outer | 5, a right triangle of one face and the two corners above its long side | 24 |
//! | half inner | 4, a right triangle of one face and the corner above one end of its long side | 12, and 12 mirror images |
//! | inner | see below | 24 |
//!
//! The inner corner is the one shape that is not the hull of its corners. It
//! is a box less a hip, a valley where two slopes meet, and it holds the same
//! seven corners a cut cube does. So its mask is not the corners it holds:
//! it is the three corners the hip it lacks does not touch, which no other
//! shape's mask can be (every other has four corners or more). Turning or
//! mirroring a mask by moving its bits still turns or mirrors the shape,
//! since the lacking hip moves with it. Only code that reads a mask as
//! "these corners are solid" has to ask [`shape_info`] first.
//!
//! The shapes' geometry is the game's own: each is one of its procedural
//! micro-wedge bricks, and the canonical pose here is that brick unturned.

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Cube,
    Ramp,
    Tet,
    Cut,
    /// One whole face of the box and a single corner of the opposite face:
    /// a quarter of a square pyramid, the piece at the corner of a hipped
    /// roof where two slopes meet along a ridge.
    Hip,
    /// A box less a hip: the valley where two slopes meet in an inside
    /// corner. The game's micro-wedge inner corner.
    Inner,
    /// Over half of the box's floor, a right triangle, rising from nothing
    /// at the right angle to full height all along the long side. The game's
    /// micro-wedge half outer corner.
    HalfOuter,
    /// A tetrahedron on the same triangle of floor, its fourth corner above
    /// one end of the long side. Its mirror image is a different brick,
    /// [`Kind::HalfInnerInv`]. The game's micro-wedge half inner corner.
    HalfInner,
    HalfInnerInv,
}

pub const KINDS: [Kind; 9] = [Kind::Cube, Kind::Ramp, Kind::Tet, Kind::Cut, Kind::Hip, Kind::Inner, Kind::HalfOuter, Kind::HalfInner, Kind::HalfInnerInv];

impl Kind {
    /// The pose every other pose of this kind is a rotation of.
    pub const fn canonical(self) -> u8 {
        match self {
            Kind::Cube => 0xff,
            Kind::Ramp => 0x77, // missing the edge x=1,y=1: solid where x + y <= 1
            Kind::Tet => 0x17,  // corner 0 and its neighbours: x + y + z <= 1
            Kind::Cut => 0x7f,  // missing corner 7: x + y + z <= 2
            Kind::Hip => 0x1f,  // the face z=0 and corner 4 above corner 0: x <= 1 - z and y <= 1 - z
            // the box less the hip of the face z=1 and corner 3; not the
            // corners it holds (see the top of this file): min(x, y) + z <= 1
            Kind::Inner => 0x07,
            Kind::HalfOuter => 0x67,    // corners 0 1 2, and 5 6 above 1 2: x + y <= 1 and z <= x + y
            Kind::HalfInner => 0x47,    // corners 0 1 2, and 6 above 2: x + y <= 1 and z <= y
            Kind::HalfInnerInv => 0x27, // corners 0 1 2, and 5 above 1: x + y <= 1 and z <= x
        }
    }

    /// Fraction of the bounding box the shape fills.
    pub const fn volume(self) -> f64 {
        match self {
            Kind::Cube => 1.0,
            Kind::Ramp => 0.5,
            Kind::Tet => 1.0 / 6.0,
            Kind::Cut => 5.0 / 6.0,
            Kind::Hip => 1.0 / 3.0,
            Kind::Inner => 2.0 / 3.0,
            Kind::HalfOuter => 1.0 / 3.0,
            Kind::HalfInner | Kind::HalfInnerInv => 1.0 / 6.0,
        }
    }

    /// Whether the kind is described by `corner` and `axis` of its
    /// [`ShapeInfo`]; the others by `pose` (see [`ShapeInfo::local`]).
    pub const fn by_corner(self) -> bool {
        matches!(self, Kind::Cube | Kind::Ramp | Kind::Tet | Kind::Cut | Kind::Hip)
    }
}

pub const CUBE: u8 = 0xff;

/// One of Brickadia's 24 brick orientations: a brick-local vector `l` maps to
/// the world as `world[i] = ±l[perm[i]]`, negated where bit `i` of `neg` is set.
#[derive(Clone, Copy)]
pub struct Orientation {
    pub perm: [u8; 3],
    pub neg: u8,
}

const fn o(perm: [u8; 3], neg: u8) -> Orientation {
    Orientation { perm, neg }
}

/// Indexed by the game's orientation number, `direction * 4 + rotation`.
/// Transcribed from omegga's `translationTable` (src/util/brick/constants.ts);
/// the row comments give the world vector for a local (x, y, z).
pub const ORIENT: [Orientation; 24] = [
    o([2, 1, 0], 0b010), // 0  X+   0: ( z, -y,  x)
    o([2, 0, 1], 0b110), // 1  X+  90: ( z, -x, -y)
    o([2, 1, 0], 0b100), // 2  X+ 180: ( z,  y, -x)
    o([2, 0, 1], 0b000), // 3  X+ 270: ( z,  x,  y)
    o([2, 1, 0], 0b001), // 4  X-   0: (-z,  y,  x)
    o([2, 0, 1], 0b101), // 5  X-  90: (-z,  x, -y)
    o([2, 1, 0], 0b111), // 6  X- 180: (-z, -y, -x)
    o([2, 0, 1], 0b011), // 7  X- 270: (-z, -x,  y)
    o([1, 2, 0], 0b000), // 8  Y+   0: ( y,  z,  x)
    o([0, 2, 1], 0b100), // 9  Y+  90: ( x,  z, -y)
    o([1, 2, 0], 0b101), // 10 Y+ 180: (-y,  z, -x)
    o([0, 2, 1], 0b001), // 11 Y+ 270: (-x,  z,  y)
    o([1, 2, 0], 0b011), // 12 Y-   0: (-y, -z,  x)
    o([0, 2, 1], 0b111), // 13 Y-  90: (-x, -z, -y)
    o([1, 2, 0], 0b110), // 14 Y- 180: ( y, -z, -x)
    o([0, 2, 1], 0b010), // 15 Y- 270: ( x, -z,  y)
    o([0, 1, 2], 0b000), // 16 Z+   0: ( x,  y,  z)
    o([1, 0, 2], 0b001), // 17 Z+  90: (-y,  x,  z)
    o([0, 1, 2], 0b011), // 18 Z+ 180: (-x, -y,  z)
    o([1, 0, 2], 0b010), // 19 Z+ 270: ( y, -x,  z)
    o([0, 1, 2], 0b101), // 20 Z-   0: (-x,  y, -z)
    o([1, 0, 2], 0b100), // 21 Z-  90: ( y,  x, -z)
    o([0, 1, 2], 0b110), // 22 Z- 180: ( x, -y, -z)
    o([1, 0, 2], 0b111), // 23 Z- 270: (-y, -x, -z)
];

/// The orientation that leaves everything as it is.
pub const UPRIGHT: u8 = 16;

/// The orientation that is a quarter turn about x, y or z: the turn that
/// takes y towards z, z towards x, or x towards y.
pub const fn quarter(axis: usize) -> u8 {
    [15, 2, 17][axis % 3]
}

/// The one orientation that does what `first` and then `then` do together.
pub fn compose(first: u8, then: u8) -> u8 {
    let (a, b) = (ORIENT[first as usize % 24], ORIENT[then as usize % 24]);
    let perm = [0, 1, 2].map(|i| a.perm[b.perm[i] as usize]);
    let neg = (0..3).fold(0u8, |n, i| n | ((b.neg >> i & 1) ^ (a.neg >> b.perm[i] & 1)) << i);
    ORIENT.iter().position(|o| o.perm == perm && o.neg == neg).expect("the orientations are closed under composition") as u8
}

/// The world corner mask of a brick whose local corner mask is `mask`, at
/// orientation `orientation`.
pub fn transform_mask(mask: u8, orientation: usize) -> u8 {
    let Orientation { perm, neg } = ORIENT[orientation];
    let mut out = 0u8;
    for c in 0..8u8 {
        if mask >> c & 1 == 0 {
            continue;
        }
        let mut w = 0u8;
        for i in 0..3 {
            let bit = (c >> perm[i] & 1) ^ (neg >> i & 1);
            w |= bit << i;
        }
        out |= 1 << w;
    }
    out
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeInfo {
    pub kind: Kind,
    /// tet: the right-angle corner. cut: the missing corner. ramp: a corner on
    /// the missing edge. hip: the apex, the one corner kept of its face.
    pub corner: u8,
    /// ramp: the axis its missing edge, and so the prism, runs along.
    /// hip: the axis from its whole face to its apex.
    pub axis: u8,
    /// the mask itself
    pub mask: u8,
    /// an orientation (an index into [`ORIENT`]) that turns the kind's
    /// canonical pose into this one
    pub pose: u8,
}

impl ShapeInfo {
    /// A point of the box (each coordinate 0 to 1) in the box of the kind's
    /// canonical pose. The newer kinds are described there, in the frame the
    /// game's own brick is made in, instead of by a corner and an axis: they
    /// have too little symmetry for those two numbers to say which way up
    /// they are.
    pub fn local(&self, t: [f64; 3]) -> [f64; 3] {
        let Orientation { perm, neg } = ORIENT[self.pose as usize];
        let mut l = [0.0; 3];
        for i in 0..3 {
            l[perm[i] as usize] = if neg >> i & 1 == 1 { 1.0 - t[i] } else { t[i] };
        }
        l
    }

    /// The reverse of [`ShapeInfo::local`].
    pub fn world(&self, l: [f64; 3]) -> [f64; 3] {
        let Orientation { perm, neg } = ORIENT[self.pose as usize];
        [0, 1, 2].map(|i| if neg >> i & 1 == 1 { 1.0 - l[perm[i] as usize] } else { l[perm[i] as usize] })
    }
}

/// The corners of its box a shape holds: its mask, for every shape but the
/// inner corner (see the top of this file), which holds all but the one
/// corner of the lacking hip's whole face that stands over its apex.
pub fn corners(mask: u8) -> u8 {
    match shape_info(mask) {
        Some(info) if info.kind == Kind::Inner => {
            let hip = shape_info(!mask).expect("an inner corner's mask is the rest of a hip's");
            CUBE & !(1 << (hip.corner ^ (1 << hip.axis)))
        }
        _ => mask,
    }
}

/// A newer kind's faces that are not square to the box, as triangles in its
/// canonical box, and a point inside it to tell their outsides by.
pub fn sloped_triangles(kind: Kind) -> (&'static [[[f64; 3]; 3]], [f64; 3]) {
    const A: [f64; 3] = [1.0, 0.0, 0.0];
    const B: [f64; 3] = [0.0, 1.0, 0.0];
    const O: [f64; 3] = [0.0, 0.0, 0.0];
    const TOP: [f64; 3] = [0.0, 0.0, 1.0];
    const OVER_A: [f64; 3] = [1.0, 0.0, 1.0];
    const OVER_B: [f64; 3] = [0.0, 1.0, 1.0];
    const FAR: [f64; 3] = [1.0, 1.0, 0.0];
    match kind {
        Kind::Inner => (&[[OVER_A, TOP, FAR], [TOP, OVER_B, FAR]], [0.2, 0.2, 0.2]),
        // the upright face on the long side, in two halves, and the slope
        Kind::HalfOuter => (&[[A, B, OVER_B], [A, OVER_B, OVER_A], [O, OVER_A, OVER_B]], [0.3, 0.3, 0.2]),
        Kind::HalfInner => (&[[A, B, OVER_B], [O, A, OVER_B]], [0.2, 0.5, 0.2]),
        Kind::HalfInnerInv => (&[[A, B, OVER_A], [O, B, OVER_A]], [0.5, 0.2, 0.2]),
        _ => (&[], [0.5; 3]),
    }
}

/// How far outside each of a newer kind's sloped faces a point of its
/// canonical box is: negative inside. A point is in the shape if it is
/// inside them all, or for the inner corner, inside either.
pub fn sloped_sides(kind: Kind, [x, y, z]: [f64; 3]) -> [f64; 2] {
    match kind {
        Kind::Inner => [y + z - 1.0, x + z - 1.0],
        Kind::HalfOuter => [x + y - 1.0, z - (x + y)],
        Kind::HalfInner => [x + y - 1.0, z - y],
        Kind::HalfInnerInv => [x + y - 1.0, z - x],
        _ => [-1.0; 2],
    }
}

fn table() -> &'static [Option<ShapeInfo>; 256] {
    static TABLE: OnceLock<[Option<ShapeInfo>; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [None; 256];
        for kind in KINDS {
            for orientation in 0..24 {
                let mask = transform_mask(kind.canonical(), orientation);
                let missing: Vec<u8> = (0..8).filter(|c| mask >> c & 1 == 0).collect();
                let (corner, axis) = match kind {
                    Kind::Cube | Kind::Inner | Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv => (0, 0),
                    Kind::Cut => (missing[0], 0),
                    // the axis along which one side keeps all four corners;
                    // the apex is the corner kept on the other side
                    Kind::Hip => {
                        let axis = (0..3u8).find(|&a| [0u8, 1].iter().any(|&side| (0..8u8).filter(|c| c >> a & 1 == side).all(|c| mask >> c & 1 == 1))).unwrap();
                        let whole = (0..8u8).filter(|c| c >> axis & 1 == 0).all(|c| mask >> c & 1 == 1) as u8;
                        // `whole` is 1 if the low side is the whole face, so the apex is on the high side
                        let apex = (0..8u8).find(|&c| c >> axis & 1 == whole && mask >> c & 1 == 1).unwrap();
                        (apex, axis)
                    }
                    Kind::Ramp => (missing[0], (missing[0] ^ missing[1]).trailing_zeros() as u8),
                    // the kept corner whose three neighbours are all kept
                    Kind::Tet => (
                        (0..8u8)
                            .find(|&c| {
                                mask >> c & 1 == 1
                                    && [1, 2, 4].iter().all(|b| mask >> (c ^ b) & 1 == 1)
                            })
                            .unwrap(),
                        0,
                    ),
                };
                // no two kinds share a mask
                assert!(t[mask as usize].is_none_or(|s: ShapeInfo| s.kind == kind));
                t[mask as usize] = Some(ShapeInfo { kind, corner, axis, mask, pose: orientation as u8 });
            }
        }
        t
    })
}

/// What shape a corner mask is, or `None` if no single brick has those corners.
pub fn shape_info(mask: u8) -> Option<ShapeInfo> {
    table()[mask as usize]
}

/// Every representable mask: 1 cube, 12 ramps, 8 tets, 8 cuts, 24 hips, and
/// 24 each of inner corners and half outer corners, and 12 each of the two
/// half inner corners.
pub fn all_masks() -> Vec<u8> {
    (0..=255u8).filter(|&m| shape_info(m).is_some()).collect()
}

/// Whether the point `(u, v, w)`, each in 0..1 across the box, is in the shape.
pub fn inside_shape(info: ShapeInfo, u: f64, v: f64, w: f64) -> bool {
    if !info.kind.by_corner() {
        let [p, q] = sloped_sides(info.kind, info.local([u, v, w]));
        return if info.kind == Kind::Inner { p < 0.0 || q < 0.0 } else { p < 0.0 && q < 0.0 };
    }
    let c = info.corner;
    let a = if c & 1 != 0 { 1.0 - u } else { u };
    let b = if c & 2 != 0 { 1.0 - v } else { v };
    let d = if c & 4 != 0 { 1.0 - w } else { w };
    // Strictly inside: a point exactly on a sloped face is outside, as a point
    // exactly on a form's surface is to the fitters. The two must agree. A
    // sample grid lines up with a 45 degree face, so a quarter of a ramp's
    // samples lie exactly on it; counting those as inside while the form's
    // own were counted as outside made a ramp look 62% of its cell, not 50%,
    // and a perfectly fitting ramp look a quarter wrong.
    match info.kind {
        Kind::Cube => true,
        Kind::Tet => a + b + d < 1.0,
        Kind::Cut => a + b + d > 1.0,
        // distance from the missing edge in the two axes across it
        Kind::Ramp => match info.axis {
            0 => b + d > 1.0,
            1 => a + d > 1.0,
            _ => a + b > 1.0,
        },
        // Measured from the apex: at a distance t along the axis towards the
        // whole face, the shape is the square from the apex's corner out to
        // t each way across.
        Kind::Hip => {
            let (t, p, q) = match info.axis {
                0 => (a, b, d),
                1 => (b, a, d),
                _ => (d, a, b),
            };
            p < t && q < t
        }
        Kind::Inner | Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv => unreachable!("answered above"),
    }
}

/// [`inside_shape`] for a point of a regular sample grid, where it matters
/// what happens exactly on a ramp's sloped face.
///
/// A sample grid lines up with a 45 degree face: with 4 samples a side, 4 of
/// a ramp's 16 sample columns lie exactly on it. All in or all out, the ramp
/// would measure 62% or 38% of its box instead of 50%, and every comparison
/// of a ramp against a form would lean one way. So of the samples on the
/// face, the middle half count as inside. "Middle" is measured across the
/// face from the ramp's own corners, so a mirror image of a ramp gets the
/// mirror image of its samples and symmetric forms still fit symmetrically.
/// (Tetrahedra and cut cubes have no samples on their faces at 4 a side.)
pub fn inside_sampled(info: ShapeInfo, u: f64, v: f64, w: f64) -> bool {
    if info.kind == Kind::Inner {
        // what is left of the box by the hip it lacks, sample for sample
        let hip = shape_info(!info.mask).expect("an inner corner's mask is the rest of a hip's");
        return !inside_sampled(hip, u, v, w);
    }
    if inside_shape(info, u, v, w) {
        return true;
    }
    if !info.kind.by_corner() {
        // The half corners' faces line up with the sample grid too. As for
        // a ramp, of the samples on a face the middle half count as inside:
        // 22 of a cell's 64 for the half outer corner, where a third is
        // 21.3, and 12 for a half inner, where a sixth is 10.7. (10 can be
        // had for the half inner by taking the low half of one face and the
        // high half of the other, but which face is which then depends on
        // which way round the brick is thought of, and a brick and its
        // mirror image could be counted differently.)
        let l = info.local([u, v, w]);
        let sides = sloped_sides(info.kind, l);
        // each face's samples are told apart by the coordinate that runs along it
        let along = match info.kind {
            Kind::HalfInner => [l[0], l[1]],
            _ => [l[0], l[0]],
        };
        return (0..2).all(|k| sides[k] < -1e-9 || (sides[k].abs() < 1e-9 && (along[k] - 0.5).abs() < 0.25));
    }
    if info.kind == Kind::Hip {
        // A hip's two sloped faces line up with the sample grid as a ramp's
        // one does. Of the samples on either face, those in the half of it
        // nearer the apex count as inside, which brings 22 of a cell's 64
        // samples in where a third of them is 21.3; all in would be 30 and
        // all out 14. Measured from the hip's own apex, so mirror images
        // get mirror-image samples.
        let c = info.corner;
        let f = [if c & 1 != 0 { 1.0 - u } else { u }, if c & 2 != 0 { 1.0 - v } else { v }, if c & 4 != 0 { 1.0 - w } else { w }];
        let (t, p, q) = match info.axis {
            0 => (f[0], f[1], f[2]),
            1 => (f[1], f[0], f[2]),
            _ => (f[2], f[0], f[1]),
        };
        let on = |x: f64| (x - t).abs() < 1e-9;
        return (on(p) && q < t / 2.0) || (on(q) && p < t / 2.0);
    }
    if info.kind != Kind::Ramp {
        return false;
    }
    let c = info.corner;
    let a = if c & 1 != 0 { 1.0 - u } else { u };
    let b = if c & 2 != 0 { 1.0 - v } else { v };
    let d = if c & 4 != 0 { 1.0 - w } else { w };
    let (p, q) = match info.axis {
        0 => (b, d),
        1 => (a, d),
        _ => (a, b),
    };
    (p + q - 1.0).abs() < 1e-9 && (p - 0.5).abs() < 0.25
}

/// Of several candidates' scores, the index of the lowest that no other
/// candidate shares.
///
/// Used wherever a fitter picks a shape for a cell. Taking simply the lowest
/// would be just as close a fit, but an exact tie is almost always two mirror
/// images (two ramps facing opposite ways on a solid's plane of symmetry) and
/// then "whichever comes first" gives one half of a symmetric form a
/// different brick from the other half. A choice nothing ties with is the
/// same seen from either side, at the cost of a slightly worse fit in those
/// few cells.
///
/// If every score is shared there is no such choice. Then the answer is the
/// lowest of the first `plain` candidates, which the caller promises look
/// the same from every side (nothing, and the full square or cube). Taking
/// the first lowest of them all, as this once did, picked the same ramp for
/// two cells that were mirror images of each other, at the crown of an arch.
pub(crate) fn least_unshared(scores: &[(u32, u32)], plain: usize) -> usize {
    let shared = |s: &(u32, u32)| scores.iter().filter(|o| *o == s).count() > 1;
    let lowest = scores.iter().min().expect("no candidates");
    if !shared(lowest) {
        return scores.iter().position(|s| s == lowest).unwrap();
    }
    (0..scores.len())
        .filter(|&i| !shared(&scores[i]))
        .min_by_key(|&i| scores[i])
        .unwrap_or_else(|| {
            (0..plain.clamp(1, scores.len()))
                .min_by_key(|&i| scores[i])
                .unwrap()
        })
}

/// A brick in cell space: `min` is its lowest cell, `ext` its size in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellBrick {
    pub mask: u8,
    pub min: [i32; 3],
    pub ext: [i32; 3],
}

impl CellBrick {
    pub fn volume(&self) -> f64 {
        let v = (self.ext[0] * self.ext[1] * self.ext[2]) as f64;
        shape_info(self.mask).map_or(0.0, |s| s.kind.volume() * v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientations_are_the_24_rotations() {
        let mut seen = std::collections::HashSet::new();
        for o in ORIENT {
            // a proper rotation: permutation parity times the sign flips is +1
            let mut p = o.perm;
            let mut swaps = 0;
            for i in 0..3 {
                while p[i] as usize != i {
                    let j = p[i] as usize;
                    p.swap(i, j);
                    swaps += 1;
                }
            }
            assert_eq!(
                (swaps + o.neg.count_ones()) % 2,
                0,
                "improper: {:?} {:03b}",
                o.perm,
                o.neg
            );
            assert!(seen.insert((o.perm, o.neg)));
        }
        assert_eq!(seen.len(), 24);
    }

    /// omegga's getScaleAxis (src/util/brick/getScaleAxis.ts), which says
    /// which local size component lies along a world axis.
    fn get_scale_axis(direction: usize, rotation: usize, mut axis: usize) -> usize {
        if direction < 2 {
            axis = [2, 1, 0][axis];
        } else if direction < 4 {
            axis = [1, 2, 0][axis];
        }
        if rotation % 2 == 1 {
            axis = [1, 0, 2][axis];
        }
        axis
    }

    #[test]
    fn perm_agrees_with_omegga_get_scale_axis() {
        for (n, o) in ORIENT.iter().enumerate() {
            for axis in 0..3 {
                assert_eq!(
                    o.perm[axis] as usize,
                    get_scale_axis(n / 4, n % 4, axis),
                    "orientation {n}"
                );
            }
        }
    }

    #[test]
    fn the_newer_shapes_have_their_poses_volumes_and_corners() {
        for (kind, held) in [(Kind::Inner, 7), (Kind::HalfOuter, 5), (Kind::HalfInner, 4), (Kind::HalfInnerInv, 4)] {
            let masks: Vec<u8> = all_masks().into_iter().filter(|&m| shape_info(m).unwrap().kind == kind).collect();
            // (a half inner corner turned end for end is itself again)
            assert_eq!(masks.len(), if held == 4 { 12 } else { 24 }, "{kind:?}");
            for mask in masks {
                let info = shape_info(mask).unwrap();
                assert_eq!(corners(mask).count_ones(), held);
                // to and from its canonical box
                let p = [0.1, 0.25, 0.7];
                let back = info.world(info.local(p));
                assert!((0..3).all(|k| (back[k] - p[k]).abs() < 1e-12));
                // its volume, on a fine grid and on the 64 samples of a cell
                let n = 40;
                let at = |i: usize, n: usize| (i as f64 + 0.5) / n as f64;
                let fine = (0..n * n * n).filter(|s| inside_shape(info, at(s % n, n), at(s / n % n, n), at(s / (n * n), n))).count() as f64 / (n * n * n) as f64;
                assert!((fine - kind.volume()).abs() < 0.02, "{kind:?} {mask:#x}: {fine}");
                let coarse = (0..64).filter(|s| inside_sampled(info, at(s % 4, 4), at(s / 4 % 4, 4), at(s / 16, 4))).count() as f64;
                assert!((coarse - 64.0 * kind.volume()).abs() < 1.5, "{kind:?} {mask:#x}: {coarse} of 64");
                // each corner it holds is on it, and the middle of each sloped face is on its surface
                let (triangles, within) = sloped_triangles(kind);
                assert!(inside_shape(info, info.world(within)[0], info.world(within)[1], info.world(within)[2]));
                for t in triangles {
                    let mid = [0, 1, 2].map(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
                    let [inwards, outwards] = [0.01, -0.01].map(|step| info.world([0, 1, 2].map(|k| mid[k] + (within[k] - mid[k]) * step)));
                    assert!(inside_shape(info, inwards[0], inwards[1], inwards[2]) && !inside_shape(info, outwards[0], outwards[1], outwards[2]), "{kind:?} {mask:#x}");
                }
            }
        }
        // a half inner corner and its mirror image are different bricks
        let mirrored = |mask: u8| (0..8u8).fold(0u8, |m, c| m | (mask >> c & 1) << (c ^ 1));
        assert_eq!(shape_info(mirrored(0x47)).unwrap().kind, Kind::HalfInnerInv);
        assert_eq!(shape_info(mirrored(0x07)).unwrap().kind, Kind::Inner);
        // and a hip with the inner corner that lacks it fills the box
        for hip in all_masks().into_iter().filter(|&m| shape_info(m).unwrap().kind == Kind::Hip) {
            let (a, b) = (shape_info(hip).unwrap(), shape_info(!hip).unwrap());
            assert_eq!(b.kind, Kind::Inner);
            let at = |i: usize| (i as f64 + 0.5) / 4.0;
            assert!((0..64).all(|s| inside_sampled(a, at(s % 4), at(s / 4 % 4), at(s / 16)) != inside_sampled(b, at(s % 4), at(s / 4 % 4), at(s / 16))));
        }
    }

    #[test]
    fn quarter_turns_compose_as_turns_do() {
        // four quarter turns about any axis are no turn; and each is a turn about its own axis
        for axis in 0..3 {
            let q = quarter(axis);
            assert_eq!((0..4).fold(UPRIGHT, |o, _| compose(o, q)), UPRIGHT);
            assert_eq!(ORIENT[q as usize].perm[axis] as usize, axis);
            assert_eq!(ORIENT[q as usize].neg >> axis & 1, 0);
            assert_ne!(q, UPRIGHT);
        }
        // x towards y: the corner at +x goes to +y
        assert_eq!(transform_mask(0b0000_0010, quarter(2) as usize), 0b0000_1000);
        // turning a shape by two turns in a row is turning it by the two together
        for first in 0..24u8 {
            for then in [quarter(0), quarter(1), quarter(2), 7, 22] {
                assert_eq!(transform_mask(transform_mask(0x1f, first as usize), then as usize), transform_mask(0x1f, compose(first, then) as usize));
            }
        }
    }

    #[test]
    fn there_are_29_shapes() {
        let count = |k| {
            all_masks()
                .iter()
                .filter(|&&m| shape_info(m).unwrap().kind == k)
                .count()
        };
        assert_eq!(count(Kind::Cube), 1);
        assert_eq!(count(Kind::Ramp), 12);
        assert_eq!(count(Kind::Tet), 8);
        assert_eq!(count(Kind::Cut), 8);
        assert_eq!(count(Kind::Hip), 24);
        // a hip's apex and axis are read back from its corners correctly
        let hip = shape_info(0x1f).unwrap();
        assert_eq!((hip.corner, hip.axis), (4, 2));
        let turned = shape_info(0b1010_1011).unwrap();
        assert_eq!((turned.kind, turned.corner, turned.axis), (Kind::Hip, 0, 0));
    }

    #[test]
    fn inside_shape_matches_the_corners_and_the_volume() {
        for mask in all_masks() {
            let info = shape_info(mask).unwrap();
            if !info.kind.by_corner() {
                continue; // see `the_newer_shapes_have_their_poses_volumes_and_corners`
            }
            // the middle of the shape's own corners, which is inside it
            let kept = mask.count_ones() as f64;
            let mid = |bit: u8| {
                (0..8u8)
                    .filter(|c| mask >> c & 1 == 1)
                    .map(|c| (c >> bit & 1) as f64)
                    .sum::<f64>()
                    / kept
            };
            for c in 0..8u8 {
                // a kept corner lies on the surface, so step off it towards the
                // shape's middle; a missing one, towards the box's
                let keep = mask >> c & 1 == 1;
                let at = |bit: u8| {
                    let (corner, towards) =
                        ((c >> bit & 1) as f64, if keep { mid(bit) } else { 0.5 });
                    corner + (towards - corner) * 0.02
                };
                assert_eq!(
                    inside_shape(info, at(0), at(1), at(2)),
                    keep,
                    "mask {mask:#x} corner {c}"
                );
            }
            let n: usize = 24;
            let mut inside = 0;
            for i in 0..n * n * n {
                let f = |k: usize| ((i / n.pow(k as u32)) % n) as f64 / n as f64 + 0.5 / n as f64;
                inside += inside_shape(info, f(0), f(1), f(2)) as usize;
            }
            let volume = inside as f64 / (n * n * n) as f64;
            // samples lying exactly on a sloped face all count as outside, hence the margin
            assert!(
                (volume - info.kind.volume()).abs() < 0.03,
                "mask {mask:#x}: {volume}"
            );
        }
    }
}
