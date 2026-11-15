//! A round tube lying flat, as stacked outlines joined by sloped pieces.
//!
//! Across its length such a tube is a circle. In bricks that is an octagon:
//! a flat top, a wall each side, and a chamfer between them. Along its
//! length the tube follows a curve, and each level of the octagon is a band
//! along that curve: full width down the walls, narrower at the flat top.
//! The wall's band is fitted as an outline (`fit_planar`), exactly as a flat
//! arch of that width would be, so it follows the curve with long sloped
//! sides and no steps. This module is the chamfer: the pieces that take the
//! wall's outline in and up to the narrower top.
//!
//! The top's outline is not fitted. It is cut back from the wall's, which
//! fixes where every piece goes:
//!
//! - A level or upright side of the wall's outline moves straight in by the
//!   chamfer's width. Between the old side and the new is a ramp.
//! - A sloped side is the long side of a triangle that fills half of a box.
//!   In the chamfer that triangle becomes a tetrahedron: the triangle at the
//!   bottom, coming to a point at its right angle, which is where the top's
//!   outline passes.
//! - At each corner of the wall's outline, between two such pieces, is a
//!   cube cut on the slant, low at the corner.
//!
//! Each piece shares whole faces with its neighbours, so the chamfer is one
//! continuous surface that starts exactly on the wall.
//!
//! The pieces are worked out as the *air* above the chamfer growing outwards
//! from the air around the wall (`grow`), and each is then turned into the
//! brick that fills the rest of its box. Growing air has one useful
//! property: where pieces from two sides of a tight curve would cross and
//! one must be left out, what is left is solid standing a little proud, not
//! a hole. Growing the solid outwards from a narrow top band, which was
//! tried first, left holes there; and a top band only a cell or two wide
//! cannot be fitted with slopes at all where it runs diagonally, since the
//! triangles of its two faces want the same cells.
//!
//! Since the top is fixed by the wall's outline, that outline is fitted
//! knowing it (`fit_planar_towards`).

use crate::fit_planar::Piece;
use crate::fit_volume::merge;
use crate::shapes::{CellBrick, CUBE};
use std::collections::HashMap;

type Point = (i32, i32);

/// An outline's inside, cell by cell: nothing, a whole cell, or part of a
/// triangle's box.
#[derive(Clone)]
pub struct Plan {
    pub(crate) min: [i32; 2],
    pub(crate) w: usize,
    pub(crate) h: usize,
    /// 0 for nothing, 1 for a whole cell, or a triangle's index plus 2
    cell: Vec<u32>,
    triangles: Vec<Piece>,
}

/// A piece of one level of the stack, seen from above: the box it fills, the
/// corners of that box it reaches at the wide end, and at the narrow end.
/// Corners are bits `cx | cy << 1`, as in a [`Piece`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Foot {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
    pub(crate) wide: u8,
    pub(crate) narrow: u8,
    /// Whether it is the valley where two ramps meet in an inside corner:
    /// whole at its wide end, and at its narrow end only the two edges the
    /// ramps end on. Its corners are a cut cube's (see [`valley`]).
    pub(crate) valley: bool,
}

/// The inner corner with the corners of the cut cube `cut`, narrowing along
/// `axis`: the valley between two ramps that meet in an inside corner.
///
/// A cut cube was used there until the game's inner corner was added to the
/// shapes. Its one sloped face lies across the corner, at an angle to both
/// ramps and a triangle's worth past the narrow outline; the inner corner's
/// two faces carry straight on from the two ramps.
pub(crate) fn valley(cut: u8, axis: usize) -> u8 {
    let lacks = (!cut).trailing_zeros() as u8;
    // the hip it leaves of its box: the whole narrow end, and the corner of
    // the wide end under the one the narrow end lacks (see `shapes`)
    let narrow_end = (0..8u8).filter(|c| c >> axis & 1 == lacks >> axis & 1).fold(0u8, |m, c| m | 1 << c);
    !(narrow_end | 1 << (lacks ^ (1 << axis)))
}

impl Plan {
    /// The pieces of a fitted outline, with `margin` cells of room around
    /// them to grow into.
    pub fn new(pieces: &[Piece], margin: i32) -> Plan {
        let lo = [
            pieces.iter().map(|p| p.x).min().unwrap_or(0) - margin,
            pieces.iter().map(|p| p.y).min().unwrap_or(0) - margin,
        ];
        let hi = [
            pieces.iter().map(|p| p.x + p.w).max().unwrap_or(0) + margin,
            pieces.iter().map(|p| p.y + p.h).max().unwrap_or(0) + margin,
        ];
        let mut plan = Plan {
            min: lo,
            w: (hi[0] - lo[0]) as usize,
            h: (hi[1] - lo[1]) as usize,
            cell: Vec::new(),
            triangles: Vec::new(),
        };
        plan.cell = vec![0; plan.w * plan.h];
        for p in pieces {
            let value = if p.mask == 15 {
                1
            } else {
                plan.triangles.push(*p);
                plan.triangles.len() as u32 + 1
            };
            plan.fill(p.x, p.y, p.w, p.h, value);
        }
        plan
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, value: u32) {
        for cy in y..y + h {
            let row = (x - self.min[0]) as usize + (cy - self.min[1]) as usize * self.w;
            self.cell[row..row + w as usize].fill(value);
        }
    }

    pub(crate) fn at(&self, x: i32, y: i32) -> u32 {
        let (cx, cy) = (x - self.min[0], y - self.min[1]);
        if cx < 0 || cy < 0 || cx as usize >= self.w || cy as usize >= self.h {
            return 0;
        }
        self.cell[cx as usize + cy as usize * self.w]
    }

    /// Whether the boxes `x..x+w`, `y..y+h` is inside the plan's room and empty.
    fn empty(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let inside = x >= self.min[0]
            && y >= self.min[1]
            && x + w <= self.min[0] + self.w as i32
            && y + h <= self.min[1] + self.h as i32;
        inside && (y..y + h).all(|cy| (x..x + w).all(|cx| self.at(cx, cy) == 0))
    }

    /// Whether the cell at `(x, y)` is solid all along its side facing
    /// `(dx, dy)`. A triangle is, along the two sides of its box that meet
    /// at its right angle, and nowhere else.
    fn solid_side(&self, x: i32, y: i32, dx: i32, dy: i32) -> bool {
        match self.at(x, y) {
            0 => false,
            1 => true,
            t => {
                let p = &self.triangles[t as usize - 2];
                // the triangle's right angle is opposite the corner it lacks
                let right = (!p.mask & 15).trailing_zeros() as i32 ^ 3;
                let (rx, ry) = (p.x + (right & 1) * p.w, p.y + (right >> 1) * p.h);
                if dx != 0 {
                    x + (dx + 1) / 2 == rx
                } else {
                    y + (dy + 1) / 2 == ry
                }
            }
        }
    }

    /// The plan's outlines, each a closed run of sides with the inside on
    /// the left of the direction of travel. A side is two points and, for a
    /// sloped one, the triangle it belongs to.
    pub(crate) fn outlines(&self) -> Vec<Vec<(Point, Point, Option<usize>)>> {
        let mut sides: Vec<(Point, Point, Option<usize>)> = Vec::new();
        for (i, p) in self.triangles.iter().enumerate() {
            let right = (!p.mask & 15).trailing_zeros() as i32 ^ 3;
            let r = (p.x + (right & 1) * p.w, p.y + (right >> 1) * p.h);
            let (e1, e2) = (
                (p.x + (1 - (right & 1)) * p.w, r.1),
                (r.0, p.y + (1 - (right >> 1)) * p.h),
            );
            // the right angle on the left of the long side
            let left = (e2.0 - e1.0) as i64 * (r.1 - e1.1) as i64
                - (e2.1 - e1.1) as i64 * (r.0 - e1.0) as i64
                > 0;
            sides.push(if left {
                (e1, e2, Some(i))
            } else {
                (e2, e1, Some(i))
            });
        }
        let (x0, y0) = (self.min[0], self.min[1]);
        for y in y0..=y0 + self.h as i32 {
            for x in x0..=x0 + self.w as i32 {
                // the upright unit side at x, from y up: between cells x - 1 and x
                let (west, east) = (
                    self.solid_side(x - 1, y, 1, 0),
                    self.solid_side(x, y, -1, 0),
                );
                if west && !east {
                    sides.push(((x, y), (x, y + 1), None));
                } else if east && !west {
                    sides.push(((x, y + 1), (x, y), None));
                }
                // the level unit side at y, from x right: between cells y - 1 and y
                let (below, above) = (
                    self.solid_side(x, y - 1, 0, 1),
                    self.solid_side(x, y, 0, -1),
                );
                if above && !below {
                    sides.push(((x, y), (x + 1, y), None));
                } else if below && !above {
                    sides.push(((x + 1, y), (x, y), None));
                }
            }
        }
        sides.sort_unstable();
        let mut used = vec![false; sides.len()];
        let mut loops = Vec::new();
        for first in 0..sides.len() {
            if used[first] {
                continue;
            }
            let mut path: Vec<(Point, Point, Option<usize>)> = Vec::new();
            let mut e = first;
            loop {
                used[e] = true;
                let side = sides[e];
                // unit sides in one line are one side
                match path.last_mut() {
                    Some(last)
                        if last.2.is_none()
                            && side.2.is_none()
                            && (last.1 .0 - last.0 .0).signum()
                                == (side.1 .0 - side.0 .0).signum()
                            && (last.1 .1 - last.0 .1).signum()
                                == (side.1 .1 - side.0 .1).signum() =>
                    {
                        last.1 = side.1
                    }
                    _ => path.push(side),
                }
                let from = sides.partition_point(|s| s.0 < side.1);
                match (from..sides.len())
                    .take_while(|&i| sides[i].0 == side.1)
                    .find(|&i| !used[i])
                {
                    Some(next) => e = next,
                    None => break,
                }
            }
            // the first and last may be one side too, begun in its middle
            if path.len() > 2 {
                let (first, last) = (path[0], path[path.len() - 1]);
                let line = |s: (Point, Point, Option<usize>)| {
                    ((s.1 .0 - s.0 .0).signum(), (s.1 .1 - s.0 .1).signum())
                };
                if first.2.is_none() && last.2.is_none() && line(first) == line(last) {
                    path[0].0 = last.0;
                    path.pop();
                }
            }
            loops.push(path);
        }
        loops
    }
}

/// One side of an outline as the chamfer sees it.
#[derive(Clone, Copy)]
struct Side {
    a: Point,
    b: Point,
    /// which way is out, as eighths of a turn from +x: even for a level or
    /// upright side, odd for a sloped one
    facing: i32,
    /// where the wide outline runs opposite this side: from, to
    out: (Point, Point),
    /// the triangle a sloped side belongs to, if the chamfer takes it
    grown: Option<usize>,
    /// a level or upright side: how far out it moves
    reach: i32,
}

/// The chamfer from a narrow outline to a wide one.
///
/// `want` is the solid the wide outline should follow (negative inside), and
/// `most` the furthest any side may move. Returns the pieces of the chamfer
/// and the wide outline's inside.
pub(crate) fn grow(narrow: &Plan, want: &dyn Fn(f64, f64) -> f64, most: i32) -> (Vec<Foot>, Plan) {
    let mut feet: Vec<Foot> = Vec::new();
    let eighth = |dx: i32, dy: i32| -> i32 {
        match (dx.signum(), dy.signum()) {
            (1, 0) => 0,
            (1, 1) => 1,
            (0, 1) => 2,
            (-1, 1) => 3,
            (-1, 0) => 4,
            (-1, -1) => 5,
            (0, -1) => 6,
            _ => 7,
        }
    };
    let toward = |facing: i32| {
        [
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
        ][facing.rem_euclid(8) as usize]
    };
    // How far the wide solid goes on beyond a point of the narrow outline,
    // straight out the way a side faces: the cells in a row whose middles are
    // inside it. Counted, not read off the solid's distance function, which
    // near a tube's flat end gives the distance to its side walls.
    let room = |x: f64, y: f64, facing: i32| {
        let d = toward(facing);
        (1..=most).take_while(|r| want(x + d.0 as f64 * (*r as f64 - 0.5), y + d.1 as f64 * (*r as f64 - 0.5)) < 0.0).count() as i32
    };

    for outline in narrow.outlines() {
        // each side, and between two sides whose facings are more than a
        // quarter turn apart, a side of no length facing each level or
        // upright direction passed on the way: at a point, the wide outline
        // has to go round it
        let n = outline.len();
        let mut sides: Vec<Side> = Vec::new();
        // for each entry of `sides`, whether the corner before it turns outwards
        let mut convex: Vec<bool> = Vec::new();
        for (i, &(a, b, triangle)) in outline.iter().enumerate() {
            let (pa, pb, _) = outline[(i + n - 1) % n];
            let (before, along) = ((pb.0 - pa.0, pb.1 - pa.1), (b.0 - a.0, b.1 - a.1));
            let turn = before.0 as i64 * along.1 as i64 - before.1 as i64 * along.0 as i64;
            // out is to the right of the direction of travel
            let (was, facing) = (eighth(before.1, -before.0), eighth(along.1, -along.0));
            let passed = if turn > 0 {
                (facing - was).rem_euclid(8)
            } else {
                (was - facing).rem_euclid(8)
            };
            if turn != 0 && passed < 5 {
                for k in 1..passed {
                    let between: i32 = (if turn > 0 { was + k } else { was - k }).rem_euclid(8);
                    if between % 2 == 0 {
                        let reach = room(a.0 as f64, a.1 as f64, between);
                        let d = toward(between);
                        let out = (a.0 + d.0 * reach, a.1 + d.1 * reach);
                        sides.push(Side {
                            a,
                            b: a,
                            facing: between,
                            out: (out, out),
                            grown: None,
                            reach,
                        });
                        convex.push(turn > 0);
                    }
                }
            }
            let side = match triangle {
                None => {
                    // A short side where the outline steps aside and carries
                    // on (it turns one way at one end and the other way at
                    // the other) is a jog, not a face: it moves out only as
                    // far as it is long. Moved the full width, its ramp and
                    // the corner pieces at its ends would reach well past
                    // its neighbours' and cut a notch; this way a jog costs
                    // a small bump instead. A short side that turns the same
                    // way at both ends is a real face, such as the last cell
                    // of an arch's leg above its foot, and moves in full.
                    let (na, nb, _) = outline[(i + 1) % n];
                    let after = (nb.0 - na.0, nb.1 - na.1);
                    let turn_after = along.0 as i64 * after.1 as i64 - along.1 as i64 * after.0 as i64;
                    let length = (b.0 - a.0).abs() + (b.1 - a.1).abs();
                    let full = room((a.0 + b.0) as f64 / 2.0, (a.1 + b.1) as f64 / 2.0, facing);
                    let reach = if turn * turn_after < 0 { full.min(length) } else { full };
                    let d = toward(facing);
                    Side {
                        a,
                        b,
                        facing,
                        out: ((a.0 + d.0 * reach, a.1 + d.1 * reach), (b.0 + d.0 * reach, b.1 + d.1 * reach)),
                        grown: None,
                        reach,
                    }
                }
                Some(t) => {
                    // the far corner of the side's box: the one the triangle lacks
                    let p = &narrow.triangles[t];
                    let lacks = (!p.mask & 15).trailing_zeros() as i32;
                    let far = (p.x + (lacks & 1) * p.w, p.y + (lacks >> 1) * p.h);
                    // taken if the wide solid reaches about that far: not
                    // where the two outlines run together, as at a flat end
                    let middle = want((a.0 + b.0) as f64 / 2.0, (a.1 + b.1) as f64 / 2.0);
                    if middle < -0.25 && want(far.0 as f64, far.1 as f64) < 0.75 {
                        Side {
                            a,
                            b,
                            facing,
                            out: (far, far),
                            grown: Some(t),
                            reach: 0,
                        }
                    } else {
                        Side {
                            a,
                            b,
                            facing,
                            out: (a, b),
                            grown: None,
                            reach: 0,
                        }
                    }
                }
            };
            sides.push(side);
            convex.push(turn > 0);
        }

        let count = sides.len();
        // how much of each level or upright side's ramp is given up at its
        // two ends, to a corner piece where the outline turns inwards
        let mut trim = vec![(0, 0); count];
        for i in 0..count {
            let (e, f) = (sides[(i + count - 1) % count], sides[i]);
            let v = f.a;
            let (p, q) = (e.out.1, f.out.0);
            let (rp, rq) = ((p.0 - v.0, p.1 - v.1), (q.0 - v.0, q.1 - v.1));
            if p == q || rp == (0, 0) || rq == (0, 0) || rp.0 * rq.0 + rp.1 * rq.1 != 0 {
                continue;
            }
            // the box with corners at the outline's corner and the two far points
            let (x, y) = (v.0.min(p.0).min(q.0), v.1.min(p.1).min(q.1));
            let (w, h) = ((rp.0 + rq.0).abs(), (rp.1 + rq.1).abs());
            let bit = |c: Point| ((c.0 != x) as u8) | ((c.1 != y) as u8) << 1;
            let three = 1 << bit(v) | 1 << bit(p) | 1 << bit(q);
            let both_long = e.facing % 2 == 0 && f.facing % 2 == 0 && e.a != e.b && f.a != f.b;
            if !convex[i] && both_long {
                // An inside corner between a level and an upright side: their
                // two ramps would cross. Each stops short and the corner is
                // the valley between them, high along both sides.
                trim[(i + count - 1) % count].1 = f.reach;
                trim[i].0 = e.reach;
                feet.push(Foot {
                    x,
                    y,
                    w,
                    h,
                    wide: 15,
                    narrow: three,
                    valley: true,
                });
            } else {
                feet.push(Foot {
                    x,
                    y,
                    w,
                    h,
                    wide: three,
                    narrow: 1 << bit(v),
                    valley: false,
                });
            }
        }
        for (i, s) in sides.iter().enumerate() {
            if let Some(t) = s.grown {
                let p = &narrow.triangles[t];
                feet.push(Foot {
                    x: p.x,
                    y: p.y,
                    w: p.w,
                    h: p.h,
                    wide: 15,
                    narrow: p.mask,
                    valley: false,
                });
            } else if s.facing % 2 == 0 && s.reach > 0 && s.a != s.b {
                // a ramp: from the side, less what its ends give up, out by its reach
                let along = ((s.b.0 - s.a.0).signum(), (s.b.1 - s.a.1).signum());
                let (a, b) = (
                    (s.a.0 + along.0 * trim[i].0, s.a.1 + along.1 * trim[i].0),
                    (s.b.0 - along.0 * trim[i].1, s.b.1 - along.1 * trim[i].1),
                );
                let d = toward(s.facing);
                let far = (b.0 + d.0 * s.reach, b.1 + d.1 * s.reach);
                let (x, y) = (a.0.min(far.0), a.1.min(far.1));
                let (w, h) = ((far.0 - a.0).abs(), (far.1 - a.1).abs());
                if w > 0 && h > 0 && (b.0 - a.0) * along.0 + (b.1 - a.1) * along.1 > 0 {
                    let bit = |c: Point| ((c.0 != x) as u8) | ((c.1 != y) as u8) << 1;
                    feet.push(Foot {
                        x,
                        y,
                        w,
                        h,
                        wide: 15,
                        narrow: 1 << bit(a) | 1 << bit(b),
                        valley: false,
                    });
                }
            }
        }
    }

    // A piece that would enter a cell the narrow outline holds is left out,
    // and so is one that wants a cell another piece has: where a curve turns
    // tighter than the chamfer is wide, pieces from its two sides cross, and
    // no brick may share a cell. Ramps choose first, then corner pieces,
    // larger before smaller, since a missing ramp is the larger hole. Pieces
    // of one kind and size that want the same cell are all left out, not
    // first come first served, so that a symmetric outline loses the same
    // pieces on both sides.
    let cells = |f: &Foot| {
        let (x, y, w, h) = (f.x, f.y, f.w, f.h);
        (y..y + h).flat_map(move |cy| (x..x + w).map(move |cx| (cx, cy)))
    };
    let own = |f: &Foot| f.wide == 15 && f.narrow.count_ones() == 3 && narrow.at(f.x, f.y) > 1;
    let rank = |f: &Foot| (std::cmp::Reverse(f.narrow.count_ones() == 2), std::cmp::Reverse(f.w * f.h));
    let (kept, mut rest): (Vec<Foot>, Vec<Foot>) = feet.into_iter().partition(|f| own(f));
    let mut feet = kept;
    rest.retain(|f| narrow.empty(f.x, f.y, f.w, f.h));
    rest.sort_by_key(|f| (rank(f), f.y, f.x));
    let mut taken: HashMap<Point, ()> = HashMap::new();
    let mut at = 0;
    while at < rest.len() {
        let end = at + rest[at..].iter().take_while(|f| rank(f) == rank(&rest[at])).count();
        let mut wanted: HashMap<Point, u32> = HashMap::new();
        let free: Vec<Foot> = rest[at..end].iter().filter(|f| cells(f).all(|c| !taken.contains_key(&c))).copied().collect();
        for f in &free {
            cells(f).for_each(|c| *wanted.entry(c).or_insert(0) += 1);
        }
        for f in free {
            if cells(&f).all(|c| wanted[&c] == 1) {
                cells(&f).for_each(|c| {
                    taken.insert(c, ());
                });
                feet.push(f);
            }
        }
        at = end;
    }

    let mut wide = narrow.clone();
    for f in &feet {
        if f.wide == 15 {
            wide.fill(f.x, f.y, f.w, f.h, 1);
        } else {
            wide.triangles.push(Piece {
                mask: f.wide,
                x: f.x,
                y: f.y,
                w: f.w,
                h: f.h,
            });
            let index = wide.triangles.len() as u32 + 1;
            wide.fill(f.x, f.y, f.w, f.h, index);
        }
    }
    (feet, wide)
}

impl Plan {
    /// The triangles still in the plan. Growing fills some triangles' boxes
    /// in whole, and those stay in the list with no cells of their own.
    pub(crate) fn live_triangles(&self) -> impl Iterator<Item = &Piece> {
        self.triangles.iter().enumerate().filter(|(i, p)| self.at(p.x, p.y) == *i as u32 + 2).map(|(_, p)| p)
    }

    /// Everything this plan is not, within its own room: the air round a solid.
    pub(crate) fn complement(&self) -> Plan {
        let mut air = self.clone();
        for cell in &mut air.cell {
            *cell = match *cell {
                0 => 1,
                1 => 0,
                triangle => triangle,
            };
        }
        for p in &mut air.triangles {
            // the other half of the box: it lacks the corner this one's right angle is at
            let right = (!p.mask & 15).trailing_zeros() ^ 3;
            p.mask = 15 & !(1 << right);
        }
        air
    }
}

/// A round tube as an octagon in section: see the module documentation.
pub struct Pipe<'a> {
    /// the band of the tube's full width, already fitted (with `fit_planar_towards`)
    pub wall: &'a [Piece],
    /// the flat top's band as a solid in the plane (negative inside)
    pub top: &'a dyn Fn(f64, f64) -> f64,
    /// the chamfer: how far in it goes, and how many layers up
    pub chamfer: i32,
    pub rise: i32,
    /// the axis the tube lies across, its first layer along it and how many
    pub axis: usize,
    pub first: i32,
    pub layers: i32,
    pub max_ext: i32,
}

/// The bricks of a [`Pipe`].
pub fn pipe(pipe: &Pipe) -> Vec<CellBrick> {
    let reach = pipe.chamfer;
    let k = pipe.rise.min(pipe.layers / 2);
    let wall = Plan::new(pipe.wall, 2);
    // the air round the wall, grown inwards over the chamfer to the air round the top
    let air = wall.complement();
    let (feet, air_top) = grow(&air, &|x, y| -(pipe.top)(x, y), reach);
    let (ua, va) = crate::spec::plane_axes(pipe.axis);
    let (w, h, layers) = (wall.w, wall.h, pipe.layers as usize);
    let limit = pipe.max_ext.max(1);

    // whole cells go in a grid, to be joined into boxes at the end; x and y
    // here are the plane's two axes and z the layers
    let mut cubes = vec![0u8; w * h * layers];
    let mut bricks: Vec<CellBrick> = Vec::new();
    let mut whole = |x: i32, y: i32, from: i32, to: i32| {
        for layer in from..to {
            cubes[(x - wall.min[0]) as usize + w * ((y - wall.min[1]) as usize + h * layer as usize)] = CUBE;
        }
    };
    // a piece through layers `from..to`, given as the corners of its box it
    // reaches at the bottom and at the top
    let mut place = |x: i32, y: i32, pw: i32, ph: i32, low: u8, high: u8, air: bool, in_valley: bool, from: i32, to: i32| {
        if to <= from || pw > limit || ph > limit {
            return;
        }
        let mut mask = (0..8u8).fold(0u8, |m, c| {
            let corner = (c >> ua & 1) | (c >> va & 1) << 1;
            let level = if c >> pipe.axis & 1 == 0 { low } else { high };
            m | (level >> corner & 1) << c
        });
        if in_valley {
            mask = valley(mask, pipe.axis);
        }
        if air {
            // a piece of air: the brick is the rest of its box
            mask = crate::fit_round::rest_of_box(mask);
        }
        // an upright piece is the same at every layer and may be cut to length
        let mut at = from;
        while at < to {
            let run = if low == high { (to - at).min(limit) } else { to - at };
            let (mut min, mut ext) = ([0; 3], [0; 3]);
            (min[ua], min[va], min[pipe.axis]) = (x, y, pipe.first + at);
            (ext[ua], ext[va], ext[pipe.axis]) = (pw, ph, run);
            if run <= limit {
                bricks.push(CellBrick { mask, min, ext });
            }
            at += run;
        }
    };

    let total = pipe.layers;
    // the walls: the fitted outline straight through
    for cy in 0..h as i32 {
        for cx in 0..w as i32 {
            let (x, y) = (wall.min[0] + cx, wall.min[1] + cy);
            if wall.at(x, y) == 1 {
                whole(x, y, k, total - k);
            }
        }
    }
    for p in &wall.triangles {
        place(p.x, p.y, p.w, p.h, p.mask, p.mask, false, false, k, total - k);
    }
    // The chamfers, at the bottom of the tube and at the top. A cell is
    // whole where the air never reaches it; each piece of air leaves the
    // rest of its box. The air is at its widest at the tube's outer face.
    let in_foot = |x: i32, y: i32| feet.iter().any(|f| x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h);
    for (from, to, wide_low) in [(0, k, true), (total - k, total, false)] {
        for cy in 0..h as i32 {
            for cx in 0..w as i32 {
                let (x, y) = (wall.min[0] + cx, wall.min[1] + cy);
                if wall.at(x, y) == 1 && air_top.at(x, y) == 0 && !in_foot(x, y) {
                    whole(x, y, from, to);
                }
            }
        }
        for f in &feet {
            let (low, high) = if wide_low { (f.wide, f.narrow) } else { (f.narrow, f.wide) };
            place(f.x, f.y, f.w, f.h, low, high, true, f.valley, from, to);
        }
    }

    let (u0, v0) = (wall.min[0], wall.min[1]);
    for b in merge(cubes, [w, h, layers], limit as usize) {
        let (mut min, mut ext) = ([0; 3], [0; 3]);
        (min[ua], min[va], min[pipe.axis]) = (b.min[0] + u0, b.min[1] + v0, b.min[2] + pipe.first);
        (ext[ua], ext[va], ext[pipe.axis]) = (b.ext[0], b.ext[1], b.ext[2]);
        bricks.push(CellBrick { mask: CUBE, min, ext });
    }
    bricks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{corners, inside_shape, shape_info, Kind};

    #[test]
    fn an_inside_corner_between_two_ramps_is_a_valley() {
        // every cut cube, narrowing along each axis in turn
        for cut in (0..8u8).map(|c| CUBE & !(1 << c)) {
            for axis in 0..3 {
                let mask = valley(cut, axis);
                let info = shape_info(mask).unwrap();
                // an inner corner, with the corners the cut cube had
                assert_eq!((info.kind, corners(mask)), (Kind::Inner, cut));
                // Its narrow end is the end the cut cube lacks a corner of.
                // Just inside that end it holds only the two edges that
                // meet opposite the lacking corner: the middle of the end,
                // which the cut cube's slanted face covered, is clear.
                let lacks = (!cut).trailing_zeros() as u8;
                let near = |bit: u8, t: f64| if lacks >> bit & 1 == 1 { 1.0 - t } else { t };
                let at = |across: f64, along: f64| {
                    let mut p = [0.0; 3];
                    for k in 0..3u8 {
                        p[k as usize] = if k as usize == axis { near(k, along) } else { near(k, across) };
                    }
                    p
                };
                let [m, e] = [at(0.6, 0.05), at(0.6, 0.95)];
                assert!(!inside_shape(info, m[0], m[1], m[2]), "{cut:#x} along {axis}");
                assert!(inside_shape(shape_info(cut).unwrap(), m[0], m[1], m[2]));
                // and at its wide end it is whole
                assert!(inside_shape(info, e[0], e[1], e[2]), "{cut:#x} along {axis}");
            }
        }

        // An L of two bars, grown by two cells: one inside corner, where the
        // ramps along the two bars meet.
        let bars = [Piece { mask: 15, x: 0, y: 0, w: 12, h: 4 }, Piece { mask: 15, x: 0, y: 4, w: 4, h: 8 }];
        let narrow = Plan::new(&bars, 4);
        let inside_l = |x: f64, y: f64| (x > 0.0 && x < 12.0 && y > 0.0 && y < 4.0) || (x > 0.0 && x < 4.0 && y > 0.0 && y < 12.0);
        // within two cells of the L, measured squarely
        let want = |x: f64, y: f64| {
            let near = [-1.75, -1.0, 0.0, 1.0, 1.75].iter().any(|dx| [-1.75, -1.0, 0.0, 1.0, 1.75].iter().any(|dy| inside_l(x + dx, y + dy)));
            if near { -1.0 } else { 1.0 }
        };
        let (feet, _) = grow(&narrow, &want, 2);
        let valleys: Vec<&Foot> = feet.iter().filter(|f| f.valley).collect();
        assert_eq!(valleys.len(), 1, "{feet:?}");
        let v = valleys[0];
        // in the crook of the L, two cells square, whole at its wide end
        assert_eq!(([v.x, v.y, v.w, v.h], v.wide, v.narrow.count_ones()), ([4, 4, 2, 2], 15, 3));
    }
}
