//! Seating: a dining chair, a stool, a bench, an armchair and a sofa.
//!
//! Every seat faces the low y side, as a bookcase does, with its back on
//! the high one.

use super::parts::{back, centred, four_legs, leg, octagon, share, Room};
use super::style::{Leg, Stand};
use super::{mask_of, shared, Kind, ACCENT, FABRIC, METAL, PANEL, WOOD};

/// How high a seat is in a piece `h` tall that has a back: a little
/// under half, which is where a dining chair's is (45 cm of 95) and
/// leaves a back tall enough to lean on at any size.
fn seat_height(h: i32) -> i32 {
    share(h, 0.46, 2, h - 2)
}

/// A chair, a stool or a bench: a board on legs, with or without a back.
pub(super) fn chair(r: &mut Room, kind: Kind) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if w < 3 || d < 3 || h < 3 {
        return r.block(lo, hi, WOOD);
    }
    let backed = match kind {
        Kind::Chair => h >= 6,
        Kind::Bench => r.f.extra && h >= 6,
        _ => false,
    };
    let least = w.min(d);
    let s = ((least as f64 / 8.0).round() as i32 + (set.heavy > 0) as i32).clamp(1, (least / 4).max(1));
    let top = if backed { lo[2] + seat_height(h) } else { hi[2] };
    let board = (s.min(2)).min(top - lo[2] - 1).max(1);
    let under = top - board;
    let frame = set.frame;
    // a cushion on the seat, a cell in from its edges, where the style has one and it has room
    let cushion = set.cushion && w >= 5 && d >= 5 && (backed || h >= 5);
    let round = kind == Kind::Stool && set.round && least >= 5;
    // (a stool's cushion is the top of the stool itself: the seat is a cell lower to make room)
    let (top, under) = if cushion && !backed { (top - 1, under - 1) } else { (top, under) };
    if under <= lo[2] {
        return r.block(lo, hi, WOOD);
    }
    if round {
        octagon(r, [lo[0], lo[1], under], [hi[0], hi[1], top], WOOD);
    } else {
        r.block([lo[0], lo[1], under], [hi[0], hi[1], top], WOOD);
    }
    // legs: under a round seat far enough in to be under it
    let inset = if round { (((least as f64 * 0.3).round() as i32 + 1) / 2).min((least - 2 * s) / 2) } else { 0 };
    let (x, y) = ((lo[0] + inset, hi[0] - inset), (lo[1] + inset, hi[1] - inset));
    let tall = under - lo[2];
    let kind_of_leg = if matches!(set.leg, Leg::Trestle | Leg::Pedestal) { Leg::Square } else { set.leg };
    if kind == Kind::Bench && set.leg == Leg::Trestle && w >= 8 {
        // a slab at each end, a little in from it, and a rail between them
        let end = (w / 10).max(1);
        for bx in [lo[0] + end, hi[0] - end - s] {
            r.block([bx, lo[1], lo[2]], [bx + s, hi[1], under], frame);
        }
        let rail = centred(lo[1], hi[1], s);
        r.block([lo[0] + end + s, rail.0, lo[2] + tall / 2], [hi[0] - end - s, rail.1, (lo[2] + tall / 2 + s).min(under)], frame);
    } else {
        four_legs(r, x, y, s, (lo[2], under), kind_of_leg, frame);
        // a long bench has a pair of legs in the middle too, or it would sag
        if kind == Kind::Bench && w >= 40 {
            let mid = centred(lo[0], hi[0], s);
            for ly in [y.0, y.1 - s] {
                r.block([mid.0, ly, lo[2]], [mid.1, ly + s, under], frame);
            }
        }
        // rails between the legs low down, all round: on a stool always (feet rest on them), elsewhere where the style has them
        // (On a turned leg they meet its ball: see `tables::table`. And not on a bench long enough for legs in its middle, which the rails would run through.)
        let low = if kind_of_leg == Leg::Turned && s >= 3 && tall >= 7 { lo[2] + 1 } else { lo[2] + (tall / 3).max(1) };
        if (kind == Kind::Stool && tall >= 8 || set.stretcher && tall >= 6) && !(kind == Kind::Bench && w >= 40) && x.1 - x.0 > 2 * s && y.1 - y.0 > 2 * s {
            for ax in [x.0, x.1 - s] {
                r.block([ax, y.0 + s, low], [ax + s, y.1 - s, low + 1], frame);
            }
            for ay in [y.0, y.1 - s] {
                r.block([x.0 + s, ay, low], [x.1 - s, ay + 1.min(s), low + 1], frame);
            }
        }
    }
    if cushion {
        let behind = if backed { hi[1] - s } else { hi[1] - 1 };
        if round {
            octagon(r, [lo[0], lo[1], top], [hi[0], hi[1], top + 1], FABRIC);
        } else {
            // (between the arms' posts, where it has arms)
            let side = if kind == Kind::Chair && r.f.extra { s.max(1) } else { 1 };
            r.block([lo[0] + side, lo[1] + 1, top], [hi[0] - side, behind, top + 1], FABRIC);
        }
    }
    if !backed {
        return;
    }
    // The back: the two rear legs carried on up as posts, and between them what the style has. It is as thick as one cell, in the middle of the posts' depth.
    for px in [lo[0], hi[0] - s] {
        r.block([px, hi[1] - s, top], [px + s, hi[1], hi[2]], frame);
    }
    let at = hi[1] - s + (s - 1) / 2;
    // (a panel back is the second wood, or upholstered where the seat is)
    let shade = if set.back == super::style::Back::Panel { if cushion { FABRIC } else { PANEL } } else if frame == METAL { WOOD } else { frame };
    back(r, (lo[0] + s, hi[0] - s), at, (top + cushion as i32, hi[2]), set.back, shade, set.heavy > 0);
    // arms: the front legs carried up, and a rail from each back to the post behind it
    if kind == Kind::Chair && r.f.extra && w >= 5 && hi[2] - top >= 4 {
        let arm = top + share(hi[2] - top, 0.45, 2, hi[2] - top - 1);
        for ax in [lo[0], hi[0] - s] {
            r.block([ax, lo[1], top], [ax + s, lo[1] + s, arm - 1], frame);
            r.block([ax, lo[1], arm - 1], [ax + s, hi[1] - s, arm], frame);
        }
    }
}

/// An armchair or a sofa: upholstered, so a body with arms and a back
/// in one colour and loose cushions in another.
///
/// From the floor: short legs, or a skirt to the floor; a base between
/// the arms; a seat cushion for each seat; a back the whole width between
/// the arms, and against it a back cushion for each seat, thicker at its
/// foot than at its top, which is the rake of the back. (A back leaning
/// as a whole would be a sheared box as wide as the sofa, which is no
/// brick; the cushions carry the slope and the back behind them stands
/// upright, as the frame of a real sofa does.)
pub(super) fn sofa(r: &mut Room, kind: Kind) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if w < 6 || d < 5 || h < 6 {
        return r.block(lo, hi, FABRIC);
    }
    let skirt = set.stand == Stand::Plinth;
    let feet = if skirt { 0 } else { share(h, 0.12, 1, 3) };
    let arm = share(w.min(2 * d), if set.rolled { 0.16 } else { 0.1 }, 2, 6).min((w - 2) / 2);
    let (seat, arm_top) = (lo[2] + share(h, 0.42, feet + 2, h - 3), lo[2] + share(h, 0.64, feet + 3, h - 1));
    let pad = share(h, 0.14, 1, 3).min(seat - lo[2] - feet - 1).max(1);
    let behind = share(d, 0.2, 2, 5);
    let floor = lo[2] + feet;
    let inner = (lo[0] + arm, hi[0] - arm);
    // the arms, each with its top sloped away outwards if it is rolled
    for (ax, outer) in [(lo[0], 0u8), (hi[0] - arm, 1)] {
        let roll = if set.rolled && r.sloped(arm, 1.max(arm / 2)) { (arm / 2).max(1) } else { 0 };
        r.block([ax, lo[1], floor], [ax + arm, hi[1], arm_top - roll], FABRIC);
        if roll > 0 {
            r.lay.piece(mask_of(|x, _, z| !(x == outer && z == 1)), [ax, lo[1], arm_top - roll], [ax + arm, hi[1], arm_top], FABRIC, 1);
        }
    }
    // the back and the base
    r.block([inner.0, hi[1] - behind, floor], [inner.1, hi[1], hi[2]], FABRIC);
    r.block([inner.0, lo[1] + 1, floor], [inner.1, hi[1] - behind, seat - pad], FABRIC);
    // seats: as many as asked for, or one for each stretch as wide as the seat is deep
    let deep = hi[1] - behind - lo[1];
    let wide = inner.1 - inner.0;
    let seats = if kind == Kind::Sofa { if r.f.count > 0 { r.f.count } else { ((wide as f64 / (deep as f64 * 0.95)).round() as i32).max(2) } } else { 1 }.clamp(1, (wide / 3).max(1));
    let lean = share(deep, 0.22, 1, 4).min(deep - 2).max(1);
    let mut at = inner.0;
    for each in shared(wide, seats, false) {
        r.block([at, lo[1], seat - pad], [at + each, hi[1] - behind, seat], ACCENT);
        // (to a little under the top of the back, so the back's own edge shows behind it)
        let up = (hi[2] - 1).max(seat + 1);
        if r.sloped(lean, up - seat) {
            r.lay.piece(mask_of(|_, y, z| !(y == 0 && z == 1)), [at, hi[1] - behind - lean, seat], [at + each, hi[1] - behind, up], ACCENT, 0);
        } else {
            r.block([at, hi[1] - behind - 1, seat], [at + each, hi[1] - behind, up], ACCENT);
        }
        at += each;
    }
    // legs at the corners, and under the middle of a long one
    if feet > 0 {
        let s = (arm / 2).clamp(1, 2);
        four_legs(r, (lo[0], hi[0]), (lo[1], hi[1]), s, (lo[2], floor), Leg::Square, set.frame);
        if w >= 40 {
            let mid = centred(lo[0], hi[0], s);
            for ly in [lo[1] + 1, hi[1] - s] {
                leg(r, [mid.0, ly], mid.1 - mid.0, (lo[2], floor), Leg::Square, [true, true], set.frame);
            }
        }
    }
}
