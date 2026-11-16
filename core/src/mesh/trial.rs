//! Ways of fitting a mesh with bricks, tried side by side and measured, so
//! that the one built into the editor is chosen on figures.
//!
//! Three routes, all asking the mesh itself for every sample (see
//! [`Stand::distance`]) and none of them a grid of filled cells:
//!
//! - [`Route::Blocks`]: cubes alone. What a program that fills cells and
//!   joins them gives, here as the baseline.
//! - [`Route::Plain`]: the mesh as a distance function, handed to the
//!   fitter every solid form uses (`fit_volume`): the best unit piece for
//!   each cell, then ramps stretched one way where that is closer.
//! - [`Route::Guided`]: the same unit pieces, then stretched pieces chosen
//!   by which way the mesh's surface faces at each cell: a ramp of any
//!   whole-number rise and run where the surface is parallel to an axis,
//!   and a stretched corner piece where it slopes along all three.
//!
//! The measures are taken from the bricks alone, against the mesh, by code
//! that knows nothing of how they were chosen (see [`measure`]).

use super::Mesh;
use crate::fit_volume::{classify, drop_spikes, fit_volume, merge, patterns, Opts, K};
use crate::par::Threads;
use crate::shapes::{inside_sampled, shape_info, CellBrick, Kind, CUBE};
use glam::DVec3;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A mesh stood in the lattice: `size` microbricks along its longest side,
/// its box's low corner `PAD` cells in from the corner of the cells fitted.
pub struct Stand<'a> {
    pub mesh: &'a Mesh,
    pub size: f64,
}

pub(super) const PAD: f64 = 2.0;

impl Stand<'_> {
    /// How many cells are fitted along each axis, from 0.
    pub fn cells(&self) -> [i32; 3] {
        (self.mesh.half * 2.0 * self.size + 2.0 * PAD).ceil().to_array().map(|v| v as i32)
    }

    pub(super) fn to_mesh(&self, p: DVec3) -> DVec3 {
        (p - PAD) / self.size - self.mesh.half
    }

    pub(super) fn from_mesh(&self, p: DVec3) -> DVec3 {
        (p + self.mesh.half) * self.size + PAD
    }

    /// Distance from the mesh's surface at `p`, in cells, negative inside:
    /// to the nearest triangle, signed by the winding number.
    pub fn distance(&self, p: DVec3) -> f64 {
        self.mesh.signed(self.to_mesh(p)).0 * self.size
    }

    /// The nearest point of the surface to `p`, in cells, and the way the
    /// triangle it is on faces.
    pub fn surface(&self, p: DVec3) -> (DVec3, DVec3) {
        let hit = self.mesh.nearest(self.to_mesh(p), f64::INFINITY).expect("a mesh has a triangle");
        ((hit.point + self.mesh.half) * self.size + PAD, self.mesh.triangle(hit.tri).1)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Route {
    Blocks,
    Plain,
    Guided,
    /// [`Route::Guided`], with the cells and their samples found from the
    /// triangles directly (see [`super::scan`]) and not by asking a distance
    /// function
    Scan,
}

/// A fit, and how long each part of it took, in milliseconds.
pub struct Fitted {
    pub bricks: Vec<CellBrick>,
    pub solid: f64,
    pub error: f64,
    pub stages: Vec<(&'static str, f64)>,
}

pub(super) fn clock() -> impl FnMut() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut last = std::time::Instant::now();
        move || {
            let ms = last.elapsed().as_secs_f64() * 1000.0;
            last = std::time::Instant::now();
            ms
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        || 0.0
    }
}

pub fn fit(stand: &Stand, route: Route, max_ext: i32, threads: Threads) -> Fitted {
    let max = stand.cells();
    let f = |p: DVec3| stand.distance(p);
    let opts = Opts { max_ext, wedges: route != Route::Blocks, stretch: true, threads };
    let mut lap = clock();
    if matches!(route, Route::Blocks | Route::Plain) {
        let fit = fit_volume(&f, [0; 3], max, &opts);
        return Fitted { bricks: fit.bricks, solid: fit.solid, error: fit.error, stages: vec![("fitted", lap())] };
    }
    let n = max.map(|v| v as usize);
    let (mut shape, sampled, solid, mut error, facing, mut stages) = if route == Route::Scan {
        let found = super::scan::scan(stand, n, true, threads);
        (found.shape, found.sampled, found.solid, found.error, Some(found.facing), found.stages)
    } else {
        let (shape, sampled, solid, error) = classify(&f, [0; 3], n, &opts);
        (shape, sampled, solid, error, None, vec![("unit pieces", lap())])
    };
    error = (error as i64 + drop_spikes(&mut shape, &sampled, n)) as u64;
    let surface = |k: usize, middle: DVec3| facing.as_ref().map_or_else(|| stand.surface(middle), |facing| facing[k]);
    let (mut bricks, saved, found, laid) = guided(&surface, &mut shape, &sampled, n, max_ext.max(1) as usize, 0, threads);
    stages.push(("stretched pieces found", found));
    stages.push(("stretched pieces laid", laid));
    lap();
    bricks.extend(merge(shape, n, max_ext.max(1) as usize));
    stages.push(("boxes joined", lap()));
    let per_cell = (K * K * K) as f64;
    Fitted { bricks, solid: solid as f64 / per_cell, error: (error as i64 - saved) as f64 / per_cell, stages }
}

/// Longest stretched ramp tried across its slope, and longest stretched
/// corner piece, in cells. A ramp is tried at every whole-number rise and
/// run up to this; a corner piece has three lengths, so a shorter limit.
const LONGEST_RAMP: usize = 12;
const LONGEST_CORNER: usize = 6;
/// How far, in degrees, a stretched piece's face may be from the surface's
/// own direction and still be tried; and how far the nearest there is may
/// be if none is that near.
const NEAR_DEGREES: f64 = 3.0;
const NEAREST_DEGREES: f64 = 9.0;

/// Each ramp, tetrahedron and cut cube with the side its solid part lies
/// to along each axis: -1 low, 1 high, 0 for a ramp's own axis.
fn poses() -> Vec<(u8, Kind, [i8; 3])> {
    patterns()
        .iter()
        .filter(|(mask, _)| *mask != 0 && *mask != CUBE)
        .map(|&(mask, bits)| {
            let mut lean = [0i32; 3];
            for s in (0..K * K * K).filter(|s| bits >> s & 1 == 1) {
                for (k, at) in [s % K, s / K % K, s / (K * K)].into_iter().enumerate() {
                    lean[k] += 2 * at as i32 + 1 - K as i32;
                }
            }
            (mask, shape_info(mask).expect("a pattern's mask is a shape").kind, lean.map(|v| v.signum() as i8))
        })
        .collect()
}

/// The samples a stretched piece covers in each cell of its box, x fastest.
fn stretched(mask: u8, ext: [usize; 3]) -> Vec<u64> {
    let info = shape_info(mask).expect("a shape");
    (0..ext[0] * ext[1] * ext[2])
        .map(|cell| {
            let at = [cell % ext[0], cell / ext[0] % ext[1], cell / (ext[0] * ext[1])];
            (0..K * K * K).fold(0u64, |bits, s| {
                let p = [s % K, s / K % K, s / (K * K)];
                let u = [0, 1, 2].map(|k| (at[k] as f64 + (p[k] as f64 + 0.5) / K as f64) / ext[k] as f64);
                bits | (inside_sampled(info, u[0], u[1], u[2]) as u64) << s
            })
        })
        .collect()
}

type Patterns = RwLock<HashMap<(u8, [usize; 3]), Arc<Vec<u64>>>>;

fn pattern_of(known: &Patterns, mask: u8, ext: [usize; 3]) -> Arc<Vec<u64>> {
    if let Some(found) = known.read().expect("no thread panics holding it").get(&(mask, ext)) {
        return found.clone();
    }
    let made = Arc::new(stretched(mask, ext));
    known.write().expect("no thread panics holding it").entry((mask, ext)).or_insert(made).clone()
}

/// The stretched pieces whose sloped face lies the way `normal` points,
/// to within a few degrees: their masks and sizes.
fn candidates(normal: DVec3, poses: &[(u8, Kind, [i8; 3])], max_ext: usize) -> Vec<(u8, [usize; 3])> {
    let size = normal.abs();
    let mut all = Vec::new();
    let degrees = |a: DVec3, b: DVec3| a.normalize().dot(b.normalize()).clamp(-1.0, 1.0).acos().to_degrees();
    // the solid lies on the side the surface faces away from
    let wanted = |active: [bool; 3]| [0, 1, 2].map(|k| if active[k] { -normal[k].signum() as i8 } else { 0 });
    let mut family = |kinds: &[Kind], active: [bool; 3], longest: usize| {
        let longest = longest.min(max_ext);
        let flat = DVec3::from_array([0, 1, 2].map(|k| if active[k] { size[k] } else { 0.0 }));
        let reach = |k: usize| if active[k] { 1..=longest } else { 1..=1 };
        let mut found: Vec<(f64, [usize; 3])> = Vec::new();
        for x in reach(0) {
            for y in reach(1) {
                for z in reach(2) {
                    let ext = [x, y, z];
                    let face = DVec3::from_array([0, 1, 2].map(|k| if active[k] { 1.0 / ext[k] as f64 } else { 0.0 }));
                    found.push((degrees(face, flat), ext));
                }
            }
        }
        let best = found.iter().map(|f| f.0).fold(f64::MAX, f64::min);
        let within = if best <= NEAR_DEGREES { NEAR_DEGREES } else if best <= NEAREST_DEGREES { best + 1e-9 } else { return };
        let signs = wanted(active);
        for &(mask, kind, lean) in poses.iter().filter(|p| kinds.contains(&p.1) && p.2 == signs) {
            let _ = kind;
            // (a piece one cell each way is a unit piece, which every cell has been offered already)
            all.extend(found.iter().filter(|f| f.0 <= within && f.1 != [1; 3]).map(|f| (mask, f.1)));
            let _ = lean;
        }
    };
    const LEVEL: f64 = 0.05;
    const NEARLY_LEVEL: f64 = 0.2;
    let across: Vec<usize> = (0..3).filter(|k| size[*k] >= LEVEL).collect();
    if across.len() == 3 {
        family(&[Kind::Tet, Kind::Cut], [true; 3], LONGEST_CORNER);
    }
    // a ramp along each axis the surface runs along, or nearly does
    for k in (0..3).filter(|k| size[*k] < NEARLY_LEVEL) {
        let active = [0, 1, 2].map(|a| a != k);
        if (0..3).filter(|a| active[*a]).all(|a| size[a] >= LEVEL) {
            family(&[Kind::Ramp], active, LONGEST_RAMP);
        }
    }
    all
}

struct Cand {
    /// how many fewer samples are wrong with the piece than with the unit
    /// pieces it replaces (less than nothing only where bricks are priced)
    saved: i32,
    /// that, and what the bricks it saves are priced at
    gain: u32,
    cells: u32,
    start: usize,
    mask: u8,
    ext: [u8; 3],
}

/// Lay stretched pieces along the mesh's surface wherever one fits it at
/// least as closely as the unit pieces there and takes fewer bricks.
/// Returns them, the disagreement they saved in samples, and how long the
/// finding and the laying took.
///
/// `fit_volume::stretch_ramps` tries every ramp up to six long, one high,
/// at every cell, because a distance function says nothing of which way a
/// surface faces. A mesh says exactly: the nearest triangle has a
/// direction. So here each cell is offered only the pieces whose face lies
/// that way, which is a handful where trying every rise and run and every
/// corner piece would be thousands, and can afford pieces far larger: a
/// roof at 1 in 2 is ramps twelve long and six high, not teeth.
///
/// `price` is what a brick is worth in samples of disagreement (the form's
/// Simplify setting, which prices a brick in cells: see `spec::Spec::simplify`).
/// At 0 a piece is laid only if it is no further from the mesh than the unit
/// pieces it replaces. Above 0 it may be further by `price` for each brick
/// it saves, which is one for each cell of the surface it covers but the
/// first.
pub(super) fn guided(surface: &(dyn Fn(usize, DVec3) -> (DVec3, DVec3) + Sync), shape: &mut [u8], sampled: &[(usize, u64)], n: [usize; 3], max_ext: usize, price: u32, threads: Threads) -> (Vec<CellBrick>, i64, f64, f64) {
    let mut lap = clock();
    let strides = [1, n[0], n[0] * n[1]];
    let mut unit = [0u64; 256];
    for &(mask, pattern) in patterns() {
        unit[mask as usize] = pattern;
    }
    let mut slot = vec![0u32; shape.len()];
    for (k, &(i, _)) in sampled.iter().enumerate() {
        slot[i] = k as u32 + 1;
    }
    let poses = poses();
    let known: Patterns = RwLock::new(HashMap::new());
    let standing: &[u8] = shape;
    let form = |i: usize| if slot[i] > 0 { sampled[slot[i] as usize - 1].1 } else { unit[standing[i] as usize] };
    let found = threads.map_ranges(sampled.len(), 40, |range| {
        let mut cands = Vec::new();
        for k in range {
            let cell = sampled[k].0;
            let at = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])];
            let middle = DVec3::new(at[0] as f64 + 0.5, at[1] as f64 + 0.5, at[2] as f64 + 0.5);
            let (on, normal) = surface(k, middle);
            for (mask, ext) in candidates(normal, &poses, max_ext) {
                let pattern = pattern_of(&known, mask, ext);
                let info = shape_info(mask).expect("a shape");
                // which axes the face slopes along, and what its plane adds up to (see `measure`)
                let active = [0, 1, 2].map(|k| !(info.kind == Kind::Ramp && info.axis as usize == k));
                let sum = if info.kind == Kind::Cut { 2.0 } else { 1.0 };
                let steep = (0..3).filter(|k| active[*k]).map(|k| 1.0 / (ext[k] * ext[k]) as f64).sum::<f64>().sqrt();
                for back in 0..ext[0] * ext[1] * ext[2] {
                    let off = [back % ext[0], back / ext[0] % ext[1], back / (ext[0] * ext[1])];
                    if (0..3).any(|k| at[k] < off[k] || at[k] - off[k] + ext[k] > n[k]) {
                        continue;
                    }
                    let start = [0, 1, 2].map(|k| at[k] - off[k]);
                    // Only where the piece's face would pass through the surface here: the face is the plane where its
                    // corner's coordinates add up to `sum`, and the surface's own point must be within half a cell of it.
                    let along: f64 = (0..3).filter(|k| active[*k]).map(|k| {
                        let t = (on[k] - start[k] as f64) / ext[k] as f64;
                        if normal[k] > 0.0 { t } else { 1.0 - t }
                    }).sum();
                    if (along - sum).abs() / steep > 0.5 {
                        continue;
                    }
                    let first = start[0] + n[0] * (start[1] + n[1] * start[2]);
                    let (mut now, mut then, mut reaches, mut replaced) = (0u32, 0u32, true, 0u32);
                    let slack = 64 + price * pattern.len() as u32;
                    for (i, piece) in pattern.iter().enumerate() {
                        let c = first + i % ext[0] + (i / ext[0] % ext[1]) * strides[1] + (i / (ext[0] * ext[1])) * strides[2];
                        let f = form(c);
                        // (never into a cell the form does not reach at all: see `fit_volume::stretch_ramps`)
                        reaches &= f != 0 || *piece == 0;
                        now += (f ^ unit[standing[c] as usize]).count_ones();
                        then += (f ^ piece).count_ones();
                        replaced += (slot[c] > 0 && standing[c] != 0) as u32;
                        if then > now + slack {
                            break;
                        }
                    }
                    let allowed = now + price * replaced.saturating_sub(1);
                    if reaches && then <= allowed {
                        cands.push(Cand { saved: now as i32 - then as i32, gain: allowed - then, cells: pattern.len() as u32, start: first, mask, ext: ext.map(|v| v as u8) });
                    }
                }
            }
        }
        cands
    });
    let mut cands: Vec<Cand> = found.into_iter().flatten().collect();
    let found_ms = lap();
    cands.sort_unstable_by_key(|c| (c.start, c.mask, c.ext));
    cands.dedup_by_key(|c| (c.start, c.mask, c.ext));
    // Pieces of one size, facing one way, that stand on one lattice of
    // boxes (each starts a whole number of its own lengths from the next)
    // are one layout of a face: laid together they tile it, as long ramps
    // course after course or as corner pieces side by side. Taken one at a
    // time by what each saves, pieces from different lattices were mixed
    // on one face, each stopping the next from being laid, and a flat
    // facet came out as scattered pieces with unit ones between (the
    // tilted box, measured in docs/algorithm.md). So the layouts are
    // ranked by what all their pieces save and cover together, and laid a
    // whole layout at a time; within one, the most saved first.
    let lean_of: HashMap<u8, [i8; 3]> = poses.iter().map(|p| (p.0, p.2)).collect();
    let layout = |c: &Cand| {
        let at = [c.start % n[0], c.start / n[0] % n[1], c.start / (n[0] * n[1])];
        (lean_of[&c.mask], c.ext, [0, 1, 2].map(|k| (at[k] % c.ext[k] as usize) as u8))
    };
    let mut worth: HashMap<([i8; 3], [u8; 3], [u8; 3]), u64> = HashMap::new();
    for c in &cands {
        *worth.entry(layout(c)).or_insert(0) += c.gain as u64 + c.cells as u64;
    }
    cands.sort_by_key(|c| (std::cmp::Reverse(worth[&layout(c)]), layout(c), std::cmp::Reverse(c.gain), c.start, c.mask));
    let mut taken = vec![false; shape.len()];
    let (mut bricks, mut saved) = (Vec::new(), 0i64);
    for c in &cands {
        let ext = c.ext.map(|v| v as usize);
        let cells = |i: usize| c.start + i % ext[0] + (i / ext[0] % ext[1]) * strides[1] + (i / (ext[0] * ext[1])) * strides[2];
        if (0..c.cells as usize).any(|i| taken[cells(i)]) {
            continue;
        }
        for i in 0..c.cells as usize {
            taken[cells(i)] = true;
            shape[cells(i)] = 0;
        }
        saved += c.saved as i64;
        bricks.push(CellBrick { mask: c.mask, min: [c.start % n[0], c.start / n[0] % n[1], c.start / (n[0] * n[1])].map(|v| v as i32), ext: ext.map(|v| v as i32) });
    }
    // ramps side by side along their own axis are one longer ramp
    let axis_of = |b: &CellBrick| shape_info(b.mask).map_or(3, |s| if s.kind == Kind::Ramp { s.axis as usize } else { 3 });
    bricks.sort_by_key(|b| {
        let a = axis_of(b).min(2);
        let mut across = b.min;
        across[a] = 0;
        (axis_of(b), b.mask, b.ext, across, b.min[a])
    });
    let mut joined: Vec<CellBrick> = Vec::with_capacity(bricks.len());
    for b in bricks {
        let a = axis_of(&b);
        if let Some(last) = joined.last_mut().filter(|last| a < 3 && last.mask == b.mask) {
            let mut same = last.min[a] + last.ext[a] == b.min[a] && (last.ext[a] + b.ext[a]) as usize <= max_ext;
            for k in (0..3).filter(|k| *k != a) {
                same &= last.min[k] == b.min[k] && last.ext[k] == b.ext[k];
            }
            if same {
                last.ext[a] += b.ext[a];
                continue;
            }
        }
        joined.push(b);
    }
    (joined, saved, found_ms, lap())
}

/// What a fit is judged on, measured from its bricks against the solid.
#[derive(Clone, Copy, Debug, Default)]
pub struct Measured {
    pub bricks: usize,
    /// the solid's volume, and the volume where the bricks and it disagree, in cells
    pub solid: f64,
    pub wrong: f64,
    /// how far the bricks' outer faces stand from the surface, in cells: the furthest and the mean
    pub worst_gap: f64,
    pub mean_gap: f64,
    /// cells the surface passes through
    pub surface: usize,
    /// The area, in cell faces, of steps the bricks make that the solid
    /// does not have (see `blend::tests::seam`): across a face between two
    /// cells, brick on one side and none on the other where the solid is
    /// the same on both.
    pub steps: f64,
    /// sloped bricks one cell each way, and those of them with no sloped brick beside them
    pub unit_sloped: usize,
    pub alone: usize,
}

/// Measure bricks filling cells `0..n` against the solid `f` (negative inside).
pub fn measure(f: &(dyn Fn(DVec3) -> f64 + Sync), n: [usize; 3], bricks: &[CellBrick], threads: Threads) -> Measured {
    let opts = Opts { max_ext: 1, wedges: true, stretch: false, threads };
    let (truth, sampled, solid, _) = classify(&f, [0; 3], n, &opts);
    let index = |c: [i32; 3]| (0..3).all(|k| c[k] >= 0 && (c[k] as usize) < n[k]).then(|| c[0] as usize + n[0] * (c[1] as usize + n[1] * c[2] as usize));
    let mut owner = vec![u32::MAX; truth.len()];
    let mut known: HashMap<(u8, [usize; 3]), Vec<u64>> = HashMap::new();
    for (b, brick) in bricks.iter().enumerate() {
        for z in 0..brick.ext[2] {
            for y in 0..brick.ext[1] {
                for x in 0..brick.ext[0] {
                    if let Some(i) = index([brick.min[0] + x, brick.min[1] + y, brick.min[2] + z]) {
                        owner[i] = b as u32;
                    }
                }
            }
        }
        if brick.mask != CUBE {
            known.entry((brick.mask, brick.ext.map(|v| v as usize))).or_insert_with(|| stretched(brick.mask, brick.ext.map(|v| v as usize)));
        }
    }
    // the samples of cell `i` that its brick fills
    let filled = |i: usize| -> u64 {
        let Some(brick) = bricks.get(owner[i] as usize) else { return 0 };
        if brick.mask == CUBE {
            return u64::MAX;
        }
        let at = [i % n[0], i / n[0] % n[1], i / (n[0] * n[1])];
        let ext = brick.ext.map(|v| v as usize);
        let off = [0, 1, 2].map(|k| at[k] - brick.min[k] as usize);
        known[&(brick.mask, ext)][off[0] + ext[0] * (off[1] + ext[1] * off[2])]
    };
    let mut slot = vec![0u32; truth.len()];
    for (k, &(i, _)) in sampled.iter().enumerate() {
        slot[i] = k as u32 + 1;
    }
    let wrong: u64 = (0..truth.len()).map(|i| ((if slot[i] > 0 { sampled[slot[i] as usize - 1].1 } else if truth[i] == CUBE { u64::MAX } else { 0 }) ^ filled(i)).count_ones() as u64).sum();
    let held = |p: DVec3| -> bool {
        let Some(i) = index(p.floor().to_array().map(|v| v as i32)) else { return false };
        let Some(brick) = bricks.get(owner[i] as usize) else { return false };
        brick.mask == CUBE || {
            let u = [0, 1, 2].map(|k| (p[k] - brick.min[k] as f64) / brick.ext[k] as f64);
            crate::shapes::inside_shape(shape_info(brick.mask).expect("a shape"), u[0], u[1], u[2])
        }
    };
    // steps: across each face of a cell the surface passes through, and between such a cell and one beside it
    const FINE: usize = 4;
    let stepped = threads.map(sampled.len(), 20, |k| {
        let cell = sampled[k].0;
        let at = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])].map(|v| v as i32);
        let mut steps = 0u32;
        for face in 0..6 {
            let (axis, high) = (face / 2, face % 2 == 1);
            // (a face two such cells share is counted from the lower of them)
            let mut beside = at;
            beside[axis] += if high { 1 } else { -1 };
            if high && index(beside).is_some_and(|i| slot[i] > 0) {
                continue;
            }
            for s in 0..FINE * FINE {
                let (a, b) = ((s % FINE) as f64 + 0.5, (s / FINE) as f64 + 0.5);
                let mut p = DVec3::new(at[0] as f64, at[1] as f64, at[2] as f64);
                p[axis] += if high { 1.0 } else { 0.0 };
                p[(axis + 1) % 3] += a / FINE as f64;
                p[(axis + 2) % 3] += b / FINE as f64;
                let mut step = DVec3::ZERO;
                step[axis] = 0.02;
                if held(p - step) != held(p + step) && f(p).abs() > 0.03 {
                    steps += 1;
                }
            }
        }
        steps
    });
    // gaps: points on every sloped face, and the middle of every cell's face of a box that nothing stands against
    let poses = poses();
    let gaps = threads.map(bricks.len(), 20, |b| {
        let brick = &bricks[b];
        let (mut worst, mut sum, mut count) = (0f64, 0f64, 0u32);
        let mut note = |p: DVec3| {
            let gap = f(p).abs();
            worst = worst.max(gap);
            sum += gap;
            count += 1;
        };
        let (min, ext) = (DVec3::from_array(brick.min.map(|v| v as f64)), DVec3::from_array(brick.ext.map(|v| v as f64)));
        if brick.mask == CUBE {
            for face in 0..6 {
                let (axis, high) = (face / 2, face % 2 == 1);
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                for a in 0..brick.ext[u] {
                    for c in 0..brick.ext[v] {
                        let mut cell = brick.min;
                        cell[u] += a;
                        cell[v] += c;
                        cell[axis] += if high { brick.ext[axis] } else { -1 };
                        if index(cell).is_some_and(|i| owner[i] != u32::MAX) {
                            continue;
                        }
                        let mut p = DVec3::new(cell[0] as f64 + 0.5, cell[1] as f64 + 0.5, cell[2] as f64 + 0.5);
                        p[axis] = (brick.min[axis] + if high { brick.ext[axis] } else { 0 }) as f64;
                        note(p);
                    }
                }
            }
        } else if let Some(&(_, kind, lean)) = poses.iter().find(|p| p.0 == brick.mask) {
            // The face is where the coordinates measured from the solid's
            // corner add up to 1 (a ramp, over its two sloping axes, and a
            // tetrahedron) or 2 (a cut cube). Points spread over it.
            let spread: &[[f64; 3]] = match kind {
                Kind::Ramp => &[[0.5, 0.5, 0.5], [0.1, 0.9, 0.1], [0.9, 0.1, 0.1], [0.1, 0.9, 0.9], [0.9, 0.1, 0.9]],
                _ => &[[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], [0.8, 0.1, 0.1], [0.1, 0.8, 0.1], [0.1, 0.1, 0.8]],
            };
            for share in spread {
                let mut l = *share;
                if kind == Kind::Ramp {
                    // (two shares over the sloping axes, the third along the ramp)
                    let axis = (0..3).find(|k| lean[*k] == 0).unwrap_or(0);
                    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                    (l[u], l[v], l[axis]) = (share[0], share[1], share[2]);
                } else if kind == Kind::Cut {
                    l = l.map(|s| 1.0 - s);
                }
                note(DVec3::from_array([0, 1, 2].map(|k| min[k] + ext[k] * if lean[k] > 0 { 1.0 - l[k] } else { l[k] })));
            }
        }
        (worst, sum, count)
    });
    let (worst_gap, sum, count) = gaps.iter().fold((0f64, 0f64, 0u32), |a, g| (a.0.max(g.0), a.1 + g.1, a.2 + g.2));
    let sloped: std::collections::HashSet<[i32; 3]> = bricks.iter().filter(|b| b.mask != CUBE).flat_map(|b| (0..b.ext[0] * b.ext[1] * b.ext[2]).map(move |i| [b.min[0] + i % b.ext[0], b.min[1] + i / b.ext[0] % b.ext[1], b.min[2] + i / (b.ext[0] * b.ext[1])])).collect();
    let unit: Vec<[i32; 3]> = bricks.iter().filter(|b| b.mask != CUBE && b.ext == [1; 3]).map(|b| b.min).collect();
    let alone = unit.iter().filter(|c| (0..6).all(|face| !sloped.contains(&[0, 1, 2].map(|k| c[k] + if k == face / 2 { [-1, 1][face % 2] } else { 0 })))).count();
    let per_cell = (K * K * K) as f64;
    Measured { bricks: bricks.len(), solid: solid as f64 / per_cell, wrong: wrong as f64 / per_cell, worst_gap, mean_gap: sum / count.max(1) as f64, surface: sampled.len(), steps: stepped.iter().sum::<u32>() as f64 / (FINE * FINE) as f64, unit_sloped: unit.len(), alone }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_two_share_a_cell(bricks: &[CellBrick]) {
        let mut seen = std::collections::HashSet::new();
        for b in bricks {
            for i in 0..b.ext[0] * b.ext[1] * b.ext[2] {
                assert!(seen.insert([b.min[0] + i % b.ext[0], b.min[1] + i / b.ext[0] % b.ext[1], b.min[2] + i / (b.ext[0] * b.ext[1])]), "two bricks in one cell at {:?}", b.min);
            }
        }
    }

    #[test]
    fn every_route_builds_a_sphere_of_the_right_volume_the_same_on_any_number_of_threads() {
        let mesh = super::super::made::sphere(24);
        let stand = Stand { mesh: &mesh, size: 24.0 };
        let exact = 4.0 / 3.0 * std::f64::consts::PI * 12f64.powi(3);
        for route in [Route::Blocks, Route::Plain, Route::Guided] {
            let one = fit(&stand, route, 80, Threads::ONE);
            let many = fit(&stand, route, 80, Threads::forced(5));
            assert_eq!(one.bricks, many.bricks, "{route:?}");
            no_two_share_a_cell(&one.bricks);
            let volume: f64 = one.bricks.iter().map(CellBrick::volume).sum();
            assert!((volume / exact - 1.0).abs() < 0.04, "{route:?}: {volume} against {exact}");
            // and the fit's own figure for what is wrong is what the bricks measure as
            let measured = measure(&|p| stand.distance(p), stand.cells().map(|v| v as usize), &one.bricks, Threads::forced(3));
            assert!((measured.wrong - one.error).abs() < 1e-6, "{route:?}: said {} and measured {}", one.error, measured.wrong);
        }
    }

    #[test]
    fn scanning_the_triangles_gives_the_bricks_that_asking_the_distance_everywhere_gives() {
        // at sizes where no face of the mesh lies along a sample or a corner of the lattice, where the two could each call it either way
        for (mesh, size) in [(super::super::made::house(), 41.3), (super::super::made::lumpy(24), 30.7), (super::super::made::gem(), 37.1), (super::super::made::torus(12), 45.2)] {
            let stand = Stand { mesh: &mesh, size };
            let n = stand.cells().map(|v| v as usize);
            let opts = Opts { max_ext: 80, wedges: true, stretch: true, threads: Threads::forced(4) };
            let (shape, _, solid, error) = classify(&|p| stand.distance(p), [0; 3], n, &opts);
            let scanned = super::super::scan::scan(&stand, n, true, Threads::forced(3));
            // The same piece in every cell, but for a cell with a corner of its own exactly on the surface (the mesh's box
            // starts on the lattice, so an edge along the box does), which the two may each count as in or out: there the
            // samples still agree, and the piece is one of two that fit them equally.
            for (i, _) in shape.iter().zip(&scanned.shape).enumerate().filter(|(_, (a, b))| a != b) {
                let at = [i % n[0], i / n[0] % n[1], i / (n[0] * n[1])];
                let corner = |c: usize| DVec3::new((at[0] + (c & 1)) as f64, (at[1] + (c >> 1 & 1)) as f64, (at[2] + (c >> 2)) as f64);
                assert!((0..8).any(|c| stand.distance(corner(c)).abs() < 1e-9), "{mesh:?} at {size}: the piece at {at:?} is {:#x} asked and {:#x} scanned", shape[i], scanned.shape[i]);
            }
            assert_eq!(solid, scanned.solid, "the volume of {mesh:?} at {size}");
            assert!(error.abs_diff(scanned.error) <= 64, "what is wrong with the unit pieces of {mesh:?} at {size}: {error} and {}", scanned.error);
            // and the whole fit from each is as close and as many bricks, to within the few pieces that are offered from other cells
            let (asked, scanned) = (fit(&stand, Route::Guided, 80, Threads::forced(4)), fit(&stand, Route::Scan, 80, Threads::forced(3)));
            assert!((asked.error - scanned.error).abs() < 0.02 * asked.error.max(50.0) && asked.bricks.len().abs_diff(scanned.bricks.len()) * 20 <= asked.bricks.len(), "{mesh:?} at {size}: {} wrong in {} bricks asked, {} in {} scanned", asked.error, asked.bricks.len(), scanned.error, scanned.bricks.len());
            assert_eq!(scanned.bricks, fit(&stand, Route::Scan, 80, Threads::ONE).bricks);
        }
    }

    #[test]
    fn a_roof_at_one_in_two_is_long_ramps_lying_in_it() {
        let mesh = super::super::made::house();
        let stand = Stand { mesh: &mesh, size: 56.0 };
        let fitted = fit(&stand, Route::Guided, 80, Threads::forced(4));
        no_two_share_a_cell(&fitted.bricks);
        let measured = measure(&|p| stand.distance(p), stand.cells().map(|v| v as usize), &fitted.bricks, Threads::forced(4));
        // a house of flat faces on the lattice is built exactly, with no step that is not its own
        assert!(measured.wrong < 1e-9 && measured.steps < 1e-9 && measured.unit_sloped == 0, "{measured:?}");
        let ramps: Vec<&CellBrick> = fitted.bricks.iter().filter(|b| b.mask != CUBE).collect();
        assert!(ramps.iter().all(|b| shape_info(b.mask).unwrap().kind == Kind::Ramp && b.ext[1] == 2 * b.ext[2]), "{ramps:?}");
        assert!(ramps.len() <= 16, "{} ramps on two slopes", ramps.len());
    }
}
