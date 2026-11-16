//! Cupboards: a cupboard or wardrobe, a chest of drawers, a sideboard and
//! a kitchen cabinet. And the fronts (doors and drawers) that a desk's
//! pedestal has too.
//!
//! A cupboard is closed, so nothing of its inside is built: it is one
//! block for its body, with its doors and drawer fronts standing in front
//! of that block as pieces of their own. A carcass of real boards with a
//! hollow inside, as the bookcase is, would be five boards and a shelf or
//! two that no one could ever see, in every cupboard of a kitchen.

use super::parts::{centred, share, Room};
use super::style::{Handle, Stand, Style};
use super::{moulding, shared, Kind, METAL, PANEL, TOP, WOOD};

/// One door or drawer front: the part of the face it covers, across and
/// up. A door says which side its handle is at (true for the high x
/// side); a drawer's is in its middle.
pub(super) struct Front {
    pub x: (i32, i32),
    pub z: (i32, i32),
    pub door: Option<bool>,
}

/// `n` stretches of `lo..hi` with `gap` between each and the next, as
/// equal as whole cells allow; fewer than `n` where they would be under
/// two cells each.
fn spans(lo: i32, hi: i32, n: i32, gap: i32) -> Vec<(i32, i32)> {
    let n = n.min((hi - lo + gap) / (2 + gap)).max(1);
    let mut at = lo;
    shared(hi - lo - (n - 1) * gap, n, false).into_iter().map(|long| {
        let span = (at, at + long);
        at += long + gap;
        span
    }).collect()
}

/// A body filling `x` by `y` by `z`, with `list` as the doors and
/// drawers on its front (the low y side).
///
/// From the front backwards: a cell for the handles, a cell for the
/// fronts, and the body. A cell is left between one front and the next,
/// which in the game is the dark line between two doors: the body shows
/// through it one cell back, in shadow. A panelled front is a frame two
/// cells deep with its panel a cell back inside it, so the panel is
/// sunk, as a real one is. The other way to show a panel, a frame laid
/// on top of a flat door, needed a third cell of depth for the handle to
/// stand on, and a shallow sideboard has not got it.
///
/// A body too shallow for all that is a plain block.
pub(super) fn fronts(r: &mut Room, list: &[Front], (x, y, z): ((i32, i32), (i32, i32), (i32, i32))) {
    let panelled = r.set.panelled && y.1 - y.0 >= 5;
    if y.1 - y.0 < 4 || list.is_empty() {
        return r.block([x.0, y.0, z.0], [x.1, y.1, z.1], WOOD);
    }
    let behind = y.0 + 2 + panelled as i32;
    r.block([x.0, behind, z.0], [x.1, y.1, z.1], WOOD);
    for front in list {
        let (wide, tall) = (front.x.1 - front.x.0, front.z.1 - front.z.0);
        if wide < 1 || tall < 1 {
            continue;
        }
        // (the frame of a panelled front: a sixth of its shorter side)
        let frame = (wide.min(tall) / 6).max(1);
        let sunk = panelled && wide >= 2 * frame + 2 && tall >= 2 * frame + 2;
        if sunk {
            r.block([front.x.0, y.0 + 1, front.z.0], [front.x.1, behind, front.z.0 + frame], PANEL);
            r.block([front.x.0, y.0 + 1, front.z.1 - frame], [front.x.1, behind, front.z.1], PANEL);
            r.block([front.x.0, y.0 + 1, front.z.0 + frame], [front.x.0 + frame, behind, front.z.1 - frame], PANEL);
            r.block([front.x.1 - frame, y.0 + 1, front.z.0 + frame], [front.x.1, behind, front.z.1 - frame], PANEL);
            r.block([front.x.0 + frame, y.0 + 2, front.z.0 + frame], [front.x.1 - frame, behind, front.z.1 - frame], PANEL);
        } else {
            r.block([front.x.0, y.0 + 1, front.z.0], [front.x.1, behind, front.z.1], PANEL);
        }
        // The handle: on a door, on the stile at the side it opens, half way up; on a drawer, in the middle (and a cell deeper where the middle is a sunk panel, to reach it).
        let (hx, hz, deep) = match front.door {
            Some(high) => {
                let off = if sunk { (frame - 1) / 2 } else { 1.min((wide - 1) / 2) };
                let at = if high { front.x.1 - off - 1 } else { front.x.0 + off };
                let long = if r.set.handle == Handle::Bar { share(tall, 0.25, 2, 8) } else { 1 + (tall >= 30) as i32 };
                ((at, at + 1), centred(front.z.0, front.z.1, long), 1)
            }
            None => {
                let long = if r.set.handle == Handle::Bar { share(wide, 0.34, 2, 10) } else { 1 + (wide >= 14) as i32 };
                (centred(front.x.0, front.x.1, long), centred(front.z.0, front.z.1, 1), 1 + sunk as i32)
            }
        };
        if r.set.handle != Handle::None {
            r.block([hx.0, y.0, hz.0], [hx.1, y.0 + deep, hz.1], METAL);
        }
    }
}

/// A cupboard of `kind` filling the piece's box.
///
/// Its top lies over the whole box. Under it the body stands on a plinth
/// set back at the front, or on four feet. A classical one has the
/// bookcase's cornice and skirting round it, and its body is set in by
/// their depth. What its front is divided into is the kind's own: doors
/// side by side for a cupboard, drawers one above another for a chest,
/// both for a sideboard, and for a kitchen cabinet a door under a drawer
/// in every unit, under a worktop of another colour.
pub(super) fn cupboard(r: &mut Room, kind: Kind) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if w < 4 || d < 4 || h < 5 {
        return r.block(lo, hi, WOOD);
    }
    let kitchen = kind == Kind::Kitchen;
    let m = (set.moulding && !kitchen && w >= 10 && d >= 6 && h >= 12) as i32;
    let slab = (h / 14).clamp(1, 2) + (kitchen && h >= 14) as i32;
    let (top, cy) = (hi[2] - slab, (lo[1] + m, hi[1]));
    // (the body is a cell in from the top's ends where the style gives a top an overhang; a kitchen's units stand side by side, so never there)
    let over = (set.overhang > 0 && m == 0 && !kitchen && w >= 10) as i32;
    let x = (lo[0] + m + over, hi[0] - m - over);
    let plinth = kitchen || m > 0 || set.stand == Stand::Plinth;
    let stand = share(h, if plinth { 0.1 } else { 0.14 }, 1 + m, 6).min(h - slab - 3);
    let z = (lo[2] + stand, top);
    r.block([lo[0], lo[1], top], hi, if kitchen || set.style == Style::Industrial { TOP } else { WOOD });
    if plinth {
        r.block([x.0, (cy.0 + 3).min(cy.1 - 1), lo[2]], [x.1, cy.1, z.0], WOOD);
    } else {
        let s = (w.min(d) / 8).clamp(1, 3);
        // (square whatever the style's leg: a foot this short has no room to taper or to be turned)
        super::parts::four_legs(r, x, (cy.0 + 2, cy.1), s, (lo[2], z.0), super::style::Leg::Square, set.frame);
    }
    if m > 0 {
        moulding(r.lay, lo, hi, m, (lo[0] + m, hi[0] - m), cy, stand, top, r.slopes);
    }
    let (count, tall) = (r.f.count, z.1 - z.0);
    let door = |n: usize, of: usize| Some(n % 2 == 0 && (n + 1 < of || of == 1));
    let mut list = Vec::new();
    match kind {
        Kind::Drawers => {
            let rows = if count > 0 { count } else { ((tall as f64 / 6.0).round() as i32).clamp(1, 8) }.min(((tall + 1) / 3).max(1));
            let mut at = z.0;
            for (n, high) in shared(tall - (rows - 1), rows, true).into_iter().enumerate() {
                // (the top row in two, if asked: it is the last laid, counting from the floor)
                let across = if r.f.extra && n as i32 == rows - 1 && rows >= 2 && w >= 12 { 2 } else { 1 };
                list.extend(spans(x.0, x.1, across, 1).into_iter().map(|x| Front { x, z: (at, at + high), door: None }));
                at += high + 1;
            }
        }
        _ => {
            let each = match kind {
                Kind::Kitchen => 10.0,
                Kind::Sideboard => 13.0,
                _ => 14.0,
            };
            let columns = spans(x.0, x.1, if count > 0 { count } else { ((w as f64 / each).round() as i32).max(1 + (kind == Kind::Sideboard) as i32) }, 1);
            let n = columns.len();
            // a drawer over every door of a kitchen's units; a cupboard's drawers are a row along its foot, one under each pair of doors
            let over = if kitchen && r.f.extra && tall >= 9 { share(tall, 0.2, 2, 5) } else { 0 };
            let under = if kind == Kind::Cupboard && r.f.extra && tall >= 14 { share(tall, 0.16, 3, 8) } else { 0 };
            if under > 0 {
                list.extend(spans(x.0, x.1, (n as i32 + 1) / 2, 1).into_iter().map(|x| Front { x, z: (z.0, z.0 + under), door: None }));
            }
            let doors = (z.0 + under + (under > 0) as i32, z.1 - over - (over > 0) as i32);
            // (a sideboard's middle, where it has three columns or more: the one in the middle, or the two)
            let middle = |c: usize| kind == Kind::Sideboard && r.f.extra && n >= 3 && tall >= 8 && (c == n / 2 || (n % 2 == 0 && c + 1 == n / 2));
            let hung = (0..n).filter(|c| !middle(*c)).count();
            let mut nth = 0;
            for (c, x) in columns.into_iter().enumerate() {
                if middle(c) {
                    list.extend(spans(doors.0, doors.1, 3, 1).into_iter().map(|z| Front { x, z, door: None }));
                    continue;
                }
                list.push(Front { x, z: doors, door: door(nth, hung) });
                nth += 1;
                if over > 0 {
                    list.push(Front { x, z: (z.1 - over, z.1), door: None });
                }
            }
        }
    }
    fronts(r, &list, (x, cy, z));
}
