//! Fit a solid in space with unit microbricks, minimising the volume where
//! the bricks and the solid disagree, then merge bricks to cut their number.
//!
//! As in the plane, every brick owns its box outright, so the disagreement is
//! a sum over cells and each cell is decided on its own: of the 30 things a
//! cell can hold (nothing, a cube, 12 ramps, 8 tetrahedra, 8 cubes missing a
//! tetrahedron) take the one that disagrees least with the solid. That is the
//! minimum for unit bricks, not an estimate of it.
//!
//! Stretched ramps and corners are not fitted here yet; merging only joins
//! bricks whose union is the same solid, so it never changes the fit.

use crate::sdf::FAR_MARGIN;
use crate::shapes::{all_masks, inside_sampled, least_unshared, shape_info, CellBrick, Kind, CUBE};
use glam::DVec3;
use std::sync::OnceLock;

/// Samples per cell, each way: 4 x 4 x 4 fills one u64, so a cell's
/// disagreement with a candidate is one XOR and one population count.
/// https://doc.rust-lang.org/std/primitive.u64.html#method.count_ones
pub(crate) const K: usize = 4;

/// Cells per block, each way. The solid is first tested once per block: where
/// a block's centre is further from the surface than the block's corners are
/// from its centre, the whole block is inside or outside and none of its
/// cells are looked at. A sphere 100 cells across is 1,000,000 cells of which
/// about 60,000 touch the surface, so this is most of the run time saved.
/// It relies on the solid being a distance function (see `sdf`), which cannot
/// change faster than 1 per cell.
const BLOCK: usize = 8;

// "far" answers from `sdf` are only safe to trust across a whole block if a
// block's half diagonal is less than the margin they are computed with
const _: () = assert!((BLOCK * BLOCK * 3) as f64 / 4.0 < (FAR_MARGIN - 1.0) * (FAR_MARGIN - 1.0));

/// Half the diagonal of one cell: a cell whose 8 corners are all further than
/// this from the surface, on the same side, cannot be crossed by it.
const CELL_REACH: f32 = 0.87;

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    /// longest brick edge, in cells
    pub max_ext: i32,
    /// false fits with cubes alone
    pub wedges: bool,
    /// false keeps every ramp one cell across (see `stretch_ramps`)
    pub stretch: bool,
}

// There is deliberately no price per brick here, as the planar fitter has.
// One was tried: a tetrahedron corrects at most a sixth of a cell and a cut
// cube the same, so any worthwhile price removed every corner piece and left
// a pit wherever three slopes meet. In the plane a price swaps several short
// triangles for one long one and the outline stays continuous; on a surface
// made of unit pieces there is nothing longer to swap to, so it could only
// make holes. Fewer bricks on a volume come from a larger brick size.

impl Default for Opts {
    fn default() -> Self {
        Opts {
            max_ext: 80,
            wedges: true,
            stretch: true,
        }
    }
}

/// A fit and how good it is, in cells cubed.
#[derive(Clone, Debug, Default)]
pub struct Fit {
    pub bricks: Vec<CellBrick>,
    /// volume of the solid itself
    pub solid: f64,
    /// volume where the bricks and the solid disagree
    pub error: f64,
}

/// Each thing a cell can hold: its corner mask and its sample pattern.
/// Entry 0 is nothing and entry 1 the cube, so `[..2]` is the cubes-only set.
pub(crate) fn patterns() -> &'static [(u8, u64)] {
    static PATTERNS: OnceLock<Vec<(u8, u64)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let at = |i: usize| (i as f64 + 0.5) / K as f64;
        let mut all = vec![(0u8, 0u64), (CUBE, u64::MAX)];
        // Not the hips: they are for forms laid out whole, with a ridge to
        // put them on (see `fit_pyramid`). Offered to every cell of a curved
        // surface they would nearly double the shapes each cell is tried
        // against, for a piece that belongs on a ridge and nowhere else.
        for mask in all_masks().into_iter().filter(|&m| m != CUBE) {
            let info = shape_info(mask).unwrap();
            // nor the kinds that came with it, for the same reason
            if !matches!(info.kind, Kind::Ramp | Kind::Tet | Kind::Cut) {
                continue;
            }
            let bits = (0..K * K * K).fold(0u64, |bits, s| {
                bits | (inside_sampled(info, at(s % K), at(s / K % K), at(s / (K * K))) as u64) << s
            });
            all.push((mask, bits));
        }
        all
    })
}

/// Fit the solid `f` (negative inside) over cells `min..max`.
pub fn fit_volume(f: &impl Fn(DVec3) -> f64, min: [i32; 3], max: [i32; 3], opts: &Opts) -> Fit {
    let n = [0, 1, 2].map(|i| (max[i] - min[i]).max(0) as usize);
    if n.contains(&0) {
        return Fit::default();
    }
    let (mut shape, sampled, solid, mut error) = classify(f, min, n, opts);
    let max_ext = opts.max_ext.max(1) as usize;
    if opts.wedges {
        error = (error as i64 + drop_spikes(&mut shape, &sampled, n)) as u64;
    }
    let mut bricks = Vec::new();
    if opts.wedges && opts.stretch {
        let (stretched, saved) = stretch_ramps(&mut shape, &sampled, n, max_ext);
        bricks = stretched;
        error -= saved;
    }
    bricks.extend(merge(shape, n, max_ext));
    for b in &mut bricks {
        for i in 0..3 {
            b.min[i] += min[i];
        }
    }
    let per_cell = (K * K * K) as f64;
    Fit {
        bricks,
        solid: solid as f64 / per_cell,
        error: error as f64 / per_cell,
    }
}

/// The samples of the cell at `cell` that are inside the form, and which of
/// its corners are.
fn cell_bits(f: &impl Fn(DVec3) -> f64, cell: DVec3) -> (u64, u8) {
    let bits = (0..K * K * K).fold(0u64, |bits, s| {
        let p = DVec3::new(
            (s % K) as f64 + 0.5,
            (s / K % K) as f64 + 0.5,
            (s / (K * K)) as f64 + 0.5,
        ) / K as f64;
        // strictly inside, to agree with the planar fitter on samples
        // lying exactly on the surface
        bits | ((f(cell + p) < 0.0) as u64) << s
    });
    let corners = (0..8u8).fold(0u8, |m, k| {
        let corner = cell + DVec3::new((k & 1) as f64, (k >> 1 & 1) as f64, (k >> 2) as f64);
        m | ((f(corner) <= 0.0) as u8) << k
    });
    (bits, corners)
}

/// The best unit brick for every cell, as a corner mask (0 for none), with the
/// solid's volume and the total disagreement, both in samples.
///
/// One byte per cell in a flat array, x fastest: the merge pass scans it in
/// runs along each axis, which a flat array serves from cache and a hash map
/// of occupied cells would not.
///
/// Also returned: the form's samples in every cell the surface passes
/// through, by cell index in ascending order. Only those cells, which are a
/// thin shell of the volume, so a sorted list and not a second full grid.
fn classify(
    f: &impl Fn(DVec3) -> f64,
    min: [i32; 3],
    n: [usize; 3],
    opts: &Opts,
) -> (Vec<u8>, Vec<(usize, u64)>, u64, u64) {
    let origin = DVec3::new(min[0] as f64, min[1] as f64, min[2] as f64);
    let patterns = if opts.wedges {
        patterns()
    } else {
        &patterns()[..2]
    };
    let per_cell = (K * K * K) as u64;
    let mut shape = vec![0u8; n[0] * n[1] * n[2]];
    let mut sampled = Vec::new();
    let (mut solid, mut error) = (0u64, 0u64);
    // one block's lattice values, reused; a grid of the whole volume's would
    // be 4 bytes for every lattice point to save the 40% a block re-evaluates
    // on the faces it shares with its neighbours
    const SIDE: usize = BLOCK + 1;
    let mut corner = [0f32; SIDE * SIDE * SIDE];

    for bz in (0..n[2]).step_by(BLOCK) {
        for by in (0..n[1]).step_by(BLOCK) {
            for bx in (0..n[0]).step_by(BLOCK) {
                let b = [bx, by, bz];
                let size = [0, 1, 2].map(|i| BLOCK.min(n[i] - b[i]));
                let half = DVec3::new(size[0] as f64, size[1] as f64, size[2] as f64) / 2.0;
                let base = origin + DVec3::new(bx as f64, by as f64, bz as f64);
                let centre = f(base + half);
                if centre > half.length() {
                    continue;
                }
                let index =
                    |x: usize, y: usize, z: usize| bx + x + n[0] * (by + y + n[1] * (bz + z));
                if centre < -half.length() {
                    for z in 0..size[2] {
                        for y in 0..size[1] {
                            shape[index(0, y, z)..index(size[0], y, z)].fill(CUBE);
                        }
                    }
                    solid += (size[0] * size[1] * size[2]) as u64 * per_cell;
                    continue;
                }
                for z in 0..=size[2] {
                    for y in 0..=size[1] {
                        for x in 0..=size[0] {
                            corner[x + SIDE * (y + SIDE * z)] =
                                f(base + DVec3::new(x as f64, y as f64, z as f64)) as f32;
                        }
                    }
                }
                for z in 0..size[2] {
                    for y in 0..size[1] {
                        for x in 0..size[0] {
                            let c: [f32; 8] = std::array::from_fn(|k| {
                                corner[x
                                    + (k & 1)
                                    + SIDE * (y + (k >> 1 & 1) + SIDE * (z + (k >> 2)))]
                            });
                            if c.iter().all(|&v| v > CELL_REACH) {
                                continue;
                            }
                            if c.iter().all(|&v| v < -CELL_REACH) {
                                shape[index(x, y, z)] = CUBE;
                                solid += per_cell;
                                continue;
                            }
                            let cell = base + DVec3::new(x as f64, y as f64, z as f64);
                            let (bits, _) = cell_bits(f, cell);
                            let corners = (0..8).fold(0u8, |m, k| m | ((c[k] <= 0.0) as u8) << k);
                            // least disagreement, then the shape whose corners
                            // best match the solid's; see `least_unshared`
                            // for what happens to exact ties
                            let mut scores = [(0u32, 0u32); 30];
                            for (score, (mask, pattern)) in scores.iter_mut().zip(patterns) {
                                *score =
                                    ((bits ^ pattern).count_ones(), (corners ^ mask).count_ones());
                            }
                            let (mask, pattern) =
                                patterns[least_unshared(&scores[..patterns.len()], 2)];
                            shape[index(x, y, z)] = mask;
                            sampled.push((index(x, y, z), bits));
                            solid += bits.count_ones() as u64;
                            error += (bits ^ pattern).count_ones() as u64;
                        }
                    }
                }
            }
        }
    }
    // blocks are visited in order but each holds a range of cells
    sampled.sort_unstable_by_key(|s| s.0);
    (shape, sampled, solid, error)
}

/// Remove tetrahedra that stand on the form like spikes, and return the
/// change in disagreement, in samples.
///
/// Each cell is given the brick that fits it best with no regard to its
/// neighbours, and where a gently curving surface clips the corner of an
/// otherwise empty cell, that brick is a tetrahedron: by volume it is right,
/// and it stands alone on the surface touching it with one face. A
/// tetrahedron belongs in an inside corner, where at least two of its three
/// flat faces lie against the form; with one or none it is taken out. The
/// volume it leaves unbuilt is at most a sixth of a cell.
///
/// Every tetrahedron is judged against the bricks as they stood at the start
/// of a pass, and the pass repeated, since taking one out can leave another
/// without its second backing. Judging and removing in one sweep would make
/// the outcome depend on the order of the sweep.
pub(crate) fn drop_spikes(shape: &mut [u8], sampled: &[(usize, u64)], n: [usize; 3]) -> i64 {
    let strides = [1, n[0], n[0] * n[1]];
    let mut unit = [0u64; 256];
    for &(mask, pattern) in patterns() {
        unit[mask as usize] = pattern;
    }
    let mut change = 0i64;
    for _ in 0..4 {
        let mut spikes = Vec::new();
        for &(cell, bits) in sampled {
            let Some(info) = shape_info(shape[cell]).filter(|s| s.kind == Kind::Tet) else {
                continue;
            };
            let at = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])];
            let backed = (0..3)
                .filter(|&k| {
                    // the cell on the other side of the face the tetrahedron's
                    // right angle sits against, along axis k
                    let low = info.corner >> k & 1 == 0;
                    if (low && at[k] == 0) || (!low && at[k] + 1 == n[k]) {
                        return false;
                    }
                    let beside = shape[if low {
                        cell - strides[k]
                    } else {
                        cell + strides[k]
                    }];
                    // The tetrahedron's three corners on that face, as the
                    // neighbour numbers them: the same corners, on its
                    // opposite side along k. It backs the face if it has all
                    // three.
                    let across = |c: u8| c ^ (1 << k);
                    [
                        info.corner,
                        info.corner ^ (1 << ((k + 1) % 3)),
                        info.corner ^ (1 << ((k + 2) % 3)),
                    ]
                    .iter()
                    .all(|&c| beside >> across(c) & 1 == 1)
                })
                .count();
            if backed < 2 {
                spikes.push((cell, bits));
            }
        }
        if spikes.is_empty() {
            break;
        }
        for (cell, bits) in spikes {
            change +=
                bits.count_ones() as i64 - (bits ^ unit[shape[cell] as usize]).count_ones() as i64;
            shape[cell] = 0;
        }
    }
    change
}

/// Longest stretched ramp tried, in cells. Longer ones are shallower than a
/// surface stays for long; this bounds the search without missing much.
const MAX_STRETCH: usize = 6;

/// One way of laying a stretched ramp: which unit ramp it is a stretch of,
/// along which axis, over how many cells, and the samples it covers in each.
struct Stretch {
    mask: u8,
    dir: usize,
    cells: Vec<u64>,
}

fn stretches() -> &'static [Stretch] {
    static ALL: OnceLock<Vec<Stretch>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = Vec::new();
        for mask in all_masks() {
            let info = shape_info(mask).unwrap();
            if info.kind != Kind::Ramp {
                continue;
            }
            // across its slope, not along its prism: lengthening a ramp along
            // its prism axis is what `merge` already does
            for dir in (0..3).filter(|d| *d != info.axis as usize) {
                for len in 2..=MAX_STRETCH {
                    let cells = (0..len)
                        .map(|i| {
                            (0..K * K * K).fold(0u64, |bits, s| {
                                let mut p = [
                                    (s % K) as f64 + 0.5,
                                    (s / K % K) as f64 + 0.5,
                                    (s / (K * K)) as f64 + 0.5,
                                ]
                                .map(|v| v / K as f64);
                                p[dir] = (i as f64 + p[dir]) / len as f64;
                                bits | (inside_sampled(info, p[0], p[1], p[2]) as u64) << s
                            })
                        })
                        .collect();
                    all.push(Stretch { mask, dir, cells });
                }
            }
        }
        all
    })
}

/// Replace runs of cells with one stretched ramp wherever that fits the form
/// more closely than the unit bricks there. Returns the ramps, and how much
/// disagreement they saved, in samples; the cells they take are cleared from
/// `shape`.
///
/// Unit ramps are all at 45 degrees, so a surface at a gentler slope, such
/// as the top of a dome, comes out as a row of teeth: each cell's ramp is
/// the best brick for that cell, and the row is still not the surface. One
/// ramp two or more cells long, one high, is. Every such ramp that starts in
/// or reaches a cell the surface crosses is measured against the form over
/// the cells it would cover; those that disagree less than what is there now
/// are taken, largest saving first.
///
/// Only ramps, and only stretched one way. Stretched corner pieces, and
/// ramps at slopes like 2 in 3, would follow a curved surface more closely
/// still, and are not done.
pub(crate) fn stretch_ramps(
    shape: &mut [u8],
    sampled: &[(usize, u64)],
    n: [usize; 3],
    max_ext: usize,
) -> (Vec<CellBrick>, u64) {
    let strides = [1, n[0], n[0] * n[1]];
    // each unit shape's samples, by mask
    let mut unit = [0u64; 256];
    for &(mask, pattern) in patterns() {
        unit[mask as usize] = pattern;
    }
    // Which sampled entry a cell is, plus one; 0 for a cell the surface does
    // not cross. A direct table, because the search below reads a cell's
    // samples tens of millions of times and a binary search for each was
    // most of its run time. It costs 4 bytes a cell, so for a very large
    // volume the search falls back to the sorted list.
    let slot: Option<Vec<u32>> = (shape.len() <= 8_000_000).then(|| {
        let mut slot = vec![0u32; shape.len()];
        for (k, &(i, _)) in sampled.iter().enumerate() {
            slot[i] = k as u32 + 1;
        }
        slot
    });
    let form = |i: usize, shape: &[u8]| {
        let k = match &slot {
            Some(slot) => slot[i] as usize,
            None => sampled
                .binary_search_by_key(&i, |s| s.0)
                .map_or(0, |k| k + 1),
        };
        // not on the surface: wholly inside (a cube) or wholly outside
        if k > 0 {
            sampled[k - 1].1
        } else {
            unit[shape[i] as usize]
        }
    };

    // The samples in the upper half of a cell along each axis: how many more
    // of a cell's inside samples are there than in the lower half says which
    // way the surface faces.
    let upper: [u64; 3] = [0, 1, 2].map(|axis| {
        (0..K * K * K)
            .filter(|s| s / K.pow(axis as u32) % K >= K / 2)
            .fold(0u64, |m, s| m | 1 << s)
    });
    // the unit ramp with its prism along `axis` and its open side at the
    // given end (0 low, 1 high) of the other two axes
    let ramp_facing = |axis: usize, open: [u8; 3]| {
        all_masks().into_iter().find(|&m| {
            let info = shape_info(m).unwrap();
            info.kind == Kind::Ramp
                && info.axis as usize == axis
                && (0..3)
                    .filter(|k| *k != axis)
                    .all(|k| info.corner >> k & 1 == open[k])
        })
    };
    let mut facing = [[[0u8; 2]; 2]; 3];
    for axis in 0..3 {
        let (p, q) = [(1, 2), (0, 2), (0, 1)][axis];
        for bits in 0..4u8 {
            let mut open = [0u8; 3];
            (open[p], open[q]) = (bits & 1, bits >> 1);
            facing[axis][open[p] as usize][open[q] as usize] = ramp_facing(axis, open).unwrap();
        }
    }

    struct Cand {
        saved: u32,
        start: usize,
        which: usize,
    }
    let mut cands = Vec::new();
    let all = stretches();
    for &(cell, bits) in sampled {
        // Which ramp could lie along the surface here. A ramp's face is
        // level along its prism axis and slopes across the other two, so the
        // prism axis is the one the form changes least along, and the ramp's
        // open side is, on each other axis, the end with less of the form.
        // Trying all twelve ramps at every cell instead finds the same fits
        // at six times the work, since eleven of them face the wrong way.
        let lean: [i32; 3] = [0, 1, 2].map(|k| {
            (bits & upper[k]).count_ones() as i32 - (bits & !upper[k]).count_ones() as i32
        });
        let axis = (0..3).min_by_key(|k| lean[*k].abs()).unwrap();
        let (p, q) = [(1, 2), (0, 2), (0, 1)][axis];
        if lean[p] == 0 || lean[q] == 0 {
            continue; // the surface is square to the grid here: nothing for a ramp to do
        }
        let pose = facing[axis][(lean[p] > 0) as usize ^ 1][(lean[q] > 0) as usize ^ 1];

        let at = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])];
        for (which, st) in all.iter().enumerate().filter(|(_, st)| st.mask == pose) {
            let len = st.cells.len();
            if len > max_ext {
                continue;
            }
            // every placement of this ramp that covers this cell
            for back in 0..len {
                if at[st.dir] < back || at[st.dir] - back + len > n[st.dir] {
                    continue;
                }
                let start = cell - back * strides[st.dir];
                let (mut now, mut then) = (0u32, 0u32);
                let mut reaches = true;
                for (i, pattern) in st.cells.iter().enumerate() {
                    let c = start + i * strides[st.dir];
                    let f = form(c, shape);
                    // A ramp may not enter a cell the form does not reach at
                    // all. Its thin end would add little there by volume and
                    // could still win overall, but it would poke through
                    // every flat face a form has: under the rim of a dome,
                    // for one.
                    reaches &= f != 0;
                    now += (f ^ unit[shape[c] as usize]).count_ones();
                    then += (f ^ pattern).count_ones();
                }
                if reaches && then < now {
                    cands.push(Cand {
                        saved: now - then,
                        start,
                        which,
                    });
                }
            }
        }
    }
    // the same placement is found from each cell it covers
    cands.sort_unstable_by_key(|c| (c.start, c.which));
    cands.dedup_by_key(|c| (c.start, c.which));

    // Largest saving first, longer first among equals. Equal candidates that
    // want the same cell are all dropped, as in the planar fitter: they are
    // nearly always mirror images, and taking whichever came first would fit
    // a symmetric form unsymmetrically.
    let key = |c: &Cand| {
        (
            std::cmp::Reverse(c.saved),
            std::cmp::Reverse(all[c.which].cells.len()),
        )
    };
    cands.sort_by_key(|c| (key(c), c.start, c.which));
    let cells = |c: &Cand| {
        // copied out, so the cells do not borrow the candidate
        let (start, step, len) = (c.start, strides[all[c.which].dir], all[c.which].cells.len());
        (0..len).map(move |i| start + i * step)
    };
    let mut taken = std::collections::HashSet::new();
    let mut wanted = std::collections::HashMap::new();
    let (mut bricks, mut saved) = (Vec::new(), 0u64);
    let mut i = 0;
    while i < cands.len() {
        let end = i + cands[i..]
            .iter()
            .take_while(|c| key(c) == key(&cands[i]))
            .count();
        let live: Vec<&Cand> = cands[i..end]
            .iter()
            .filter(|c| cells(c).all(|k| !taken.contains(&k)))
            .collect();
        wanted.clear();
        for c in &live {
            cells(c).for_each(|k| *wanted.entry(k).or_insert(0u32) += 1);
        }
        for c in &live {
            if cells(c).all(|k| wanted[&k] == 1) {
                let st = &all[c.which];
                cells(c).for_each(|k| {
                    taken.insert(k);
                    shape[k] = 0;
                });
                saved += c.saved as u64;
                let mut ext = [1i32; 3];
                ext[st.dir] = st.cells.len() as i32;
                let min = [
                    c.start % n[0],
                    c.start / n[0] % n[1],
                    c.start / (n[0] * n[1]),
                ]
                .map(|v| v as i32);
                bricks.push(CellBrick {
                    mask: st.mask,
                    min,
                    ext,
                });
            }
        }
        i = end;
    }

    // stretched ramps side by side along their prism axis are one longer ramp
    let axis_of = |b: &CellBrick| shape_info(b.mask).unwrap().axis as usize;
    bricks.sort_by_key(|b| {
        let a = axis_of(b);
        let mut across = b.min;
        across[a] = 0;
        (b.mask, b.ext, across, b.min[a])
    });
    let mut joined: Vec<CellBrick> = Vec::with_capacity(bricks.len());
    for b in bricks {
        let a = axis_of(&b);
        if let Some(last) = joined.last_mut() {
            let mut same = last.mask == b.mask
                && last.min[a] + last.ext[a] == b.min[a]
                && (last.ext[a] + b.ext[a]) as usize <= max_ext;
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
    (joined, saved)
}

/// Join cubes into boxes and ramps into longer ramps.
///
/// Cubes merge greedily: grow a run along x, then that run along y, then the
/// slab along z. Finding the fewest boxes is NP-hard; this is the standard
/// greedy pass, linear in the number of cells, and in practice within a small
/// factor of the best. https://0fps.net/2012/06/30/meshing-in-a-minecraft-game/
/// Ramps of one pose merge only along their prism axis, the one direction in
/// which two ramps side by side are one longer ramp. Corner pieces stay unit.
pub(crate) fn merge(mut shape: Vec<u8>, n: [usize; 3], max_ext: usize) -> Vec<CellBrick> {
    let index = |x: usize, y: usize, z: usize| x + n[0] * (y + n[1] * z);
    let mut bricks = Vec::new();
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                let mask = shape[index(x, y, z)];
                if mask == 0 {
                    continue;
                }
                let at = [x, y, z];
                let mut ext = [1usize; 3];
                let info = shape_info(mask).unwrap();
                match info.kind {
                    Kind::Cube => {
                        while x + ext[0] < n[0]
                            && ext[0] < max_ext
                            && shape[index(x + ext[0], y, z)] == CUBE
                        {
                            ext[0] += 1;
                        }
                        let run = ext[0];
                        let row = |y: usize, z: usize, shape: &[u8]| {
                            shape[index(x, y, z)..index(x + run, y, z)]
                                .iter()
                                .all(|&m| m == CUBE)
                        };
                        while y + ext[1] < n[1] && ext[1] < max_ext && row(y + ext[1], z, &shape) {
                            ext[1] += 1;
                        }
                        let rows = ext[1];
                        while z + ext[2] < n[2]
                            && ext[2] < max_ext
                            && (y..y + rows).all(|yy| row(yy, z + ext[2], &shape))
                        {
                            ext[2] += 1;
                        }
                    }
                    Kind::Ramp => {
                        let a = info.axis as usize;
                        let step = |k: usize| {
                            let mut p = at;
                            p[a] += k;
                            index(p[0], p[1], p[2])
                        };
                        while at[a] + ext[a] < n[a]
                            && ext[a] < max_ext
                            && shape[step(ext[a])] == mask
                        {
                            ext[a] += 1;
                        }
                    }
                    Kind::Tet | Kind::Cut | Kind::Hip | Kind::Inner | Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv => {}
                }
                for zz in z..z + ext[2] {
                    for yy in y..y + ext[1] {
                        shape[index(x, yy, zz)..index(x + ext[0], yy, zz)].fill(0);
                    }
                }
                bricks.push(CellBrick {
                    mask,
                    min: at.map(|v| v as i32),
                    ext: ext.map(|v| v as i32),
                });
            }
        }
    }
    bricks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::f64::consts::PI;

    const R: f64 = 12.0;

    fn sphere(p: DVec3) -> f64 {
        p.length() - R
    }

    fn fit(wedges: bool) -> Fit {
        fit_volume(
            &sphere,
            [-14; 3],
            [14; 3],
            &Opts {
                wedges,
                ..Opts::default()
            },
        )
    }

    #[test]
    fn stretched_ramps_fit_closer_and_flatten_the_poles() {
        let unit = fit_volume(
            &sphere,
            [-14; 3],
            [14; 3],
            &Opts {
                stretch: false,
                ..Opts::default()
            },
        );
        let stretched = fit(true);
        assert!(
            stretched.error < unit.error,
            "stretched {} vs unit {}",
            stretched.error,
            unit.error
        );
        let is_ramp = |b: &CellBrick| shape_info(b.mask).unwrap().kind == Kind::Ramp;
        // long across its slope, not just along its prism
        let across = |b: &CellBrick| {
            (0..3)
                .filter(|k| *k != shape_info(b.mask).unwrap().axis as usize)
                .map(|k| b.ext[k])
                .max()
                .unwrap()
        };
        assert!(unit
            .bricks
            .iter()
            .filter(|b| is_ramp(b))
            .all(|b| across(b) == 1));
        let long = stretched
            .bricks
            .iter()
            .filter(|b| is_ramp(b) && across(b) >= 2)
            .count();
        assert!(long >= 24, "only {long} stretched ramps");
        // In the top layer the surface is nearly level: there are ramps lying
        // flat there, long across and one cell high, on all four sides, and
        // none standing taller than a cell. (Before, the top layer had only
        // 45 degree ramps, in teeth.)
        let top: Vec<&CellBrick> = stretched
            .bricks
            .iter()
            .filter(|b| is_ramp(b) && b.min[2] == 11)
            .collect();
        assert!(top.iter().all(|b| b.ext[2] == 1), "{top:?}");
        let flat = top.iter().filter(|b| across(b) >= 2).count();
        assert!(
            flat >= 4,
            "only {flat} flat ramps in the top layer: {top:?}"
        );
        assert!(
            unit.bricks
                .iter()
                .filter(|b| is_ramp(b) && b.min[2] == 11)
                .count()
                > 0
        );
    }

    #[test]
    fn bricks_never_share_a_cell() {
        let mut seen = HashSet::new();
        for b in fit(true).bricks {
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        assert!(seen.insert((b.min[0] + x, b.min[1] + y, b.min[2] + z)));
                    }
                }
            }
        }
    }

    #[test]
    fn a_sphere_has_the_right_volume() {
        let fit = fit(true);
        let exact = 4.0 / 3.0 * PI * R * R * R;
        assert!(
            (fit.solid / exact - 1.0).abs() < 0.01,
            "sampled solid {} vs {exact}",
            fit.solid
        );
        let volume: f64 = fit.bricks.iter().map(CellBrick::volume).sum();
        assert!(
            (volume / exact - 1.0).abs() < 0.02,
            "bricks {volume} vs {exact}"
        );
    }

    #[test]
    fn wedges_fit_closer_than_cubes() {
        let (cubes, wedges) = (fit(false), fit(true));
        assert!(cubes.bricks.iter().all(|b| b.mask == CUBE));
        assert!(
            wedges.error < cubes.error * 0.7,
            "wedges {} vs cubes {}",
            wedges.error,
            cubes.error
        );
        for kind in [Kind::Ramp, Kind::Tet, Kind::Cut] {
            assert!(
                wedges
                    .bricks
                    .iter()
                    .any(|b| shape_info(b.mask).unwrap().kind == kind),
                "no {kind:?}"
            );
        }
    }

    #[test]
    fn the_reported_error_is_the_real_disagreement() {
        let fit = fit(true);
        // measured independently: finer samples, straight from the bricks
        let n = 6;
        let mut wrong = 0u64;
        let mut cell = std::collections::HashMap::new();
        for b in &fit.bricks {
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        cell.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z], *b);
                    }
                }
            }
        }
        for z in -14..14 {
            for y in -14..14 {
                for x in -14..14 {
                    let brick = cell.get(&[x, y, z]);
                    for s in 0..n * n * n {
                        let q = DVec3::new(
                            (s % n) as f64 + 0.5,
                            (s / n % n) as f64 + 0.5,
                            (s / (n * n)) as f64 + 0.5,
                        ) / n as f64;
                        let p = DVec3::new(x as f64, y as f64, z as f64) + q;
                        let in_brick = brick.is_some_and(|b| {
                            let u = [0, 1, 2].map(|i| (p[i] - b.min[i] as f64) / b.ext[i] as f64);
                            inside_sampled(shape_info(b.mask).unwrap(), u[0], u[1], u[2])
                        });
                        wrong += (in_brick != (sphere(p) < 0.0)) as u64;
                    }
                }
            }
        }
        let real = wrong as f64 / (n * n * n) as f64;
        assert!(
            (fit.error - real).abs() < 0.08 * real,
            "reported {} vs measured {real}",
            fit.error
        );
    }

    #[test]
    fn a_symmetric_solid_gets_a_symmetric_fit() {
        let n = [28usize; 3];
        let (shape, _, _, _) = classify(&sphere, [-14; 3], n, &Opts::default());
        let flip = |mask: u8, axis: usize| {
            (0..8u8).fold(0u8, |m, c| m | (mask >> c & 1) << (c ^ (1 << axis)))
        };
        for z in 0..28 {
            for y in 0..28 {
                for x in 0..28 {
                    let mask = shape[x + 28 * (y + 28 * z)];
                    assert_eq!(
                        shape[27 - x + 28 * (y + 28 * z)],
                        flip(mask, 0),
                        "x mirror at {x},{y},{z}"
                    );
                    assert_eq!(
                        shape[x + 28 * (27 - y + 28 * z)],
                        flip(mask, 1),
                        "y mirror at {x},{y},{z}"
                    );
                    assert_eq!(
                        shape[x + 28 * (y + 28 * (27 - z))],
                        flip(mask, 2),
                        "z mirror at {x},{y},{z}"
                    );
                }
            }
        }
    }

    /// From a saved report: a thin torus had tetrahedra scattered over its
    /// nearly flat top, each standing alone on the surface.
    #[test]
    fn no_tetrahedron_stands_alone() {
        let torus = |p: DVec3| ((p.x.hypot(p.y) - 18.5).hypot(p.z)) - 2.75;
        let fit = fit_volume(&torus, [-23, -23, -5], [23, 23, 5], &Opts::default());
        let mut cells = std::collections::HashMap::new();
        for b in &fit.bricks {
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        cells.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z], b.mask);
                    }
                }
            }
        }
        let tets: Vec<&CellBrick> = fit
            .bricks
            .iter()
            .filter(|b| shape_info(b.mask).unwrap().kind == Kind::Tet)
            .collect();
        assert!(
            tets.len() > 50,
            "only {} tetrahedra: the test is not looking at much",
            tets.len()
        );
        for t in tets {
            let corner = shape_info(t.mask).unwrap().corner;
            // any brick at all beside each of its three flat faces
            let touching = (0..3)
                .filter(|&k| {
                    let mut beside = t.min;
                    beside[k] += if corner >> k & 1 == 0 { -1 } else { 1 };
                    cells.contains_key(&beside)
                })
                .count();
            assert!(
                touching >= 2,
                "a tetrahedron at {:?} touches the form on {touching} of its faces",
                t.min
            );
        }
        // the form is still all there: taking the spikes out costs little
        assert!(
            fit.error / fit.solid < 0.1,
            "error {} of {}",
            fit.error,
            fit.solid
        );
    }

    #[test]
    fn skipping_blocks_changes_nothing() {
        // the same solid, reported as a distance 100 times too small, defeats
        // both the block test and the cell test: every cell is sampled
        let slow = |p: DVec3| sphere(p) / 100.0;
        let a = fit_volume(&sphere, [-14; 3], [14; 3], &Opts::default());
        let b = fit_volume(&slow, [-14; 3], [14; 3], &Opts::default());
        assert_eq!(a.bricks, b.bricks);
        assert_eq!(a.error, b.error);
    }
}
