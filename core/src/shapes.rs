//! The four microbrick shapes, as subsets of a box's 8 corners.
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

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Cube,
    Ramp,
    Tet,
    Cut,
    /// One whole face of the box and a single corner of the opposite face:
    /// a quarter of a square pyramid, the piece at the corner of a hipped
    /// roof where two slopes meet along a ridge.
    Hip,
}

pub const KINDS: [Kind; 5] = [Kind::Cube, Kind::Ramp, Kind::Tet, Kind::Cut, Kind::Hip];

impl Kind {
    /// The pose every other pose of this kind is a rotation of.
    pub const fn canonical(self) -> u8 {
        match self {
            Kind::Cube => 0xff,
            Kind::Ramp => 0x77, // missing the edge x=1,y=1: solid where x + y <= 1
            Kind::Tet => 0x17,  // corner 0 and its neighbours: x + y + z <= 1
            Kind::Cut => 0x7f,  // missing corner 7: x + y + z <= 2
            Kind::Hip => 0x1f,  // the face z=0 and corner 4 above corner 0: x <= 1 - z and y <= 1 - z
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
        }
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
                    Kind::Cube => (0, 0),
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
                t[mask as usize] = Some(ShapeInfo { kind, corner, axis });
            }
        }
        t
    })
}

/// What shape a corner mask is, or `None` if no single brick has those corners.
pub fn shape_info(mask: u8) -> Option<ShapeInfo> {
    table()[mask as usize]
}

/// Every representable mask: 1 cube, 12 ramps, 8 tets, 8 cuts and 24 hips.
pub fn all_masks() -> Vec<u8> {
    (0..=255u8).filter(|&m| shape_info(m).is_some()).collect()
}

/// Whether the point `(u, v, w)`, each in 0..1 across the box, is in the shape.
pub fn inside_shape(info: ShapeInfo, u: f64, v: f64, w: f64) -> bool {
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
    if inside_shape(info, u, v, w) {
        return true;
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
