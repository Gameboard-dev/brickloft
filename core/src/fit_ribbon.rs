//! A flat band whose curve rises and falls: a ring pulled up at one side.
//!
//! Seen along the axis it lay flat across, such a band has the outline it
//! always had; only its height changes along it. So it is built from the
//! flat band's own bricks, which fit that outline as well as it can be
//! fitted, each raised to the height of the curve where it stands:
//!
//! - a brick the curve is level over is moved up or down whole;
//! - a block the curve climbs over is cut into single cells, and each cell
//!   is made from the heights of its four corners (see [`parts_of`]): a
//!   sloped piece on top, the piece that answers it underneath, and whatever
//!   is left between them;
//! - a three-cornered brick of the outline is given a sloped top where one
//!   brick can make it, which is where it rises towards its square corner or
//!   away from it.
//!
//! Where none of that can be done (a cell whose corners are at four
//! different heights, a three-cornered brick the curve crosses askew, a
//! climb steeper than the band is thick) the piece is moved whole to the
//! height of its middle, and there is a step at its edges. The alternative
//! was fitting the whole band cell by cell as a tube, which lost the
//! outline as well: reports 27 and 28, a third of their faces steps.

use crate::fit_round::rest_of_box;
use crate::fit_stack::valley;
use crate::shapes::{shape_info, CellBrick, Kind, CUBE};
use crate::spec::plane_axes;

/// A square of a sloping surface in parts, from the heights of its four corners
/// (numbered as a brick's are: x, then y) and how far up from the lowest to
/// the highest of them the true surface is over its middle.
///
/// Each part is the corners it stands on (all four, or three for half the
/// square), the level its top piece starts from and the level that piece
/// reaches, the corners at which it reaches it (0 for a level top, with no
/// piece), and whether the piece is a valley. `None` if the square cannot
/// be made of single bricks: its corners at three or four heights with
/// neither pair of opposite corners level with each other.
///
/// The corners may differ by any whole number of cells: a brick is as tall
/// as it needs to be.
pub(crate) fn parts_of(c: [i32; 4], middle: f64) -> Option<Vec<(u8, i32, i32, u8, bool)>> {
    let (lo, hi) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
    let high = (0..4).filter(|k| c[*k] == hi).fold(0u8, |m, k| m | 1 << k);
    let mut parts = Vec::new();
    // a triangle: three corners, by the one it lacks
    let without = |k: usize| 15 & !(1u8 << k);
    if hi == lo {
        parts.push((15, lo, lo, 0, false));
    } else if c.iter().all(|v| *v == lo || *v == hi) {
        match high.count_ones() {
            // a saddle
            2 if high == 0b0110 || high == 0b1001 => {
                let low = 15 & !high;
                let (first, second) = (low.trailing_zeros() as usize, 7 - low.leading_zeros() as usize);
                if middle > 0.5 {
                    // a ridge between the two high corners: each half rises to it
                    parts.push((without(first), lo, hi, high, false));
                    parts.push((without(second), lo, hi, high, false));
                } else {
                    // a valley between the two low ones: each half has one corner up
                    let (a, b) = (high.trailing_zeros() as usize, 7 - high.leading_zeros() as usize);
                    parts.push((without(b), lo, hi, 1 << a, false));
                    parts.push((without(a), lo, hi, 1 << b, false));
                }
            }
            // a ramp
            2 => parts.push((15, lo, hi, high, false)),
            1 => {
                let k = high.trailing_zeros() as usize;
                if middle > 0.25 {
                    parts.push((15, lo, hi, high, false)); // a hip
                } else {
                    // only the half at the high corner rises: the corner opposite is the one it lacks
                    parts.push((without(3 - k), lo, hi, high, false));
                    parts.push((without(k), lo, lo, 0, false));
                }
            }
            _ => parts.push((15, lo, hi, high, middle < 0.75)),
        }
    } else {
        // Three heights. Corners 0 and 3 are opposite, and 1 and 2; one
        // pair is level with itself, and each of the other two corners
        // makes a triangle with it, rising to it or from it.
        let (a, b) = if c[0] == c[3] {
            (0, 3)
        } else if c[1] == c[2] {
            (1, 2)
        } else {
            return None;
        };
        let level = c[a];
        for r in (0..4).filter(|r| *r != a && *r != b) {
            let triangle = without(3 - r);
            match c[r].cmp(&level) {
                std::cmp::Ordering::Greater => parts.push((triangle, level, c[r], 1 << r, false)),
                std::cmp::Ordering::Less => parts.push((triangle, c[r], level, 1 << a | 1 << b, false)),
                std::cmp::Ordering::Equal => parts.push((triangle, level, level, 0, false)),
            }
        }
    }
    Some(parts)
}

/// A band to raise. Everything is in cells.
pub struct Ribbon<'a> {
    /// the band's bricks as fitted flat: all of one thickness across `axis`
    pub flat: &'a [CellBrick],
    pub axis: usize,
    /// how far along `axis` the curve is from where it lay flat, over the
    /// point (u, v) of the plane across it
    pub lift: &'a dyn Fn(f64, f64) -> f64,
    pub max_ext: i32,
}

/// The bricks of `ribbon`, or `None` if its flat bricks are not all of one
/// thickness, or hold a shape a flat band is not made of.
pub fn fit_ribbon(ribbon: &Ribbon) -> Option<Vec<CellBrick>> {
    let axis = ribbon.axis;
    let (ua, va) = plane_axes(axis);
    let first = ribbon.flat.first()?;
    let (w0, thick) = (first.min[axis], first.ext[axis]);
    if ribbon.flat.iter().any(|b| b.min[axis] != w0 || b.ext[axis] != thick) {
        return None;
    }
    let limit = ribbon.max_ext.max(1);
    let lifted = |u: i32, v: i32| (ribbon.lift)(u as f64, v as f64).round() as i32;
    let mut bricks: Vec<CellBrick> = Vec::new();

    // a brick over `[u, v, wu, wv]` of the plane and `from..to` along the
    // axis, its mask given with the plane's corners first and the axis last
    let mut brick = |[u, v, wu, wv]: [i32; 4], local: u8, from: i32, to: i32| {
        if to <= from || shape_info(local).is_none() {
            return;
        }
        let mask = (0..8u8).filter(|c| local >> c & 1 == 1).fold(0u8, |m, c| m | 1 << ((c & 1) << ua | (c >> 1 & 1) << va | (c >> 2) << axis));
        // one that is the same all the way along the axis may be cut to length
        let upright = local & 15 == local >> 4;
        let mut at = from;
        while at < to {
            let run = if upright { (to - at).min(limit) } else { to - at };
            let (mut min, mut ext) = ([0; 3], [0; 3]);
            (min[ua], min[va], min[axis]) = (u, v, at);
            (ext[ua], ext[va], ext[axis]) = (wu, wv, run);
            bricks.push(CellBrick { mask, min, ext });
            at += run;
        }
    };
    // One part of the band over a box of the plane: standing on the corners
    // `foot` of the box, its top starting `from` cells up and reaching
    // `peak` at the corners `up`.
    let mut part = |on: [i32; 4], foot: u8, from: i32, peak: i32, up: u8, in_valley: bool| {
        if up == 0 || peak == from {
            brick(on, foot | foot << 4, w0 + from, w0 + from + thick);
            return;
        }
        let top = if in_valley { valley(foot | up << 4, 2) } else { foot | up << 4 };
        // what the same piece leaves of its box, where the under side is
        let under = if foot == 15 { rest_of_box(top) } else { (foot & !up) | foot << 4 };
        // Half a square that rises towards one end of its long side is a
        // brick (a half inner corner) on top or underneath, but not both:
        // what it would leave of its box is no brick. The side that can be
        // sloped is, and the other is left level: the band is a little too
        // thick or too thin there, and smooth on one face.
        match (shape_info(top).is_some(), shape_info(under).is_some()) {
            (true, true) => {
                brick(on, top, w0 + thick + from, w0 + thick + peak);
                brick(on, under, w0 + from, w0 + peak);
                brick(on, foot | foot << 4, w0 + peak, w0 + thick + from);
            }
            (true, false) => {
                brick(on, top, w0 + thick + from, w0 + thick + peak);
                brick(on, foot | foot << 4, w0 + from, w0 + thick + from);
            }
            (false, true) => {
                brick(on, under, w0 + from, w0 + peak);
                brick(on, foot | foot << 4, w0 + peak, w0 + thick + peak);
            }
            (false, false) => brick(on, foot | foot << 4, w0 + from, w0 + from + thick),
        }
    };

    for b in ribbon.flat {
        let (u, v, wu, wv) = (b.min[ua], b.min[va], b.ext[ua], b.ext[va]);
        let on = [u, v, wu, wv];
        // the corners of the box, numbered as a brick's are: u, then v
        let corner = |k: usize| (u + (k as i32 & 1) * wu, v + (k as i32 >> 1) * wv);
        let level = |k: usize| lifted(corner(k).0, corner(k).1);
        let middle = |at: [i32; 4]| (ribbon.lift)(at[0] as f64 + at[2] as f64 / 2.0, at[1] as f64 + at[3] as f64 / 2.0);
        if b.mask == CUBE {
            let all_level = (0..=wv).all(|j| (0..=wu).all(|i| lifted(u + i, v + j) == level(0)));
            if all_level {
                part(on, 15, level(0), level(0), 0, false);
                continue;
            }
            for j in 0..wv {
                for i in 0..wu {
                    let cell = [u + i, v + j, 1, 1];
                    let c = [lifted(u + i, v + j), lifted(u + i + 1, v + j), lifted(u + i, v + j + 1), lifted(u + i + 1, v + j + 1)];
                    let (lo, hi) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
                    let share = (middle(cell) - lo as f64) / (hi - lo).max(1) as f64;
                    match parts_of(c, share).filter(|_| hi - lo <= thick) {
                        Some(parts) => parts.into_iter().for_each(|(foot, from, peak, up, in_valley)| part(cell, foot, from, peak, up, in_valley)),
                        None => {
                            let whole = middle(cell).round() as i32;
                            part(cell, 15, whole, whole, 0, false);
                        }
                    }
                }
            }
            continue;
        }
        // a three-cornered brick of the outline: a ramp standing along the axis
        shape_info(b.mask).filter(|i| i.kind == Kind::Ramp && i.axis as usize == axis)?;
        let foot = (0..4u8).filter(|k| b.mask >> ((k & 1) << ua | (k >> 1) << va) & 1 == 1).fold(0u8, |m, k| m | 1 << k);
        let lacks = (!foot & 15).trailing_zeros() as usize;
        let square = 3 - lacks;
        let held: Vec<usize> = (0..4).filter(|k| foot >> k & 1 == 1).collect();
        let (lo, hi) = (held.iter().map(|k| level(*k)).min().unwrap(), held.iter().map(|k| level(*k)).max().unwrap());
        let up = held.iter().filter(|k| level(**k) == hi).fold(0u8, |m, k| m | 1 << k);
        let two_levels = held.iter().all(|k| level(*k) == lo || level(*k) == hi);
        let _ = square;
        if hi == lo {
            part(on, foot, lo, lo, 0, false);
        } else if two_levels && hi - lo <= thick {
            part(on, foot, lo, hi, up, false);
        } else {
            // at the height of its own middle, a third of the way in from its square corner
            let (sq, others): ((i32, i32), Vec<(i32, i32)>) = (corner(square), held.iter().filter(|k| **k != square).map(|k| corner(*k)).collect());
            let whole = (ribbon.lift)((sq.0 + others[0].0 + others[1].0) as f64 / 3.0, (sq.1 + others[0].1 + others[1].1) as f64 / 3.0).round() as i32;
            part(on, foot, whole, whole, 0, false);
        }
    }
    (!bricks.is_empty()).then_some(bricks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::inside_shape;

    fn holding(bricks: &[CellBrick], p: [f64; 3]) -> usize {
        bricks
            .iter()
            .filter(|b| {
                let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
                t.iter().all(|v| (0.0..1.0).contains(v)) && inside_shape(shape_info(b.mask).unwrap(), t[0], t[1], t[2])
            })
            .count()
    }

    #[test]
    fn a_square_is_made_from_its_corners_heights() {
        // level: one part, no piece
        assert_eq!(parts_of([3; 4], 0.0), Some(vec![(15, 3, 3, 0, false)]));
        // two corners next to each other higher, by any amount: one ramp that tall
        assert_eq!(parts_of([0, 4, 0, 4], 0.5), Some(vec![(15, 0, 4, 0b1010, false)]));
        // one corner higher: a hip if the surface is full over the middle, half a square if it is hollow
        assert_eq!(parts_of([0, 0, 0, 1], 0.5), Some(vec![(15, 0, 1, 0b1000, false)]));
        assert_eq!(parts_of([0, 0, 0, 1], 0.0).unwrap().len(), 2);
        // three higher: a valley, or a cut cube where it bulges
        assert_eq!(parts_of([1, 1, 1, 0], 0.5), Some(vec![(15, 0, 1, 0b0111, true)]));
        assert_eq!(parts_of([1, 1, 1, 0], 1.0), Some(vec![(15, 0, 1, 0b0111, false)]));
        // two opposite higher: two halves, rising to the ridge between them
        assert_eq!(parts_of([0, 1, 1, 0], 1.0), Some(vec![(0b1110, 0, 1, 0b0110, false), (0b0111, 0, 1, 0b0110, false)]));
        // a slope across the diagonal: half rising to the level pair, half rising from it
        assert_eq!(parts_of([0, 1, 1, 3], 0.5), Some(vec![(0b0111, 0, 1, 0b0110, false), (0b1110, 1, 3, 0b1000, false)]));
        // four heights with neither pair of opposite corners level: no single bricks
        assert_eq!(parts_of([0, 2, 1, 3], 0.5), None);
    }

    #[test]
    fn a_band_climbing_along_itself_is_ramps_above_and_below() {
        // a straight band along x, three wide and two thick, lying across z,
        // and an end of it cut on the slant: a three-cornered brick
        let flat = [CellBrick { mask: CUBE, min: [0, 0, 5], ext: [12, 3, 2] }, CellBrick { mask: 0x77, min: [12, 0, 5], ext: [3, 3, 2] }];
        // level, then one up in two across, then level again four higher
        let lift = |u: f64, _: f64| ((u - 2.0) / 2.0).clamp(0.0, 4.0);
        let bricks = fit_ribbon(&Ribbon { flat: &flat, axis: 2, lift: &lift, max_ext: 80 }).unwrap();
        // every point over the band: in one brick exactly when within a cell of the true slab
        let n = 41;
        for k in 0..n * n * n {
            let p = [((k % n) as f64 + 0.31) / n as f64 * 15.0, ((k / n % n) as f64 + 0.47) / n as f64 * 3.0, 4.0 + ((k / (n * n)) as f64 + 0.29) / n as f64 * 9.0];
            let inside_outline = p[0] < 12.0 || (p[0] - 12.0) + p[1] < 3.0;
            let mid = 6.0 + lift(p[0], p[1]);
            let held = holding(&bricks, p);
            assert!(held <= 1, "{held} bricks hold {p:?}");
            if inside_outline && (p[2] - mid).abs() < 0.4 {
                assert_eq!(held, 1, "a hole at {p:?}");
            }
            if !inside_outline || (p[2] - mid).abs() > 1.6 {
                assert_eq!(held, 0, "a brick at {p:?}, off the band");
            }
        }
        // sloped bricks where it climbs, none where it is level
        let sloped: Vec<&CellBrick> = bricks.iter().filter(|b| b.mask != CUBE && b.mask != 0x77).collect();
        assert!(sloped.len() >= 8 && sloped.iter().all(|b| b.min[0] >= 2 && b.min[0] < 10), "{} sloped bricks", sloped.len());
        // the slanted end, which the curve is level over, is its own brick moved up whole
        assert!(bricks.iter().any(|b| b.mask == 0x77 && b.min == [12, 0, 9] && b.ext == [3, 3, 2]));
        // bricks of two thicknesses are not a band
        let odd = [flat[0], CellBrick { ext: [3, 3, 1], ..flat[1] }];
        assert!(fit_ribbon(&Ribbon { flat: &odd, axis: 2, lift: &lift, max_ext: 80 }).is_none());
    }
}
