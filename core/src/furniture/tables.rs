//! Tables: a dining table, a coffee table, a side table and a desk.
//!
//! All four are one thing, a top on what carries it, and differ in what
//! their settings start at and in one part each: a coffee table may have
//! a shelf, a side table a drawer, a desk a pedestal of drawers. So they
//! are one function, and a style's leg or top is the same on all of them.

use super::parts::{centred, four_legs, octagon, share, Room};
use super::storage::{fronts, Front};
use super::style::{Leg, Style};
use super::{mask_of, Kind, METAL, PANEL, TOP, WOOD};

/// A table filling the piece's box, its top at the top of it.
pub(super) fn table(r: &mut Room, kind: Kind) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if w < 3 || d < 3 || h < 2 {
        return r.block(lo, hi, WOOD);
    }
    let least = w.min(d);
    // The top: an eighth of the height, and a cell more where the style is heavy or carries a slab on a thin frame.
    let slab = set.heavy > 0 || set.style == Style::Industrial;
    let thick = (h / 8 + slab as i32).clamp(1, 4).min(h - 1);
    let under = hi[2] - thick;
    // A leg: a ninth of the shorter side, heavier or thinner as the style is, and never so thick that two meet.
    let s = (((least as f64) / 9.0).round() as i32 + set.heavy).clamp(1, (least / 4).max(1));
    // (a desk's top is a rectangle whatever the style: it stands against a wall)
    let round = set.round && kind != Kind::Desk && least >= 5;
    let top_shade = if set.frame == METAL || set.planks { TOP } else { WOOD };
    if round {
        octagon(r, [lo[0], lo[1], under], hi, top_shade);
    } else if set.planks && d >= 6 {
        // boards along its length, in two tones turn about, each three or four cells wide
        let boards = super::shared(d, (d / 4).max(2), false);
        let mut y = lo[1];
        for (n, wide) in boards.iter().enumerate() {
            r.block([lo[0], y, under], [hi[0], y + wide, hi[2]], if n % 2 == 0 { TOP } else { WOOD });
            y += wide;
        }
    } else {
        r.block([lo[0], lo[1], under], hi, top_shade);
    }
    let tall = under - lo[2];
    if tall < 1 {
        return;
    }
    // What carries it stands in from the top's edge: by the style's overhang, and under a round top far enough that a leg's corner is under the top and not out past its cut corner.
    let cut = if round { ((least as f64 * 0.3).round() as i32 + 1) / 2 } else { 0 };
    let inset = set.overhang.max(cut).min((least - 2 * s) / 4).max(if round { cut.min((least - 2 * s) / 2) } else { 0 });
    let (x, y) = ((lo[0] + inset, hi[0] - inset), (lo[1] + inset, hi[1] - inset));
    let frame = set.frame;
    match set.leg {
        Leg::Pedestal if least >= 6 && tall >= 4 => {
            // a base on the floor, a column, and a block spreading the top's weight
            let foot = (tall / 8).max(1);
            let (bx, by) = (centred(lo[0], hi[0], share(w, 0.55, 3, w)), centred(lo[1], hi[1], share(d, 0.55, 3, d)));
            octagon(r, [bx.0, by.0, lo[2]], [bx.1, by.1, lo[2] + foot], frame);
            let (cx, cy) = (centred(lo[0], hi[0], share(least, 0.2, 2, least)), centred(lo[1], hi[1], share(least, 0.2, 2, least)));
            r.block([cx.0, cy.0, lo[2] + foot], [cx.1, cy.1, under - 1], frame);
            r.block([(cx.0 - 1).max(bx.0), (cy.0 - 1).max(by.0), under - 1], [(cx.1 + 1).min(bx.1), (cy.1 + 1).min(by.1), under], frame);
        }
        Leg::Trestle if w >= 8 && tall >= 4 && kind != Kind::Desk => {
            // An end frame a little in from each end: a foot along the floor, a post, a head under the top; and a rail from post to post.
            let (end, bar) = ((w / 8).max(set.overhang), s.clamp(1, (tall / 4).max(1)));
            let post = centred(y.0, y.1, (d / 4).max(s));
            for tx in [lo[0] + end, hi[0] - end - s] {
                r.block([tx, y.0, lo[2]], [tx + s, y.1, lo[2] + bar], frame);
                r.block([tx, post.0, lo[2] + bar], [tx + s, post.1, under - bar], frame);
                r.block([tx, y.0, under - bar], [tx + s, y.1, under], frame);
                // a brace under each end of the head, where the style is heavy enough to want one
                let brace = ((y.1 - post.1).min(under - bar - lo[2] - bar) - 1).min(tall / 4);
                if set.heavy > 0 && brace >= 1 && r.sloped(brace, brace) {
                    r.lay.piece(mask_of(|_, y, z| !(y == 1 && z == 0)), [tx, post.1, under - bar - brace], [tx + s, post.1 + brace, under - bar], frame, 0);
                    r.lay.piece(mask_of(|_, y, z| !(y == 0 && z == 0)), [tx, post.0 - brace, under - bar - brace], [tx + s, post.0, under - bar], frame, 0);
                }
            }
            let (rail, at) = (centred(post.0, post.1, s), lo[2] + bar + ((tall - 2 * bar) / 3).max(0));
            r.block([lo[0] + end + s, rail.0, at], [hi[0] - end - s, rail.1, (at + bar).min(under - bar)], frame);
        }
        _ => {
            let leg = if matches!(set.leg, Leg::Trestle | Leg::Pedestal) { Leg::Square } else { set.leg };
            let drawers = kind == Kind::Desk && w >= 14 && d >= 5 && tall >= 5;
            if drawers {
                desk(r, x, y, s, (lo[2], under), leg);
            } else {
                four_legs(r, x, y, s, (lo[2], under), leg, frame);
            }
            let between = (x.1 - x.0 - 2 * s, y.1 - y.0 - 2 * s);
            // An apron under the top, from leg to leg. A side table's is deep enough to be a drawer, and its front is one.
            let drawer = kind == Kind::Side && r.f.extra && tall >= 6;
            let apron = if drawer { share(tall, 0.3, 2, 6) } else if set.apron && !drawers { share(tall, 0.16, 1, 4) } else { 0 }.min(tall - 1);
            // (set back a cell from the legs' faces where a leg is thick enough to stand proud of it)
            let back = (s >= 2) as i32;
            if apron > 0 && between.0 >= 1 && between.1 >= 1 {
                for ay in [y.0 + back, y.1 - back - 1] {
                    let front = ay == y.0 + back;
                    r.block([x.0 + s, ay, under - apron], [x.1 - s, ay + 1, under], if drawer && front { PANEL } else { WOOD });
                    // (the drawer's knob, where the top or the legs stand out far enough to cover it)
                    if drawer && front && inset + back >= 1 {
                        let knob = centred(x.0 + s, x.1 - s, 1 + (between.0 >= 8) as i32);
                        let up = centred(under - apron, under, 1);
                        r.block([knob.0, ay - 1, up.0], [knob.1, ay, up.1], METAL);
                    }
                }
                for ax in [x.0 + back, x.1 - back - 1] {
                    r.block([ax, y.0 + s, under - apron], [ax + 1, y.1 - s, under], WOOD);
                }
            }
            // Rails low down, from front leg to back leg at each end and one along the middle joining them; or for a coffee table a shelf.
            // (On a turned leg they meet its ball, which is the one place low down where the leg is its full width: against the shaft they touched nothing.)
            let turned = leg == Leg::Turned && s >= 3 && tall >= 7;
            let low = if turned { lo[2] + 1 } else { lo[2] + (tall / 4).max(1) };
            let rail = if turned { (tall / 8).max(1) } else { (s / 2).max(1) };
            if kind == Kind::Coffee && r.f.extra && between.0 >= 1 && tall >= 4 {
                r.block([x.0 + s, y.0, low], [x.1 - s, y.1, low + 1], WOOD);
            } else if (set.stretcher || (kind == Kind::Dining && r.f.extra)) && !drawers && between.0 >= 1 && between.1 >= 1 && tall >= 6 && low + rail < under - apron {
                for ax in [x.0, x.1 - s] {
                    r.block([ax, y.0 + s, low], [ax + s, y.1 - s, low + rail], frame);
                }
                let mid = centred(y.0 + s, y.1 - s, rail);
                r.block([x.0 + s, mid.0, low], [x.1 - s, mid.1, low + rail], frame);
            }
        }
    }
}

/// What a desk stands on in place of four legs: a pedestal of drawers at
/// one end and two legs at the other, or a pedestal at both, with a
/// panel across the back between them to hide the sitter's knees.
fn desk(r: &mut Room, x: (i32, i32), y: (i32, i32), s: i32, z: (i32, i32), leg: Leg) {
    let (w, tall) = (x.1 - x.0, z.1 - z.0);
    let wide = share(w, 0.3, 5, 16);
    let both = r.f.extra && w >= 2 * wide + 8;
    // (which end the one pedestal is at is the seed's to say)
    let right = r.set.more < 0.5;
    let mut knees = x;
    for at_right in [false, true] {
        if both || at_right == right {
            let px = if at_right { (x.1 - wide, x.1) } else { (x.0, x.0 + wide) };
            // drawers one above another, as many as asked for, or one for every six cells of height
            let rows = if r.f.count > 0 { r.f.count } else { (tall / 6).max(1) }.min(((tall + 1) / 3).max(1));
            let heights = super::shared(tall - (rows - 1), rows, true);
            let mut list = Vec::new();
            let mut at = z.0;
            for high in heights {
                list.push(Front { x: px, z: (at, at + high), door: None });
                at += high + 1;
            }
            fronts(r, &list, (px, (y.0, y.1), z));
            if at_right { knees.1 = px.0 } else { knees.0 = px.1 }
        } else {
            let lx = if at_right { x.1 - s } else { x.0 };
            for (ly, back) in [(y.0, false), (y.1 - s, true)] {
                super::parts::leg(r, [lx, ly], s, z, leg, [!at_right, !back], r.set.frame);
            }
            if at_right { knees.1 = lx } else { knees.0 = lx + s }
        }
    }
    if knees.1 > knees.0 {
        r.block([knees.0, y.1 - 1, z.1 - share(tall, 0.45, 1, tall)], [knees.1, y.1, z.1], WOOD);
    }
}
