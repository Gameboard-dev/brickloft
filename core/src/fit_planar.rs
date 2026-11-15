//! Fit a solid in the plane with rectangles and right triangles, as one
//! continuous outline.
//!
//! The outline of the result is a polygon whose corners are lattice points
//! along the solid's surface. Each sloped side of it is the hypotenuse of
//! exactly one triangle; each level or upright side is a plain brick edge.
//! Consecutive sides share a corner by construction, so slopes meet end to
//! end and there is no place for a stray step.
//!
//! 1. Sample the solid finely and give each cell, on its own, whichever of
//!    empty, square or a unit triangle disagrees with it least. The outline of
//!    that is a fine staircase with 45 degree chamfers that hugs the surface;
//!    its corners are the lattice points the polygon may use.
//! 2. Cut each outline at its extremes, where it turns back on itself, into
//!    stretches that run one way. Along each stretch choose which of its
//!    points to keep as corners by an exact shortest-path search, minimising
//!    the area where pieces and solid disagree plus a cost per corner.
//! 3. Fill the polygon: a triangle per sloped side, rectangles for the rest.
//!
//! An earlier version chose each triangle separately, on its own merit. That
//! minimised disagreement a little better and left a step wherever two
//! triangles' ends did not happen to coincide, which no amount of patching
//! afterwards fully removed. Continuity has to be the structure, not a repair.
//!
//! Every piece owns the cells of its bounding box outright, because the game
//! refuses bricks whose boxes overlap.

use crate::shapes::least_unshared;
use std::collections::{HashMap, HashSet};

/// Samples per cell, each way. 8 x 8 fills one u64 per cell.
const K: usize = 8;
const FULL: u64 = u64::MAX;

/// What a corner of the outline costs, in cells squared, before any
/// `brick_cost`. A step is two corners where a slope is one, so even with no
/// brick cost a slope is preferred unless the step is the closer fit by more
/// than this. It is small: a tenth of a cell is about the least disagreement
/// that can be seen at all.
const CORNER: f64 = 0.1;

/// What it costs an outline to change the way it turns, in samples: three
/// quarters of a cell. Enough that two slopes are not alternated to gain the
/// fraction of a cell that alternating them fits closer by; not so much that
/// a form that really does bend both ways, an S or the foot of an arch, is
/// straightened to avoid it.
const WIGGLE: i64 = (K * K) as i64 * 3 / 4;

/// A rectangle or right triangle covering cells `x..x+w`, `y..y+h`. `mask` has
/// a bit per filled corner of that box, bit `cx | cy<<1`; 15 is a rectangle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub mask: u8,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Piece {
    pub fn area(&self) -> f64 {
        (self.w * self.h) as f64 * if self.mask == 15 { 1.0 } else { 0.5 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    /// What one more corner of the outline (and so, nearly, one more brick)
    /// costs, in cells squared of disagreement. 0 follows the solid as
    /// closely as a continuous outline can; higher values give fewer, longer
    /// pieces.
    ///
    /// A fixed tolerance on distance would also thin the pieces out, but it
    /// cannot tell a corner that fixes a lot from one that fixes a sliver; a
    /// price can.
    pub brick_cost: f64,
    /// longest brick edge, in cells
    pub max_ext: i32,
    /// false fits with squares alone
    pub wedges: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            brick_cost: 0.0,
            max_ext: 80,
            wedges: true,
        }
    }
}

/// A fit and how good it is, in cells squared.
#[derive(Clone, Debug, Default)]
pub struct Fit {
    pub pieces: Vec<Piece>,
    /// area of the solid itself
    pub solid: f64,
    /// area where the pieces and the solid disagree
    pub error: f64,
}

/// The samples of row `sy` that lie inside a right triangle spanning `nx` by
/// `ny` samples with its right angle at `corner`: a half-open range of `sx`.
///
/// A sample centre `((sx + ½) / nx, (sy + ½) / ny)` is inside when its two
/// distances from the right angle sum to less than 1. That is solved for `sx`
/// in integers, because a float comparison would put samples lying exactly on
/// a 45° hypotenuse on either side depending on rounding, and the unit
/// patterns and the stretched triangles must agree on them for a merged
/// diagonal to cost exactly what its unit pieces did.
fn row_span(corner: u8, sy: usize, nx: usize, ny: usize) -> (usize, usize) {
    let sy = if corner & 2 != 0 { ny - 1 - sy } else { sy };
    // inside: (2 sx + 1) ny + (2 sy + 1) nx < 2 nx ny
    let r = 2 * nx * ny - (2 * sy + 1) * nx;
    let count = ((r + ny - 1) / (2 * ny)).min(nx);
    if corner & 1 != 0 {
        (nx - count, nx)
    } else {
        (0, count)
    }
}

/// Bits `lo..hi` of a sample row.
fn row_bits(lo: usize, hi: usize) -> u8 {
    if hi <= lo {
        return 0;
    }
    (((1u16 << hi) - 1) as u8) & !(((1u16 << lo) - 1) as u8)
}

/// The sample pattern of a unit piece with corner mask `mask`.
fn unit_pattern(mask: u8) -> u64 {
    match mask {
        0 => 0,
        15 => FULL,
        _ => {
            // the right angle is opposite the missing corner
            let corner = (!mask & 15).trailing_zeros() as u8 ^ 3;
            (0..K).fold(0u64, |bits, sy| {
                let (lo, hi) = row_span(corner, sy, K, K);
                bits | (row_bits(lo, hi) as u64) << (sy * K)
            })
        }
    }
}

const UNIT_MASKS: [u8; 6] = [0, 15, 0b0111, 0b1011, 0b1101, 0b1110];

/// Fit the solid `g` (negative inside) over cells `min..max`.
pub fn fit_planar(g: &impl Fn(f64, f64) -> f64, min: [i32; 2], max: [i32; 2], opts: &Opts) -> Fit {
    fit_planar_towards(g, min, max, opts, None)
}

/// [`fit_planar`], for an outline that a narrower one is to be cut back from.
///
/// `fit_stack` joins this outline to a narrower one above it with sloped
/// pieces, and where the narrower outline then lies is fixed by this one: it
/// passes through the right angle of each sloped side's triangle. `narrower`
/// is the solid that outline should follow (negative inside), with how far
/// in it is, in cells. A sloped side whose right angle is off that surface
/// is charged for the area that would put between the two, so the search
/// sizes its sides to land the narrower outline where it belongs as well as
/// to fit this one. Fitting the two outlines separately would fit each a
/// little better and leave them with corners in different places, which is a
/// step wherever they differ.
pub fn fit_planar_towards(
    g: &impl Fn(f64, f64) -> f64,
    min: [i32; 2],
    max: [i32; 2],
    opts: &Opts,
    narrower: Option<(&dyn Fn(f64, f64) -> f64, i32)>,
) -> Fit {
    let (w, h) = (
        (max[0] - min[0]).max(0) as usize,
        (max[1] - min[1]).max(0) as usize,
    );
    if w == 0 || h == 0 {
        return Fit::default();
    }
    let at = |x: f64, y: f64| g(min[0] as f64 + x, min[1] as f64 + y);

    // 1. sample the solid, K x K to a cell
    let vw = w + 1;
    // A lattice point within a billionth of the surface is on it. Forms are
    // snapped to the grid, so many of their surfaces pass exactly through
    // lattice points, and there the computed value is rounding noise either
    // side of zero: left as it comes, the same point reads inside on one side
    // of a symmetric form and outside on the other.
    let corner: Vec<f64> = (0..vw * (h + 1))
        .map(|i| at((i % vw) as f64, (i / vw) as f64))
        .map(|v| if v.abs() < 1e-9 { 0.0 } else { v })
        .collect();
    let mut solid = vec![0u64; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = [
                corner[x + y * vw],
                corner[x + 1 + y * vw],
                corner[x + (y + 1) * vw],
                corner[x + 1 + (y + 1) * vw],
            ];
            // a distance can't change sign within a cell whose corners are all
            // further from the surface than the cell's half diagonal
            solid[x + y * w] = if c.iter().all(|&v| v < -0.75) {
                FULL
            } else if c.iter().all(|&v| v > 0.75) {
                0
            } else {
                let mut bits = 0u64;
                for s in 0..K * K {
                    let (u, v) = (
                        ((s % K) as f64 + 0.5) / K as f64,
                        ((s / K) as f64 + 0.5) / K as f64,
                    );
                    // strictly inside: a sample exactly on the surface is outside, as
                    // one exactly on a triangle's hypotenuse is (see `row_span`)
                    bits |= ((at(x as f64 + u, y as f64 + v) < 0.0) as u64) << s;
                }
                bits
            };
        }
    }

    // the best unit piece for each cell on its own
    let patterns = UNIT_MASKS.map(unit_pattern);
    let mut shape = vec![0u8; w * h];
    let mut unit_error = vec![0u32; w * h];
    let mut undecided: Vec<(usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = x + y * w;
            let inside = |dx: usize, dy: usize| (corner[x + dx + (y + dy) * vw] <= 0.0) as u8;
            let corners = inside(0, 0) | inside(1, 0) << 1 | inside(0, 1) << 2 | inside(1, 1) << 3;
            // least disagreement, then the piece whose corners best match the
            // solid's; see `least_unshared` for what happens to exact ties
            let scores: [(u32, u32); 6] = std::array::from_fn(|k| {
                (
                    (solid[i] ^ patterns[k]).count_ones(),
                    (corners ^ UNIT_MASKS[k]).count_ones(),
                )
            });
            // without wedges, only the first two: nothing and the square
            let best = least_unshared(&scores[..if opts.wedges { 6 } else { 2 }], 2);
            shape[i] = UNIT_MASKS[best];
            unit_error[i] = scores[best].0;
            // exactly half full, and no triangle to settle it: decided below
            if best < 2 && scores[0].0 == scores[1].0 {
                undecided.push((x, y));
            }
        }
    }
    // A cell exactly half full, where empty and square are equally wrong, is
    // settled by its neighbours: a square if at least two of the cells beside
    // it hold something, empty if not. Left to a fixed choice, a row of such
    // cells along the crown of an arch came out empty between two cells that
    // were not, a notch in the top of the outline. Only neighbours that were
    // decided on their own count, so the order the cells are visited in does
    // not matter.
    let settled: Vec<bool> = undecided
        .iter()
        .map(|&(x, y)| {
            let holds = |x: usize, y: usize| shape[x + y * w] != 0 && !undecided.contains(&(x, y));
            let beside = [
                (x > 0).then(|| (x - 1, y)),
                (x + 1 < w).then(|| (x + 1, y)),
                (y > 0).then(|| (x, y - 1)),
                (y + 1 < h).then(|| (x, y + 1)),
            ];
            beside
                .into_iter()
                .flatten()
                .filter(|&(nx, ny)| holds(nx, ny))
                .count()
                >= 2
        })
        .collect();
    for (&(x, y), square) in undecided.iter().zip(settled) {
        shape[x + y * w] = if square { 15 } else { 0 };
    }

    let mut pieces: Vec<Piece> = Vec::new();
    let mut claimed = vec![false; w * h];
    // which cells are inside the outline; without wedges, the unit squares
    let mut inside: Vec<bool> = shape.iter().map(|s| *s == 15).collect();

    if opts.wedges {
        // 2. the outline as a polygon. A triangle is judged against the unit
        // pieces in its box, and there are far more candidate triangles than
        // cells, so the unit errors go in a summed-area table: any box's
        // total is four lookups instead of a loop over its cells.
        // https://en.wikipedia.org/wiki/Summed-area_table
        let mut sat = vec![0i64; vw * (h + 1)];
        for y in 0..h {
            for x in 0..w {
                sat[x + 1 + (y + 1) * vw] =
                    unit_error[x + y * w] as i64 + sat[x + (y + 1) * vw] + sat[x + 1 + y * vw]
                        - sat[x + y * vw];
            }
        }
        let mut solid_sat = vec![0i64; vw * (h + 1)];
        for y in 0..h {
            for x in 0..w {
                solid_sat[x + 1 + (y + 1) * vw] = solid[x + y * w].count_ones() as i64
                    + solid_sat[x + (y + 1) * vw]
                    + solid_sat[x + 1 + y * vw]
                    - solid_sat[x + y * vw];
            }
        }
        let corner_cost = ((opts.brick_cost.max(0.0) + CORNER) * (K * K) as f64).round() as i64;
        let reach = narrower.map_or(0, |w| w.1);
        let far_corner =
            narrower.map(|(f, _)| move |x: i32, y: i32| f((min[0] + x) as f64, (min[1] + y) as f64));
        let far_corner: Option<&dyn Fn(i32, i32) -> f64> =
            far_corner.as_ref().map(|f| f as &dyn Fn(i32, i32) -> f64);
        let mut search = Search {
            far_corner,
            reach,
            solid: &solid,
            rows: row_counts(&solid, w, h),
            sat: &sat,
            solid_sat: &solid_sat,
            corner: &corner,
            shape: &shape,
            w,
            h,
            max_ext: opts.max_ext.max(1),
            corner_cost,
            claimed_sat: vec![0; vw * (h + 1)],
            measured: HashMap::new(),
        };

        let (loops, touching) = outlines(&shape, w, h);
        // every loop's stretches, longest first: where two stretches want the
        // same cells (the two faces of a thin wall), the longer, which is the
        // outer face of a curve, chooses first and the other fits round it
        let mut stretches: Vec<(usize, usize, Vec<(i32, i32)>)> = Vec::new();
        let value = |p: (i32, i32)| {
            (p.0 >= 0 && p.1 >= 0 && p.0 as usize <= w && p.1 as usize <= h)
                .then(|| corner[p.0 as usize + p.1 as usize * vw])
        };
        for (l, outline) in loops.iter().enumerate() {
            let n = outline.len();
            let cuts = turning_points(outline, &touching, &value);
            for (k, &(from, tip_from)) in cuts.iter().enumerate() {
                let (to, tip_to) = cuts[(k + 1) % cuts.len()];
                let count = if to > from { to - from } else { to + n - from };
                // from one cut to the next, starting and ending on the true
                // tip where a cut has one
                let nodes = tip_from
                    .into_iter()
                    .chain((0..=count).map(|i| outline[(from + i) % n]))
                    .chain(tip_to)
                    .collect();
                stretches.push((l, from, nodes));
            }
        }
        stretches.sort_by_key(|s| (std::cmp::Reverse(s.2.len()), s.0, s.1));

        // each stretch's chosen corners, to be joined back into polygons
        let mut chosen: Vec<(usize, usize, Vec<(i32, i32)>)> = Vec::new();
        let mut at = 0;
        while at < stretches.len() {
            // Stretches of one length are solved together, each seeing only
            // what longer stretches took and not one another. A stretch and
            // its mirror image are the same length; solved one after the
            // other, a third stretch of that length between them would see
            // one's triangles and not the other's, and a symmetric form would
            // come out lopsided.
            let group = at..at
                + stretches[at..]
                    .iter()
                    .take_while(|s| s.2.len() == stretches[at].2.len())
                    .count();
            let cells = |p: &Piece| {
                // copied out, so the cells do not borrow the piece
                let (x, y, pw, ph) = (p.x, p.y, p.w, p.h);
                (y..y + ph)
                    .flat_map(move |cy| (x..x + pw).map(move |cx| cx as usize + cy as usize * w))
            };
            // Where two of them want the same cell (the inner and outer face
            // of a thin wall), the larger triangle has it and the other
            // stretch is solved again round what was taken. Rounds, because
            // that second answer can collide in turn; after the last, a
            // triangle still contested is left out, and its side of the
            // polygon is filled as plain squares. Largest-first is the same
            // seen from either side; first-come is not.
            let mut open: Vec<usize> = group.clone().collect();
            let mut answers: HashMap<usize, Vec<(i32, i32)>> = HashMap::new();
            for round in 0..4 {
                let mut wanted: HashMap<usize, i32> = HashMap::new();
                let mut triangles: Vec<(usize, Piece)> = Vec::new();
                for &k in &open {
                    let nodes = &stretches[k].2;
                    let keep = search.solve(nodes, 0);
                    let points: Vec<(i32, i32)> = keep.iter().map(|&i| nodes[i]).collect();
                    for side in points.windows(2) {
                        if let Some((piece, _)) = search
                            .triangle(side[0], side[1])
                            .filter(|_| search.free(side[0], side[1]))
                        {
                            // the largest area wanting each cell, and whether two share it
                            for c in cells(&piece) {
                                let area = piece.w * piece.h;
                                let held = wanted.entry(c).or_insert(0);
                                *held = if *held == area || *held == -area {
                                    -area
                                } else if area > held.abs() {
                                    area
                                } else {
                                    *held
                                };
                            }
                            triangles.push((k, piece));
                        }
                    }
                    answers.insert(k, points);
                }
                // a triangle is placed if it is the single largest on all its cells
                let mut lost: HashSet<usize> = HashSet::new();
                for (k, piece) in triangles {
                    if cells(&piece).all(|c| wanted[&c] == piece.w * piece.h) {
                        cells(&piece).for_each(|c| claimed[c] = true);
                        pieces.push(piece);
                    } else {
                        lost.insert(k);
                    }
                }
                search.claim(&claimed);
                // those that lost a triangle go round again, their placed
                // triangles withdrawn so that the new answer starts clean
                open.retain(|k| lost.contains(k));
                if open.is_empty() || round == 3 {
                    break;
                }
                let redo: HashSet<(i32, i32, i32, i32)> = open
                    .iter()
                    .flat_map(|k| {
                        answers[k]
                            .windows(2)
                            .map(|s| {
                                (
                                    s[0].0.min(s[1].0),
                                    s[0].1.min(s[1].1),
                                    (s[0].0 - s[1].0).abs(),
                                    (s[0].1 - s[1].1).abs(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                pieces.retain(|p| {
                    let mine = p.mask != 15 && redo.contains(&(p.x, p.y, p.w, p.h));
                    if mine {
                        cells(p).for_each(|c| claimed[c] = false);
                    }
                    !mine
                });
                search.claim(&claimed);
            }
            for k in group.clone() {
                chosen.push((
                    stretches[k].0,
                    stretches[k].1,
                    answers.remove(&k).unwrap_or_default(),
                ));
            }
            at = group.end;
        }

        // 3. fill the polygons
        chosen.sort_by_key(|c| (c.0, c.1));
        let mut polygons: Vec<Vec<(i32, i32)>> = vec![Vec::new(); loops.len()];
        for (l, _, points) in chosen {
            // each stretch ends on the corner the next begins with
            polygons[l].extend(points.into_iter().skip(1));
        }
        inside = scan_fill(&polygons, w, h);
    }

    // A square cell with no square beside it is one brick whatever its
    // shape, and where the outline turns the search had to keep a corner
    // there: give it the shape that fits its cell best. Judged from the cells
    // themselves, before any are merged, because which squares a merge
    // leaves single depends on the order of the sweep, and that order is not
    // the same seen from the two sides of a symmetric form.
    if opts.wedges {
        let square = |x: usize, y: usize| inside[x + y * w] && !claimed[x + y * w];
        let alone: Vec<(usize, usize)> = (0..w * h)
            .map(|i| (i % w, i / w))
            .filter(|&(x, y)| {
                square(x, y)
                    && !((x > 0 && square(x - 1, y))
                        || (x + 1 < w && square(x + 1, y))
                        || (y > 0 && square(x, y - 1))
                        || (y + 1 < h && square(x, y + 1)))
            })
            .collect();
        for (x, y) in alone {
            let i = x + y * w;
            let at_corner = |dx: usize, dy: usize| (corner[x + dx + (y + dy) * vw] <= 0.0) as u8;
            let corners = at_corner(0, 0)
                | at_corner(1, 0) << 1
                | at_corner(0, 1) << 2
                | at_corner(1, 1) << 3;
            // the square and the four triangles, on fit alone
            let scores: [(u32, u32); 5] = std::array::from_fn(|k| {
                (
                    (solid[i] ^ patterns[k + 1]).count_ones(),
                    (corners ^ UNIT_MASKS[k + 1]).count_ones(),
                )
            });
            claimed[i] = true;
            pieces.push(Piece {
                mask: UNIT_MASKS[least_unshared(&scores, 1) + 1],
                x: x as i32,
                y: y as i32,
                w: 1,
                h: 1,
            });
        }
    }

    // everything inside the outline that no triangle took, as rectangles. A
    // greedy sweep: fewest rectangles is not needed, only few, and the sweep
    // is linear in the number of cells.
    for y in 0..h {
        for x in 0..w {
            let free =
                |x: usize, y: usize, claimed: &[bool]| inside[x + y * w] && !claimed[x + y * w];
            if !free(x, y, &claimed) {
                continue;
            }
            let limit = opts.max_ext.max(1) as usize;
            let mut rw = 1;
            while x + rw < w && rw < limit && free(x + rw, y, &claimed) {
                rw += 1;
            }
            let mut rh = 1;
            while y + rh < h && rh < limit && (x..x + rw).all(|c| free(c, y + rh, &claimed)) {
                rh += 1;
            }
            for cy in y..y + rh {
                claimed[x + cy * w..x + rw + cy * w].fill(true);
            }
            pieces.push(Piece {
                mask: 15,
                x: x as i32,
                y: y as i32,
                w: rw as i32,
                h: rh as i32,
            });
        }
    }

    // The disagreement is measured from the finished pieces, sample by
    // sample, not totted up from the figures the search steered by: those
    // compare a triangle with the unit pieces it replaces, and the polygon's
    // fill is not always what the unit pieces were.
    let mut cover = vec![0u64; w * h];
    for p in &pieces {
        cover_piece(p, &mut cover, w);
    }
    let error: u64 = solid
        .iter()
        .zip(&cover)
        .map(|(s, c)| (s ^ c).count_ones() as u64)
        .sum();

    for p in &mut pieces {
        p.x += min[0];
        p.y += min[1];
    }
    let per_cell = (K * K) as f64;
    Fit {
        pieces,
        solid: solid.iter().map(|s| s.count_ones() as f64).sum::<f64>() / per_cell,
        error: error as f64 / per_cell,
    }
}

/// A point to cut an outline at: its index in the outline, and, at a sharp
/// tip, the lattice point one step beyond it that the tip really reaches.
type Cut = (usize, Option<(i32, i32)>);

/// How far an outline must come back, in cells, for it to count as having
/// turned. The outline the cuts are found on is a staircase of unit pieces
/// that wobbles a cell either way as each cell is rounded on its own; a
/// reversal of one cell is that rounding, and one of two or more is the form.
const TURN: i32 = 2;

/// Where an outline turns back on itself: the points to cut it at, so that
/// each stretch between two of them runs one way in x and one way in y, give
/// or take a wobble of one cell.
///
/// A triangle's hypotenuse can only stand in for a stretch that runs one way,
/// so no triangle spans a turning point, and the stretches can be solved
/// separately.
///
/// A turn is found in each coordinate by following it round the outline and
/// noting each extreme it comes back from by at least [`TURN`]. An extreme is
/// usually not one point but several, the flat top of an arch. The cut goes
/// where those are nearest the true surface (at every such point, if several
/// are equally near). Choosing by nearness to the surface is the same from
/// either direction, so a symmetric form is cut symmetrically.
///
/// Where the outline reverses on the spot, at the tip of a spike, the unit
/// pieces often stop a cell short of the true tip, because the last cell is
/// mostly empty. If the lattice point one step further on is at least as near
/// the surface, the cut is moved out to it.
///
/// `value` is the solid's value at a lattice point, `None` outside the grid.
fn turning_points(
    outline: &[(i32, i32)],
    touching: &HashSet<(i32, i32)>,
    value: &impl Fn((i32, i32)) -> Option<f64>,
) -> Vec<Cut> {
    let n = outline.len();
    let coordinate = |i: usize, c: usize| {
        if c == 0 {
            outline[i % n].0
        } else {
            outline[i % n].1
        }
    };
    let near = |i: usize| value(outline[i % n]).map_or(f64::INFINITY, f64::abs);
    let mut cut = vec![false; n];
    for c in 0..2 {
        // Start from the lowest point, which is certainly an extreme, and go
        // round once and a little more, so that it too is come back to.
        let start = (0..n).min_by_key(|&i| coordinate(i, c)).unwrap();
        let mut rising = true;
        let mut extreme = coordinate(start, c);
        // the points at the current extreme since it was last beaten
        let mut at_extreme: Vec<usize> = vec![start];
        let mut first_turn = None;
        for i in (start + 1..).take(2 * n) {
            let v = coordinate(i, c);
            let beyond = if rising { v > extreme } else { v < extreme };
            let back = if rising { extreme - v } else { v - extreme };
            if beyond {
                extreme = v;
                at_extreme.clear();
                at_extreme.push(i);
            } else if v == extreme {
                at_extreme.push(i);
            } else if back >= TURN {
                // turned: cut where the extreme is nearest the surface
                let nearest = at_extreme
                    .iter()
                    .map(|&k| near(k))
                    .fold(f64::INFINITY, f64::min);
                for &k in at_extreme.iter().filter(|&&k| near(k) <= nearest + 1e-9) {
                    cut[k % n] = true;
                }
                // once round: stop when the first turn found is found again
                match first_turn {
                    None => first_turn = Some(at_extreme[0] % n),
                    Some(first) if first == at_extreme[0] % n => break,
                    _ => {}
                }
                rising = !rising;
                extreme = v;
                at_extreme.clear();
                at_extreme.push(i);
            }
        }
    }
    let step = |i: usize| {
        let (a, b) = (outline[i % n], outline[(i + 1) % n]);
        [b.0 - a.0, b.1 - a.1]
    };
    let mut tip: Vec<Option<(i32, i32)>> = vec![None; n];
    for i in 0..n {
        let p = outline[i];
        // a point where the outline touches itself: which way it continues
        // from there is an accident of tracing
        cut[i] |= touching.contains(&p);
        let (before, after) = (step(i + n - 1), step(i));
        for c in 0..2 {
            // a reversal on the spot, at a turn already found: a tip
            if cut[i] && before[c] * after[c] < 0 {
                // the step that runs straight along `c`, into or out of the tip
                let along = if before[1 - c] == 0 {
                    Some(before)
                } else if after[1 - c] == 0 {
                    Some([-after[0], -after[1]])
                } else {
                    None
                };
                if let Some(d) = along.filter(|_| !(before[1 - c] == 0 && after[1 - c] == 0)) {
                    let beyond = (p.0 + d[0], p.1 + d[1]);
                    if value(beyond).is_some_and(|v| v.abs() <= near(i) + 1e-9) {
                        tip[i] = Some(beyond);
                    }
                }
            }
        }
    }
    let mut cuts: Vec<Cut> = (0..n).filter(|i| cut[*i]).map(|i| (i, tip[i])).collect();
    if cuts.is_empty() {
        cuts.push((0, None)); // an outline too small to turn: one stretch, all the way round
    }
    cuts
}

/// The state of the search for corners: the solid's samples, the unit pieces
/// and their errors, and which cells earlier stretches have already taken.
struct Search<'a> {
    /// the narrower outline's solid at a lattice point; see `fit_planar_towards`
    far_corner: Option<&'a dyn Fn(i32, i32) -> f64>,
    /// how far in the narrower outline is, in cells (0 if there is none)
    reach: i32,
    solid: &'a [u64],
    /// running counts of the solid's samples along each sample row; see [`row_counts`]
    rows: Option<Vec<u32>>,
    /// summed-area table of the unit pieces' errors
    sat: &'a [i64],
    /// the solid's value at each lattice point
    corner: &'a [f64],
    shape: &'a [u8],
    w: usize,
    h: usize,
    max_ext: i32,
    /// the cost of one corner, in samples
    corner_cost: i64,
    /// summed-area table of how many of each cell's samples are in the solid
    solid_sat: &'a [i64],
    /// summed-area table of the cells already taken
    claimed_sat: Vec<i64>,
    /// each triangle measured so far, by its box and which way its hypotenuse
    /// runs: the piece and how many samples it gets wrong
    measured: HashMap<(i32, i32, i32, i32, bool), Option<(Piece, u32)>>,
}

impl Search<'_> {
    fn over(&self, table: &[i64], x0: i32, y0: i32, x1: i32, y1: i32) -> i64 {
        let vw = self.w + 1;
        let at = |x: i32, y: i32| table[x as usize + y as usize * vw];
        at(x1, y1) + at(x0, y0) - at(x1, y0) - at(x0, y1)
    }

    /// Note the cells taken so far, after a stretch has added to them.
    fn claim(&mut self, claimed: &[bool]) {
        let vw = self.w + 1;
        for y in 0..self.h {
            for x in 0..self.w {
                self.claimed_sat[x + 1 + (y + 1) * vw] = claimed[x + y * self.w] as i64
                    + self.claimed_sat[x + (y + 1) * vw]
                    + self.claimed_sat[x + 1 + y * vw]
                    - self.claimed_sat[x + y * vw];
            }
        }
    }

    /// Whether the box with corners `p` and `q` is free of cells already taken.
    fn free(&self, p: (i32, i32), q: (i32, i32)) -> bool {
        self.over(
            &self.claimed_sat,
            p.0.min(q.0),
            p.1.min(q.1),
            p.0.max(q.0),
            p.1.max(q.1),
        ) == 0
    }

    /// The triangle whose hypotenuse runs from `p` to `q`, and how many
    /// samples in its box it gets wrong. `None` if the two are in line (no
    /// triangle), too far apart for one brick, or neither way round is better.
    fn triangle(&mut self, p: (i32, i32), q: (i32, i32)) -> Option<(Piece, u32)> {
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        if dx == 0 || dy == 0 || dx.abs() > self.max_ext || dy.abs() > self.max_ext {
            return None;
        }
        let (x, y) = (p.0.min(q.0), p.1.min(q.1));
        let vw = self.w + 1;
        // A hypotenuse whose middle is two cells or more from the surface
        // cuts across the form (or the air) and cannot be the better fit.
        // Checked from the lattice point nearest its middle, before any
        // measuring: on a large curve most long pairs fail here, and
        // measuring them all was most of the search's time.
        //
        // The middle of a hypotenuse of odd length falls between lattice
        // points, and both neighbours are looked at, the nearer deciding.
        // Rounding the middle up, as this once did, picked the neighbour on
        // the same side for a triangle and for its mirror image: one of the
        // pair passed this test and the other failed it, and the two feet of
        // a horseshoe arch were cut differently.
        let nearest = [(p.0 + q.0).div_euclid(2), (p.0 + q.0 + 1).div_euclid(2)]
            .into_iter()
            .flat_map(|mx| [(p.1 + q.1).div_euclid(2), (p.1 + q.1 + 1).div_euclid(2)].map(|my| self.corner[mx as usize + my as usize * vw].abs()))
            .fold(f64::INFINITY, f64::min);
        if nearest > 2.0 {
            return None;
        }
        let key = (x, y, dx.abs(), dy.abs(), dx * dy > 0);
        if let Some(known) = self.measured.get(&key) {
            return *known;
        }
        // The right angle goes at whichever of the box's other two corners
        // is deeper inside the solid. If neither is, each way round is
        // measured; if they still tie, the unit piece already there decides.
        let (c1, c2) = ((q.0, p.1), (p.0, q.1));
        let value = |c: (i32, i32)| self.corner[c.0 as usize + c.1 as usize * vw];
        let make = |c: (i32, i32)| {
            let corner = (c.0 != x) as u8 | ((c.1 != y) as u8) << 1;
            let piece = Piece {
                mask: 15 & !(1 << (corner ^ 3)),
                x,
                y,
                w: dx.abs(),
                h: dy.abs(),
            };
            let wrong = match &self.rows {
                Some(rows) => triangle_error_by_rows(&piece, corner, rows, self.w),
                None => triangle_error(&piece, corner, self.solid, self.w),
            };
            (piece, wrong)
        };
        let result = if (value(c1) - value(c2)).abs() > 1e-9 {
            Some(make(if value(c1) < value(c2) { c1 } else { c2 }))
        } else {
            let (a, b) = (make(c1), make(c2));
            let unit = (dx.abs() == 1 && dy.abs() == 1)
                .then(|| self.shape[x as usize + y as usize * self.w]);
            match a.1.cmp(&b.1) {
                std::cmp::Ordering::Less => Some(a),
                std::cmp::Ordering::Greater => Some(b),
                std::cmp::Ordering::Equal => [a, b].into_iter().find(|t| Some(t.0.mask) == unit),
            }
        };
        self.measured.insert(key, result);
        result
    }

    /// What going straight from `p` to `q` costs, in samples: a corner, plus
    /// how much worse (or better) the pieces it implies are than the unit
    /// pieces they replace. `off` is how far, in cells, the points of the
    /// stretch between them stray from the segment's row and from its
    /// column, and `next` whether they are neighbours on the stretch. `None`
    /// if the side cannot be used.
    fn side(&mut self, p: (i32, i32), q: (i32, i32), off: [i32; 2], next: bool, inner: bool, backwards: bool) -> Option<i64> {
        if p == q {
            return Some(0);
        }
        if p.0 == q.0 || p.1 == q.1 {
            // A level or upright side. Along a run that is itself straight it
            // changes nothing. It may also span a wobble, where the outline
            // steps a cell aside and back: that fills or empties the cells
            // between, and the cost is what those cells then get wrong
            // instead of what their unit pieces did. Anything wider than a
            // wobble it may not cut across.
            let level = p.1 == q.1;
            let stray = if level { off[1] } else { off[0] };
            // Towards a narrower outline, a level or upright side carries a ramp
            // as wide as that outline is far, and at each end a corner piece
            // as wide again. A side shorter than that has no room for them:
            // its pieces cross its neighbours' and are lost. It is charged
            // for the area it leaves unbuilt, so the search keeps to sides a
            // chamfer can be built on and takes a slope where it would have
            // taken two short steps.
            let length = (q.0 - p.0).abs() + (q.1 - p.1).abs();
            // Only a side with a corner of the outline at both ends: one
            // that reaches the end of its stretch goes on into the next, and
            // how long it is cannot be told from here.
            let short = if inner { ((self.reach - length).max(0) * self.reach) as i64 * (K * K) as i64 / 2 } else { 0 };
            if stray == 0 {
                return Some(self.corner_cost + short);
            }
            if stray > 1 {
                return None;
            }
            // The outline runs with the solid on its left. The row (or
            // column) of cells on that side of the line becomes full, the
            // one on the other side empty.
            let (lo, hi) = if level {
                (p.0.min(q.0), p.0.max(q.0))
            } else {
                (p.1.min(q.1), p.1.max(q.1))
            };
            let line = if level { p.1 } else { p.0 };
            let forward = if level { q.0 > p.0 } else { q.1 > p.1 };
            // level and going +x, or upright and going -y: the solid is on the higher side
            // (walked backwards, as `solve` also does, it is on the right:
            // without allowing for that, this cost was worked out for the
            // wrong side on every backward pass, and a stretch and its
            // mirror image, which is the same stretch walked the other way,
            // could be given different corners)
            let solid_above = (level == forward) != backwards;
            let (inner, outer) = if solid_above {
                (line, line - 1)
            } else {
                (line - 1, line)
            };
            let limit = if level { self.h } else { self.w } as i32;
            if inner < 0 || outer < 0 || inner >= limit || outer >= limit {
                return None;
            }
            let strip = |table: &[i64], at: i32| {
                if level {
                    self.over(table, lo, at, hi, at + 1)
                } else {
                    self.over(table, at, lo, at + 1, hi)
                }
            };
            let cells = (hi - lo) as i64 * (K * K) as i64;
            let filled = cells - strip(self.solid_sat, inner) - strip(self.sat, inner);
            let emptied = strip(self.solid_sat, outer) - strip(self.sat, outer);
            return Some(self.corner_cost + short + filled + emptied);
        }
        // Towards a narrower outline, a sloped side may be no deeper than
        // that outline is far in: its shorter leg at most `reach`. The pieces
        // that carry the chamfer take their depth from these legs, and every
        // brick owns its whole box; with deeper triangles the pieces from
        // the two faces of a band reach into each other's boxes across the
        // narrow top between them, and one of each such pair is lost.
        if self.reach > 0 && (q.0 - p.0).abs().min((q.1 - p.1).abs()) > self.reach && !next {
            return None;
        }
        let fits = self.free(p, q);
        match self.triangle(p, q) {
            Some((piece, wrong)) if fits => {
                // The narrower outline passes through the triangle's right
                // angle. Off that outline's surface by `d`, it puts a sliver
                // about `d` wide and half the side long between the two.
                let towards = self.far_corner.map_or(0, |f| {
                    let right = (!piece.mask & 15).trailing_zeros() as i32 ^ 3;
                    let d = f(piece.x + (right & 1) * piece.w, piece.y + (right >> 1) * piece.h)
                    .abs()
                    .min(4.0);
                    (d * (piece.w as f64).hypot(piece.h as f64) / 2.0 * (K * K) as f64).round()
                        as i64
                });
                Some(
                    self.corner_cost + towards + wrong as i64
                        - self.over(
                            self.sat,
                            piece.x,
                            piece.y,
                            piece.x + piece.w,
                            piece.y + piece.h,
                        ),
                )
            }
            // A unit step of the outline can always be walked, even where an
            // earlier stretch's triangle already covers its cell (the two
            // faces of a thin wall crossing): it then adds no piece.
            _ if next => Some(self.corner_cost),
            _ => None,
        }
    }

    /// The cheapest way along `nodes` from first to last, as the indexes of
    /// the points kept as corners.
    ///
    /// An exact shortest-path search over sides: the least cost of arriving
    /// at point `j` by a side from point `i`, found from every side that can
    /// lead into `i`. Greedy choice, taking the longest side that fits from
    /// each point, is simpler and was tried in spirit by the fitter this
    /// replaced: it commits to a long side that leaves an awkward remainder.
    /// https://en.wikipedia.org/wiki/Shortest_path_problem#Directed_acyclic_graphs
    ///
    /// The search remembers the side it arrived by, and which way the outline
    /// last turned, because one thing it charges for cannot be seen from a
    /// single side: a change in the way the outline turns ([`WIGGLE`]).
    /// Round a curve an outline should turn one way throughout. Choosing
    /// each side on its fit alone, a slope the lattice has no single side
    /// for is made by alternating the two nearest (1 in 1, 1 in 2, 1 in 1,
    /// 1 in 2), which fits a little closer by area and turns left, right,
    /// left, right all the way along: seen from above, a curve with a ripple
    /// in it. With the charge, the same sides are taken in order of slope.
    ///
    /// `backwards` says the points are given against the outline's own
    /// direction, so that the solid is on their right.
    ///
    /// Also returned: whether the answer was a toss-up between ending on a
    /// left turn and ending on a right turn, which a mirror image would call
    /// the other way.
    fn cheapest(&mut self, nodes: &[(i32, i32)], backwards: bool) -> (Vec<usize>, bool) {
        // an arrival at a point: the point it came from, and for each way the
        // outline may have last turned (not yet, left, right) the least cost
        // of arriving so and the arrival at the earlier point it follows
        struct Arrival {
            from: usize,
            cost: [i64; 3],
            back: [(usize, usize); 3],
        }
        let last = nodes.len() - 1;
        let mut arrivals: Vec<Vec<Arrival>> = (0..=last).map(|_| Vec::new()).collect();
        // the way the stretch runs: a side may not run against it
        let run = ((nodes[last].0 - nodes[0].0).signum(), (nodes[last].1 - nodes[0].1).signum());
        for j in 1..=last {
            let q = nodes[j];
            // how far the points from i to j stray from q's column, and its row
            let mut off = [0, 0];
            for i in (0..j).rev() {
                let p = nodes[i];
                // The points strictly between p and q are the ones seen so
                // far, so their strays are `off` as it stood before p. Kept as
                // the scan goes; working them out afresh for each pair made
                // the whole search cubic in the length of a stretch.
                let between = off;
                off = [off[0].max((p.0 - q.0).abs()), off[1].max((p.1 - q.1).abs())];
                // beyond one brick's reach both ways, and no longer a straight
                // run: nothing further back can reach q either
                if off[0] > 1 && off[1] > 1 && ((q.0 - p.0).abs() > self.max_ext || (q.1 - p.1).abs() > self.max_ext) {
                    break;
                }
                if i > 0 && arrivals[i].is_empty() {
                    continue;
                }
                let against = (q.0 - p.0).signum() * run.0 < 0 || (q.1 - p.1).signum() * run.1 < 0;
                if against && j - i > 1 {
                    continue;
                }
                let Some(cost) = self.side(p, q, between, j - i == 1, i > 0 && j < last, backwards) else { continue };
                let mut arrival = Arrival { from: i, cost: [i64::MAX; 3], back: [(0, 0); 3] };
                if i == 0 {
                    arrival.cost[0] = cost;
                }
                for (e, earlier) in arrivals[i].iter().enumerate() {
                    let h = nodes[earlier.from];
                    let turn = ((p.0 - h.0) as i64 * (q.1 - p.1) as i64 - (p.1 - h.1) as i64 * (q.0 - p.0) as i64).signum();
                    for was in 0..3 {
                        if earlier.cost[was] == i64::MAX {
                            continue;
                        }
                        let now = match turn {
                            0 => was,
                            1 => 1,
                            _ => 2,
                        };
                        let wiggle = if was != 0 && now != was { WIGGLE } else { 0 };
                        let total = earlier.cost[was] + cost + wiggle;
                        // Between equal costs, the way that kept turning as
                        // it was, then the one that had not turned yet. Not
                        // simply the first met: left and right change places
                        // in a mirror image, and "left before right" would
                        // choose differently for the two sides of a
                        // symmetric form.
                        let rank = |from: usize| if from == now { 0 } else if from == 0 { 1 } else { 2 };
                        let better = total < arrival.cost[now] || (total == arrival.cost[now] && (e, rank(was)) < (arrival.back[now].0, rank(arrival.back[now].1)));
                        if better {
                            arrival.cost[now] = total;
                            arrival.back[now] = (e, was);
                        }
                    }
                }
                if arrival.cost.iter().any(|c| *c != i64::MAX) {
                    arrivals[j].push(arrival);
                }
            }
        }
        // the cheapest arrival at the last point, and back along the way it came
        let mut at = (0..arrivals[last].len())
            .flat_map(|e| (0..3).map(move |turned| (e, turned)))
            .min_by_key(|&(e, turned)| arrivals[last][e].cost[turned])
            .expect("a unit step of the outline can always be walked");
        let least = arrivals[last][at.0].cost[at.1];
        let toss_up = arrivals[last].iter().any(|a| a.cost[1] == least) && arrivals[last].iter().any(|a| a.cost[2] == least);
        let mut keep = vec![last];
        let mut point = last;
        while point != 0 {
            let arrival = &arrivals[point][at.0];
            keep.push(arrival.from);
            point = arrival.from;
            at = arrival.back[at.1];
        }
        keep.reverse();
        (keep, toss_up)
    }

    /// [`Search::cheapest`], made the same from either end.
    ///
    /// Where two ways along a stretch cost exactly the same, the search keeps
    /// whichever it met first, which depends on the direction it ran in. A
    /// symmetric form's two sides are the same stretch walked in opposite
    /// directions, so they would get different corners. So the search is run
    /// from both ends: if the answers agree there was no such tie. If they
    /// differ, the stretch is cut at its middle and each half solved the same
    /// way, which treats a stretch and its mirror image alike whichever end
    /// each is walked from.
    fn solve(&mut self, nodes: &[(i32, i32)], depth: u32) -> Vec<usize> {
        let last = nodes.len() - 1;
        if last == 0 {
            return vec![0];
        }
        let (forward, toss_up) = self.cheapest(nodes, false);
        let reversed: Vec<(i32, i32)> = nodes.iter().rev().copied().collect();
        // a toss-up seen from either end is one: this stretch walked
        // forwards is its mirror image walked backwards
        let (back, back_toss_up) = self.cheapest(&reversed, true);
        let toss_up = toss_up || back_toss_up;
        let mut backward: Vec<usize> = back.into_iter().map(|i| last - i).collect();
        backward.reverse();
        if (forward == backward && !toss_up) || last == 1 || depth >= 12 {
            return forward;
        }
        // the middle point, or for an odd number of steps the middle step
        let (a, b) = (last / 2, last.div_ceil(2));
        let mut keep = self.solve(&nodes[..=a], depth + 1);
        if b > a {
            keep.push(b);
        }
        keep.extend(
            self.solve(&nodes[b..], depth + 1)
                .into_iter()
                .skip(1)
                .map(|i| i + b),
        );
        keep
    }
}

/// Which cells lie inside the polygons, by the even-odd rule: along each row
/// of cells, through the cells' centres, inside begins and ends at alternate
/// crossings of the polygons' sides. A hole's outline is one more polygon.
/// https://en.wikipedia.org/wiki/Even%E2%80%93odd_rule
fn scan_fill(polygons: &[Vec<(i32, i32)>], w: usize, h: usize) -> Vec<bool> {
    let mut inside = vec![false; w * h];
    let mut crossings: Vec<f64> = Vec::new();
    for y in 0..h {
        let level = y as f64 + 0.5;
        crossings.clear();
        for polygon in polygons {
            for (i, &a) in polygon.iter().enumerate() {
                let b = polygon[(i + 1) % polygon.len()];
                if (a.1 as f64 > level) != (b.1 as f64 > level) {
                    crossings.push(
                        a.0 as f64 + (level - a.1 as f64) / (b.1 - a.1) as f64 * (b.0 - a.0) as f64,
                    );
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks_exact(2) {
            // Cells whose centres, at x + 0.5, are strictly between the two
            // crossings. Strictly at both ends: a centre exactly on a sloped
            // side is outside whichever side of the form it is on. Counting
            // it in at the left crossing and out at the right, as a half-open
            // range does, filled one such cell and not its mirror image.
            let (from, to) = (
                ((pair[0] - 0.5 + 1e-9).floor() + 1.0).max(0.0) as usize,
                ((pair[1] - 0.5 - 1e-9).ceil().max(0.0) as usize).min(w),
            );
            if from < to {
                inside[from + y * w..to + y * w].fill(true);
            }
        }
    }
    inside
}

/// Mark the samples a piece covers (in the fitter's own cell coordinates).
fn cover_piece(p: &Piece, cover: &mut [u64], w: usize) {
    let (x, y, pw, ph) = (p.x as usize, p.y as usize, p.w as usize, p.h as usize);
    if p.mask == 15 {
        for cy in y..y + ph {
            cover[x + cy * w..x + pw + cy * w].fill(FULL);
        }
        return;
    }
    // a triangle: one run of samples in each sample row, as in `triangle_error`
    let corner = (!p.mask & 15).trailing_zeros() as u8 ^ 3;
    let (nx, ny) = (pw * K, ph * K);
    for sy in 0..ny {
        let (lo, hi) = row_span(corner, sy, nx, ny);
        for j in 0..pw {
            let run = row_bits(
                lo.saturating_sub(j * K).min(K),
                hi.saturating_sub(j * K).min(K),
            );
            cover[x + j + (y + sy / K) * w] |= (run as u64) << ((sy % K) * K);
        }
    }
}

/// The closed outlines of the unit pieces, as lattice points with the pieces
/// on the left of the direction of travel.
///
/// Also returned: the points where the outline touches itself, two pieces
/// meeting corner to corner. Which way an outline continues through such a
/// point is arbitrary, so nothing may be built on it: see `candidates`.
fn outlines(shape: &[u8], w: usize, h: usize) -> (Vec<Vec<(i32, i32)>>, HashSet<(i32, i32)>) {
    let vw = (w + 1) as u32;
    let id = |x: usize, y: usize| x as u32 + y as u32 * vw;
    // each piece's edges, anticlockwise; an edge shared by two pieces appears
    // once in each direction and is not part of the outline
    let mut edges = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let m = shape[x + y * w];
            if m == 0 {
                continue;
            }
            let ring = [(0, 0, 1u8), (1, 0, 2), (1, 1, 8), (0, 1, 4)];
            let kept: Vec<u32> = ring
                .iter()
                .filter(|c| m & c.2 != 0)
                .map(|c| id(x + c.0, y + c.1))
                .collect();
            for i in 0..kept.len() {
                edges.push((kept[i], kept[(i + 1) % kept.len()]));
            }
        }
    }
    let all: HashSet<(u32, u32)> = edges.iter().copied().collect();
    let mut border: Vec<(u32, u32)> = edges
        .into_iter()
        .filter(|&(a, b)| !all.contains(&(b, a)))
        .collect();
    border.sort_unstable();

    // a point that more than one outline edge leaves
    let touching: HashSet<(i32, i32)> = border
        .windows(2)
        .filter(|e| e[0].0 == e[1].0)
        .map(|e| ((e[0].0 % vw) as i32, (e[0].0 / vw) as i32))
        .collect();

    let mut used = vec![false; border.len()];
    let mut loops = Vec::new();
    for first in 0..border.len() {
        if used[first] {
            continue;
        }
        let mut path = Vec::new();
        let mut e = first;
        loop {
            used[e] = true;
            let (a, b) = border[e];
            path.push(((a % vw) as i32, (a / vw) as i32));
            // an unused edge leaving b; where outlines touch at a point, any will do
            let from = border.partition_point(|x| x.0 < b);
            match (from..border.len())
                .take_while(|&i| border[i].0 == b)
                .find(|&i| !used[i])
            {
                Some(next) => e = next,
                None => break,
            }
        }
        loops.push(path);
    }
    (loops, touching)
}

/// Most cells a grid may have for [`row_counts`] to be kept: 256 bytes a
/// cell, so about 64 MB at this limit.
const ROW_COUNT_CELLS: usize = 250_000;

/// For each row of samples, how many of the solid's samples lie before each
/// position along it: `K * h` rows of `K * w + 1` counts.
///
/// With these a triangle is measured in one step per sample row instead of
/// one per cell of the row (see [`triangle_error_by_rows`]). A large curve
/// has triangles tens of cells across, and measuring them cell by cell was
/// nine tenths of the time a large flat arch took to fit.
/// https://en.wikipedia.org/wiki/Prefix_sum
///
/// `None` for a grid too large to keep them for; it is then measured the
/// slow way.
fn row_counts(solid: &[u64], w: usize, h: usize) -> Option<Vec<u32>> {
    if w * h > ROW_COUNT_CELLS {
        return None;
    }
    let across = K * w + 1;
    let mut counts = vec![0u32; across * K * h];
    for y in 0..h {
        for sy in 0..K {
            let row = &mut counts[(y * K + sy) * across..(y * K + sy + 1) * across];
            for x in 0..w {
                let bits = (solid[x + y * w] >> (sy * K)) as u8;
                for sx in 0..K {
                    row[x * K + sx + 1] = row[x * K + sx] + (bits >> sx & 1) as u32;
                }
            }
        }
    }
    Some(counts)
}

/// [`triangle_error`] from [`row_counts`]: the same number, found a sample
/// row at a time.
///
/// In each row the triangle covers one run of samples. What it gets wrong
/// there is the samples of its run the solid lacks, and the solid's samples
/// in the rest of the box's row; both are differences of two counts.
fn triangle_error_by_rows(p: &Piece, corner: u8, rows: &[u32], w: usize) -> u32 {
    let (nx, ny) = (p.w as usize * K, p.h as usize * K);
    let across = K * w + 1;
    let mut error = 0;
    for sy in 0..ny {
        let (lo, hi) = row_span(corner, sy, nx, ny);
        let at = (p.y as usize * K + sy) * across + p.x as usize * K;
        let in_run = rows[at + hi.max(lo)] - rows[at + lo];
        let in_box = rows[at + nx] - rows[at];
        error += (hi.saturating_sub(lo) as u32 - in_run) + (in_box - in_run);
    }
    error
}

/// How many samples in the triangle's box disagree with the solid. `corner`
/// is its right-angle corner, `cx | cy<<1`.
///
/// A triangle covers one run of each sample row, so the row is compared a
/// cell (8 samples, one byte) at a time: XOR the solid's byte with the run's
/// bits and count what is left. Testing samples one by one would be 8 times
/// the work, in the function the fitter spends most of its time in.
/// `count_ones` compiles to a single instruction where the CPU has one:
/// https://doc.rust-lang.org/std/primitive.u8.html#method.count_ones
fn triangle_error(p: &Piece, corner: u8, solid: &[u64], w: usize) -> u32 {
    let (cw, nx, ny) = (p.w as usize, p.w as usize * K, p.h as usize * K);
    let mut error = 0;
    for sy in 0..ny {
        let (lo, hi) = row_span(corner, sy, nx, ny);
        let shift = (sy % K) * K;
        let row = (p.y as usize + sy / K) * w + p.x as usize;
        for (j, cell) in solid[row..row + cw].iter().enumerate() {
            let run = row_bits(
                lo.saturating_sub(j * K).min(K),
                hi.saturating_sub(j * K).min(K),
            );
            error += ((cell >> shift) as u8 ^ run).count_ones();
        }
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_triangle_measures_the_same_by_rows_as_by_cells() {
        // a solid with no pattern to it, and every triangle that fits in it
        let (w, h) = (9, 7);
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let solid: Vec<u64> = (0..w * h)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            })
            .collect();
        let rows = row_counts(&solid, w, h).unwrap();
        let mut tried = 0;
        for corner in 0..4u8 {
            for (x, y, pw, ph) in (0..w as i32).flat_map(|x| (0..h as i32).flat_map(move |y| (1..=5).flat_map(move |pw| (1..=5).map(move |ph| (x, y, pw, ph))))) {
                if x + pw > w as i32 || y + ph > h as i32 {
                    continue;
                }
                let piece = Piece { mask: 15 & !(1 << (corner ^ 3)), x, y, w: pw, h: ph };
                assert_eq!(triangle_error_by_rows(&piece, corner, &rows, w), triangle_error(&piece, corner, &solid, w), "{piece:?}");
                tried += 1;
            }
        }
        assert!(tried > 2000);
    }

    use std::f64::consts::PI;

    fn no_overlap(pieces: &[Piece]) {
        let mut seen = HashSet::new();
        for p in pieces {
            assert!(p.w >= 1 && p.h >= 1);
            for y in p.y..p.y + p.h {
                for x in p.x..p.x + p.w {
                    assert!(seen.insert((x, y)), "cell {x},{y} is covered twice");
                }
            }
        }
    }

    /// The disagreement measured independently of the fitter, on a finer grid.
    fn measured_error(
        g: &impl Fn(f64, f64) -> f64,
        pieces: &[Piece],
        min: [i32; 2],
        max: [i32; 2],
    ) -> f64 {
        let n = 16;
        let mut wrong = 0u64;
        for sy in 0..(max[1] - min[1]) * n {
            for sx in 0..(max[0] - min[0]) * n {
                let (x, y) = (
                    min[0] as f64 + (sx as f64 + 0.5) / n as f64,
                    min[1] as f64 + (sy as f64 + 0.5) / n as f64,
                );
                let in_piece = pieces.iter().any(|p| {
                    let (u, v) = ((x - p.x as f64) / p.w as f64, (y - p.y as f64) / p.h as f64);
                    if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                        return false;
                    }
                    if p.mask == 15 {
                        return true;
                    }
                    let missing = (!p.mask & 15).trailing_zeros();
                    let a = if missing & 1 != 0 { 1.0 - u } else { u };
                    let b = if missing & 2 != 0 { 1.0 - v } else { v };
                    a + b > 1.0
                });
                wrong += (in_piece != (g(x, y) < 0.0)) as u64;
            }
        }
        wrong as f64 / (n * n) as f64
    }

    #[test]
    fn a_lattice_rectangle_is_one_exact_piece() {
        let g = |x: f64, y: f64| ((x - 5.0).abs() - 4.0).max((y - 2.0).abs() - 1.0);
        let fit = fit_planar(&g, [-2, -2], [12, 6], &Opts::default());
        assert_eq!(
            fit.pieces,
            vec![Piece {
                mask: 15,
                x: 1,
                y: 1,
                w: 8,
                h: 2
            }]
        );
        assert_eq!(fit.error, 0.0);
        assert_eq!(fit.solid, 16.0);
    }

    #[test]
    fn a_lattice_triangle_is_one_stretched_wedge() {
        // under the line from (0, 0) to (6, 2), right of x = 0... a right
        // triangle with its right angle at (6, 0)
        let g = |x: f64, y: f64| ((y * 6.0 - x * 2.0) / 40f64.sqrt()).max(-y).max(x - 6.0);
        let fit = fit_planar(&g, [-2, -2], [9, 5], &Opts::default());
        // filled corner is +x -y, so the missing one is -x +y: bit 2
        assert_eq!(
            fit.pieces,
            vec![Piece {
                mask: 0b1011,
                x: 0,
                y: 0,
                w: 6,
                h: 2
            }]
        );
        assert_eq!(fit.error, 0.0);
    }

    #[test]
    fn a_45_degree_edge_merges_into_one_wedge() {
        let g = |x: f64, y: f64| ((x + y - 7.0) / 2f64.sqrt()).max(-x).max(-y);
        let fit = fit_planar(&g, [-2, -2], [10, 10], &Opts::default());
        assert_eq!(
            fit.pieces,
            vec![Piece {
                mask: 0b0111,
                x: 0,
                y: 0,
                w: 7,
                h: 7
            }]
        );
    }

    fn ring(x: f64, y: f64) -> f64 {
        let d = x.hypot(y);
        (d - 20.0).max(17.0 - d)
    }

    #[test]
    fn a_ring_is_symmetric_and_the_right_area() {
        let fit = fit_planar(&ring, [-23, -23], [23, 23], &Opts::default());
        no_overlap(&fit.pieces);

        let exact = PI * (20.0 * 20.0 - 17.0 * 17.0);
        assert!(
            (fit.solid / exact - 1.0).abs() < 0.005,
            "sampled solid {} vs {exact}",
            fit.solid
        );
        let area: f64 = fit.pieces.iter().map(Piece::area).sum();
        assert!((area / exact - 1.0).abs() < 0.02, "area {area} vs {exact}");

        // stretched wedges, not just unit ones
        let long = fit
            .pieces
            .iter()
            .filter(|p| p.mask != 15 && p.w.max(p.h) >= 3)
            .count();
        assert!(long >= 8, "only {long} long wedges");

        // every triangle has its mirror image in x and in y. Rectangles are
        // merged in scan order and need not pair up, but the cells they cover must.
        let set: HashSet<(u8, i32, i32, i32, i32)> = fit
            .pieces
            .iter()
            .map(|p| (p.mask, p.x, p.y, p.w, p.h))
            .collect();
        let flip_x = |m: u8| (m & 0b0101) << 1 | (m & 0b1010) >> 1;
        let flip_y = |m: u8| (m & 0b0011) << 2 | (m & 0b1100) >> 2;
        let mut covered = HashSet::new();
        for p in &fit.pieces {
            if p.mask == 15 {
                covered
                    .extend((p.y..p.y + p.h).flat_map(|y| (p.x..p.x + p.w).map(move |x| (x, y))));
                continue;
            }
            assert!(
                set.contains(&(flip_x(p.mask), -p.x - p.w, p.y, p.w, p.h)),
                "no x mirror of {p:?}"
            );
            assert!(
                set.contains(&(flip_y(p.mask), p.x, -p.y - p.h, p.w, p.h)),
                "no y mirror of {p:?}"
            );
        }
        for &(x, y) in &covered {
            assert!(
                covered.contains(&(-x - 1, y)) && covered.contains(&(x, -y - 1)),
                "cell {x},{y} has no mirror"
            );
        }
    }

    #[test]
    fn the_reported_error_is_the_real_disagreement() {
        let fit = fit_planar(&ring, [-23, -23], [23, 23], &Opts::default());
        let real = measured_error(&ring, &fit.pieces, [-23, -23], [23, 23]);
        assert!(
            (fit.error - real).abs() < 0.05 * real + 0.5,
            "reported {} vs measured {real}",
            fit.error
        );
    }

    #[test]
    fn a_brick_cost_trades_fit_for_fewer_bricks() {
        let fit = |brick_cost: f64| {
            fit_planar(
                &ring,
                [-23, -23],
                [23, 23],
                &Opts {
                    brick_cost,
                    ..Opts::default()
                },
            )
        };
        let (close, balanced, sparse) = (fit(0.0), fit(0.25), fit(2.0));
        no_overlap(&balanced.pieces);
        assert!(
            balanced.pieces.len() < close.pieces.len(),
            "{} vs {}",
            balanced.pieces.len(),
            close.pieces.len()
        );
        assert!(sparse.pieces.len() < balanced.pieces.len());
        assert!(close.error <= balanced.error && balanced.error <= sparse.error);
        // the price is honoured: each brick saved cost at most its price in fit
        let saved = (close.pieces.len() - balanced.pieces.len()) as f64;
        assert!(
            balanced.error - close.error <= 0.25 * saved + 1e-9,
            "{} more error for {saved} fewer bricks",
            balanced.error - close.error
        );
        // the reported error is still the real one, not error plus costs
        let real = measured_error(&ring, &balanced.pieces, [-23, -23], [23, 23]);
        assert!(
            (balanced.error - real).abs() < 0.05 * real + 0.5,
            "reported {} vs measured {real}",
            balanced.error
        );
    }

    #[test]
    fn stretched_wedges_beat_unit_pieces_and_squares_alone() {
        let units = fit_planar(
            &ring,
            [-23, -23],
            [23, 23],
            &Opts {
                max_ext: 1,
                ..Opts::default()
            },
        );
        let squares_only = fit_planar(
            &ring,
            [-23, -23],
            [23, 23],
            &Opts {
                wedges: false,
                ..Opts::default()
            },
        );
        assert!(squares_only.pieces.iter().all(|p| p.mask == 15));
        assert!(squares_only.error > units.error);
        let full = fit_planar(&ring, [-23, -23], [23, 23], &Opts::default());
        assert!(
            full.error < units.error * 0.8,
            "stretched {} vs unit {}",
            full.error,
            units.error
        );
        // per cell of outline (inner and outer circles), well under a tenth of a cell
        let outline = 2.0 * PI * (20.0 + 17.0);
        assert!(
            full.error / outline < 0.1,
            "{} over an outline of {outline}",
            full.error
        );
        // squares alone: every cell is the better of empty and full
        let squares: f64 = {
            let n = 8;
            let mut total = 0.0;
            for cy in -23..23 {
                for cx in -23..23 {
                    let mut inside = 0;
                    for s in 0..n * n {
                        let (u, v) = (
                            ((s % n) as f64 + 0.5) / n as f64,
                            ((s / n) as f64 + 0.5) / n as f64,
                        );
                        inside += (ring(cx as f64 + u, cy as f64 + v) < 0.0) as u32;
                    }
                    total += inside.min(n * n - inside) as f64 / (n * n) as f64;
                }
            }
            total
        };
        assert!(
            units.error < squares * 0.7,
            "unit pieces {} vs squares {squares}",
            units.error
        );
    }
}
