//! Fit a sphere or ellipsoid as one faceted solid whose every corner is
//! shared: no steps anywhere, by construction.
//!
//! The volume fitter gives each cell the brick that suits it best and the
//! cells do not agree with one another: a gentle slope comes out as treads
//! with a chamfer between them. Here the whole surface is laid out at once.
//!
//! A sphere is `x² + y² + z² <= r²`: a sum of one term per axis. Cut each
//! axis at chosen distances from the centre, `t0 < t1 < ... < tK`, and number
//! them 0 to K. Those cuts divide space into boxes. Call a box corner inside
//! when the numbers of its three coordinates add up to K or less, and fill
//! each box with the convex hull of its inside corners. Then:
//!
//! - In every box that hull is nothing, a tetrahedron, a cube less a
//!   tetrahedron, a ramp, or the whole box: exactly the four microbricks,
//!   stretched to the box. (The corner sums in a box are `s`, three of
//!   `s + 1`, three of `s + 2` and `s + 3`, so the inside ones are a corner
//!   and its neighbours, or all but one; where a cut pair shares a number,
//!   across the flat middle, the same count gives a ramp.)
//! - Neighbouring boxes share their corners, so their bricks meet face to
//!   face along every edge: the surface has no steps and no gaps.
//! - With `ti = r * sqrt(i / K)` every corner on the surface satisfies
//!   `x² + y² + z² = r²`: it lies exactly on the sphere. Different radii per
//!   axis give an ellipsoid the same way.
//! - If the gaps between cuts never grow going outwards, the solid is convex:
//!   it is the set where a sum of three convex functions is at most K.
//!   https://en.wikipedia.org/wiki/Convex_function#Operations_that_preserve_convexity
//!
//! The cuts must fall on the lattice, so each is rounded to it, and the
//! corners are then within half a cell of the sphere instead of on it. K is
//! taken as large as leaves every gap at least one cell: about half the
//! radius, which puts no facet more than about a quarter of a cell from the
//! true surface.
//!
//! A hollow sphere is the same solid less a smaller one from the same cuts
//! (corner sums of `K - n` or less): the wall is `n` numbered layers thick,
//! and needs `n >= 2` for every box to hold a single brick. A dome or segment
//! is cut off flat at one of the cuts across z.

use crate::shapes::{shape_info, CellBrick, Kind, CUBE};
use glam::DVec3;

/// An ellipsoid to fit. Everything is in cells.
#[derive(Clone, Copy, Debug)]
pub struct Round {
    pub center: DVec3,
    pub radii: DVec3,
    /// the wall's thickness, for a hollow one
    pub shell: Option<f64>,
    /// keep only heights `lo..hi`
    pub lo: f64,
    pub hi: f64,
    pub max_ext: i32,
}

/// How much further out than the true surface the corners may be aimed, in
/// cells. Corners on the surface itself make a solid that lies inside it
/// everywhere else, by up to a quarter of a cell; aiming a little outside
/// shares the disagreement between the two sides. Each is tried and the
/// closest fit kept.
const SWELLS: [f64; 3] = [0.0, 0.15, 0.3];

/// The cuts along one axis, as distances from the centre, numbered from 0.
///
/// `first` is the nearest a cut can be to the centre: 0 if the centre is on a
/// lattice plane, half a cell if it is in the middle of a cell. `first_u` and
/// `step_u` give the ideal: cut `i` belongs where `(t / radius)²` is
/// `first_u + i * step_u`. `fixed` are distances some cut must fall on
/// exactly, for a flat cut through the solid. `None` if no such cuts exist.
///
/// The cuts are the lattice values nearest the ideal ones, in the sense of
/// least total squared distance, among those whose gaps never grow going
/// outwards (which keeps the solid convex). Rounding each cut on its own
/// gives gaps like 2, 1, 2, 1: a surface that folds in and out along every
/// ring. So they are chosen together, by an exact search from the outside in
/// over the few lattice values near each ideal; the state is a cut and the
/// gap outside it. Outside in, because the outer gaps are the narrow ones
/// and it is those the lattice constrains: one cell is the least a gap can be.
/// https://en.wikipedia.org/wiki/Dynamic_programming
fn cuts(
    radius: f64,
    first: f64,
    count: usize,
    first_u: f64,
    step_u: f64,
    fixed: &[f64],
) -> Option<Vec<f64>> {
    let ideal: Vec<f64> = (0..=count)
        .map(|i| radius * (first_u + i as f64 * step_u).max(0.0).sqrt())
        .collect();
    // each fixed distance takes the place of the cut that should be nearest it
    let mut pinned: Vec<Option<f64>> = vec![None; count + 1];
    for &at in fixed {
        if at <= first {
            // across the flat middle, which only its own edge can cut
            if at != first {
                return None;
            }
        } else if at < ideal[count] - 0.5 {
            let i = (1..count)
                .min_by(|a, b| (ideal[*a] - at).abs().total_cmp(&(ideal[*b] - at).abs()))?;
            if pinned[i].is_some_and(|p| p != at) {
                return None;
            }
            pinned[i] = Some(at);
        }
    }
    // the lattice values a cut may take: its pinned one, or those within a cell of its ideal
    let choices = |i: usize| -> Vec<f64> {
        match (i, pinned[i]) {
            (0, _) => vec![first],
            (_, Some(at)) => vec![at],
            _ => {
                let near = first + (ideal[i] - first).round();
                [near - 1.0, near, near + 1.0]
                    .into_iter()
                    .filter(|t| *t > first && (t - ideal[i]).abs() <= 1.0)
                    .collect()
            }
        }
    };
    // states[i]: (cut i, the gap outside it, cost so far, which state of i + 1 it came from)
    let mut states: Vec<Vec<(f64, f64, f64, usize)>> = vec![Vec::new(); count + 1];
    states[count] = choices(count)
        .into_iter()
        .map(|t| (t, 1.0, (t - ideal[count]).powi(2), 0))
        .collect();
    for i in (0..count).rev() {
        let mut found: Vec<(f64, f64, f64, usize)> = Vec::new();
        for t in choices(i) {
            for (from, &(outer, gap, cost, _)) in states[i + 1].iter().enumerate() {
                // a pinned cut is where it must be, whatever that does to the gaps
                let free = pinned[i].is_none() && pinned[i + 1].is_none();
                if outer - t < if free { gap } else { 1.0 } {
                    continue;
                }
                let state = (t, outer - t, cost + (t - ideal[i]).powi(2), from);
                match found.iter_mut().find(|s| s.0 == state.0 && s.1 == state.1) {
                    Some(s) if s.2 > state.2 => *s = state,
                    Some(_) => {}
                    None => found.push(state),
                }
            }
        }
        states[i] = found;
    }
    let mut at = (0..states[0].len()).min_by(|a, b| states[0][*a].2.total_cmp(&states[0][*b].2))?;
    let mut t = Vec::with_capacity(count + 1);
    for level in &states {
        t.push(level[at].0);
        at = level[at].3;
    }
    Some(t)
}

/// Where along an axis the solid's numbering reaches `level`: the cuts read
/// as a function from number back to distance, straight between cuts.
fn reach(t: &[f64], level: f64) -> f64 {
    if level < 0.0 {
        return 0.0;
    }
    let i = (level.floor() as usize).min(t.len() - 1);
    if i + 1 >= t.len() {
        return t[t.len() - 1];
    }
    t[i] + (level - i as f64) * (t[i + 1] - t[i])
}

/// The number the solid gives a distance along an axis: 0 across the flat
/// middle, rising by one from each cut to the next.
fn number(t: &[f64], d: f64) -> f64 {
    if d <= t[0] {
        return 0.0;
    }
    match t.iter().position(|v| *v > d) {
        Some(i) => (i - 1) as f64 + (d - t[i - 1]) / (t[i] - t[i - 1]),
        // beyond the last cut: outside whatever the other axes say
        None => t.len() as f64 * 4.0,
    }
}

/// How far the solid these cuts make is from the true one: the mean distance
/// between their surfaces measured up z, over the part of the solid seen from
/// above. `layers` is the wall's thickness in numbered layers (0 for solid),
/// `inner` the hollow's size as a fraction of the whole.
///
/// The faceted solid is `number(x) + number(y) + number(z) <= count`, so its
/// height over a point is found directly; nothing has to be built to score
/// a choice of cuts, and many can be tried.
fn mismatch(t: &[Vec<f64>; 3], count: usize, layers: usize, radii: [f64; 3], inner: f64) -> f64 {
    const N: usize = 40;
    let span = |k: usize| radii[k].max(t[k][count]);
    let (mut off, mut columns) = (0.0, 0.0);
    for i in 0..N * N {
        let (x, y) = (
            ((i % N) as f64 + 0.5) / N as f64 * span(0),
            ((i / N) as f64 + 0.5) / N as f64 * span(1),
        );
        let flat = number(&t[0], x) + number(&t[1], y);
        let round = (x / radii[0]).powi(2) + (y / radii[1]).powi(2);
        let height = |size: f64, level: usize| {
            let truly = radii[2] * (size * size - round).max(0.0).sqrt();
            let built = if flat > level as f64 {
                0.0
            } else {
                reach(&t[2], level as f64 - flat)
            };
            (truly, built)
        };
        let (truly, built) = height(1.0, count);
        if truly > 0.0 || built > 0.0 {
            columns += 1.0;
            off += (truly - built).abs();
            if layers > 0 {
                let (truly, built) = height(inner, count - layers);
                off += (truly - built).abs();
            }
        }
    }
    off / f64::max(columns, 1.0)
}

/// One axis's cuts laid out through the centre: each position with its number.
pub(crate) fn through(center: f64, t: &[f64]) -> Vec<(i32, usize)> {
    let mut all: Vec<(i32, usize)> = t
        .iter()
        .enumerate()
        .rev()
        .map(|(i, d)| ((center - d).round() as i32, i))
        .collect();
    // a centre on a lattice plane has one cut numbered 0, not two
    all.extend(
        t.iter()
            .enumerate()
            .skip((t[0] == 0.0) as usize)
            .map(|(i, d)| ((center + d).round() as i32, i)),
    );
    all
}

/// The solid left when a brick is taken out of its box.
pub(crate) fn rest_of_box(mask: u8) -> u8 {
    let info = shape_info(mask).expect("a brick");
    let c = info.corner;
    match info.kind {
        Kind::Cube => 0,
        // a hip and the valley that lacks it, whose mask is written as
        // what the hip leaves (see `shapes`)
        Kind::Hip | Kind::Inner => !mask,
        // what these leave is no one brick
        Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv => 0,
        // a corner piece and the cube that lacks it
        Kind::Tet => CUBE & !(1 << c),
        Kind::Cut => 1 << c | 1 << (c ^ 1) | 1 << (c ^ 2) | 1 << (c ^ 4),
        // the ramp with its back to the same slope: it keeps the edge this
        // one lacks, and lacks the edge diagonally across
        Kind::Ramp => {
            let across = 7 & !(1 << info.axis);
            let far = [c ^ across, c ^ across ^ (1 << info.axis)];
            CUBE & !(1 << far[0]) & !(1 << far[1])
        }
    }
}

/// Fit `round`, or `None` where this method does not apply: a solid too
/// small for even one ring of facets, a wall too thin to hold two layers of
/// them, or a centre off the half-cell grid. The caller then fits the solid
/// cell by cell instead.
pub fn fit_round(round: &Round) -> Option<Vec<CellBrick>> {
    let c = round.center.to_array();
    // each centre coordinate on a lattice plane or in the middle of a cell
    let first: [f64; 3] = [0, 1, 2].map(|k| (c[k] - c[k].floor()).min(c[k].ceil() - c[k]));
    if first.iter().any(|f| *f != 0.0 && *f != 0.5)
        || round.radii.to_array().iter().any(|r| !(*r >= 1.0))
    {
        return None;
    }
    // Where the flat cuts fall, on the lattice. A cut asked for through the
    // middle of a row of cells goes to the face above it, both for the lower
    // cut and the upper: a segment then keeps its height, where rounding
    // each cut outwards would make it a cell taller.
    let plane = |z: f64| if z.is_finite() { (z + 0.5).floor() } else { z };
    let (lo_z, hi_z) = (plane(round.lo), plane(round.hi));
    // as distances from the centre, which is how the cuts along z are counted
    let flat: Vec<f64> = [lo_z, hi_z]
        .into_iter()
        .filter(|z| z.is_finite())
        .map(|z| (z - c[2]).abs())
        .collect();
    let true_radii = round.radii.to_array();
    let least = true_radii.iter().copied().fold(f64::INFINITY, f64::min);
    // the hollow, as `sdf::Field::Ellipsoid` has it: the same shape, smaller
    let inner = round.shell.map_or(0.0, |thick| 1.0 - thick / least);
    if round.shell.is_some() && inner <= 0.0 {
        return None; // a wall as thick as the solid: not hollow at all
    }

    // Every number of rings the lattice has room for, at each swell: keep
    // the cuts that come closest to the true solid. More rings are finer
    // facets but each cut is then nearer its neighbours than rounding can
    // place it, so the closest fit is somewhere in between and is found by
    // trying them all; there are few, and scoring one builds nothing.
    let mut found: Vec<(f64, usize, [Vec<f64>; 3], usize)> = Vec::new();
    for count in 1..=least.ceil() as usize {
        for swell in SWELLS {
            let radii = true_radii.map(|r| r + swell);
            let first_u: [f64; 3] = [0, 1, 2].map(|k| (first[k] / radii[k]).powi(2));
            let step_u = (1.0 - first_u.iter().sum::<f64>()) / count as f64;
            let none: [f64; 0] = [];
            let made = [0, 1, 2].map(|k| {
                cuts(
                    radii[k],
                    first[k],
                    count,
                    first_u[k],
                    step_u,
                    if k == 2 { &flat } else { &none },
                )
            });
            let [Some(x), Some(y), Some(z)] = made else {
                continue;
            };
            let t = [x, y, z];
            // a wall: every number of layers from the two it needs, for each
            // box to hold a single brick, up to all but one
            let walls = if round.shell.is_some() {
                2..count.max(2)
            } else {
                0..1
            };
            for layers in walls {
                found.push((
                    mismatch(&t, count, layers, true_radii, inner),
                    count,
                    t.clone(),
                    layers,
                ));
            }
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    found.dedup_by(|a, b| a.2 == b.2 && a.3 == b.3);
    // Axes that are alike keep the same cuts, so that a sphere stays the
    // same seen along each of them.
    let alike = |a: usize, b: usize| {
        true_radii[a] == true_radii[b]
            && first[a] == first[b]
            && (flat.is_empty() || (a != 2 && b != 2))
    };
    // The few closest are then improved a cut at a time: each cut is tried
    // one cell in and one cell out, wherever that leaves the gaps in order,
    // and the best such move made until none helps. The search above places
    // cuts by their distance from the ideal; this places them by what that
    // does to the solid, which is the thing wanted, and from a good start
    // it has little left to do.
    // https://en.wikipedia.org/wiki/Hill_climbing
    let mut best: Option<(f64, usize, [Vec<f64>; 3], usize)> = None;
    for (mut score, count, mut t, layers) in found.into_iter().take(4) {
        while count <= 64 {
            let mut better: Option<(f64, [Vec<f64>; 3])> = None;
            for axis in (0..3).filter(|a| (0..*a).all(|b| !alike(*a, b))) {
                for i in 1..=count {
                    for step in [-1.0, 1.0] {
                        let mut moved = t.clone();
                        for k in (0..3).filter(|k| alike(axis, *k)) {
                            moved[k][i] += step;
                        }
                        let line = &moved[axis];
                        let gaps: Vec<f64> = line.windows(2).map(|w| w[1] - w[0]).collect();
                        let pinned = axis == 2 && flat.contains(&t[2][i]);
                        if pinned
                            || gaps.iter().any(|g| *g < 1.0)
                            || gaps.windows(2).any(|g| g[1] > g[0])
                        {
                            continue;
                        }
                        let now = mismatch(&moved, count, layers, true_radii, inner);
                        if now < better.as_ref().map_or(score, |b| b.0) - 1e-9 {
                            better = Some((now, moved));
                        }
                    }
                }
            }
            match better {
                Some((now, moved)) => (score, t) = (now, moved),
                None => break,
            }
        }
        if best.as_ref().is_none_or(|b| score < b.0 - 1e-9) {
            best = Some((score, count, t, layers));
        }
    }
    let (score, count, t, layers) = best?;
    // A wall further than this from the true one, counting both its faces,
    // is too thin for the layers the lattice can give it.
    if round.shell.is_some() && score > 1.0 {
        return None;
    }

    let lines = [0, 1, 2].map(|k| through(c[k], &t[k]));
    let n = [0, 1, 2].map(|k| lines[k].len() - 1);
    // what each box holds, x fastest
    let mut held = vec![0u8; n[0] * n[1] * n[2]];
    for z in 0..n[2] {
        // boxes beyond a flat cut hold nothing; the cuts are on box faces
        if (lines[2][z].0 as f64) < lo_z || (lines[2][z + 1].0 as f64) > hi_z {
            continue;
        }
        for y in 0..n[1] {
            for x in 0..n[0] {
                let at = [x, y, z];
                let within = |limit: usize| {
                    (0..8u8).fold(0u8, |m, corner| {
                        let sum: usize = (0..3)
                            .map(|k| lines[k][at[k] + (corner >> k & 1) as usize].1)
                            .sum();
                        m | ((sum <= limit) as u8) << corner
                    })
                };
                let brick = |mask: u8| if shape_info(mask).is_some() { mask } else { 0 };
                let outer = brick(within(count));
                let hollow = if layers > 0 {
                    brick(within(count - layers))
                } else {
                    0
                };
                held[x + n[0] * (y + n[1] * z)] = if hollow == 0 {
                    outer
                } else {
                    // two or more layers apart, the hollow reaches no box the
                    // outer surface passes through
                    debug_assert_eq!(outer, CUBE);
                    rest_of_box(hollow)
                };
            }
        }
    }

    bricks_of(held, lines.map(|line| line.into_iter().map(|cut| cut.0).collect()), round.max_ext)
}

/// A grid of boxes, each holding one brick shape or nothing, as bricks.
///
/// `held` has a corner mask per box, x fastest, and `lines` the positions of
/// the cuts between boxes along each axis. Whole boxes join their
/// neighbours, along x, then y, then z, as the volume fitter's cubes do (see
/// `fit_volume::merge`); the rest are one brick each. `None` if there is
/// nothing, or a box is longer than a brick may be.
pub(crate) fn bricks_of(mut held: Vec<u8>, lines: [Vec<i32>; 3], max_ext: i32) -> Option<Vec<CellBrick>> {
    let n = [0, 1, 2].map(|k| lines[k].len() - 1);
    let limit = max_ext.max(1);
    let size = |k: usize, from: usize, to: usize| lines[k][to] - lines[k][from];
    let index = |x: usize, y: usize, z: usize| x + n[0] * (y + n[1] * z);
    let mut bricks = Vec::new();
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                let mask = held[index(x, y, z)];
                if mask == 0 {
                    continue;
                }
                let mut end = [x + 1, y + 1, z + 1];
                if mask == CUBE {
                    while end[0] < n[0] && held[index(end[0], y, z)] == CUBE && size(0, x, end[0] + 1) <= limit {
                        end[0] += 1;
                    }
                    let run = end[0];
                    let row = |y: usize, z: usize, held: &[u8]| held[index(x, y, z)..index(run, y, z)].iter().all(|m| *m == CUBE);
                    while end[1] < n[1] && row(end[1], z, &held) && size(1, y, end[1] + 1) <= limit {
                        end[1] += 1;
                    }
                    while end[2] < n[2] && (y..end[1]).all(|yy| row(yy, end[2], &held)) && size(2, z, end[2] + 1) <= limit {
                        end[2] += 1;
                    }
                }
                for zz in z..end[2] {
                    for yy in y..end[1] {
                        held[index(x, yy, zz)..index(end[0], yy, zz)].fill(0);
                    }
                }
                let ext = [size(0, x, end[0]), size(1, y, end[1]), size(2, z, end[2])];
                if ext.iter().any(|e| *e > limit) {
                    return None; // one box is longer than a brick may be
                }
                bricks.push(CellBrick { mask, min: [lines[0][x], lines[1][y], lines[2][z]], ext });
            }
        }
    }
    (!bricks.is_empty()).then_some(bricks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::inside_shape;
    use std::collections::HashMap;

    fn ball(radius: f64, center: f64) -> Round {
        Round {
            center: DVec3::splat(center),
            radii: DVec3::splat(radius),
            shell: None,
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
            max_ext: 80,
        }
    }

    /// Whether `p` is inside the bricks.
    fn solid(cells: &HashMap<[i32; 3], CellBrick>, p: DVec3) -> bool {
        cells
            .get(&p.floor().to_array().map(|v| v as i32))
            .is_some_and(|b| {
                let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
                inside_shape(shape_info(b.mask).unwrap(), t[0], t[1], t[2])
            })
    }

    fn cells(bricks: &[CellBrick]) -> HashMap<[i32; 3], CellBrick> {
        let mut cells = HashMap::new();
        for b in bricks {
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        assert!(
                            cells
                                .insert([b.min[0] + x, b.min[1] + y, b.min[2] + z], *b)
                                .is_none(),
                            "{b:?} overlaps another brick"
                        );
                    }
                }
            }
        }
        cells
    }

    #[test]
    fn the_cuts_are_on_the_lattice_and_their_gaps_never_grow() {
        for (radius, first) in [
            (12.0, 0.5),
            (13.5, 0.5),
            (9.0, 0.0),
            (40.0, 0.5),
            (7.5, 0.0),
        ] {
            let count = (radius / 3.0) as usize;
            let t = cuts(radius, first, count, 0.0, 1.0 / count as f64, &[]).unwrap();
            assert_eq!(t[0], first);
            assert!(
                (t[count] - radius).abs() <= 0.5,
                "radius {radius}: reaches {}",
                t[count]
            );
            assert!(t.iter().all(|v| (v - first).fract() == 0.0), "{t:?}");
            let gaps: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
            assert!(
                gaps.windows(2).all(|g| g[1] <= g[0]) && gaps.iter().all(|g| *g >= 1.0),
                "radius {radius}: gaps {gaps:?}"
            );
        }
    }

    #[test]
    fn a_ball_is_convex_and_close_to_the_sphere() {
        for (radius, center) in [(12.0, 0.5), (13.5, 0.5), (9.0, 0.0), (20.0, 0.5)] {
            let bricks = fit_round(&ball(radius, center)).unwrap();
            let cells = cells(&bricks);
            // every sample of a fine grid: inside the bricks exactly when well
            // inside the sphere, outside when well outside
            let reach = radius as i32 + 2;
            let (mut wrong, mut total) = (0, 0);
            let mut inside = Vec::new();
            for i in 0..(2 * reach * 2).pow(3) {
                let n = 2 * reach * 2;
                let p = DVec3::new((i % n) as f64, (i / n % n) as f64, (i / (n * n)) as f64) / 2.0
                    - reach as f64
                    + 0.25
                    + center.fract();
                let d = (p - DVec3::splat(center)).length() - radius;
                let held = solid(&cells, p);
                assert!(
                    !(held && d > 0.9) && !(!held && d < -0.9),
                    "radius {radius}: {p} is {d} from the surface and {}",
                    if held { "built" } else { "not built" }
                );
                wrong += (held != (d < 0.0)) as usize;
                total += (d < 0.0) as usize;
                if held && i % 37 == 0 {
                    inside.push(p);
                }
            }
            assert!(
                (wrong as f64) < 0.06 * total as f64,
                "radius {radius}: {wrong} of {total} samples wrong"
            );
            // convex: the middle of any two points inside is inside
            for pair in inside.windows(2).step_by(3) {
                assert!(
                    solid(&cells, (pair[0] + pair[1]) / 2.0 + 1e-4),
                    "radius {radius}: not convex between {} and {}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn no_face_is_a_step() {
        // every brick on the surface is a sloped one, or lies on the flat
        // middle of a face of the solid: no cube pokes out of a slope
        let bricks = fit_round(&ball(13.5, 0.5)).unwrap();
        let cells = cells(&bricks);
        let sloped = bricks.iter().filter(|b| b.mask != CUBE).count();
        assert!(sloped > 150, "only {sloped} sloped bricks");
        for b in bricks.iter().filter(|b| b.mask == CUBE) {
            for k in 0..3 {
                for (side, step) in [(b.min[k] - 1, -1.0), (b.min[k] + b.ext[k], 1.0)] {
                    let mut probe = b.min;
                    probe[k] = side;
                    if cells.contains_key(&probe) {
                        continue;
                    }
                    // an exposed face of a cube: only at the very ends of an axis
                    let mut face = DVec3::from_array(
                        [0, 1, 2].map(|i| b.min[i] as f64 + b.ext[i] as f64 / 2.0),
                    ) - 0.5;
                    face[k] = side as f64;
                    let mut out = DVec3::ZERO;
                    out[k] = step;
                    assert!(
                        face.normalize().dot(out) > 0.95,
                        "cube {b:?} shows a face along axis {k}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_hollow_dome_has_a_wall_and_a_flat_base() {
        let dome = Round {
            shell: Some(2.0),
            lo: 0.0,
            ..ball(13.5, 0.5)
        };
        let bricks = fit_round(&dome).unwrap();
        let cells = cells(&bricks);
        assert!(
            bricks.iter().all(|b| b.min[2] >= 0),
            "something is below the base"
        );
        assert!(bricks.iter().any(|b| b.min[2] == 0));
        // up through the wall at the top, and out through it at the base
        for (from, step) in [
            (DVec3::new(0.5, 0.5, 0.25), DVec3::Z),
            (DVec3::new(0.5, 0.5, 0.25), DVec3::X),
        ] {
            let wall: Vec<f64> = (0..160)
                .map(|i| i as f64 / 8.0)
                .filter(|d| solid(&cells, from + step * *d))
                .collect();
            let (near, far) = (wall[0], wall[wall.len() - 1]);
            assert!(
                near > 9.0
                    && (far - near - 2.0).abs() <= 1.0
                    && wall.len() as f64 > (far - near) * 8.0 - 2.0,
                "along {step}: wall from {near} to {far}"
            );
        }
        // too thin a wall for two layers of facets is left to the other fitter
        assert!(fit_round(&Round {
            shell: Some(0.5),
            ..ball(13.5, 0.5)
        })
        .is_none());
    }

    #[test]
    fn the_rest_of_a_box_fills_it() {
        for mask in crate::shapes::all_masks() {
            let rest = rest_of_box(mask);
            if mask == CUBE || matches!(shape_info(mask).unwrap().kind, Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv) {
                assert_eq!(rest, 0);
                continue;
            }
            let (a, b) = (
                shape_info(mask).unwrap(),
                shape_info(rest).unwrap_or_else(|| panic!("{mask:#x} leaves {rest:#x}")),
            );
            // every point is in exactly one of the two, bar those on the face between
            for i in 0..512 {
                let p = [
                    (i % 8) as f64 + 0.37,
                    (i / 8 % 8) as f64 + 0.21,
                    (i / 64) as f64 + 0.43,
                ]
                .map(|v| v / 8.0);
                assert_ne!(
                    inside_shape(a, p[0], p[1], p[2]),
                    inside_shape(b, p[0], p[1], p[2]),
                    "{mask:#x} and {rest:#x} at {p:?}"
                );
            }
        }
    }
}
