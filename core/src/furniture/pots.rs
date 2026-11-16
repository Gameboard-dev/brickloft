//! Plants in pots: a potted plant, a hanging basket and a window box.
//!
//! The container is furniture, laid out by rule and exact. The plant in
//! it is one of the Plant form's own ([`crate::tree`]), grown by that
//! code and fitted cell by cell as a plant is, since a plant is a shape
//! and not a set of boards. So these are the kinds of furniture that are
//! not exact all through, and the slowest: the pot is a dozen bricks and
//! the plant hundreds or thousands.
//!
//! The plant code is reached through what it already offers any caller
//! (`Tree::of`, `Model::new`, `Model::eval`, `Model::bounds`,
//! `Model::colour`) and nothing was added to it.

use super::parts::{centred, octagon, ring, share, Room};
use super::style::Style;
use super::Kind;
use crate::shapes::CellBrick;
use crate::tree::{Kind as Plant, Model as Grown, Tree};
use glam::DVec3;

/// The colour numbers of a plant in a pot, which are its own: the pot,
/// its rim or the band of its glaze (and a basket's chains), the soil,
/// and five for the plant.
pub const POT: u8 = 0;
pub const RIM: u8 = 1;
pub const SOIL: u8 = 2;
pub const STEM: u8 = 3;
pub const LEAF: u8 = 4;
pub const LEAF_DARK: u8 = 5;
pub const FLOWER: u8 = 6;
pub const FLOWER_TOO: u8 = 7;

/// The plants that are grown in pots, in the order a piece's setting
/// numbers them from 1 (0 lets the seed pick): ones that are small, or
/// that are kept small indoors.
pub const PLANTS: [(Plant, &str); 11] = [
    (Plant::Yucca, "Yucca"),
    (Plant::Boxwood, "Boxwood ball"),
    (Plant::Hydrangea, "Hydrangea"),
    (Plant::Peony, "Peonies"),
    (Plant::Tulip, "Tulips"),
    (Plant::Daffodil, "Daffodils"),
    (Plant::Daisy, "Daisies"),
    (Plant::Petunia, "Petunias"),
    (Plant::Cypress, "Cypress"),
    (Plant::Palm, "Palm"),
    (Plant::Cherry, "Cherry tree"),
];

/// The tallest a plant in a pot is grown, in cells, however tall its box is.
pub const MOST_PLANT: f64 = 120.0;

/// The furniture colour a part of a plant has (see `tree`'s numbers):
/// wood and stems one colour, the three greens two, and whatever else it
/// has (petals, a second petal, hearts, fruit) the two flower colours.
pub(super) fn plant_colour(shade: u8) -> u8 {
    match shade {
        crate::tree::TRUNK | crate::tree::LIMB => STEM,
        crate::tree::LEAF | crate::tree::LEAF_LIGHT => LEAF,
        crate::tree::LEAF_DARK => LEAF_DARK,
        crate::tree::TOP => FLOWER,
        _ => FLOWER_TOO,
    }
}

/// Which plant a piece has: the one its setting names, or one its seed picks.
fn plant_of(r: &Room) -> Plant {
    match r.f.second {
        n if n >= 1 && n as usize <= PLANTS.len() => PLANTS[n as usize - 1].0,
        // (a basket or a window box is given what trails or flowers, and a pot anything)
        _ => {
            let from: &[Plant] = match r.f.kind {
                Kind::Basket => &[Plant::Petunia, Plant::Petunia, Plant::Daisy, Plant::Hydrangea],
                Kind::WindowBox => &[Plant::Petunia, Plant::Tulip, Plant::Daffodil, Plant::Daisy, Plant::Boxwood],
                _ => &[Plant::Yucca, Plant::Boxwood, Plant::Hydrangea, Plant::Peony, Plant::Tulip, Plant::Daffodil, Plant::Cypress, Plant::Palm, Plant::Cherry],
            };
            from[((r.set.more * from.len() as f64) as usize).min(from.len() - 1)]
        }
    }
}

/// A plant grown with its foot at `foot`, as large as fits within
/// `lo..hi`, fitted to bricks that keep out of everything laid so far
/// and inside that box, and added to the piece.
///
/// A plant keeps its own proportions: it is grown at the height there is
/// room for, with its spread in the proportion its kind starts at, and
/// if that is wider than the box it is grown again smaller. Stretched to
/// fill the box instead, a tulip in a wide pot had a cup as wide as the
/// pot. What still reaches past the box (a frond, a trailing stem) is cut
/// off at its side, so the piece is never larger than it says.
///
/// It gives way to the pot: wherever a brick of the pot is, the plant is
/// not, as grass gives way to what stands in it. The pot was laid first
/// and is exact, and stays so.
pub(super) fn grow(r: &mut Room, plant: Plant, foot: DVec3, lo: [i32; 3], hi: [i32; 3]) {
    // (no taller than a tree in a pot is: the box may be dragged to hundreds of cells, and a plant fitted at that size is a second's work and a hundred thousand bricks)
    let room = DVec3::new((hi[0] - lo[0]) as f64 / 2.0, (hi[1] - lo[1]) as f64 / 2.0, (hi[2] as f64 - foot.z).min(MOST_PLANT));
    if room.min_element() < 2.0 {
        return;
    }
    let mut tree = Tree { seed: r.f.seed, ..Tree::of(plant) };
    // (a box ball is clipped close, and what hangs from a basket trails as far as it will)
    match (plant, r.f.kind) {
        (Plant::Boxwood, _) => tree.bend = 0,
        (Plant::Petunia, Kind::Basket | Kind::WindowBox) => tree.bend = 100,
        _ => {}
    }
    let (tall, spread) = Tree::usual_size(plant);
    let mut scale = room.z / tall;
    let mut grown = Grown::new(&tree, foot, tall * scale, spread * scale);
    // how far it reaches from its foot, against how far it may
    let (low, top) = grown.bounds();
    let reach = DVec3::new((foot.x - low.x).max(top.x - foot.x), (foot.y - low.y).max(top.y - foot.y), top.z - foot.z);
    let fits = (room.x / reach.x).min(room.y / reach.y).min(room.z / reach.z);
    // (grown again only where it was well over: a leaf's width past the side is cut off instead)
    if fits < 0.9 {
        scale *= fits.max(0.2);
        grown = Grown::new(&tree, foot, tall * scale, spread * scale);
    }
    // the pot as it is now: its bricks' boxes, which the plant keeps out of
    let pot: Vec<(DVec3, DVec3)> = r.lay.bricks.iter().map(|b| (DVec3::new(b.min[0] as f64, b.min[1] as f64, b.min[2] as f64), DVec3::new((b.min[0] + b.ext[0]) as f64, (b.min[1] + b.ext[1]) as f64, (b.min[2] + b.ext[2]) as f64))).collect();
    let outside = |p: DVec3, lo: DVec3, hi: DVec3| {
        let q = (p - (lo + hi) / 2.0).abs() - (hi - lo) / 2.0;
        q.max(DVec3::ZERO).length() + q.max_element().min(0.0)
    };
    let (box_lo, box_hi) = (DVec3::new(lo[0] as f64, lo[1] as f64, lo[2] as f64), DVec3::new(hi[0] as f64, hi[1] as f64, hi[2] as f64));
    let field = |p: DVec3| {
        let mut d = grown.eval(p).max(outside(p, box_lo, box_hi));
        for (lo, hi) in &pot {
            d = d.max(-outside(p, *lo, *hi));
        }
        d
    };
    let (low, top) = grown.bounds();
    let min = [0, 1, 2].map(|k| (low[k].floor() as i32 - 1).max(lo[k]));
    let max = [0, 1, 2].map(|k| (top[k].ceil() as i32 + 1).min(hi[k]));
    let threads = crate::par::Threads::available();
    let fit = crate::fit_volume::fit_volume(&field, min, max, &crate::fit_volume::Opts { max_ext: r.lay.longest, wedges: r.slopes, stretch: true, threads });
    let (bricks, shades): (Vec<CellBrick>, Vec<u8>) = grown.colour(fit.bricks, r.lay.longest, threads);
    // (what the plant's shape holds, how far its bricks are from it, and what its bricks hold: see `Model::new`)
    r.grown = (r.grown.0 + fit.solid, r.grown.1 + fit.error, r.grown.2 + bricks.iter().map(CellBrick::volume).sum::<f64>());
    r.lay.bricks.extend(bricks);
    r.lay.shades.extend(shades.into_iter().map(plant_colour));
}

/// A pot of `shape` filling `lo..hi` (1 a terracotta pot, 2 a glazed
/// one, 3 a square planter, 4 a tall one), with soil heaped a cell above
/// its rim. Says where the soil's top is.
///
/// A round pot is stacked octagons: a saucer, a body that steps a cell
/// wider all round half way up, and a rim wider again. The other way to
/// make one was the Revolve form's fitter, which turns a profile as a
/// lathe does. It was not used, and not tried: it fits sloped rings to a
/// curve and measures how far off they are, and a pot is to be exact as
/// the rest of the furniture's containers are, with the plant the only
/// part that is fitted.
fn pot(r: &mut Room, lo: [i32; 3], hi: [i32; 3], shape: i32) -> i32 {
    let (w, d, h) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    let top = hi[2] - 1;
    let inner = |by: i32| ([lo[0] + by, lo[1] + by], [hi[0] - by, hi[1] - by]);
    if w < 5 || d < 5 || h < 4 {
        r.block(lo, [hi[0], hi[1], top], POT);
        r.block([lo[0], lo[1], top], hi, SOIL);
        return hi[2];
    }
    match shape {
        3 => {
            // a box on four feet, with a band round its top
            let feet = (h >= 6) as i32;
            for (x, y) in [(lo[0], lo[1]), (hi[0] - 1, lo[1]), (lo[0], hi[1] - 1), (hi[0] - 1, hi[1] - 1)] {
                r.block([x, y, lo[2]], [x + 1, y + 1, lo[2] + feet], RIM);
            }
            let band = (h / 5).max(1);
            r.block([lo[0], lo[1], lo[2] + feet], [hi[0], hi[1], top - band], POT);
            r.block([lo[0], lo[1], top - band], [hi[0], hi[1], top], RIM);
        }
        4 => {
            // four sides that lean out to the top, by a sloped ring round a core
            let lean = (w.min(d) / 6).max(1);
            ring(r, lo, [hi[0], hi[1], top], lean, 1, POT);
            r.block([lo[0] + lean, lo[1] + lean, lo[2]], [hi[0] - lean, hi[1] - lean, top], POT);
        }
        _ => {
            let (saucer, rim) = ((h >= 7) as i32, (h / 5).max(1));
            if saucer > 0 {
                let (a, b) = inner(0);
                octagon(r, [a[0], a[1], lo[2]], [b[0], b[1], lo[2] + 1], RIM);
            }
            let (body, half) = ((lo[2] + saucer, top - rim), lo[2] + saucer + (top - rim - lo[2] - saucer) / 2);
            if shape == 2 {
                // a glazed pot: straight sided, with a band of another colour two thirds of the way up
                let band = body.0 + (body.1 - body.0) * 2 / 3;
                let (a, b) = inner(1);
                octagon(r, [a[0], a[1], body.0], [b[0], b[1], band], POT);
                octagon(r, [a[0], a[1], band], [b[0], b[1], body.1], RIM);
                octagon(r, [a[0], a[1], body.1], [b[0], b[1], top], POT);
            } else {
                let ((a, b), (c, e)) = (inner(2), inner(1));
                octagon(r, [a[0], a[1], body.0], [b[0], b[1], half], POT);
                octagon(r, [c[0], c[1], half], [e[0], e[1], body.1], POT);
                let (a, b) = inner(0);
                octagon(r, [a[0], a[1], body.1], [b[0], b[1], top], RIM);
            }
        }
    }
    let (a, b) = inner(1 + (shape != 3 && shape != 4) as i32);
    octagon(r, [a[0], a[1], top], [b[0], b[1], hi[2]], SOIL);
    hi[2]
}

/// A potted plant: a pot in the middle of the floor of its box, and a plant in it.
pub(super) fn potted(r: &mut Room) {
    let ((w, d, h), lo, hi) = (r.size(), r.lo, r.hi);
    // (too small for a pot with a plant in it: a block, as every kind is at such a size)
    if w < 5 || d < 5 || h < 6 {
        return r.block(lo, hi, POT);
    }
    let shape = match r.f.choice {
        n @ 1..=4 => n,
        // (the style's pot: glazed or a square planter in a modern room, a tall planter in an industrial one, terracotta in the country)
        _ => match r.set.style {
            Style::Modern => [2, 3][(r.set.more < 0.5) as usize],
            Style::Industrial => [4, 3][(r.set.more < 0.4) as usize],
            Style::Classical => [1, 2][(r.set.more < 0.5) as usize],
            _ => 1,
        },
    };
    // The pot: a third of the height (a tall planter nearly half), or as many tenths as asked; and six tenths of the width, since a plant is wider than its pot.
    let tall = if r.f.count > 0 { share(h, r.f.count as f64 / 10.0, 2, h - 2) } else { share(h, if shape == 4 { 0.45 } else { 0.3 }, 2, h - 2) };
    let wide = share(w.min(d), if shape == 4 { 0.45 } else { 0.6 }, 3, w.min(d));
    let (x, y) = (centred(lo[0], hi[0], wide), centred(lo[1], hi[1], wide));
    let soil = pot(r, [x.0, y.0, lo[2]], [x.1, y.1, lo[2] + tall], shape);
    let plant = plant_of(r);
    grow(r, plant, DVec3::new((x.0 + x.1) as f64 / 2.0, (y.0 + y.1) as f64 / 2.0, soil as f64 - 1.0), lo, hi);
}

/// A hanging basket: a bowl hung by two rods from a bar under a hook on
/// the ceiling (the top of its box), with a plant that trails over it.
pub(super) fn basket(r: &mut Room) {
    let ((w, d, h), lo, hi) = (r.size(), r.lo, r.hi);
    if w < 7 || d < 5 || h < 10 {
        return potted(r);
    }
    // the bowl hangs a third of the way up, leaving room below it for what trails
    let (deep, wide) = (share(h, 0.14, 3, 8), share(w.min(d), 0.62, 5, w.min(d)));
    let foot = lo[2] + share(h, 0.3, 1, h - deep - 4);
    let (x, y) = (centred(lo[0], hi[0], wide), centred(lo[1], hi[1], wide));
    let top = foot + deep;
    // narrow at its foot, and a cell wider all round at each step up
    let steps = (deep - 1).min(3).min((wide - 3) / 2).max(1);
    let mut z = foot;
    for step in (0..steps).rev() {
        let up = if step == 0 { top - 1 } else { z + (top - 1 - foot) / steps };
        octagon(r, [x.0 + step, y.0 + step, z], [x.1 - step, y.1 - step, up], POT);
        z = up;
    }
    octagon(r, [x.0 + 1, y.0 + 1, top - 1], [x.1 - 1, y.1 - 1, top], SOIL);
    // the hanger: a rod up from each side of the rim, a bar across, a hook
    let mid = centred(lo[1], hi[1], 1);
    for rod in [x.0, x.1 - 1] {
        r.block([rod, mid.0, top - 1], [rod + 1, mid.1, hi[2] - 2], RIM);
    }
    r.block([x.0, mid.0, hi[2] - 2], [x.1, mid.1, hi[2] - 1], RIM);
    let hook = centred(lo[0], hi[0], 1);
    r.block([hook.0, mid.0, hi[2] - 1], [hook.1, mid.1, hi[2]], RIM);
    let plant = plant_of(r);
    grow(r, plant, DVec3::new((x.0 + x.1) as f64 / 2.0, (y.0 + y.1) as f64 / 2.0, top as f64 - 1.0), lo, [hi[0], hi[1], hi[2] - 2]);
}

/// A window box: a trough as long as its box, and plants along it.
pub(super) fn window_box(r: &mut Room) {
    let ((w, d, h), lo, hi) = (r.size(), r.lo, r.hi);
    if w < 2 * d || d < 4 || h < 6 {
        return potted(r);
    }
    let tall = if r.f.count > 0 { share(h, r.f.count as f64 / 10.0, 2, h - 2) } else { share(h, 0.3, 2, h - 2) };
    let y = centred(lo[1], hi[1], share(d, 0.7, 3, d));
    let (top, band) = (lo[2] + tall - 1, (tall / 4).max(1));
    r.block([lo[0], y.0, lo[2]], [hi[0], y.1, top - band], POT);
    r.block([lo[0], y.0, top - band], [hi[0], y.1, top], RIM);
    r.block([lo[0] + 1, y.0 + 1, top], [hi[0] - 1, y.1 - 1, top + 1], SOIL);
    // one plant for every stretch as long as the box is deep, each in its own length of the trough
    let plant = plant_of(r);
    let n = ((w as f64 / d as f64).round() as i32).clamp(1, 12);
    let mut at = lo[0];
    for each in super::shared(w, n, false) {
        // (each from numbers of its own, or they would be the one plant several times)
        r.f.seed = r.f.seed.wrapping_add(0x9E37_79B9);
        grow(r, plant, DVec3::new(at as f64 + each as f64 / 2.0, (y.0 + y.1) as f64 / 2.0, top as f64), [at, lo[1], lo[2]], [at + each, hi[1], hi[2]]);
        at += each;
    }
}
