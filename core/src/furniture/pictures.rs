//! Pictures: a framed painting and a framed portrait, to hang on a wall.
//!
//! The frame is furniture like the rest. The picture in it is a mosaic
//! one cell deep, a cell to a pixel, drawn by rule from the seed and
//! then joined into the largest bricks of one colour
//! ([`crate::pattern::joined`], which is what joins a mosaic's tiles).
//!
//! Every shape in a picture is decided with additions, multiplications,
//! comparisons and square roots alone, each of which gives the same
//! answer to the last bit on any machine. A sine or a power does not: the
//! standard leaves their last bit to the library, and a hill drawn with
//! a sine could differ by a pixel between two computers. So hills are
//! triangle waves, and a sky grades by a square.
//!
//! A picture is flat against the high y side of its box (the wall), and
//! its face is to the low y side.

use super::parts::{share, Room};
use super::style::Style;
use super::{mask_of, Furniture, Kind};
use crate::pattern::Numbers;
use crate::shapes::{CellBrick, CUBE};

/// The colour numbers of a picture: its frame, the liner inside a
/// moulded frame, its mount, and from [`FIRST`] on the picture's own,
/// which are each subject's affair (the editor's sets of colours say
/// what each is).
pub const FRAME: u8 = 0;
pub const LINER: u8 = 1;
pub const MOUNT: u8 = 2;
pub const FIRST: u8 = 3;

/// What a painting may be of, in the order its setting numbers them from 1 (0 lets the seed pick).
pub const SUBJECTS: [&str; 3] = ["sunset", "landscape", "abstract"];

/// Which subject a painting has: the one its setting names, or one its seed picks. From 0.
pub fn subject(f: &Furniture) -> usize {
    match f.choice {
        n @ 1..=3 => n as usize - 1,
        _ => ((Numbers(f.seed ^ 0x50B1_EC70_F7AE_0001).unit() * 3.0) as usize).min(2),
    }
}

/// A wave from -1 to 1 and back once for every 1 of `x`, in straight lines.
fn wave(x: f64) -> f64 {
    let t = x - x.floor();
    1.0 - 4.0 * (t - 0.5).abs()
}

/// A picture of `kind` filling the piece's box.
pub(super) fn picture(r: &mut Room, kind: Kind) {
    let ((w, d, h), lo, hi, set) = (r.size(), r.lo, r.hi, r.set);
    if w < 5 || h < 5 {
        return r.block(lo, hi, FRAME);
    }
    let least = w.min(h);
    // how wide the frame is: a line in the two plain styles, and a seventh of the picture in the grandest
    let wide = match set.style {
        Style::Modern | Style::Industrial => 1,
        Style::Farmhouse => share(least, 0.08, 1, 3),
        Style::Rustic => share(least, 0.12, 2, 5),
        _ => share(least, 0.1, 2, 5),
    }
    .min((least - 3) / 2)
    .max(1);
    r.block(lo, [hi[0], hi[1], lo[2] + wide], FRAME);
    r.block([lo[0], lo[1], hi[2] - wide], hi, FRAME);
    r.block([lo[0], lo[1], lo[2] + wide], [lo[0] + wide, hi[1], hi[2] - wide], FRAME);
    r.block([hi[0] - wide, lo[1], lo[2] + wide], [hi[0], hi[1], hi[2] - wide], FRAME);
    let (mut x, mut z) = ((lo[0] + wide, hi[0] - wide), (lo[2] + wide, hi[2] - wide));
    // the canvas is the back cell of the box's depth, and the frame stands proud of it by the rest
    let back = hi[1] - 1;
    // A moulded frame has a liner inside it that slopes from the frame's face down to the canvas: a ramp along each side and a hip in each corner, as a cornice is.
    let deep = back - lo[1];
    if set.moulding && deep >= 1 && x.1 - x.0 >= 7 && z.1 - z.0 >= 7 && r.sloped(1, deep) {
        let y = (lo[1], back);
        for (at, inner) in [(z.0, 1u8), (z.1 - 1, 0)] {
            r.lay.piece(mask_of(|_, y, z| !(y == 0 && z == inner)), [x.0 + 1, y.0, at], [x.1 - 1, y.1, at + 1], LINER, 0);
            for (cx, side) in [(x.0, 0u8), (x.1 - 1, 1)] {
                // the whole of its back, and of its front the one corner that touches the frame
                r.lay.piece(mask_of(|x, y, z| y == 1 || (x == side && z != inner)), [cx, y.0, at], [cx + 1, y.1, at + 1], LINER, 1);
            }
        }
        for (at, inner) in [(x.0, 1u8), (x.1 - 1, 0)] {
            r.lay.piece(mask_of(|x, y, _| !(y == 0 && x == inner)), [at, y.0, z.0 + 1], [at + 1, y.1, z.1 - 1], LINER, 2);
        }
        // (and behind the liner, in the canvas's own layer, a strip of it all round: without it the canvas touched nothing, and there was a slot round it to the wall)
        r.block([x.0, back, z.0], [x.1, back + 1, z.0 + 1], LINER);
        r.block([x.0, back, z.1 - 1], [x.1, back + 1, z.1], LINER);
        r.block([x.0, back, z.0 + 1], [x.0 + 1, back + 1, z.1 - 1], LINER);
        r.block([x.1 - 1, back, z.0 + 1], [x.1, back + 1, z.1 - 1], LINER);
        (x, z) = ((x.0 + 1, x.1 - 1), (z.0 + 1, z.1 - 1));
    }
    let (pw, ph) = (x.1 - x.0, z.1 - z.0);
    // a mount inside the frame, a tenth of the picture wide, where it is asked for
    let mount = if r.f.extra { share(pw.min(ph), 0.08, 1, 4).min((pw.min(ph) - 3) / 2).max(0) } else { 0 };
    let (iw, ih) = (pw - 2 * mount, ph - 2 * mount);
    let drawn: Box<dyn Fn(i32, i32) -> u8> = match kind {
        Kind::Portrait => portrait(&r.f, iw, ih),
        _ => match subject(&r.f) {
            0 => sunset(r.f.seed, iw, ih),
            1 => landscape(r.f.seed, iw, ih),
            _ => abstract_(r.f.seed, iw, ih),
        },
    };
    // each row in runs of one colour, and the runs joined up the picture into the largest bricks
    let mut runs = Vec::new();
    for j in 0..ph {
        let at = |i: i32| if i < mount || j < mount || i >= pw - mount || j >= ph - mount { MOUNT } else { drawn(i - mount, j - mount) };
        let mut from = 0;
        for i in 1..=pw {
            if i == pw || at(i) != at(from) {
                // (in lengths no longer than a brick may be)
                for start in (from..i).step_by(r.lay.longest.max(1) as usize) {
                    runs.push((CellBrick { mask: CUBE, min: [x.0 + start, back, z.0 + j], ext: [(i - start).min(r.lay.longest.max(1)), 1, 1] }, at(from)));
                }
                from = i;
            }
        }
    }
    for (brick, shade) in crate::pattern::joined(runs, r.lay.longest) {
        r.lay.bricks.push(brick);
        r.lay.shades.push(shade);
    }
    let _ = d;
}

/// A sunset over water, `w` by `h` pixels: a sky in eight bands that
/// brighten towards the horizon (colours 3 to 10 from the top), a sun
/// (11) partly below it with a glow (12), dark hills (15) along the far
/// shore to either side of the sun, and water (13 and 14 in streaks)
/// with the sun's broken reflection (12) under it.
fn sunset(seed: u64, w: i32, h: i32) -> Box<dyn Fn(i32, i32) -> u8> {
    let mut n = Numbers(seed ^ 0x5075_E7A1_B0A7_0001);
    let (wf, hf) = (w as f64, h as f64);
    let horizon = (hf * (0.34 + 0.12 * n.unit())).round();
    let (sx, radius) = (wf * (0.3 + 0.4 * n.unit()), (wf.min(hf) * (0.11 + 0.07 * n.unit())).max(1.5));
    let sz = horizon + radius * (-0.2 + 0.7 * n.unit());
    let (f, p, tall) = (1.0 + 2.0 * n.unit(), n.unit(), hf * (0.05 + 0.06 * n.unit()));
    Box::new(move |i, j| {
        let (x, z) = (i as f64 + 0.5, j as f64 + 0.5);
        let up = z - horizon;
        if up < 0.0 {
            // water: every other row under the sun is its reflection, narrowing as it nears the viewer
            let row = (-up) as i32;
            let across = radius * (1.0 - 0.6 * (-up) / horizon.max(1.0));
            return if row % 2 == 0 && (x - sx).abs() < across { 12 } else if row % 3 == 2 { 14 } else { 13 };
        }
        let from_sun = ((x - sx) * (x - sx) + (z - sz) * (z - sz)).sqrt();
        if from_sun <= radius {
            return 11;
        }
        // hills on the far shore, lower towards the sun so that it sets between them
        let away = ((x - sx).abs() / wf - 0.15).max(0.0);
        if up < tall * (0.6 + 0.4 * wave(x / wf * f + p)) * (away * 4.0).min(1.0) {
            return 15;
        }
        if from_sun <= radius + (radius * 0.5).max(1.0) {
            return 12;
        }
        let t = 1.0 - up / (hf - horizon).max(1.0);
        FIRST + ((t * t * 8.0) as u8).min(7)
    })
}

/// A landscape: a sky in four bands (3 to 6) with clouds (7), four
/// ranges of hills one behind another, paler the further off (8 the
/// furthest to 11 the nearest), which on some seeds are mountains with
/// snow (15) at the back and on some hold a lake (14), and a few firs
/// (12, on trunks of 13) on the nearest hill.
fn landscape(seed: u64, w: i32, h: i32) -> Box<dyn Fn(i32, i32) -> u8> {
    let mut n = Numbers(seed ^ 0x1A2D_5CA9_E000_0001);
    let (wf, hf) = (w as f64, h as f64);
    let ranges: Vec<(f64, f64, f64, f64, f64)> = (0..4).map(|k| (hf * (0.56 - 0.14 * k as f64), hf * (0.04 + 0.025 * k as f64), 0.7 + 1.6 * n.unit(), n.unit(), n.unit())).collect();
    let (mountains, lake) = (n.unit() < 0.5, n.unit() < 0.5);
    let peaks = 1.5 + 2.0 * n.unit();
    let ridge = move |k: usize, x: f64| {
        let (base, amp, f, p, q) = ranges[k];
        if k == 0 && mountains {
            // sharp peaks: a triangle wave, and a smaller one on it
            return base + hf * 0.2 * (0.5 + 0.5 * wave(x / wf * peaks + p)) + hf * 0.04 * wave(x / wf * peaks * 3.1 + q);
        }
        base + amp * (wave(x / wf * f + p) + 0.5 * wave(x / wf * f * 2.3 + q))
    };
    let clouds: Vec<(f64, f64, f64)> = (0..2 + (n.unit() * 2.0) as usize).map(|_| (wf * n.unit(), hf * (0.74 + 0.18 * n.unit()), wf * (0.08 + 0.1 * n.unit()))).collect();
    let tall = (hf * 0.15).max(3.0).round();
    let firs: Vec<f64> = (0..2 + (n.unit() * 3.0) as usize).map(|_| (wf * (0.08 + 0.84 * n.unit())).floor() + 0.5).collect();
    let water = hf * 0.22;
    Box::new(move |i, j| {
        let (x, z) = (i as f64 + 0.5, j as f64 + 0.5);
        // the firs first, being in front of everything: a trunk for the lowest quarter, and a crown that narrows to its tip
        for fx in &firs {
            let (foot, off) = (ridge(3, *fx).floor() - 1.0, (x - fx).abs());
            let up = z - foot;
            if up > 0.0 && up < tall {
                if up < tall * 0.25 {
                    if off < 0.6 {
                        return 13;
                    }
                } else if off < 0.6 + (tall - up) * 0.34 {
                    return 12;
                }
            }
        }
        for k in (0..4).rev() {
            if z < ridge(k, x) {
                if lake && k <= 2 && z < water {
                    return 14;
                }
                return if k == 0 && mountains && z > ranges_snow(hf) + hf * 0.03 * wave(x / wf * 7.0) { 15 } else { 8 + k as u8 };
            }
        }
        if clouds.iter().any(|(cx, cz, long)| {
            let (a, b) = ((x - cx) / long, (z - cz) / (long * 0.3).max(0.8));
            a * a + b * b < 1.0
        }) {
            return 7;
        }
        let t = ((z - hf * 0.45) / (hf * 0.55)).clamp(0.0, 0.999);
        FIRST + 3 - (t * 4.0) as u8
    })
}

/// How high the snow line of a landscape's mountains is.
fn ranges_snow(hf: f64) -> f64 {
    hf * 0.68
}

/// An abstract: one of three compositions the seed picks. Rectangles
/// divided by dark lines, some filled and most left pale; bands across
/// with one block and one disc set against them; or arcs about a point
/// near a corner. Colour 3 is the line, 4 to 9 the hues, and 10 the pale ground.
fn abstract_(seed: u64, w: i32, h: i32) -> Box<dyn Fn(i32, i32) -> u8> {
    let mut n = Numbers(seed ^ 0xAB57_2AC7_0000_0001);
    let (wf, hf) = (w as f64, h as f64);
    match ((n.unit() * 3.0) as usize).min(2) {
        0 => {
            // Divide the canvas in two, and each part again, a few times over: always across its longer side, between a third and two thirds of the way, so that no part is a sliver.
            let mut parts = vec![(0, 0, w, h)];
            for _ in 0..3 + (n.unit() * 3.0) as usize {
                let at = (n.unit() * parts.len() as f64) as usize % parts.len();
                let (x, z, pw, ph) = parts[at];
                let cut = 0.34 + 0.32 * n.unit();
                if pw >= ph && pw >= 7 {
                    let left = (pw as f64 * cut) as i32;
                    parts[at] = (x, z, left, ph);
                    parts.push((x + left + 1, z, pw - left - 1, ph));
                } else if ph >= 7 {
                    let low = (ph as f64 * cut) as i32;
                    parts[at] = (x, z, pw, low);
                    parts.push((x, z + low + 1, pw, ph - low - 1));
                }
            }
            let fills: Vec<u8> = parts.iter().map(|_| if n.unit() < 0.45 { 4 + (n.unit() * 6.0) as u8 % 6 } else { 10 }).collect();
            Box::new(move |i, j| parts.iter().zip(&fills).find(|((x, z, pw, ph), _)| i >= *x && i < x + pw && j >= *z && j < z + ph).map_or(3, |(_, fill)| *fill))
        }
        1 => {
            let mut bands = Vec::new();
            let mut at = 0;
            while at < h {
                let tall = (2 + (n.unit() * hf * 0.2) as i32).min(h - at);
                bands.push((at + tall, 4 + (n.unit() * 6.0) as u8 % 6));
                at += tall;
            }
            let block = ((wf * (0.12 + 0.5 * n.unit())) as i32, (hf * (0.15 + 0.3 * n.unit())) as i32, (wf * 0.24).max(2.0) as i32, (hf * 0.4).max(2.0) as i32);
            let disc = (wf * (0.3 + 0.5 * n.unit()), hf * (0.3 + 0.5 * n.unit()), (wf.min(hf) * 0.16).max(1.5));
            Box::new(move |i, j| {
                let (x, z) = (i as f64 + 0.5 - disc.0, j as f64 + 0.5 - disc.1);
                if x * x + z * z < disc.2 * disc.2 {
                    return 10;
                }
                if i >= block.0 && i < block.0 + block.2 && j >= block.1 && j < block.1 + block.3 {
                    return 3;
                }
                bands.iter().find(|band| j < band.0).map_or(10, |band| band.1)
            })
        }
        _ => {
            let centre = (wf * if n.unit() < 0.5 { 0.2 } else { 0.8 }, hf * if n.unit() < 0.5 { 0.2 } else { 0.8 });
            let (ring, turn) = ((wf.min(hf) * 0.12).max(2.0), (n.unit() * 6.0) as u8);
            Box::new(move |i, j| {
                let (x, z) = (i as f64 + 0.5 - centre.0, j as f64 + 0.5 - centre.1);
                let out = ((x * x + z * z).sqrt() / ring) as u8;
                // (every fourth ring pale, and beyond the seventh the ground)
                if out >= 7 || out % 4 == 3 { 10 } else { 4 + (out + turn) % 6 }
            })
        }
    }
}

/// The colour numbers of a portrait from [`FIRST`]: two of background;
/// four skins and the shade of each; five colours of hair; the white of
/// an eye, its pupil, a mouth; four colours of clothes and a collar;
/// glasses, a blush and a hat. The seed picks which skin, hair and
/// clothes a sitter has, so one set of colours holds every sitter.
const GROUND: u8 = 3;
const SKIN: u8 = 5;
const HAIR: u8 = 13;
const WHITE: u8 = 18;
const PUPIL: u8 = 19;
const MOUTH: u8 = 20;
const CLOTHES: u8 = 21;
const COLLAR: u8 = 25;
const GLASSES: u8 = 26;
const BLUSH: u8 = 27;
const HAT: u8 = 28;

/// A portrait: a head and shoulders, drawn as a cartoon is.
///
/// Everything is placed in whole cells about the picture's middle
/// column, or the line between its two middle columns, by how far a cell
/// is to either side: so a face is its own mirror image exactly, which is
/// most of what makes a few dozen pixels read as a face. The eyes are a
/// pupil each, with a white beside it where the head is wide enough, on
/// one row; a brow over each; a nose a cell or two of the skin's shade;
/// and a mouth whose ends turn up. The mouth always smiles, a little or
/// broadly: at this size a straight mouth reads as displeased.
///
/// Hair is one of six ways (short, a side parting, long, a bun, curls,
/// none) in one of five colours, and a sitter may wear glasses, a hat or
/// a beard, as the setting says or the seed picks. A head needs nine
/// cells of width for two eyes with a nose between them, which a picture
/// about eighteen cells wide inside its frame gives; smaller than that it
/// is still a head with eyes and a mouth, and no more.
fn portrait(f: &Furniture, w: i32, h: i32) -> Box<dyn Fn(i32, i32) -> u8> {
    let mut n = Numbers(f.seed ^ 0xFACE_0FA5_177E_0001);
    let mut pick = |of: f64| (n.unit() * of) as u8;
    let (skin, hair, clothes, style) = (pick(4.0).min(3), pick(5.0).min(4), pick(4.0).min(3), pick(6.0).min(5));
    let wears = match f.choice {
        c @ 1..=4 => c as u8 - 1,
        _ => [0, 0, 0, 1, 2, 3][pick(6.0).min(5) as usize],
    };
    let (smile, square, blush) = (pick(2.0), pick(3.0) == 0, pick(3.0) == 0);
    // twice how far a cell's middle is from the picture's middle line, so that it is a whole number for an even width and an odd one alike
    let off = move |i: i32| (2 * i + 1 - w).abs();
    // the head: half its width and half its height, in cells, and where its middle is
    // (never less than a cell of either, however small the picture: a size no face fits is still drawn, and nothing here may fail)
    let rx = ((w as f64 * 0.27).round() as i32).min((h as f64 * 0.24) as i32 + 1).max(1);
    let ry = ((rx as f64 * 1.2).round() as i32).min((h - 4) / 3).max(1);
    let cz = (h as f64 * 0.58).round() as i32;
    let (rxf, ryf) = (rx as f64, ry as f64);
    // how far inside the head a cell is: under 1 inside an oval, or a squarer one
    let within = move |i: i32, j: i32, grow: f64| {
        let (a, b) = (off(i) as f64 / 2.0 / (rxf + grow), (j as f64 + 0.5 - cz as f64) / (ryf + grow));
        if square { (a * a * a * a + b * b * b * b).sqrt() } else { a * a + b * b }
    };
    let eyes = cz + (ry as f64 * 0.12).round() as i32;
    let apart = ((rx as f64 * 0.45).round() as i32).max(1);
    let wide = rx >= 5;
    let mouth = cz - (ry as f64 * 0.5).round() as i32;
    let grin = ((rx as f64 * 0.3).round() as i32).max(1);
    let shoulders = (h as f64 * 0.24).round();
    Box::new(move |i, j| {
        let side = off(i);
        // (how many cells from the middle line: 0 for the middle column, or for the two columns beside the line)
        let out = side / 2;
        let face = within(i, j, 0.0) < 1.0;
        if face {
            // a hat sits over the brow, and hides the hair
            if wears == 2 && j >= cz + (ry * 2 / 3) {
                return HAT;
            }
            // glasses: a rim round each eye, and a bridge between
            let eye = out >= apart - 1 && out <= apart + wide as i32 && j == eyes;
            if wears == 1 && rx >= 5 {
                let rim = out >= apart - 1 && out <= apart + 2 && (j - eyes).abs() <= 1;
                if rim && !(eye && out >= apart && out <= apart + 1) && !(out > apart - 1 && out < apart + 2 && j == eyes) {
                    return GLASSES;
                }
                if out < apart - 1 && j == eyes {
                    return GLASSES;
                }
            }
            if j == eyes && out == apart {
                return PUPIL;
            }
            if j == eyes && wide && out == apart + 1 {
                return WHITE;
            }
            if j == eyes + 2 && ry >= 5 && out >= apart - (wide as i32) && out <= apart + 1 {
                return HAIR + hair;
            }
            // the mouth: a row, with its two ends a cell higher; a broad smile is a row deeper, white inside
            if out <= grin {
                if (j == mouth && out < grin) || (j == mouth + 1 && out == grin) {
                    return MOUTH;
                }
                if smile == 1 && ry >= 6 && j == mouth + 1 && out < grin {
                    return WHITE;
                }
                if smile == 1 && ry >= 6 && j == mouth + 2 && out <= grin {
                    return MOUTH;
                }
            }
            if wears == 3 && j < mouth + 3 + (smile == 1 && ry >= 6) as i32 && (j < mouth || out > grin) {
                return HAIR + hair;
            }
            if out == 0 && j < eyes - 1 && j > mouth + 2 + (smile == 1) as i32 {
                return SKIN + 4 + skin;
            }
            if blush && ry >= 5 && j == eyes - 2 && out >= apart && out <= apart + 1 {
                return BLUSH;
            }
            // the hairline: hair over the top of the head, lower at the sides, and by the style across the brow
            let line = cz + ry / 2 + if style == 1 { (i * 2 / w.max(1)).min(1) } else { 0 };
            let fringe = j > line + (out / 2).min(1);
            if style != 5 && wears != 2 && (fringe || (j >= cz + ry / 3 && within(i, j, -1.0) >= 1.0)) {
                return HAIR + hair;
            }
            return SKIN + skin;
        }
        // round the head: a hat's crown and brim, hair that stands out from it, ears
        if wears == 2 {
            if j >= cz + ry * 2 / 3 && j <= cz + ry + 1 + ry / 3 && out <= rx - 1 {
                return HAT;
            }
            if j == cz + ry * 2 / 3 && out <= rx + 2 {
                return HAT;
            }
        } else if style != 5 {
            let grow = if style == 4 { 2.0 } else { 1.0 };
            if within(i, j, grow) < 1.0 && j >= cz - if style == 2 { ry + ry / 2 } else { 0 } {
                return HAIR + hair;
            }
            if style == 2 && out <= rx + 1 && out >= rx - 1 && j < cz && j as f64 >= shoulders - 1.0 {
                return HAIR + hair;
            }
            if style == 3 && out <= (rx / 2).max(1) && j > cz + ry && j <= cz + ry + (rx / 2).max(1) + 1 {
                return HAIR + hair;
            }
        }
        if out == rx && (j - eyes).abs() <= (ry / 4).max(0) && within(i, j, 1.0) < 1.0 {
            return SKIN + 4 + skin;
        }
        // the neck, and the shoulders: an arch across the foot of the picture, with a collar at the neck
        let neck = (rx / 2).max(1);
        if out < neck && j < cz && j as f64 >= shoulders - 1.0 {
            return SKIN + 4 + skin;
        }
        let (a, b) = (side as f64 / 2.0 / (w as f64 * 0.47), (j as f64 + 0.5) / shoulders.max(1.0));
        if a * a + b * b < 1.0 {
            return if out <= neck && (j as f64) >= shoulders - 2.0 - (neck - out) as f64 { COLLAR } else { CLOTHES + clothes };
        }
        // the ground behind: darker towards the foot
        GROUND + ((j as f64) < h as f64 * 0.4) as u8
    })
}
