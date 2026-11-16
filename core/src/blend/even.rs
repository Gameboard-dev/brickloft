//! Evening out a join fitted cell by cell: fewer teeth, for a little less
//! closeness.
//!
//! Each cell of a join is first given the brick that disagrees least with
//! the joined solid inside that cell, with no regard to its neighbours
//! (`fit_cell`). On a surface that slopes gently, or runs askew to all three
//! axes, the best brick changes from cell to cell: a ramp, nothing, a ramp,
//! a tetrahedron. Each is right by volume and together they are a row of
//! teeth, which is what a seam was complained of for. What the eye objects
//! to is the upright sides of the teeth: faces of bricks that stand where
//! the solid has no face at all.
//!
//! So each cell is chosen again, by what it costs in two things together:
//!
//! ```text
//! cost = disagreement with the solid in the cell  +  STEP * steps on the cell's six faces
//! ```
//!
//! A **step** is measured on the face between two cells: the part of it
//! where one cell's brick comes up to the face and the other's does not.
//! Bricks that follow a sloping surface exactly have none, since both come
//! up to the same part of the face they share: a ramp against the cube it
//! leans on, two ramps side by side, the tetrahedra and cut cubes of a
//! surface askew to all three axes. A tooth has three. A step is not counted
//! where the solid itself has one, which is where its surface lies flat in
//! the face (the top of a cube), or the bricks would be drawn away from
//! every flat face that lies on the cells' own planes.
//!
//! The choice for one cell depends on its neighbours' bricks, so it is made
//! over and over (iterated conditional modes,
//! <https://en.wikipedia.org/wiki/Iterated_conditional_modes>). The cells
//! are taken as the two colours of a chessboard in turn. A cell's six
//! neighbours are all of the other colour, so every cell of one colour can
//! be chosen from the same standing bricks: the result does not depend on
//! the order they are taken in, and each choice lowers the total cost, so
//! the passes end. Choosing every cell at once from the bricks as they stood
//! was the alternative: two neighbours can then each change to suit the
//! other as it was, and change back on the next pass, for ever.
//!
//! Two other ways to be rid of the teeth were weighed. Dropping every
//! tetrahedron with fewer than two faces backed is done already
//! (`fit_volume::drop_spikes`) and stays; it knows nothing of ramps or cut
//! cubes, and removes where a neighbouring change would have served better.
//! A price for each brick, as the planar fitter has, was tried in the volume
//! fitter and makes pits (see the note in `fit_volume`): a tetrahedron in an
//! inside corner costs as much as one standing alone. Counting steps prices
//! the one and not the other.

use crate::fit_volume::{patterns, K};
use crate::shapes::{inside_shape, least_unshared, shape_info, CUBE};
use std::sync::OnceLock;

/// What a sixteenth of a cell's face of step is counted as, in sixty-fourths
/// of a cell of disagreement: the two are a cell's samples on a face and in
/// its volume, so that everything is whole numbers.
///
/// Measured on the three joins of `tests::seams` at blends of 0, 2 and 6,
/// before any brick is stretched: at 1 the steps were 8 to 18% fewer than
/// with no evening out, the teeth standing alone went from 36 to 5, and the
/// disagreement with the solid stayed within 2% either way. At 2 and 4 the
/// steps fell by 2 to 5% more and the disagreement rose by 1 to 4%: little
/// more to gain, for a surface pressed flatter than the solid is.
pub(super) const STEP: u32 = 1;

/// Passes over both colours at most. The choices settle in three or four on
/// the joins measured; the bound is against a slow tail on some other.
const PASSES: usize = 8;

/// For each brick a cell can hold, by its mask: the part of each of the
/// cell's six faces it comes up to, as sixteen samples a face. Face
/// `2 * axis` is the low one along that axis and `2 * axis + 1` the high.
/// The samples are numbered along the lower of the other two axes first, so
/// that two cells number the face they share alike.
fn faces() -> &'static [[u16; 6]; 256] {
    static FACES: OnceLock<[[u16; 6]; 256]> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut all = [[0u16; 6]; 256];
        for &(mask, _) in patterns().iter().skip(1) {
            let info = shape_info(mask).expect("a pattern is a shape");
            for face in 0..6 {
                let (axis, (p, q)) = (face / 2, [(1, 2), (0, 2), (0, 1)][face / 2]);
                for s in 0..K * K {
                    let mut at = [0.0; 3];
                    (at[axis], at[p], at[q]) = ((face % 2) as f64, ((s % K) as f64 + 0.5) / K as f64, ((s / K) as f64 + 0.5) / K as f64);
                    all[mask as usize][face] |= (inside_shape(info, at[0], at[1], at[2]) as u16) << s;
                }
            }
        }
        all
    })
}

/// The samples of a cell that lie nearest one of its faces, numbered as
/// [`faces`] numbers a face.
fn layer(bits: u64, face: usize) -> u16 {
    let (axis, (p, q)) = (face / 2, [(1, 2), (0, 2), (0, 1)][face / 2]);
    let stride = |a: usize| K.pow(a as u32);
    let base = (face % 2) * (K - 1) * stride(axis);
    (0..K * K).fold(0u16, |all, s| all | ((bits >> (base + (s % K) * stride(p) + (s / K) * stride(q)) & 1) as u16) << s)
}

/// Choose again the brick of every cell the solid's surface crosses, so as
/// to make fewer steps (see the top of this file).
///
/// `shape` is the block of cells as fitted, one mask a cell, `n` cells each
/// way. `sampled` is the solid's samples and corners in each cell its
/// surface crosses, in the order of the cells. `free` says which cells may
/// be changed, and `known` which hold what was fitted to them: a face shared
/// with a cell that is neither costs nothing, since what stands there is
/// not known here. A free cell must not be at the edge of the block.
pub(super) fn even_out(shape: &mut [u8], sampled: &[(usize, u64, u8)], n: [usize; 3], free: impl Fn(usize) -> bool, known: impl Fn(usize) -> bool) {
    let strides = [1, n[0], n[0] * n[1]];
    let (patterns, faces) = (patterns(), faces());
    let mut number = [0u8; 256];
    for (at, (mask, _)) in patterns.iter().enumerate() {
        number[*mask as usize] = at as u8;
    }
    // which entry of `sampled` a cell is, plus one: a table and not a search, for it is read six times a cell a pass
    let mut slot = vec![0u32; shape.len()];
    for (at, s) in sampled.iter().enumerate() {
        slot[s.0] = at as u32 + 1;
    }
    let layers: Vec<[u16; 6]> = sampled.iter().map(|s| std::array::from_fn(|face| layer(s.1, face))).collect();
    let colour = |cell: usize| (cell % n[0] + cell / n[0] % n[1] + cell / (n[0] * n[1])) % 2;
    // A cell is chosen again only if a neighbour's brick has changed since
    // it last was: the choice for any other could only come out as it did.
    let mut stale = vec![true; sampled.len()];
    for _ in 0..PASSES {
        let mut changed = false;
        for turn in 0..2 {
            for (at, &(cell, bits, corners)) in sampled.iter().enumerate() {
                if colour(cell) != turn || !free(cell) || !std::mem::take(&mut stale[at]) {
                    continue;
                }
                // across each face: what the neighbour's brick comes up to, and where a difference from it is a step
                let mut across = [(0u16, 0u16); 6];
                for (face, across) in across.iter_mut().enumerate() {
                    let beside = if face % 2 == 0 { cell - strides[face / 2] } else { cell + strides[face / 2] };
                    if !known(beside) {
                        continue;
                    }
                    let theirs = match slot[beside] {
                        0 => [0, u16::MAX][(shape[beside] == CUBE) as usize],
                        k => layers[k as usize - 1][face ^ 1],
                    };
                    *across = (faces[shape[beside] as usize][face ^ 1], !(layers[at][face] ^ theirs));
                }
                let mut scores = [(0u32, 0u32); 30];
                for (score, (mask, pattern)) in scores.iter_mut().zip(patterns) {
                    let steps: u32 = (0..6).map(|face| ((faces[*mask as usize][face] ^ across[face].0) & across[face].1).count_ones()).sum();
                    *score = ((bits ^ pattern).count_ones() + STEP * steps, (corners ^ mask).count_ones());
                }
                // The cheapest, if it is cheaper than what stands and nothing
                // ties with it: of two that tie one is nearly always the
                // mirror image of the other, and taking whichever comes first
                // fits a symmetric join unsymmetrically (see `least_unshared`).
                let now = number[shape[cell] as usize] as usize;
                let best = least_unshared(&scores, 2);
                if scores[best] < scores[now] {
                    shape[cell] = patterns[best].0;
                    changed = true;
                    for face in 0..6 {
                        let beside = if face % 2 == 0 { cell - strides[face / 2] } else { cell + strides[face / 2] };
                        if let Some(k) = (slot[beside] as usize).checked_sub(1) {
                            stale[k] = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use glam::DVec3;

    /// A block of cells fitted to the solid `f` (negative inside) a cell at
    /// a time, as a join's are: each cell's brick, and the samples and
    /// corners of every cell.
    pub(in crate::blend) fn fitted(f: impl Fn(DVec3) -> f64, n: [usize; 3]) -> (Vec<u8>, Vec<(usize, u64, u8)>) {
        let (mut shape, mut sampled) = (Vec::new(), Vec::new());
        for cell in 0..n[0] * n[1] * n[2] {
            let at = DVec3::new((cell % n[0]) as f64, (cell / n[0] % n[1]) as f64, (cell / (n[0] * n[1])) as f64);
            let bits = (0..K * K * K).fold(0u64, |bits, s| bits | ((f(at + DVec3::new((s % K) as f64 + 0.5, (s / K % K) as f64 + 0.5, (s / (K * K)) as f64 + 0.5) / K as f64) < 0.0) as u64) << s);
            let corners = (0..8).fold(0u8, |m, c| m | ((f(at + DVec3::new((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2) as f64)) <= 0.0) as u8) << c);
            let scores: Vec<(u32, u32)> = patterns().iter().map(|(mask, pattern)| ((bits ^ pattern).count_ones(), (corners ^ mask).count_ones())).collect();
            shape.push(patterns()[least_unshared(&scores, 2)].0);
            sampled.push((cell, bits, corners));
        }
        (shape, sampled)
    }

    /// The steps in a block, in sixteenths of a cell's face, and its
    /// disagreement with the solid, in sixty-fourths of a cell: counted
    /// here face by face, apart from how `even_out` counts them.
    fn steps_and_wrong(shape: &[u8], sampled: &[(usize, u64, u8)], n: [usize; 3]) -> (u32, u32) {
        let strides = [1, n[0], n[0] * n[1]];
        let unit = |mask: u8| patterns().iter().find(|p| p.0 == mask).unwrap().1;
        let (mut steps, mut wrong) = (0, 0);
        for &(cell, bits, _) in sampled {
            wrong += (bits ^ unit(shape[cell])).count_ones();
            let at = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])];
            for axis in (0..3).filter(|a| at[*a] + 1 < n[*a]) {
                let beside = cell + strides[axis];
                let solid_steps = layer(bits, 2 * axis + 1) ^ layer(sampled[beside].1, 2 * axis);
                steps += ((faces()[shape[cell] as usize][2 * axis + 1] ^ faces()[shape[beside] as usize][2 * axis]) & !solid_steps).count_ones();
            }
        }
        (steps, wrong)
    }

    /// Whether a cell is clear of the edge of a block `n` cells each way.
    fn inner(n: [usize; 3]) -> impl Fn(usize) -> bool {
        move |cell| (0..3).all(|a| (1..n[a] - 1).contains(&(cell / [1, n[0], n[0] * n[1]][a] % n[a])))
    }

    fn evened(f: impl Fn(DVec3) -> f64, n: [usize; 3]) -> ((u32, u32), (u32, u32)) {
        let (mut shape, sampled) = fitted(f, n);
        let before = steps_and_wrong(&shape, &sampled, n);
        even_out(&mut shape, &sampled, n, inner(n), |_| true);
        (before, steps_and_wrong(&shape, &sampled, n))
    }

    #[test]
    fn bricks_that_follow_a_surface_exactly_have_no_steps_and_are_left_alone() {
        // a slope of 45 degrees across one axis is ramps on cubes, and one askew to all three is tetrahedra and cut cubes
        let planes: [(&str, fn(DVec3) -> f64); 3] = [("a ramp's slope", |p| p.x + p.z - 9.0), ("a corner's slope", |p| p.x + p.y + p.z - 13.0), ("a flat on the cells' own plane", |p| p.z - 4.0)];
        for (name, plane) in planes {
            let (before, after) = evened(plane, [10, 10, 10]);
            // (a ramp's own samples are not quite a 45 degree solid's, where the samples lie on the slope: see `shapes::inside_sampled`)
            assert!(before.0 == 0 && after == before && before.1 <= 1000, "{name}: steps and disagreement {before:?} before and {after:?} after");
        }
    }

    #[test]
    fn a_gentle_slope_fitted_as_teeth_is_evened_out() {
        // rising 1 in 3 one way and 1 in 7 the other: no unit brick lies along it, and cell by cell it is ramps and tetrahedra standing apart
        let slope = |p: DVec3| (p.z - 3.3 - p.x / 3.0 - p.y / 7.0) / 1.1;
        let n = [14, 14, 12];
        let (before, after) = evened(slope, n);
        assert!(after.0 * 10 < before.0 * 9, "{} steps before and {} after", before.0, after.0);
        // at the cost of some closeness, and never more than the steps taken away are counted as worth
        assert!(after.1 >= before.1 && after.1 - before.1 <= STEP * (before.0 - after.0), "disagreement {} before and {} after, for {} fewer steps", before.1, after.1, before.0 - after.0);
        // and a second go changes nothing: the passes had ended
        let (mut shape, sampled) = fitted(slope, n);
        even_out(&mut shape, &sampled, n, inner(n), |_| true);
        let once = shape.clone();
        even_out(&mut shape, &sampled, n, inner(n), |_| true);
        assert!(shape == once);
    }
}
