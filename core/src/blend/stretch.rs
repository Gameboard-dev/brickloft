//! Stretched sloped bricks for a join: one brick lying along the joined
//! solid's surface where the cell by cell fit has a staircase of small ones.
//!
//! A unit ramp, tetrahedron or cut cube slopes at 45 degrees and no other
//! angle, so a surface at any gentler slope is fitted cell by cell as teeth
//! and steps. The game stretches every brick along each axis by itself, and
//! a stretched one slopes at whatever its three lengths make: a tetrahedron
//! 4 by 2 by 1 cells is a facet rising 1 in 4 one way and 1 in 2 the other.
//! A sphere's own fitter builds its facets of exactly these, laid out from
//! the sphere's centre (`fit_round`). A join's fillet has no centre and no
//! layout, and was fitted with unit bricks and ramps stretched one way
//! (`fit_volume::stretch_ramps`), which is why it showed as a rough band
//! beside a sphere's facets.
//!
//! Here the layout is taken from the solid itself, a cell at a time. Where
//! the surface crosses a cell, the way the solid's distance changes across
//! that cell is the surface's slope there (its gradient). A stretched brick
//! lies flat against a surface of that slope when its lengths along the
//! three axes are in the inverse proportions of the gradient's three parts:
//!
//! ```text
//! length along x : length along y : length along z  =  1 / |gx| : 1 / |gy| : 1 / |gz|
//! ```
//!
//! A part of the gradient too small for any length allowed to match it is
//! taken as none: with one such part the brick is a ramp, level along that
//! axis, and with all three parts the brick is a tetrahedron or a cut cube,
//! whichever the solid holds more of. Each such brick, at a few sizes, is
//! tried in the one place where the cell is the first that its sloped face
//! crosses, and is a candidate if it disagrees with the solid less than the
//! unit bricks standing in its box do. Candidates are then taken largest
//! saving first, as stretched ramps are, and none overlaps another.
//!
//! No candidate is ever worse by volume than what it replaces, so the join's
//! closeness to the solid only improves. Neighbouring bricks are chosen
//! independently, and agree along their edges only as well as each agrees
//! with the solid: this is not a mesh, and a sliver of step is left between
//! two facets of different slope, as between two of a sphere's.
//!
//! Trying every stretched brick in every position was the alternative, and
//! needs no gradient: it is some 60 sizes of 28 bricks in up to 200
//! positions for every cell, a thousand times the work, to find the same
//! bricks, since a brick that does not lie along the gradient cannot fit.

use crate::fit_volume::{patterns, K};
use crate::par::Threads;
use crate::shapes::{inside_sampled, shape_info, CellBrick, Kind};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// The longest a stretched brick may be along any axis, in cells, and the
/// most cells its box may hold. Six is the volume fitter's longest ramp. The
/// second bound is on the work: a candidate is measured over every cell of
/// its box.
const LONGEST: i32 = 6;
const MOST_CELLS: i32 = 48;

/// The sizes tried for one slope: the lengths in the proportions above, the
/// shortest of them this many cells. Halves let a slope of 2 in 3 be a brick
/// 2 by 3 where 1 by 1 and 1 by 2 are both far from it.
const SCALES: [f64; 5] = [1.0, 1.5, 2.0, 3.0, 4.0];

/// A stretched brick's samples in each cell of its box, x fastest: for
/// measuring it against the solid's.
type Samples = Arc<[u64]>;

/// The samples of the brick `mask` stretched to `ext` cells.
///
/// Kept from one join to the next, since a join being dragged asks for the
/// same few hundred bricks every time and working one out is a thousand
/// inside-or-outside tests. It is a table of facts about bricks, the same
/// whoever fills it and in whatever order, so sharing it among threads
/// changes no result. (<https://doc.rust-lang.org/std/sync/struct.RwLock.html>)
fn samples(mask: u8, ext: [i32; 3]) -> Samples {
    static KNOWN: OnceLock<RwLock<HashMap<(u8, [i32; 3]), Samples>>> = OnceLock::new();
    let known = KNOWN.get_or_init(Default::default);
    if let Some(found) = known.read().ok().and_then(|k| k.get(&(mask, ext)).cloned()) {
        return found;
    }
    let info = shape_info(mask).expect("a stretched brick is a shape");
    let part = |i: usize, cell: i32, len: i32| (cell as f64 + (i as f64 + 0.5) / K as f64) / len as f64;
    let mut all = Vec::with_capacity((ext[0] * ext[1] * ext[2]) as usize);
    for z in 0..ext[2] {
        for y in 0..ext[1] {
            for x in 0..ext[0] {
                all.push((0..K * K * K).fold(0u64, |bits, s| bits | (inside_sampled(info, part(s % K, x, ext[0]), part(s / K % K, y, ext[1]), part(s / (K * K), z, ext[2])) as u64) << s));
            }
        }
    }
    let all: Samples = all.into();
    if let Ok(mut known) = known.write() {
        known.insert((mask, ext), all.clone());
    }
    all
}

/// The bricks that could lie along a surface whose distance changes by
/// `slope` across a cell: their masks, and the sizes to try them at.
fn lying_along(slope: [f64; 3], max_ext: i32) -> (Vec<u8>, Vec<[i32; 3]>) {
    let steep = slope.map(f64::abs);
    let most = steep.iter().copied().fold(0.0, f64::max);
    if !(most > 1e-6 && most.is_finite()) {
        return (Vec::new(), Vec::new());
    }
    let longest = LONGEST.min(max_ext);
    // an axis the surface is level along, as far as any brick allowed could tell
    let level = steep.map(|s| s / most < 1.0 / (longest as f64 + 0.5));
    let mut sizes: Vec<[i32; 3]> = Vec::new();
    for scale in SCALES {
        let ext = [0, 1, 2].map(|k| if level[k] { 1 } else { (scale * most / steep[k]).round().max(1.0) as i32 });
        if ext != [1; 3] && ext.iter().all(|e| *e <= longest) && ext.iter().product::<i32>() <= MOST_CELLS && !sizes.contains(&ext) {
            sizes.push(ext);
        }
    }
    // the corner of the box deepest in the solid: at the low end of an axis the distance rises along
    let deep = (0..3).fold(0u8, |c, k| c | ((slope[k] < 0.0) as u8) << k);
    let with = |keep: &dyn Fn(u8) -> bool| (0..8u8).filter(|c| keep(*c)).fold(0u8, |mask, c| mask | 1 << c);
    let masks: Vec<u8> = match level.iter().filter(|l| **l).count() {
        // a tetrahedron with its right angle there, and a cube less the corner opposite
        0 => vec![with(&|c| (c ^ deep).count_ones() <= 1), with(&|c| c != deep ^ 7)],
        // a ramp level along that axis: a box less the edge furthest from the solid
        1 => {
            let across = (0..3).filter(|k| !level[*k]).fold(0u8, |m, k| m | 1 << k);
            vec![with(&|c| (c ^ deep) & across != across)]
        }
        // level along two axes: a flat face, which blocks fit already
        _ => Vec::new(),
    };
    debug_assert!(masks.iter().all(|m| shape_info(*m).is_some_and(|s| matches!(s.kind, Kind::Tet | Kind::Cut | Kind::Ramp))));
    (masks.into_iter().filter(|m| shape_info(*m).is_some()).collect(), sizes)
}

struct Candidate {
    saved: u32,
    start: usize,
    mask: u8,
    ext: [i32; 3],
}

/// Replace the unit bricks of a join with stretched ones wherever one lies
/// closer to the solid (see the top of this file). Returns the stretched
/// bricks; the cells they take are cleared from `shape`.
///
/// `shape` is the block of cells as fitted, `n` cells each way. `crossed`
/// is the solid's samples in each cell its surface crosses, in the order of
/// the cells, and `slopes` how its distance changes across each of them.
/// `free` says which cells a brick may take: every cell of its box must be
/// one.
pub(super) fn stretch(shape: &mut [u8], crossed: &[(usize, u64)], slopes: &[[f64; 3]], n: [usize; 3], max_ext: usize, free: &(impl Fn(usize) -> bool + Sync), threads: Threads) -> Vec<CellBrick> {
    let strides = [1, n[0], n[0] * n[1]];
    let mut unit = [0u64; 256];
    for &(mask, pattern) in patterns() {
        unit[mask as usize] = pattern;
    }
    // which entry of `crossed` a cell is, plus one (as in `fit_volume::stretch_ramps`, and for its reason)
    let mut slot = vec![0u32; shape.len()];
    for (at, c) in crossed.iter().enumerate() {
        slot[c.0] = at as u32 + 1;
    }
    let (longest_run, max_ext) = (max_ext.max(1) as i32, max_ext.min(LONGEST as usize) as i32);
    // The bricks each cell asks for, and their samples: looked up here, on
    // one thread, so that the measuring below reads a finished table.
    let asked: Vec<(Vec<u8>, Vec<[i32; 3]>)> = crossed.iter().zip(slopes).map(|(c, slope)| if free(c.0) { lying_along(*slope, max_ext) } else { (Vec::new(), Vec::new()) }).collect();
    let mut table: HashMap<(u8, [i32; 3]), (Samples, [i32; 3])> = HashMap::new();
    for (masks, sizes) in &asked {
        for (&mask, &ext) in masks.iter().flat_map(|m| sizes.iter().map(move |e| (m, e))) {
            table.entry((mask, ext)).or_insert_with(|| {
                let cells = samples(mask, ext);
                // the first cell its sloped face crosses: the one a placement is tried from
                let lead = cells.iter().position(|bits| *bits != 0 && *bits != u64::MAX).unwrap_or(0) as i32;
                (cells, [lead % ext[0], lead / ext[0] % ext[1], lead / (ext[0] * ext[1])])
            });
        }
    }
    let standing: &[u8] = shape;
    let found = threads.map_ranges(crossed.len(), 400, |range| {
        let mut found = Vec::new();
        for at in range {
            let (cell, (masks, sizes)) = (crossed[at].0, &asked[at]);
            let here = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])].map(|v| v as i32);
            for (&mask, &ext) in masks.iter().flat_map(|m| sizes.iter().map(move |e| (m, e))) {
                let (cells, lead) = &table[&(mask, ext)];
                let low = [0, 1, 2].map(|k| here[k] - lead[k]);
                if (0..3).any(|k| low[k] < 0 || low[k] + ext[k] > n[k] as i32) {
                    continue;
                }
                let start = low[0] as usize + n[0] * (low[1] as usize + n[1] * low[2] as usize);
                let (mut now, mut then, mut fits) = (0u32, 0u32, true);
                'cells: for z in 0..ext[2] as usize {
                    for y in 0..ext[1] as usize {
                        for x in 0..ext[0] as usize {
                            let c = start + x + y * strides[1] + z * strides[2];
                            let brick = cells[x + ext[0] as usize * (y + ext[1] as usize * z)];
                            let solid = match slot[c] {
                                0 => unit[standing[c] as usize],
                                k => crossed[k as usize - 1].1,
                            };
                            // Not into a cell that is not the join's to fit, and
                            // not into one the solid does not reach at all: the
                            // thin end of a brick there adds little by volume and
                            // pokes through whatever flat face the solid has.
                            if !free(c) || (brick != 0 && solid == 0) {
                                fits = false;
                                break 'cells;
                            }
                            now += (solid ^ unit[standing[c] as usize]).count_ones();
                            then += (solid ^ brick).count_ones();
                        }
                    }
                }
                if fits && then < now {
                    found.push(Candidate { saved: now - then, start, mask, ext });
                }
            }
        }
        found
    });
    let mut found: Vec<Candidate> = found.into_iter().flatten().collect();
    // Largest saving first, and the larger brick first among equals, as
    // `stretch_ramps` has it. Of equal candidates, those that want no cell
    // another of them wants are taken first, and then the rest in the order
    // of where they start. `stretch_ramps` drops all of the rest, so that
    // a symmetric form is never fitted unsymmetrically by whichever of two
    // mirror images came first. Here that dropped nearly every brick on a
    // flat slope: a plane rising 1 in 4 one way and 1 in 2 the other passes
    // through the corners of a box 4 by 2 by 1 and of the same box moved 2
    // along and 1 back, the two fit equally and overlap, and neither was
    // laid. A join that is symmetric may therefore be fitted a little
    // unsymmetrically where two such bricks tie.
    let key = |c: &Candidate| (std::cmp::Reverse(c.saved), std::cmp::Reverse(c.ext.iter().product::<i32>()));
    found.sort_by_key(|c| (key(c), c.start, c.mask, c.ext));
    found.dedup_by_key(|c| (c.start, c.mask, c.ext));
    let cells_of = |c: &Candidate| {
        let (start, ext) = (c.start, c.ext.map(|e| e as usize));
        (0..ext[2]).flat_map(move |z| (0..ext[1]).flat_map(move |y| (0..ext[0]).map(move |x| start + x + y * strides[1] + z * strides[2])))
    };
    let mut taken = vec![false; shape.len()];
    let mut wanted: HashMap<usize, u32> = HashMap::new();
    let mut bricks = Vec::new();
    let mut at = 0;
    while at < found.len() {
        let end = at + found[at..].iter().take_while(|c| key(c) == key(&found[at])).count();
        let live: Vec<&Candidate> = found[at..end].iter().filter(|c| cells_of(c).all(|k| !taken[k])).collect();
        wanted.clear();
        for c in &live {
            cells_of(c).for_each(|k| *wanted.entry(k).or_insert(0) += 1);
        }
        let alone = |c: &&&Candidate| cells_of(c).all(|k| wanted[&k] == 1);
        for c in live.iter().filter(alone).chain(live.iter().filter(|c| !alone(c))) {
            if cells_of(c).any(|k| taken[k]) {
                continue;
            }
            for k in cells_of(c) {
                taken[k] = true;
                shape[k] = 0;
            }
            bricks.push(CellBrick { mask: c.mask, min: [c.start % n[0], c.start / n[0] % n[1], c.start / (n[0] * n[1])].map(|v| v as i32), ext: c.ext });
        }
        at = end;
    }
    // stretched ramps side by side along the axis they are level along are one longer ramp
    let level = |b: &CellBrick| shape_info(b.mask).filter(|s| s.kind == Kind::Ramp).map(|s| s.axis as usize);
    bricks.sort_by_key(|b| {
        let mut across = b.min;
        if let Some(a) = level(b) {
            across[a] = 0;
        }
        (b.mask, b.ext, across, b.min)
    });
    let mut joined: Vec<CellBrick> = Vec::with_capacity(bricks.len());
    for b in bricks {
        if let (Some(a), Some(last)) = (level(&b), joined.last_mut()) {
            if last.mask == b.mask && last.min[a] + last.ext[a] == b.min[a] && last.ext[a] + b.ext[a] <= longest_run && (0..3).filter(|k| *k != a).all(|k| last.min[k] == b.min[k] && last.ext[k] == b.ext[k]) {
                last.ext[a] += b.ext[a];
                continue;
            }
        }
        joined.push(b);
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::cells;
    use crate::blend::even::tests::fitted;
    use glam::DVec3;

    fn index(at: [i32; 3], n: [usize; 3]) -> usize {
        at[0] as usize + n[0] * (at[1] as usize + n[1] * at[2] as usize)
    }

    /// The stretched bricks laid on a plane through `n` cells each way
    /// whose distance changes by `slope` a cell, and the disagreement with
    /// it before and after, in sixty-fourths of a cell.
    fn laid(slope: DVec3, through: DVec3, n: [usize; 3]) -> (Vec<CellBrick>, u32, u32) {
        let (mut shape, sampled) = fitted(|p| (p - through).dot(slope), n);
        let crossed: Vec<(usize, u64)> = sampled.iter().map(|s| (s.0, s.1)).collect();
        let unit = |mask: u8| patterns().iter().find(|p| p.0 == mask).unwrap().1;
        let before: u32 = crossed.iter().map(|c| (c.1 ^ unit(shape[c.0])).count_ones()).sum();
        let bricks = stretch(&mut shape, &crossed, &vec![slope.to_array(); crossed.len()], n, 80, &|_| true, Threads::ONE);
        // what is left of the unit bricks, and each stretched brick against the plane's samples in its cells
        let mut after: u32 = crossed.iter().map(|c| (c.1 ^ unit(shape[c.0])).count_ones()).sum();
        for b in &bricks {
            for (at, brick) in cells(&CellBrick { mask: b.mask, min: [0; 3], ext: b.ext }).zip(samples(b.mask, b.ext).iter()) {
                let solid = crossed[index([0, 1, 2].map(|k| b.min[k] + at[k]), n)].1;
                // (the cell was counted above as empty, the brick having taken it)
                after = after + (solid ^ brick).count_ones() - solid.count_ones();
            }
        }
        (bricks, before, after)
    }

    #[test]
    fn a_plane_askew_to_all_three_axes_is_laid_with_stretched_corner_pieces_that_lie_in_it() {
        // rising 1 in 4 along x and 1 in 2 along y: the plane through the corners of boxes 4 by 2 by 1
        let (bricks, before, after) = laid(DVec3::new(-0.25, -0.5, 1.0), DVec3::new(8.0, 6.0, 7.0), [16, 12, 12]);
        let kind = |b: &CellBrick| shape_info(b.mask).unwrap().kind;
        let whole: Vec<&CellBrick> = bricks.iter().filter(|b| b.ext == [4, 2, 1]).collect();
        assert!(whole.len() >= 12 && whole.iter().any(|b| kind(b) == Kind::Tet) && whole.iter().any(|b| kind(b) == Kind::Cut), "{} bricks 4 by 2 by 1 of {}: {bricks:?}", whole.len(), bricks.len());
        // every one of them has its sloped face in the plane: none of its samples differs from the plane's
        for b in &whole {
            for (at, brick) in cells(&CellBrick { mask: b.mask, min: [0; 3], ext: b.ext }).zip(samples(b.mask, b.ext).iter()) {
                let cell = [0, 1, 2].map(|k| (b.min[k] + at[k]) as f64);
                let solid = (0..K * K * K).fold(0u64, |bits, s| bits | (((DVec3::from_array(cell) + DVec3::new((s % K) as f64 + 0.5, (s / K % K) as f64 + 0.5, (s / (K * K)) as f64 + 0.5) / K as f64 - DVec3::new(8.0, 6.0, 7.0)).dot(DVec3::new(-0.25, -0.5, 1.0)) < 0.0) as u64) << s);
                assert!((solid ^ brick).count_ones() <= 4, "{b:?} is {} samples off the plane in the cell {at:?} of it", (solid ^ brick).count_ones());
            }
        }
        assert!(after * 4 < before, "disagreement {before} before and {after} after");
    }

    #[test]
    fn a_slope_across_one_axis_is_laid_with_ramps_at_that_slope_and_a_flat_with_nothing() {
        // rising 2 in 3 along y, level along x: ramps 3 by 2 or twice that, which no ramp stretched one way is
        let (bricks, before, after) = laid(DVec3::new(0.0, -2.0, 3.0) / 3.0, DVec3::new(5.0, 6.0, 6.0), [10, 12, 12]);
        assert!(!bricks.is_empty() && bricks.iter().all(|b| shape_info(b.mask).unwrap().kind == Kind::Ramp && b.ext[1] * 2 == b.ext[2] * 3 && b.ext[0] > 1), "{bricks:?}");
        assert!(after * 4 < before, "disagreement {before} before and {after} after");
        assert!(laid(DVec3::Z, DVec3::new(5.0, 5.0, 5.0), [10, 10, 10]).0.is_empty());
    }

    #[test]
    fn no_stretched_brick_shares_a_cell_with_another_or_is_laid_where_it_may_not_be() {
        // a dome: every slope there is, and so every brick
        let (n, middle) = ([28usize, 28, 16], DVec3::new(14.0, 14.0, -6.0));
        let (mut shape, sampled) = fitted(|p| (p - middle).length() - 18.3, n);
        let crossed: Vec<(usize, u64)> = sampled.iter().map(|s| (s.0, s.1)).collect();
        let slopes: Vec<[f64; 3]> = crossed.iter().map(|c| (DVec3::new((c.0 % n[0]) as f64 + 0.5, (c.0 / n[0] % n[1]) as f64 + 0.5, (c.0 / (n[0] * n[1])) as f64 + 0.5) - middle).normalize().to_array()).collect();
        let free = |cell: usize| cell % n[0] >= 14;
        let on = |threads: Threads| stretch(&mut shape.clone(), &crossed, &slopes, n, 80, &free, threads);
        let bricks = on(Threads::ONE);
        assert!(bricks.len() > 20, "only {} stretched bricks on a dome", bricks.len());
        let mut taken = std::collections::HashSet::new();
        for b in &bricks {
            for at in cells(b) {
                assert!(at[0] >= 14 && taken.insert(at), "{b:?} is where it may not be, or shares a cell");
            }
        }
        // and the same bricks on any number of threads
        assert!([2, 5].iter().all(|t| on(Threads::forced(*t)) == bricks));
        stretch(&mut shape, &crossed, &slopes, n, 80, &free, Threads::ONE);
        assert!(taken.iter().all(|at| shape[index(*at, n)] == 0));
    }
}
