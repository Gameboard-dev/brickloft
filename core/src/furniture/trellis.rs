//! A garden trellis in a planter, with a climber growing up it.
//!
//! Planter, lattice and climber are all laid by rule, brick by brick, so
//! this kind is exact as a table is, where the other plants in pots are
//! fitted. The Plant form has no climber, and its nearest thing, a hedge
//! laid along lines (`tree::Model::laid_all`, as a maze's hedges are), is
//! a mass of leaf several cells thick: along the slats of a lattice one
//! cell wide it buried them, and the trellis was a green slab. A climber
//! here is what it is on a real trellis seen from across a garden: a few
//! thin stems that go up the uprights and along the rails, and single
//! leaves and flowers beside them, with the lattice showing between.
//!
//! It is shallow, and everything is against its back (the high y side),
//! so it can stand against a wall: from the wall forwards, the uprights,
//! the rails, the stems, and the leaves.

use super::parts::{share, Room};
use super::pots::{FLOWER, FLOWER_TOO, LEAF, LEAF_DARK, POT, RIM, SOIL, STEM};
use super::{mask_of, shared};
use crate::pattern::Numbers;
use std::collections::HashSet;

/// The climbers there are, in the order a piece's setting numbers them
/// from 1 (0 lets the seed pick one); the last is no plant at all.
pub const CLIMBERS: [&str; 5] = ["Ivy", "Clematis", "Climbing rose", "Wisteria", "A bare trellis"];

pub(super) fn trellis(r: &mut Room) {
    let ((w, d, h), lo, hi) = (r.size(), r.lo, r.hi);
    if w < 7 || d < 5 || h < 10 {
        return r.block(lo, hi, POT);
    }
    // the planter: a box the whole width and depth, a fifth of the height, with a band round its top and soil in it
    let tall = share(h, 0.2, 3, 12);
    let (top, band) = (lo[2] + tall - 1, (tall / 4).max(1));
    r.block(lo, [hi[0], hi[1], top - band], POT);
    r.block([lo[0], lo[1], top - band], [hi[0], hi[1], top], RIM);
    // (the soil stops four cells short of the back, where the lattice and the climber on it stand on the planter itself)
    r.block([lo[0] + 1, lo[1] + 1, top], [hi[0] - 1, hi[1] - 4, top + 1], SOIL);
    let (x, z, back) = ((lo[0] + 1, hi[0] - 1), (top, hi[2]), hi[1] - 1);
    if d < 6 {
        return;
    }
    // How open the lattice is: the cells between one slat and the next, as asked for or a seventh of its width.
    let open = if r.f.count > 0 { r.f.count + 1 } else { ((x.1 - x.0) / 7).clamp(2, 6) };
    let shape = match r.f.choice {
        n @ 1..=3 => n,
        _ => 1 + ((r.set.more * 3.0) as i32).min(2),
    };
    // where its uprights are, the two at its sides among them, as evenly as whole cells allow
    let bays = ((x.1 - x.0 - 1) / (open + 1)).max(1);
    let mut uprights = vec![x.0];
    for gap in shared(x.1 - x.0 - 1 - bays, bays, false) {
        uprights.push(uprights[uprights.len() - 1] + gap + 1);
    }
    // How tall each upright is: all of the height, or for an arched top less towards the sides, by the circle through the top's middle.
    let rise = if shape == 3 { ((x.1 - x.0) / 3).min((z.1 - z.0) / 3) } else { 0 };
    let height = |ux: i32| {
        let across = (2.0 * (ux - x.0) as f64 / (x.1 - x.0 - 1) as f64 - 1.0).abs();
        z.1 - (rise as f64 * (1.0 - (1.0 - across * across).max(0.0).sqrt())).round() as i32
    };
    // (the cells the lattice fills in the plane of its rails, for the climber to find its way by)
    let mut slats: HashSet<(i32, i32)> = HashSet::new();
    if shape == 2 && r.sloped(1, 1) {
        // A diamond lattice: a frame, and laths at 45 degrees each way, the ones rising to the right against the wall and the others in front of them. A lath is a ramp and its opposite in each cell it crosses, which is its true width; stepped out of whole cells it was a stair.
        r.block([x.0, back, z.0], [x.0 + 1, back + 1, z.1], RIM);
        r.block([x.1 - 1, back, z.0], [x.1, back + 1, z.1], RIM);
        r.block([x.0 + 1, back, z.1 - 1], [x.1 - 1, back + 1, z.1], RIM);
        let step = (open + 1).max(3);
        for cx in x.0 + 1..x.1 - 1 {
            for cz in z.0..z.1 - 1 {
                for (layer, rising) in [(back, true), (back - 1, false)] {
                    // (which lath of its family a cell is on, and the cell under it shares that lath's other half)
                    let on = if rising { (cx - x.0 - (cz - z.0)).rem_euclid(step) } else { (cx - x.0 + cz - z.0).rem_euclid(step) };
                    let side = (rising as u8, !rising as u8);
                    if on == 0 {
                        r.lay.piece(mask_of(|x, _, z| !(x == side.0 && z == 0)), [cx, layer, cz], [cx + 1, layer + 1, cz + 1], RIM, 1);
                        slats.insert((cx, cz));
                    } else if on == if rising { step - 1 } else { 1 } {
                        r.lay.piece(mask_of(|x, _, z| !(x == side.1 && z == 1)), [cx, layer, cz], [cx + 1, layer + 1, cz + 1], RIM, 1);
                    }
                }
            }
        }
    } else {
        // A square lattice, in two layers as a joiner nails one: the uprights against the wall, and the rails across in front of them.
        for ux in &uprights {
            r.block([*ux, back, z.0], [ux + 1, back + 1, height(*ux)], RIM);
            slats.extend((z.0..height(*ux)).map(|cz| (*ux, cz)));
        }
        let rows = ((z.1 - rise - z.0 - 1) / (open + 1)).max(1);
        let mut at = z.0 - 1;
        for gap in shared(z.1 - rise - z.0 - rows, rows, true) {
            at += gap + 1;
            r.block([x.0, back - 1, at], [x.1, back, at + 1], RIM);
            slats.extend((x.0..x.1).map(|cx| (cx, at)));
        }
        // the arch: a short rail from each upright to the next, at the top of the lower of the two
        for pair in uprights.windows(2).filter(|_| rise > 0) {
            let at = height(pair[0]).min(height(pair[1])) - 1;
            if at > z.1 - rise - 1 {
                r.block([pair[0], back - 1, at], [pair[1], back, at + 1], RIM);
            }
        }
    }
    let climber = match r.f.second {
        n @ 1..=5 => n,
        _ => 1 + ((r.set.more * 7.0) as i32 % 4),
    };
    if climber < 5 && r.f.books > 0 {
        climb(r, (x, z), back - 2, &uprights, &slats, climber);
    }
}

/// The climber: stems from the soil up the lattice, and leaves and
/// flowers beside them, in the two layers in front of the rails.
///
/// A stem goes up an upright, and now and then turns along a rail to the
/// next upright and goes on up from there, as a shoot tied in to a
/// trellis does. It stops at a height that the coverage sets, lower for
/// some stems than others, so the top of the lattice is bare on a young
/// plant and nearly reached on an old one. Leaves are single bricks one
/// or two cells long, set beside a stem's cells or in front of them,
/// thickly near the soil and thinly at the stem's end. Every leaf is
/// against a stem's cell or a leaf that is, so none hangs in the air, and
/// each cell is taken once (the cells taken are kept as they are laid),
/// so none shares room.
fn climb(r: &mut Room, (x, z): ((i32, i32), (i32, i32)), layer: i32, uprights: &[i32], slats: &HashSet<(i32, i32)>, kind: i32) {
    let cover = r.f.books.clamp(0, 100) as f64 / 100.0;
    let mut numbers = Numbers(r.f.seed ^ 0xC11B_E255_7E11_15ED);
    // (every cell of the two layers that something is in: across, up, and which layer)
    let mut taken: HashSet<(i32, i32, i32)> = HashSet::new();
    let put = |r: &mut Room, taken: &mut HashSet<(i32, i32, i32)>, cells: &[(i32, i32)], front: i32, shade: u8| {
        if cells.iter().any(|c| c.0 < x.0 || c.0 >= x.1 || c.1 < z.0 || c.1 >= z.1 || taken.contains(&(c.0, c.1, front))) {
            return false;
        }
        let (lo, hi) = ((cells[0].0.min(cells[cells.len() - 1].0), cells[0].1.min(cells[cells.len() - 1].1)), (cells[0].0.max(cells[cells.len() - 1].0), cells[0].1.max(cells[cells.len() - 1].1)));
        r.block([lo.0, layer - front, lo.1], [hi.0 + 1, layer - front + 1, hi.1 + 1], shade);
        taken.extend(cells.iter().map(|c| (c.0, c.1, front)));
        true
    };
    // one stem for every dozen cells of width, more on an old plant, each from the soil at an upright
    let stems = (((x.1 - x.0) as f64 / 12.0 * (0.5 + 2.0 * cover)).round() as usize).clamp(1, uprights.len());
    let mut walked: Vec<(i32, i32, f64)> = Vec::new();
    for n in 0..stems {
        let mut at = uprights[(uprights.len() * (2 * n + 1) / (2 * stems)).min(uprights.len() - 1)];
        // how high this stem gets: most of the way up at full coverage, and never all of it
        let reach = z.0 + (((z.1 - z.0) as f64 * (0.3 + 0.68 * cover) * (0.8 + 0.2 * numbers.unit())) as i32).max(3);
        let mut up = z.0;
        while up < reach.min(z.1 - 1) {
            // up a few cells, as one brick
            let run = (2 + (numbers.unit() * 5.0) as i32).min(reach - up).max(1);
            let cells: Vec<(i32, i32)> = (up..up + run).map(|cz| (at, cz)).collect();
            if !put(r, &mut taken, &cells, 0, STEM) {
                break;
            }
            walked.extend(cells.iter().map(|c| (c.0, c.1, (c.1 - z.0) as f64 / (reach - z.0).max(1) as f64)));
            up += run;
            // and sometimes along a rail to the upright beside it, where there is a rail here to follow
            let side = if numbers.unit() < 0.5 { -1 } else { 1 };
            let next = uprights.iter().position(|u| *u == at).and_then(|i| uprights.get((i as i32 + side).max(0) as usize)).copied().filter(|u| *u != at);
            if let Some(to) = next.filter(|_| numbers.unit() < 0.45 && up < reach && slats.contains(&(at + side, up))) {
                let (a, b) = (at.min(to), at.max(to));
                let along: Vec<(i32, i32)> = (a..=b).filter(|cx| *cx != at).map(|cx| (cx, up)).collect();
                if !along.is_empty() && !taken.contains(&(at, up, 0)) && put(r, &mut taken, &along, 0, STEM) {
                    // (and the cell of the turn itself, so the stem is one piece)
                    put(r, &mut taken, &[(at, up)], 0, STEM);
                    walked.extend(along.iter().map(|c| (c.0, c.1, (up - z.0) as f64 / (reach - z.0).max(1) as f64)));
                    (at, up) = (to, up + 1);
                }
            }
        }
    }
    // Leaves: at each cell a stem went through, some of the time (most of the time low down, seldom at its tip), a leaf beside it or in front of it.
    let flowers = [0.0, 0.14, 0.2, 0.0][(kind - 1).clamp(0, 3) as usize] * (0.5 + cover);
    for (cx, cz, along) in walked.clone() {
        for _ in 0..3 {
            if numbers.unit() > (0.2 + 0.75 * cover) * (1.0 - 0.5 * along) {
                continue;
            }
            let side = if numbers.unit() < 0.5 { -1 } else { 1 };
            let long = numbers.unit() < 0.4;
            let green = if numbers.unit() < 0.35 { LEAF_DARK } else { LEAF };
            // a flower in place of a leaf, on the kinds that flower: a clematis's of one colour, a rose's of two
            let bloom = numbers.unit() < flowers;
            let shade = if bloom { if kind == 3 && numbers.unit() < 0.5 { FLOWER_TOO } else { FLOWER } } else { green };
            let (where_, front) = if numbers.unit() < 0.6 { ((cx + side, cz), 0) } else { ((cx, cz), 1) };
            let cells = if long && !bloom { vec![where_, (where_.0 + side, where_.1)] } else { vec![where_] };
            // and on an old plant a second leaf over or under the first, which is what closes the gaps between stems
            if put(r, &mut taken, &cells, front, shade) && numbers.unit() < cover * cover {
                let over = if numbers.unit() < 0.5 { -1 } else { 1 };
                let last = cells[cells.len() - 1];
                put(r, &mut taken, &[(last.0, last.1 + over)], front, if green == LEAF { LEAF_DARK } else { LEAF });
            }
        }
    }
    // Wisteria: trusses hanging from where its stems run along a rail, each a column a cell wide and a few long, paler at its tip.
    if kind == 4 {
        for (cx, cz, _) in walked {
            let long = 2 + (numbers.unit() * 3.0) as i32;
            if numbers.unit() < 0.3 + 0.3 * cover && taken.contains(&(cx, cz, 0)) && cz - long > z.0 {
                let cells: Vec<(i32, i32)> = (cz - long + 1..=cz).map(|c| (cx, c)).collect();
                if cells.iter().all(|c| !taken.contains(&(c.0, c.1, 1))) {
                    put(r, &mut taken, &cells[1..], 1, FLOWER);
                    put(r, &mut taken, &[cells[0]], 1, FLOWER_TOO);
                }
            }
        }
    }
}
