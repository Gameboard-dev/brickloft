//! A bed: a frame on legs, a headboard, a mattress, pillows and a blanket.
//! Its head is at the high y side.

use super::parts::{back, share, Room};
use super::style::{Back, Style};
use super::{shared, ACCENT, FABRIC, LINEN, PANEL, WOOD};

pub(super) fn bed(r: &mut Room) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if w < 5 || d < 8 || h < 5 {
        return r.block(lo, hi, WOOD);
    }
    let frame = set.frame;
    let s = (w / 12 + (set.heavy > 0) as i32).clamp(1, 3);
    // from the floor: legs, a frame, a mattress; the headboard goes on up to the top of the box
    let legs = share(h, 0.14, 1, 4);
    let rail = share(h, 0.14, 1, 4);
    let mattress = share(h, 0.2, 2, 6).min(h - legs - rail - 1);
    let (base, top) = (lo[2] + legs + rail, lo[2] + legs + rail + mattress);
    let foot = r.f.extra;
    let y = (lo[1] + if foot { s } else { 0 }, hi[1] - s);
    r.block([lo[0], y.0, lo[2] + legs], [hi[0], y.1, base], frame);
    // the headboard: two posts to the floor, and between them what the style has, from the frame up
    let head = |r: &mut Room, at: i32, up: i32| {
        for px in [lo[0], hi[0] - s] {
            r.block([px, at, lo[2]], [px + s, at + s, up], frame);
        }
        let kind = if frame == super::METAL { Back::Ladder } else { set.back };
        let shade = match kind {
            Back::Panel if set.cushion && set.style != Style::Classical => FABRIC,
            Back::Panel => PANEL,
            _ => frame,
        };
        back(r, (lo[0] + s, hi[0] - s), at + (s - 1) / 2, (lo[2] + legs, up), if kind == Back::Spindles { Back::Slats } else { kind }, shade, set.heavy > 0);
    };
    head(r, hi[1] - s, hi[2]);
    if foot {
        // a footboard, to a little over the mattress
        head(r, lo[1], (top + share(h, 0.12, 1, 4)).min(hi[2]));
    } else {
        for px in [lo[0], hi[0] - s] {
            r.block([px, lo[1], lo[2]], [px + s, lo[1] + s, lo[2] + legs], frame);
        }
    }
    // the mattress, a cell in from the frame's sides where the bed is wide enough to show the frame
    let side = (w >= 10) as i32;
    let (mx, my) = ((lo[0] + side, hi[0] - side), (y.0, y.1));
    r.block([mx.0, my.0, base], [mx.1, my.1, top], LINEN);
    // a blanket over the foot two thirds of it, turned back at its head end in another colour
    let covered = share(my.1 - my.0, 0.66, 2, my.1 - my.0 - 3);
    let turned = share(covered, 0.15, 1, 4);
    r.block([mx.0, my.0, top], [mx.1, my.0 + covered - turned, top + 1], FABRIC);
    r.block([mx.0, my.0 + covered - turned, top], [mx.1, my.0 + covered, top + 1], ACCENT);
    // pillows at the head: one, or two side by side where it is wide enough for two sleepers
    let n = match r.f.second {
        1 => 1,
        2 => 2,
        _ => 1 + (w >= 26) as i32,
    }
    .min(((mx.1 - mx.0 - 1) / 4).max(1));
    let deep = share(my.1 - my.0, 0.14, 2, 7).min(my.1 - my.0 - covered - 1);
    let wide = mx.1 - mx.0 - 2 - (n - 1);
    if deep >= 1 && wide >= n && top + 1 < hi[2] {
        let mut at = mx.0 + 1;
        for each in shared(wide, n, false) {
            r.block([at, my.1 - 1 - deep, top], [at + each, my.1 - 1, top + 1 + (h >= 20) as i32], ACCENT);
            at += each + 1;
        }
    }
}
