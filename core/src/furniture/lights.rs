//! Lights: a table lamp, a floor lamp, a pendant, a chandelier, a wall
//! sconce and a candlestick.
//!
//! Each has one or more bricks that are its light (see `Room::lamp`):
//! written to the game's file they glow, and carry a real light if the
//! piece says its lights shine. A pendant and a chandelier hang from the
//! top of their box, which is the ceiling; a sconce is fixed to the high
//! y side of its box, which is the wall.

use super::parts::{centred, octagon, ring, share, Room};
use super::style::{Shade, Style};
use super::{Kind, LIGHT, LINEN, METAL, PANEL, WOOD};

/// The most bricks of one piece that carry a light in the game. A
/// chandelier has eight candles at most; the game draws every light, and
/// a room of chandeliers is already many.
pub const MOST_LIGHTS: usize = 8;

/// A shade filling `lo..hi` with its bulb inside, and where a stem coming
/// from below or a rod from above meets the bulb: the bulb's low and high
/// faces.
///
/// The bulb is not a small thing hidden in a hollow shade. A drum is a
/// wall round a block of light that stops a cell short of the wall's top
/// and foot, so the light shows from above and below as the mouth of a
/// lit shade does. A cone is a sloped ring round such a block. A globe is
/// light all through. A shade one brick thick round a true bulb would
/// have hidden the glowing brick from every side, and would have hung in
/// the air round it, touching nothing.
fn shade(r: &mut Room, lo: [i32; 3], hi: [i32; 3], kind: Shade) -> (i32, i32) {
    let (w, d, h) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    let colour = if r.set.style == Style::Industrial { METAL } else { LINEN };
    let least = w.min(d);
    match kind {
        Shade::Globe if least >= 3 && h >= 3 => {
            // three layers, the middle one the widest: all of them glow, and the middle one is the light
            let cap = (h / 4).max(1);
            r.lamp([lo[0], lo[1], lo[2] + cap], [hi[0], hi[1], hi[2] - cap]);
            r.block([lo[0] + 1, lo[1] + 1, lo[2]], [hi[0] - 1, hi[1] - 1, lo[2] + cap], LIGHT);
            r.block([lo[0] + 1, lo[1] + 1, hi[2] - cap], [hi[0] - 1, hi[1] - 1, hi[2]], LIGHT);
            (lo[2], hi[2])
        }
        Shade::Cone if least >= 3 && h >= 2 && r.sloped(((least - 1) / 3).max(1), h) => {
            // narrower at its top by a third of its width each side
            let m = ((least - 1) / 3).max(1);
            ring(r, lo, hi, m, 0, colour);
            r.lamp([lo[0] + m, lo[1] + m, lo[2]], [hi[0] - m, hi[1] - m, hi[2]]);
            (lo[2], hi[2])
        }
        _ if least >= 3 => {
            ring_of_walls(r, lo, hi, colour);
            let rim = (h >= 3) as i32;
            r.lamp([lo[0] + 1, lo[1] + 1, lo[2] + rim], [hi[0] - 1, hi[1] - 1, hi[2] - rim]);
            (lo[2] + rim, hi[2] - rim)
        }
        _ => {
            r.lamp(lo, hi);
            (lo[2], hi[2])
        }
    }
}

/// Four walls one cell thick round the inside of `lo..hi`.
fn ring_of_walls(r: &mut Room, lo: [i32; 3], hi: [i32; 3], shade: u8) {
    r.block(lo, [hi[0], lo[1] + 1, hi[2]], shade);
    r.block([lo[0], hi[1] - 1, lo[2]], hi, shade);
    r.block([lo[0], lo[1] + 1, lo[2]], [lo[0] + 1, hi[1] - 1, hi[2]], shade);
    r.block([hi[0] - 1, lo[1] + 1, lo[2]], [hi[0], hi[1] - 1, hi[2]], shade);
}

/// A table lamp or a floor lamp: a base, a stem, and a shade at the top.
pub(super) fn standing(r: &mut Room, kind: Kind) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if h < 4 {
        return r.lamp(lo, hi);
    }
    let floor = kind == Kind::FloorLamp;
    // the shade: two fifths of a table lamp's height, a fifth of a floor lamp's
    let tall = share(h, if floor { 0.2 } else { 0.4 }, 2, h - 2);
    let (low, _) = shade(r, [lo[0], lo[1], hi[2] - tall], hi, set.shade);
    let foot = share(h, if floor { 0.04 } else { 0.1 }, 1, 3);
    let (bx, by) = (centred(lo[0], hi[0], share(w, if floor { 0.8 } else { 0.6 }, 2, w)), centred(lo[1], hi[1], share(d, if floor { 0.8 } else { 0.6 }, 2, d)));
    let frame = if set.frame == METAL || matches!(set.style, Style::Modern | Style::Classical) { METAL } else { WOOD };
    octagon(r, [bx.0, by.0, lo[2]], [bx.1, by.1, lo[2] + foot], frame);
    let (sx, sy) = (centred(lo[0], hi[0], 1), centred(lo[1], hi[1], 1));
    let mut from = lo[2] + foot;
    // a table lamp's body: a jar of the second colour for the lower part of the stem, where the style is one that has pottery
    if !floor && matches!(set.style, Style::Farmhouse | Style::Classical | Style::Rustic) && w >= 5 && d >= 5 && low - from >= 3 {
        let (jx, jy) = (centred(lo[0], hi[0], (bx.1 - bx.0 - 2).max(sx.1 - sx.0 + 2)), centred(lo[1], hi[1], (by.1 - by.0 - 2).max(sy.1 - sy.0 + 2)));
        let jar = share(low - from, 0.7, 1, low - from - 1);
        octagon(r, [jx.0, jy.0, from], [jx.1, jy.1, from + jar], PANEL);
        from += jar;
    }
    r.block([sx.0, sy.0, from], [sx.1, sy.1, low], frame);
}

/// A pendant: a rose on the ceiling, a rod, and a shade at its end.
pub(super) fn pendant(r: &mut Room) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if h < 4 {
        return r.lamp(lo, hi);
    }
    let tall = share(h, 0.4, 2, h - 2);
    let (_, high) = shade(r, lo, [hi[0], hi[1], lo[2] + tall], set.shade);
    let (rx, ry) = (centred(lo[0], hi[0], share(w, 0.34, 1, w)), centred(lo[1], hi[1], share(d, 0.34, 1, d)));
    r.block([rx.0, ry.0, hi[2] - 1], [rx.1, ry.1, hi[2]], METAL);
    let (sx, sy) = (centred(lo[0], hi[0], 1), centred(lo[1], hi[1], 1));
    r.block([sx.0, sy.0, high], [sx.1, sy.1, hi[2] - 1], METAL);
}

/// A chandelier: a rod from the ceiling to a hub, arms out from the hub
/// along x (and along y, if it has four), and a candle with a flame at
/// the end of each arm; or two candles on each of four arms.
pub(super) fn chandelier(r: &mut Room) {
    let ((w, d, h), lo, hi) = (r.size(), r.lo, r.hi);
    if w < 5 || h < 5 {
        return pendant(r);
    }
    let arms = match r.f.second {
        1 => 2,
        2 => 4,
        3 => 8,
        _ => if d < 5 { 2 } else if w.min(d) >= 17 { 8 } else { 4 },
    };
    let arms = if d < 5 { 2 } else { arms };
    // from the foot of the box up: the arms, a candle on each, its flame; the hub is at the arms' level and the rod goes on up from it
    let candle = share(h, 0.2, 1, 4);
    let level = lo[2] + share(h, 0.25, 1, h - candle - 3);
    let (cx, cy) = (centred(lo[0], hi[0], 1), centred(lo[1], hi[1], 1));
    let (hub_x, hub_y) = (centred(lo[0], hi[0], cx.1 - cx.0 + 2), if d >= 5 { centred(lo[1], hi[1], cy.1 - cy.0 + 2) } else { (lo[1], hi[1]) });
    octagon(r, [hub_x.0, hub_y.0, level - 1], [hub_x.1, hub_y.1, level + 2], METAL);
    r.block([cx.0, cy.0, level + 2], [cx.1, cy.1, hi[2] - 1], METAL);
    let (rx, ry) = (centred(lo[0], hi[0], cx.1 - cx.0 + 2), centred(lo[1], hi[1], (cy.1 - cy.0 + 2).min(d)));
    r.block([rx.0, ry.0, hi[2] - 1], [rx.1, ry.1, hi[2]], METAL);
    // each arm: a bar from the hub to the edge of the box, and on it a candle at its end (and one half way, for eight)
    // What is at the end of an arm is the style's: a candle standing on it with a flame, on a cup that lifts it a cell where the seed says so; or, in the two styles that never had candles, a bulb hanging under it.
    let set = r.set;
    let bulbs = matches!(set.style, Style::Modern | Style::Industrial);
    let cup = (!bulbs && set.more < 0.5 && candle >= 2) as i32;
    let light = |r: &mut Room, x: (i32, i32), y: (i32, i32)| {
        if bulbs {
            return r.lamp([x.0, y.0, level - 1], [x.1, y.1, level]);
        }
        r.block([x.0, y.0, level + 1], [x.1, y.1, level + 1 + cup], METAL);
        r.block([x.0, y.0, level + 1 + cup], [x.1, y.1, level + 1 + candle], LINEN);
        r.lamp([x.0, y.0, level + 1 + candle], [x.1, y.1, level + 2 + candle]);
    };
    // (and a finial under the hub, where the seed says so and there is room)
    if set.more >= 0.25 && set.more < 0.75 && level - 2 >= lo[2] {
        r.block([cx.0, cy.0, level - 2], [cx.1, cy.1, level - 1], METAL);
    }
    for (from, to, along_x) in [(lo[0], hub_x.0, true), (hub_x.1, hi[0], true), (lo[1], hub_y.0, false), (hub_y.1, hi[1], false)] {
        if to <= from || (!along_x && arms == 2) {
            continue;
        }
        let across = if along_x { cy } else { cx };
        let span = |a: (i32, i32)| if along_x { ((a.0, a.1), across) } else { (across, (a.0, a.1)) };
        let (bx, by) = span((from, to));
        r.block([bx.0, by.0, level], [bx.1, by.1, level + 1], METAL);
        let thick = across.1 - across.0;
        let outer = if from < if along_x { cx.0 } else { cy.0 } { (from, from + thick) } else { (to - thick, to) };
        let (ex, ey) = span(outer);
        light(r, ex, ey);
        if arms == 8 && to - from >= 2 * thick + 2 {
            let half = centred(from, to, thick);
            // (not against the one at the end, nor against the hub)
            if half.0 > outer.1 || half.1 < outer.0 {
                let (mx, my) = span(half);
                light(r, mx, my);
            }
        }
    }
}

/// A sconce: a plate on the wall, an arm out from it, and a shade.
pub(super) fn sconce(r: &mut Room) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if d < 3 || h < 3 {
        return r.lamp(lo, hi);
    }
    // the shade in the upper part, standing a cell off the wall where there is room
    let off = 1 + (d >= 5) as i32;
    let tall = share(h, 0.55, 2, h - 1);
    let (low, _) = shade(r, [lo[0], lo[1], hi[2] - tall], [hi[0], hi[1] - off, hi[2]], set.shade);
    let (px, pz) = (centred(lo[0], hi[0], share(w, 0.5, 1, w)), (lo[2], (hi[2] - tall + 2).min(hi[2])));
    r.block([px.0, hi[1] - 1, pz.0], [px.1, hi[1], pz.1], METAL);
    // an arm from the plate under the shade, and up into its mouth to the bulb
    let under = hi[2] - tall;
    let ax = centred(lo[0], hi[0], 1);
    let ay = centred(lo[1], hi[1] - off, 1);
    r.block([ax.0, ay.0, under - 1], [ax.1, hi[1] - 1, under], METAL);
    r.block([ax.0, ay.0, under], [ax.1, ay.1, low], METAL);
}

/// A candlestick: a base, a stem with a knop, a pan, a candle, a flame.
/// Three candles, on a bar across the stem, where it is wide enough and
/// that is asked for or it is wide enough to be meant as one.
pub(super) fn candlestick(r: &mut Room) {
    let ((w, d, h), lo, hi) = (r.size(), r.lo, r.hi);
    if h < 4 {
        return r.lamp(lo, hi);
    }
    let three = match r.f.second {
        1 => false,
        2 => w >= 7,
        _ => w >= 9 && w >= 2 * d,
    };
    let (cx, cy) = (centred(lo[0], hi[0], 1), centred(lo[1], hi[1], 1));
    let thick = (cx.1 - cx.0, cy.1 - cy.0);
    let candle = share(h, 0.3, 1, 6);
    let pan = hi[2] - 1 - candle - 1;
    // (the base no wider than it is deep, however wide a three-armed one is)
    let (bx, by) = (centred(lo[0], hi[0], d.min(w)), (lo[1], hi[1]));
    if pan <= lo[2] {
        r.block([cx.0, cy.0, lo[2]], [cx.1, cy.1, hi[2] - 1], LINEN);
        return r.lamp([cx.0, cy.0, hi[2] - 1], [cx.1, cy.1, hi[2]]);
    }
    // (of wood in the two country styles, and of metal in the others; square-footed where the style is)
    let set = r.set;
    let stuff = if matches!(set.style, Style::Farmhouse | Style::Rustic) { WOOD } else { METAL };
    if matches!(set.style, Style::Modern | Style::Industrial) {
        r.block([bx.0, by.0, lo[2]], [bx.1, by.1, lo[2] + 1], stuff);
    } else {
        octagon(r, [bx.0, by.0, lo[2]], [bx.1, by.1, lo[2] + 1], stuff);
    }
    // a knop half way up the stem, a cell wider than it all round, where the base is wider still; the stem is laid below it and above it
    let knop = lo[2] + 1 + (pan - lo[2] - 1) / 2;
    let wider = |c: (i32, i32), limit: (i32, i32)| ((c.0 - 1).max(limit.0), (c.1 + 1).min(limit.1));
    let (kx, ky) = (wider(cx, bx), wider(cy, by));
    if pan - lo[2] >= 5 && bx.1 - bx.0 >= thick.0 + 2 && set.more < 0.7 {
        r.block([cx.0, cy.0, lo[2] + 1], [cx.1, cy.1, knop], stuff);
        r.block([kx.0, ky.0, knop], [kx.1, ky.1, knop + 1], stuff);
        r.block([cx.0, cy.0, knop + 1], [cx.1, cy.1, pan], stuff);
    } else {
        r.block([cx.0, cy.0, lo[2] + 1], [cx.1, cy.1, pan], stuff);
    }
    let candle_at = |r: &mut Room, x: (i32, i32), short: i32| {
        r.block([x.0, cy.0, pan + 1], [x.1, cy.1, hi[2] - 1 - short], LINEN);
        r.lamp([x.0, cy.0, hi[2] - 1 - short], [x.1, cy.1, hi[2] - short]);
    };
    if three {
        // the bar, as wide as the box, with a candle at each end a cell shorter than the one in the middle
        r.block([lo[0], cy.0, pan], [hi[0], cy.1, pan + 1], stuff);
        candle_at(r, (lo[0], lo[0] + thick.0), (candle > 1) as i32);
        candle_at(r, (hi[0] - thick.0, hi[0]), (candle > 1) as i32);
    } else {
        r.block([kx.0, ky.0, pan], [kx.1, ky.1, pan + 1], stuff);
    }
    candle_at(r, cx, 0);
}
