//! Forms joined into one build: a smooth union, fitted afresh where they meet.
//!
//! Every form is fitted with bricks by itself, by whichever fitter suits it
//! (`spec::build`). Forms joined on purpose overlap, and the game refuses a
//! brick whose box overlaps another's, so something has to give where they
//! share room. Two things are done here, and the forms' own bricks are kept
//! everywhere else.
//!
//! **Where their surfaces meet, the join is fitted afresh.** The solid there
//! is the smooth minimum of the forms' distance functions
//! (<https://iquilezles.org/articles/smin/>, the quadratic one):
//!
//! ```text
//! h = max(k - |a - b|, 0)        smin(a, b) = min(a, b) - h * h / (4 * k)
//! ```
//!
//! `k` is the joined form's blend. At 0 this is the plain union, with a sharp
//! crease. Above 0 the crease is filled by a fillet that leaves each surface
//! `k` cells from the other. That solid is fitted cell by cell
//! (`fit_volume`'s method) in a *zone*: the cells near where the two
//! surfaces meet, and no others (see [`seeds`]). Cutting bricks back was all
//! that was done before, and it left a notch a cell or two deep wherever a
//! sloped brick had to be left out, since a slope cannot be cut down to
//! cells.
//!
//! **Everywhere else the forms keep their own bricks**, because a sphere, a
//! turned profile or a flat outline laid out by its own fitter is smoother
//! than anything fitted cell by cell. Deep inside, where whole blocks of the
//! two overlap, the form further down the order gives up the room (see
//! [`give_up_room`]).
//!
//! # The seam
//!
//! The zone's bricks and the forms' own must agree where they meet: no gap,
//! and no two boxes sharing room. Three things see to it.
//!
//! - The zone is a set of whole cells on the forms' common lattice, and
//!   every brick of every form in the join gives up the cells of it that it
//!   covers. A block is cut down to the cells outside it. A sloped brick
//!   cannot be cut across its slope, so the zone *grows* to swallow any
//!   sloped brick it touches, whole, or slice by slice along the one axis a
//!   ramp is the same all the way along. That is repeated until nothing
//!   sloped lies across the zone's edge.
//! - The zone reaches a margin beyond the cells the union differs in. At its
//!   edge the union is therefore exactly the nearer form alone, and what
//!   meets there is two fits of one solid: the form's own and the cell by
//!   cell one. They agree to within a sliver, as they do round a point added
//!   to a sphere (`spec::patched`, which keeps a form's layout the same way).
//! - A cell of the zone that comes out a whole cube where the form it
//!   belongs to already had a block is handed back, and the block is not cut
//!   there. Two cubes joined are then cut exactly as they were before, into
//!   a few blocks, with no zone left at all.
//!
//! A box-shaped zone was the alternative. It needs no growing, but a box
//! round a ring-shaped seam takes in the whole disc inside the ring, and
//! refits much of both forms' surfaces for nothing.
//!
//! # Which forms blend
//!
//! Forms on one grid whose cells line up: where one stands a whole number of
//! cells from the other along every axis. A form set an odd unit (half a
//! cell) off the other has cells that straddle the other's, and no brick is
//! half a cell wide to fill the difference, so no zone can meet both forms'
//! bricks without a gap or an overlap. Such a pair is joined as before, by
//! cutting alone. So is a form laid as a pattern (a mosaic, stepping
//! stones), whose bricks are not its distance function: refitting it would
//! fill the gaps between its stones.
//!
//! The zone is fitted at single cells whatever brick size the forms were
//! built at.
//!
//! # Why not on the graphics card
//!
//! It was asked for and set aside, for three reasons.
//!
//! - Choosing a brick for each cell is a small part of the work. Dropping
//!   spikes, stretching ramps and merging bricks are branching and depend on
//!   the order things are taken in, so they would stay here, with the samples
//!   read back from the card on every change.
//! - Graphics cards round differently from one another. Two machines could
//!   then get different bricks from the same forms, and saves, reports and
//!   tests all rely on a build being the same everywhere.
//! - The editor draws through WebGL2, which has no compute shaders, and this
//!   library runs with no graphics at all for its tests and tools. A path
//!   like this one would have to exist, and match exactly, in any case.
//!
//! The independent work here (testing blocks of cells for the zone, and
//! sampling each cell of it) is kept as plain maps over a list, in
//! [`seeds`] and [`refit`], so that it can be shared among threads with no
//! change to the result.

use crate::par::Threads;
use crate::fit_volume::{drop_spikes, merge, patterns, stretch_ramps, K};
use crate::shapes::{least_unshared, shape_info, CellBrick, Kind, CUBE};
use crate::spec::{Built, Probe, Spec, MAX_CELLS};
use glam::DVec3;
use std::collections::{HashMap, HashSet};

/// The largest blend, in cells.
///
/// A form's distance function is exact only out to `sdf::FAR_MARGIN` (8
/// cells) from its surface, and says "far" beyond. A fillet's surface lies
/// where the further form is at most `1.25 * k` away (`k` from the nearer,
/// which is itself at most `k / 4` outside), and 1.25 x 6 = 7.5 is within
/// that. A larger blend would have its fillet's rim cut short wherever a
/// curve's distance gave out.
pub const MAX_BLEND: f64 = 6.0;

/// Most forms fitted as one join. The distances of all of them are held in
/// an array on the stack for every sample taken, and a join of more forms
/// than this at one place has not been seen. Any further forms are joined by
/// cutting alone.
const MOST: usize = 8;

/// How far outside the joined solid's surface a cell's middle may be and
/// the cell still be looked at, in cells. A form's own sloped bricks lie
/// within about a cell and a half of its surface.
const OUTSIDE: f64 = 1.5;

/// How far inside the nearer form's surface a cell's middle may be and the
/// cell still be looked at, in cells. A cell further in than `OUTSIDE`:
/// where one form's surface dips just under the other's, the cells round
/// its sloped bricks would otherwise be cut out of the other form and left
/// half empty a cell and a half down, near enough the surface to show
/// through a thin place in it.
const INSIDE: f64 = 2.5;

/// How far two forms' distances may differ at a cell's middle and the
/// forms' own sloped bricks still overlap there, in cells: twice `INSIDE`,
/// since each form's sloped bricks reach that far from its own surface.
const APART: f64 = 3.0;

/// The thinnest a fillet may be and still be fitted, in cells. A fillet
/// thins to nothing at its rim, and the last part of it is less than any
/// brick can show: left out of the zone, the forms keep their own bricks
/// there. At a blend of 6 that makes the zone a third narrower.
const THIN: f64 = 0.1;

/// How far two forms' distances may differ at a cell's middle for the cell
/// to be in a zone with blend `k`: far enough that the forms' own sloped
/// bricks cannot overlap, and far enough that no point of the cell (0.87
/// from its middle, over which the difference changes by up to twice that)
/// has a fillet of `THIN` or more.
fn apart(k: f64) -> f64 {
    // a fillet is `h * h / (4 * k)` deep where the distances differ by `k - h`
    APART.max(k - (4.0 * k * THIN).sqrt() + 1.74)
}

/// Cells per block, each way, when looking for the zone (see [`seeds`]).
/// Smaller than the volume fitter's 8: what is looked for is a thin ring
/// where two surfaces cross, and a block's half diagonal is added to every
/// bound, so large blocks pass nearly everything near either surface.
const BLOCK: i32 = 4;

/// The fewest blocks, and the fewest cells of a zone, worth a thread of
/// their own (see `par::Threads::map_ranges`): about half a millisecond's
/// work each. PLACEHOLDER
const BLOCKS_A_THREAD: usize = 400;
const CELLS_A_THREAD: usize = 400;

/// A cell whose middle is further than this from the surface is taken as
/// wholly inside or outside without sampling it. Half a cell's diagonal is
/// 0.87; the rest allows for a blended distance changing a little faster
/// than a plain one where three forms meet.
const SURE: f64 = 1.3;

/// A form to be put together with others: what it is, the bricks built from
/// it, and where it stands.
pub struct Member<'a> {
    pub spec: &'a Spec,
    pub built: &'a Built,
    /// where it stands, in units (two to a cell)
    pub place: [i32; 3],
    /// Which grid it is on. Only forms with the same number are put
    /// together: grids pass through one another.
    pub grid: u32,
}

/// A brick of the result, in the cells of the form it belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub brick: CellBrick,
    /// which of the members it belongs to: for a brick of the zone, the one
    /// whose own distance is the least at the brick's middle
    pub member: usize,
    /// The brick of that member it is, is cut from, or takes the place of,
    /// for its colour. `None` for a brick of the zone where the member had
    /// none: the fillet itself.
    pub from: Option<usize>,
    /// A single cube left over from a cut, or added by the zone. These are
    /// left for the caller to join into blocks, which it does by colour, and
    /// colour is no business of this library's.
    pub loose: bool,
}

/// The smooth minimum of two distances: see the top of this file.
pub fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    a.min(b) - fillet(a, b, k)
}

/// How far a blend of `k` lowers the smaller of two distances.
fn fillet(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return 0.0;
    }
    let h = k - (a - b).abs();
    // (not `h.max(0.0)`: two forms both "far" differ by a number that is no number, which must count as no fillet)
    if h > 0.0 { h * h / (4.0 * k) } else { 0.0 }
}

type Box3 = ([i32; 3], [i32; 3]);

fn meet(a: &Box3, b: &Box3) -> bool {
    (0..3).all(|k| a.0[k] < b.1[k] && b.0[k] < a.1[k])
}

/// A brick's box in units.
fn in_units(b: &CellBrick, place: [i32; 3]) -> Box3 {
    ([0, 1, 2].map(|k| 2 * b.min[k] + place[k]), [0, 1, 2].map(|k| 2 * (b.min[k] + b.ext[k]) + place[k]))
}

fn cells(b: &CellBrick) -> impl Iterator<Item = [i32; 3]> {
    let (m, e) = (b.min, b.ext);
    (0..e[2]).flat_map(move |z| (0..e[1]).flat_map(move |y| (0..e[0]).map(move |x| [m[0] + x, m[1] + y, m[2] + z])))
}

/// Every brick of the forms put together: their own where they stand clear
/// of one another, and the join fitted afresh where joined forms meet.
///
/// The result does not depend on the order the members are given in, apart
/// from the numbers in `member`: which form gives way to which is settled by
/// what the forms are and where they stand (see [`order`]). The editor lists
/// the form being edited first, and choosing another form to edit must not
/// change a brick.
pub fn union(members: &[Member]) -> Vec<Piece> {
    union_with(members, Threads::available())
}

/// [`union`], on so many threads. The bricks are the same on any number.
pub fn union_with(members: &[Member], threads: Threads) -> Vec<Piece> {
    let boxes: Vec<Option<Box3>> = members.iter().map(|m| m.built.bricks.iter().map(|b| in_units(b, m.place)).reduce(|a, b| ([0, 1, 2].map(|k| a.0[k].min(b.0[k])), [0, 1, 2].map(|k| a.1[k].max(b.1[k]))))).collect();
    let order = order(members, &boxes);
    let mut pieces: Vec<Vec<Piece>> = members.iter().enumerate().map(|(member, m)| m.built.bricks.iter().enumerate().map(|(n, brick)| Piece { brick: *brick, member, from: Some(n), loose: false }).collect()).collect();
    for cluster in clusters(members, &boxes, &order) {
        refit(members, &cluster, &mut pieces, threads);
    }
    give_up_room(members, &order, &mut pieces);
    pieces.into_iter().flatten().collect()
}

/// The members from the one that gives way to nobody to the one that gives
/// way to all: forms that are not joined first, then the joined ones by
/// where they stand and what they are.
///
/// The order in the list was used before, and the list is reordered
/// whenever another form is chosen to edit, so two joined forms changed
/// which of them was cut.
fn order(members: &[Member], boxes: &[Option<Box3>]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by_key(|&i| (members[i].spec.blend.is_some(), members[i].place, boxes[i], members[i].built.bricks.len(), i));
    order
}

/// How far from its bricks a form can matter to a join with blend `k`, in
/// cells: its distance at a cell of the zone is at most `1.25 * k` and the
/// margins (see [`seeds`]).
fn reach(k: f64) -> f64 {
    1.25 * k + OUTSIDE + apart(k) + 0.5
}

/// The blend between two members, if they are joined to one another: the
/// larger of the two forms' own, since either may be the one that was moved
/// into the other.
fn blend_of(a: &Spec, b: &Spec) -> Option<f64> {
    let k = |s: &Spec| s.blend.map(|k| if k.is_finite() { k.clamp(0.0, MAX_BLEND) } else { 0.0 });
    match (k(a), k(b)) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0).max(b.unwrap_or(0.0))),
    }
}

/// The groups of members to be fitted as one join each: those that are
/// joined, stand on one grid with their cells lined up, and come within
/// reach of one another, directly or through others of the group. Each
/// group is in the order of `order`.
fn clusters(members: &[Member], boxes: &[Option<Box3>], order: &[usize]) -> Vec<Vec<usize>> {
    let parity = |m: &Member| m.place.map(|u| u.rem_euclid(2));
    // (a pattern's bricks are not its distance function: see the top of this file)
    let able = |i: usize| boxes[i].is_some() && members[i].built.shades.is_empty();
    let mut group: Vec<usize> = (0..members.len()).collect();
    fn top(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    for i in 0..members.len() {
        for j in i + 1..members.len() {
            let (a, b) = (&members[i], &members[j]);
            if a.grid != b.grid || parity(a) != parity(b) || !able(i) || !able(j) {
                continue;
            }
            let Some(k) = blend_of(a.spec, b.spec) else { continue };
            let wide = (2.0 * reach(k)).ceil() as i32;
            let (Some(a), Some(b)) = (boxes[i], boxes[j]) else { continue };
            if meet(&(a.0.map(|v| v - wide), a.1.map(|v| v + wide)), &b) {
                let (i, j) = (top(&mut group, i), top(&mut group, j));
                group[i] = j;
            }
        }
    }
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    for &i in order {
        let t = top(&mut group, i);
        match seen.iter().position(|s| *s == t) {
            Some(at) => out[at].push(i),
            None => {
                seen.push(t);
                out.push(vec![i]);
            }
        }
    }
    out.retain_mut(|c| {
        c.truncate(MOST);
        c.len() > 1
    });
    out
}

/// One form of a join, on the lattice the join is fitted on.
struct Part {
    member: usize,
    probe: Probe,
    /// what is added to a cell of the form to find it on the join's lattice
    off: [i32; 3],
    /// the box of its bricks on that lattice, and the same grown by how far it can matter
    low: [i32; 3],
    high: [i32; 3],
    near: (DVec3, DVec3),
}

/// The forms of one join as one solid.
struct Joined {
    parts: Vec<Part>,
    /// the blend between each two parts, 0 where there is none
    k: [[f64; MOST]; MOST],
}

impl Joined {
    /// Part `i`'s own distance at `p`, or "far" where it cannot matter.
    fn own(&self, i: usize, p: DVec3) -> f64 {
        let part = &self.parts[i];
        if p.cmplt(part.near.0).any() || p.cmpgt(part.near.1).any() {
            return f64::INFINITY;
        }
        part.probe.distance(p - DVec3::new(part.off[0] as f64, part.off[1] as f64, part.off[2] as f64))
    }

    /// Every part's own distance at `p`.
    fn each(&self, p: DVec3) -> [f64; MOST] {
        let mut d = [f64::INFINITY; MOST];
        for (i, d) in d.iter_mut().enumerate().take(self.parts.len()) {
            *d = self.own(i, p);
        }
        d
    }

    /// The joined solid's distance where its parts' own distances are `d`:
    /// negative inside.
    ///
    /// For two forms it is their smooth minimum. For more, each form's
    /// distance is lowered by the largest of its fillets with the others,
    /// and the least of those is taken. Folding the smooth minimum over
    /// the forms one after another, the usual way, gives a result that
    /// depends on the order they are taken in, and here the order is only
    /// the order they happen to be listed in.
    fn together(&self, d: &[f64; MOST]) -> f64 {
        let n = self.parts.len();
        let mut least = f64::INFINITY;
        for i in 0..n {
            let most = (0..n).filter(|j| *j != i).map(|j| fillet(d[i], d[j], self.k[i][j])).fold(0.0, f64::max);
            least = least.min(d[i] - most);
        }
        least
    }

    /// Which part a place belongs to: the one whose own distance is the least there.
    fn owner(&self, p: DVec3) -> usize {
        (0..self.parts.len()).map(|i| (i, self.own(i, p))).fold((0, f64::INFINITY), |best, now| if now.1 < best.1 { now } else { best }).0
    }
}

/// The cells where a join differs from its forms fitted separately, before
/// the zone is grown to whole bricks (see [`refit`]).
///
/// A cell is one if, at its middle and for some two forms joined with blend
/// `k`, the two distances `a` and `b` have
///
/// ```text
/// |a - b| < apart(k)      and      -INSIDE < min(a, b)      and      smin(a, b) < OUTSIDE
/// ```
///
/// The first says the two surfaces are near one another: near enough for a
/// fillet thick enough to show, or for each form's own sloped bricks to
/// overlap the other's (see [`apart`]). The other two say the union's
/// surface is near. Outside these the union is the nearer form alone, to
/// within less than a brick can show, and that form's own bricks are right.
///
/// Blocks of cells are tested first by their middles and passed over whole
/// where neither bound can be met inside them, as the volume fitter passes
/// over blocks far from the surface: a distance changes by at most 1 a cell.
fn seeds(solid: &Joined, threads: Threads) -> Vec<[i32; 3]> {
    let mut blocks: Vec<(usize, usize, [i32; 3], [i32; 3])> = Vec::new();
    for i in 0..solid.parts.len() {
        for j in i + 1..solid.parts.len() {
            let (a, b) = (&solid.parts[i], &solid.parts[j]);
            if blend_of_parts(solid, i, j).is_none() {
                continue;
            }
            let wide = reach(solid.k[i][j]).ceil() as i32;
            let low = [0, 1, 2].map(|k| (a.low[k] - wide).max(b.low[k] - wide));
            let high = [0, 1, 2].map(|k| (a.high[k] + wide).min(b.high[k] + wide));
            if (0..3).any(|k| high[k] <= low[k]) {
                continue;
            }
            for z in (low[2]..high[2]).step_by(BLOCK as usize) {
                for y in (low[1]..high[1]).step_by(BLOCK as usize) {
                    for x in (low[0]..high[0]).step_by(BLOCK as usize) {
                        blocks.push((i, j, [x, y, z], [0, 1, 2].map(|k| BLOCK.min(high[k] - [x, y, z][k]))));
                    }
                }
            }
        }
    }
    // Each block by itself, so threads may share them.
    let found = threads.map(blocks.len(), BLOCKS_A_THREAD, |block| {
        let (i, j, at, size) = blocks[block];
        let k = solid.k[i][j];
        let (inside, outside) = (-INSIDE, k / 4.0 + OUTSIDE + apart(k));
        let half = DVec3::new(size[0] as f64, size[1] as f64, size[2] as f64) / 2.0;
        let base = DVec3::new(at[0] as f64, at[1] as f64, at[2] as f64);
        let far = half.length();
        let out_of_range = |d: f64, slack: f64| !(d > inside - slack && d < outside + slack);
        let mut cells = Vec::new();
        if out_of_range(solid.own(i, base + half), far) || out_of_range(solid.own(j, base + half), far) {
            return cells;
        }
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let p = base + DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5);
                    let a = solid.own(i, p);
                    if out_of_range(a, 0.0) {
                        continue;
                    }
                    let b = solid.own(j, p);
                    if (a - b).abs() < apart(k) && a.min(b) > -INSIDE && smooth_min(a, b, k) < OUTSIDE {
                        cells.push([at[0] + x, at[1] + y, at[2] + z]);
                    }
                }
            }
        }
        cells
    });
    found.into_iter().flatten().collect()
}

/// Whether two parts of a join are joined to one another, and with what blend.
fn blend_of_parts(solid: &Joined, i: usize, j: usize) -> Option<f64> {
    (solid.k[i][j] >= 0.0).then_some(solid.k[i][j])
}

/// The axis a sloped brick is the same all the way along, if it is longer
/// than a cell that way: a ramp's ridge. It can be cut across that axis
/// into shorter bricks of the same shape, and across no other.
fn sliced_along(b: &CellBrick) -> Option<usize> {
    (0..3).find(|&a| b.ext[a] > 1 && (0..8u8).all(|c| b.mask >> c & 1 == b.mask >> (c ^ 1 << a) & 1))
}

/// How far a form's distance at a cell's middle may be from what its
/// distances at the cell's corners give for the middle, and the form still
/// be taken as smooth across the cell (see [`fit_cell`]).
const SMOOTH: f64 = 0.02;

/// Two more places in a cell where a form's true distance is compared with
/// what its corners give, besides the middle. The middle alone passed a
/// form whose distance jumps part of the way across the cell (the square
/// end of an arch's foot), where the corners happened to average to it.
const OFF_MIDDLE: [DVec3; 2] = [DVec3::new(0.25, 0.3, 0.2), DVec3::new(0.7, 0.8, 0.75)];

/// A value at `t` (each coordinate 0 to 1 across a cell) from its values
/// at the cell's eight corners, corner `c` being at `(c & 1, c >> 1 & 1, c >> 2)`.
fn between(v: &[f64; 8], t: DVec3) -> f64 {
    let along_x = |lo: f64, hi: f64| lo + (hi - lo) * t.x;
    let (low, high) = (along_x(v[0], v[1]) + (along_x(v[2], v[3]) - along_x(v[0], v[1])) * t.y, along_x(v[4], v[5]) + (along_x(v[6], v[7]) - along_x(v[4], v[5])) * t.y);
    low + (high - low) * t.z
}

/// The best brick for one cell of the joined solid, and the solid's samples
/// in it if they were taken: `fit_volume`'s choice, for a cell on its own.
///
/// The volume fitter asks the solid for its distance at each of a cell's 64
/// samples. Here that would be every form's distance 64 times, and it was
/// nine tenths of a join's time. Each form's distance is instead taken at
/// the cell's eight corners and worked out between them
/// (<https://en.wikipedia.org/wiki/Trilinear_interpolation>), and the forms
/// are put together sample by sample from that. The crease of a join is in
/// how the forms are put together, which is still done exactly at every
/// sample; what is worked out between corners is one form's own distance,
/// which is nearly straight across a cell wherever the form is smooth. Where
/// it is not (a form's own edge crossing the cell: its true distance at the
/// middle, or at either of two places off it, is not what the corners
/// give), that form is asked at every sample as before.
fn fit_cell(solid: &Joined, at: DVec3, wedges: bool) -> (u8, Option<u64>) {
    let n = solid.parts.len();
    let middle = solid.each(at + 0.5);
    let whole = solid.together(&middle);
    if whole > SURE {
        return (0, None);
    }
    if whole < -SURE {
        return (CUBE, None);
    }
    let corner_at = |c: usize| at + DVec3::new((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2) as f64);
    // each part's distance at the eight corners, and whether it is smooth across the cell
    let mut corner = [[0.0f64; 8]; MOST];
    let mut smooth = [false; MOST];
    for i in 0..n {
        for (c, value) in corner[i].iter_mut().enumerate() {
            *value = solid.own(i, corner_at(c));
        }
        // (a form that is "far" at any of them is no number to work out between)
        smooth[i] = middle[i].is_finite() && corner[i].iter().all(|v| v.is_finite()) && (between(&corner[i], DVec3::splat(0.5)) - middle[i]).abs() < SMOOTH && OFF_MIDDLE.iter().all(|t| (between(&corner[i], *t) - solid.own(i, at + *t)).abs() < SMOOTH);
    }
    let mut d = [f64::INFINITY; MOST];
    let mut bits = 0u64;
    for s in 0..K * K * K {
        let t = DVec3::new((s % K) as f64 + 0.5, (s / K % K) as f64 + 0.5, (s / (K * K)) as f64 + 0.5) / K as f64;
        for i in 0..n {
            d[i] = if smooth[i] { between(&corner[i], t) } else { solid.own(i, at + t) };
        }
        // strictly inside, as the volume fitter has it
        bits |= ((solid.together(&d) < 0.0) as u64) << s;
    }
    let corners = (0..8).fold(0u8, |m, c| {
        for i in 0..n {
            d[i] = corner[i][c];
        }
        m | ((solid.together(&d) <= 0.0) as u8) << c
    });
    let patterns = if wedges { patterns() } else { &patterns()[..2] };
    let mut scores = [(0u32, 0u32); 30];
    for (score, (mask, pattern)) in scores.iter_mut().zip(patterns) {
        *score = ((bits ^ pattern).count_ones(), (corners ^ mask).count_ones());
    }
    (patterns[least_unshared(&scores[..patterns.len()], 2)].0, Some(bits))
}

/// Fit one join afresh where its forms meet, and cut their own bricks back
/// to make room for it: `pieces` of each member of `cluster` are replaced.
fn refit(members: &[Member], cluster: &[usize], pieces: &mut [Vec<Piece>], threads: Threads) {
    let parity = members[cluster[0]].place.map(|u| u.rem_euclid(2));
    let parts: Vec<Part> = cluster
        .iter()
        .filter_map(|&member| {
            let m = &members[member];
            // (a form that cannot be asked where it is takes no part, and is cut as before)
            let probe = Probe::new(m.spec, m.built.grain).ok()?;
            let off = [0, 1, 2].map(|k| (m.place[k] - parity[k]) / 2);
            let low = [0, 1, 2].map(|k| m.built.bricks.iter().map(|b| b.min[k]).min().unwrap_or(0) + off[k]);
            let high = [0, 1, 2].map(|k| m.built.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap_or(0) + off[k]);
            let wide = reach(MAX_BLEND);
            let near = (DVec3::new(low[0] as f64, low[1] as f64, low[2] as f64) - wide, DVec3::new(high[0] as f64, high[1] as f64, high[2] as f64) + wide);
            Some(Part { member, probe, off, low, high, near })
        })
        .collect();
    if parts.len() < 2 {
        return;
    }
    // (less than nothing where two parts are not joined to one another: two forms that stand clear, both joined to a third)
    let mut k = [[-1.0; MOST]; MOST];
    for i in 0..parts.len() {
        for j in 0..parts.len() {
            if let Some(blend) = blend_of(members[parts[i].member].spec, members[parts[j].member].spec).filter(|_| i != j) {
                k[i][j] = blend;
            }
        }
    }
    let solid = Joined { parts, k };
    let parts = &solid.parts;
    let mut zone: HashSet<[i32; 3]> = seeds(&solid, threads).into_iter().collect();
    if zone.is_empty() {
        return;
    }
    // Grow the zone to whole sloped bricks, or whole slices of them, until
    // none lies across its edge. A brick swallowed for one form may bring
    // the zone onto a sloped brick of another, hence the repeating.
    let (mut low, mut high) = zone.iter().fold(([i32::MAX; 3], [i32::MIN; 3]), |(lo, hi), c| ([0, 1, 2].map(|k| lo[k].min(c[k])), [0, 1, 2].map(|k| hi[k].max(c[k] + 1))));
    let own_box = |part: &Part, b: &CellBrick| -> Box3 { ([0, 1, 2].map(|k| b.min[k] + part.off[k]), [0, 1, 2].map(|k| b.min[k] + b.ext[k] + part.off[k])) };
    loop {
        let mut grown = false;
        for part in parts {
            for b in members[part.member].built.bricks.iter().filter(|b| b.mask != CUBE) {
                let (min, max) = own_box(part, b);
                if !meet(&(min, max), &(low, high)) {
                    continue;
                }
                let along = sliced_along(b);
                let slices = along.map_or(1, |a| b.ext[a]);
                for s in 0..slices {
                    let (mut min, mut ext) = (min, b.ext);
                    if let Some(a) = along {
                        (min[a], ext[a]) = (min[a] + s, 1);
                    }
                    let slice = CellBrick { mask: b.mask, min, ext };
                    let inside = cells(&slice).filter(|c| zone.contains(c)).count();
                    if inside > 0 && inside < (ext[0] * ext[1] * ext[2]) as usize {
                        zone.extend(cells(&slice));
                        for k in 0..3 {
                            (low[k], high[k]) = (low[k].min(min[k]), high[k].max(min[k] + ext[k]));
                        }
                        grown = true;
                    }
                }
            }
        }
        if !grown {
            break;
        }
    }
    // The zone as a block of cells, with a cell to spare all round: the
    // passes below (spikes, stretched ramps, merging) are the volume
    // fitter's own, and they work on a block.
    let origin = low.map(|v| v - 1);
    let n = [0, 1, 2].map(|k| (high[k] - low[k] + 2) as usize);
    if (n[0] * n[1] * n[2]) as i64 > MAX_CELLS {
        return;
    }
    let index = |c: [i32; 3]| (c[0] - origin[0]) as usize + n[0] * ((c[1] - origin[1]) as usize + n[1] * (c[2] - origin[2]) as usize);
    let cell_at = |i: usize| [origin[0] + (i % n[0]) as i32, origin[1] + (i / n[0] % n[1]) as i32, origin[2] + (i / (n[0] * n[1])) as i32];
    const IN: u8 = 1;
    const BESIDE: u8 = 2;
    let mut mark = vec![0u8; n[0] * n[1] * n[2]];
    for c in &zone {
        mark[index(*c)] = IN;
    }
    let wedges = parts.iter().all(|p| members[p.member].spec.wedges);
    let max_ext = parts.iter().map(|p| members[p.member].spec.max_ext).min().unwrap_or(1).max(1) as usize;
    let mut wanted: Vec<usize> = (0..mark.len()).filter(|i| mark[*i] == IN).collect();
    // Each cell by itself, so threads may share them.
    let fit_at = |i: usize| fit_cell(&solid, DVec3::from_array(cell_at(i).map(|v| v as f64)), wedges);
    let fitted: Vec<(u8, Option<u64>)> = threads.map(wanted.len(), CELLS_A_THREAD, |at| fit_at(wanted[at]));
    let mut shape = vec![0u8; mark.len()];
    let mut sampled: Vec<(usize, u64)> = Vec::new();
    for (&i, &(mask, bits)) in wanted.iter().zip(&fitted) {
        shape[i] = mask;
        sampled.extend(bits.map(|bits| (i, bits)));
    }
    // The cells beside a tetrahedron at the zone's edge are fitted too,
    // and then left out. A tetrahedron is kept only where two of its faces
    // are backed by its neighbours (`drop_spikes`), and one at the zone's
    // edge is backed by a cell outside it: judged against nothing there,
    // every such one was taken for a spike and left a pit along the seam.
    // (Every cell beside the zone was fitted at first, which was nearly as
    // many cells again to back a few dozen tetrahedra.)
    let strides = [1, n[0], n[0] * n[1]];
    let mut beside: Vec<usize> = wanted.iter().filter(|i| shape_info(shape[**i]).is_some_and(|s| s.kind == Kind::Tet)).flat_map(|&i| strides.into_iter().flat_map(move |s| [i - s, i + s])).filter(|i| mark[*i] == 0).collect();
    beside.sort_unstable();
    beside.dedup();
    for &i in &beside {
        mark[i] = BESIDE;
        let (mask, bits) = fit_at(i);
        shape[i] = mask;
        sampled.extend(bits.map(|bits| (i, bits)));
    }
    wanted.extend(&beside);
    sampled.sort_unstable_by_key(|s| s.0);
    let mut sloped: Vec<CellBrick> = Vec::new();
    if wedges {
        drop_spikes(&mut shape, &sampled, n);
    }
    for i in wanted.iter().filter(|i| mark[**i] == BESIDE) {
        shape[*i] = 0;
    }
    sampled.retain(|s| mark[s.0] == IN);
    if wedges {
        // (a stretched ramp never enters a cell the solid does not reach, and so never leaves the zone)
        sloped = stretch_ramps(&mut shape, &sampled, n, max_ext, threads).0;
    }
    // Which brick of each form covers each cell of the zone.
    let mut cover: Vec<HashMap<usize, usize>> = vec![HashMap::new(); parts.len()];
    for (part, cover) in parts.iter().zip(&mut cover) {
        for (at, b) in members[part.member].built.bricks.iter().enumerate() {
            let (min, max) = own_box(part, b);
            if !meet(&(min, max), &(low, high)) {
                continue;
            }
            let (from, to) = ([0, 1, 2].map(|k| min[k].max(low[k])), [0, 1, 2].map(|k| max[k].min(high[k])));
            for c in cells(&CellBrick { mask: CUBE, min: from, ext: [0, 1, 2].map(|k| to[k] - from[k]) }) {
                if mark[index(c)] == IN {
                    cover.insert(index(c), at);
                }
            }
        }
    }
    let middle_of = |b: &CellBrick| DVec3::new(b.min[0] as f64 + b.ext[0] as f64 / 2.0, b.min[1] as f64 + b.ext[1] as f64 / 2.0, b.min[2] as f64 + b.ext[2] as f64 / 2.0);
    let mut fresh: Vec<Vec<Piece>> = vec![Vec::new(); parts.len()];
    // The zone's cubes, a cell at a time. One that its form already had as
    // part of a block goes back to that block, which is then not cut there.
    let mut back: Vec<HashSet<usize>> = vec![HashSet::new(); parts.len()];
    for &i in &wanted {
        if shape[i] != CUBE {
            continue;
        }
        shape[i] = 0;
        let cell = CellBrick { mask: CUBE, min: cell_at(i), ext: [1; 3] };
        // (to the first form in the order that has a block there, as in the cutting that follows: the form that gives way to nobody is then never cut where it is solid)
        if let Some(home) = (0..parts.len()).find(|&p| cover[p].get(&i).is_some_and(|at| members[parts[p].member].built.bricks[*at].mask == CUBE)) {
            back[home].insert(i);
            continue;
        }
        let owner = solid.owner(middle_of(&cell));
        let part = &parts[owner];
        fresh[owner].push(Piece { brick: CellBrick { min: [0, 1, 2].map(|k| cell.min[k] - part.off[k]), ..cell }, member: part.member, from: cover[owner].get(&i).copied(), loose: true });
    }
    // and its sloped bricks: the stretched ramps, and the rest joined along their ridges
    sloped.extend(merge(shape, n, max_ext));
    for b in sloped {
        let b = CellBrick { min: [0, 1, 2].map(|k| b.min[k] + origin[k]), ..b };
        let middle = middle_of(&b);
        let owner = solid.owner(middle);
        let part = &parts[owner];
        let from = cover[owner].get(&index(middle.floor().to_array().map(|v| v as i32))).copied();
        fresh[owner].push(Piece { brick: CellBrick { min: [0, 1, 2].map(|k| b.min[k] - part.off[k]), ..b }, member: part.member, from, loose: false });
    }
    // The forms' own bricks, less the zone.
    for ((part, back), fresh) in parts.iter().zip(&back).zip(fresh) {
        let gone = |c: [i32; 3]| (0..3).all(|k| c[k] >= low[k] && c[k] < high[k]) && mark[index(c)] == IN && !back.contains(&index(c));
        let mut left: Vec<Piece> = Vec::new();
        for (at, b) in members[part.member].built.bricks.iter().enumerate() {
            let whole = Piece { brick: *b, member: part.member, from: Some(at), loose: false };
            let (min, max) = own_box(part, b);
            if !meet(&(min, max), &(low, high)) {
                left.push(whole);
                continue;
            }
            let on_lattice = CellBrick { mask: b.mask, min, ext: b.ext };
            let back_home = |c: &CellBrick| CellBrick { min: [0, 1, 2].map(|k| c.min[k] - part.off[k]), ..*c };
            if b.mask != CUBE {
                // whole, or the slices of it the zone left, each run of them one brick
                match sliced_along(b) {
                    None => {
                        if !cells(&on_lattice).any(gone) {
                            left.push(whole);
                        }
                    }
                    Some(a) => {
                        let kept: Vec<bool> = (0..b.ext[a])
                            .map(|s| {
                                let (mut min, mut ext) = (min, b.ext);
                                (min[a], ext[a]) = (min[a] + s, 1);
                                !cells(&CellBrick { mask: b.mask, min, ext }).any(gone)
                            })
                            .collect();
                        let mut s = 0;
                        while s < kept.len() {
                            let run = kept[s..].iter().take_while(|k| **k == kept[s]).count();
                            if kept[s] {
                                let mut piece = whole;
                                (piece.brick.min[a], piece.brick.ext[a]) = (b.min[a] + s as i32, run as i32);
                                left.push(piece);
                            }
                            s += run;
                        }
                    }
                }
                continue;
            }
            // A block: the box of what the zone takes of it is gone through
            // cell by cell, and the rest comes off whole, as up to six slabs.
            let taken = cells(&on_lattice).filter(|c| gone(*c)).fold(None, |all: Option<Box3>, c| Some(all.map_or((c, c.map(|v| v + 1)), |(lo, hi)| ([0, 1, 2].map(|k| lo[k].min(c[k])), [0, 1, 2].map(|k| hi[k].max(c[k] + 1))))));
            let Some((from, to)) = taken else {
                left.push(whole);
                continue;
            };
            let (mut lo, mut hi) = (min, max);
            for k in 0..3 {
                for (start, end) in [(lo[k], from[k]), (to[k], hi[k])] {
                    if end > start {
                        let (mut min, mut ext) = (lo, [0, 1, 2].map(|j| hi[j] - lo[j]));
                        (min[k], ext[k]) = (start, end - start);
                        left.push(Piece { brick: back_home(&CellBrick { mask: CUBE, min, ext }), ..whole });
                    }
                }
                (lo[k], hi[k]) = (from[k], to[k]);
            }
            for c in cells(&CellBrick { mask: CUBE, min: lo, ext: [0, 1, 2].map(|k| hi[k] - lo[k]) }).filter(|c| !gone(*c)) {
                left.push(Piece { brick: back_home(&CellBrick { mask: CUBE, min: c, ext: [1; 3] }), loose: true, ..whole });
            }
        }
        left.extend(fresh);
        pieces[part.member] = left;
    }
}

/// Bricks' boxes in units, found by the part of space they lie in.
///
/// Every brick of a form was compared with every brick of the form it gives
/// way to. With a zone cut out of both, each is thousands of single cells,
/// and that comparison was the longest part of a join.
struct Room {
    boxes: Vec<Box3>,
    /// which boxes reach into each bucket of `BUCKET` units a side
    buckets: HashMap<[i32; 3], Vec<u32>>,
    /// boxes too large to list bucket by bucket: looked at every time
    large: Vec<u32>,
}

const BUCKET: i32 = 16;

impl Room {
    fn new(boxes: Vec<Box3>) -> Room {
        let (mut buckets, mut large): (HashMap<[i32; 3], Vec<u32>>, Vec<u32>) = (HashMap::new(), Vec::new());
        for (at, b) in boxes.iter().enumerate() {
            let (from, to) = (b.0.map(|v| v.div_euclid(BUCKET)), b.1.map(|v| (v - 1).div_euclid(BUCKET)));
            if (0..3).map(|k| (to[k] - from[k] + 1) as i64).product::<i64>() > 512 {
                large.push(at as u32);
                continue;
            }
            for z in from[2]..=to[2] {
                for y in from[1]..=to[1] {
                    for x in from[0]..=to[0] {
                        buckets.entry([x, y, z]).or_default().push(at as u32);
                    }
                }
            }
        }
        Room { boxes, buckets, large }
    }

    /// The boxes that share room with `with`, in the order they were given.
    fn hits(&self, with: &Box3) -> Vec<Box3> {
        let (from, to) = (with.0.map(|v| v.div_euclid(BUCKET)), with.1.map(|v| (v - 1).div_euclid(BUCKET)));
        let mut found: Vec<u32> = self.large.clone();
        for z in from[2]..=to[2] {
            for y in from[1]..=to[1] {
                for x in from[0]..=to[0] {
                    if let Some(some) = self.buckets.get(&[x, y, z]) {
                        found.extend(some);
                    }
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found.into_iter().map(|at| self.boxes[at as usize]).filter(|b| meet(b, with)).collect()
    }
}

/// Make every joined form give up the room it still shares with the forms
/// before it in `order`: with every form on its grid that is not joined,
/// and with the joined ones before it.
///
/// This is all a join was before the zone, and it is still what happens
/// deep inside, where blocks overlap blocks, and between forms that cannot
/// be fitted as one (see the top of this file). A brick's room is its box,
/// as the game judges it. A brick that shares none is kept whole. A block
/// that shares some is cut down to the cells that share none. A sloped
/// brick that shares any is left out, since a slope cannot be cut down to
/// cells: inside the other form that is unseen, and near its surface the
/// zone has already taken the brick's place.
fn give_up_room(members: &[Member], order: &[usize], pieces: &mut [Vec<Piece>]) {
    let all_of = |pieces: &[Piece], place: [i32; 3]| pieces.iter().map(|p| in_units(&p.brick, place)).reduce(|a, b| ([0, 1, 2].map(|k| a.0[k].min(b.0[k])), [0, 1, 2].map(|k| a.1[k].max(b.1[k]))));
    for (rank, &i) in order.iter().enumerate() {
        let me = &members[i];
        if me.spec.blend.is_none() {
            continue;
        }
        let Some(mine) = all_of(&pieces[i], me.place) else { continue };
        let before: Vec<Box3> = order[..rank]
            .iter()
            .filter(|&&j| members[j].grid == me.grid && all_of(&pieces[j], members[j].place).is_some_and(|theirs| meet(&theirs, &mine)))
            .flat_map(|&j| pieces[j].iter().map(move |p| in_units(&p.brick, members[j].place)).filter(|b| meet(b, &mine)))
            .collect();
        if before.is_empty() {
            continue;
        }
        let room = Room::new(before);
        let mut left: Vec<Piece> = Vec::with_capacity(pieces[i].len());
        for piece in &pieces[i] {
            let whole = in_units(&piece.brick, me.place);
            let hits = room.hits(&whole);
            if hits.is_empty() {
                left.push(*piece);
                continue;
            }
            if piece.brick.mask != CUBE || piece.loose {
                continue;
            }
            // The part of the block the other forms' bricks reach, in its
            // own cells. Outside it the block is whole: it comes off as
            // slabs, up to six, and only the part within is gone through
            // cell by cell.
            let (mut low, mut high) = (piece.brick.min, [0, 1, 2].map(|k| piece.brick.min[k] + piece.brick.ext[k]));
            for k in 0..3 {
                let from = hits.iter().map(|hit| (hit.0[k] - me.place[k]).div_euclid(2)).min().unwrap().max(low[k]);
                let to = hits.iter().map(|hit| (hit.1[k] - me.place[k] + 1).div_euclid(2)).max().unwrap().min(high[k]);
                for (start, end) in [(low[k], from), (to, high[k])] {
                    if end > start {
                        let (mut min, mut ext) = (low, [0, 1, 2].map(|j| high[j] - low[j]));
                        (min[k], ext[k]) = (start, end - start);
                        left.push(Piece { brick: CellBrick { mask: CUBE, min, ext }, ..*piece });
                    }
                }
                (low[k], high[k]) = (from, to);
            }
            for c in cells(&CellBrick { mask: CUBE, min: low, ext: [0, 1, 2].map(|k| high[k] - low[k]) }) {
                let cell = CellBrick { mask: CUBE, min: c, ext: [1; 3] };
                let room = in_units(&cell, me.place);
                if !hits.iter().any(|hit| meet(hit, &room)) {
                    left.push(Piece { brick: cell, loose: true, ..*piece });
                }
            }
        }
        pieces[i] = left;
    }
}

/// Whether a point, in the cells of the form a piece belongs to, is inside
/// the piece's brick: for checking a join against the solid it was fitted to.
pub fn holds(brick: &CellBrick, p: DVec3) -> bool {
    let t = [0, 1, 2].map(|k| (p[k] - brick.min[k] as f64) / brick.ext[k] as f64);
    if t.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return false;
    }
    shape_info(brick.mask).is_some_and(|info| crate::shapes::inside_shape(info, t[0], t[1], t[2]))
}

#[cfg(test)]
mod tests;
