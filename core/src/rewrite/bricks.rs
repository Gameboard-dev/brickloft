//! A finished grid written out as bricks, plain or with wedges.
//!
//! Plain: each cell that is not the grid's first value (its background)
//! is a cube of so many microbricks, and cubes of one value are joined
//! into the largest boxes (`pattern::joined`).
//!
//! With wedges: the same, and then the grid is read for staircases. A
//! rule program can only say "a cell is this value", so its stairs and
//! roofs are steps one cell high, and Brickadia has the sloped pieces
//! that such steps stand for. Wherever there is a flight of two steps or
//! more, the notch above each step is filled with a ramp, so the flight
//! becomes one slope a player walks up without jumping. Where two flights
//! meet in a corner the notch takes an inner corner piece (a valley) or a
//! hip piece (the corner of a pyramid). The underside of a flight is
//! closed the same way with the same pieces upside down.
//!
//! Nothing is taken away: every cell of the grid is still there as a
//! box, and the wedges are added in cells that were empty. That is the
//! choice that needs defending. The other reading cuts the step itself
//! down to a ramp, which gives a thinner flight; but it removes cells the
//! program put there, some of which carry a colour that means something
//! (a landing, a window sill), and it cannot be told from the grid alone
//! which cells are only steps. Filling the notch changes no cell and can
//! be judged by one rule: is this empty cell the notch of a flight?
//!
//! A flight is asked for, not a single step, because a single step is
//! also the edge of every slab lying on a floor, and those are meant
//! square. This rule knows nothing of the program that made the grid, so
//! it serves an imported program as it serves the ones that come with the
//! editor.

use super::fault::{within, Fault, Limits, Thing};
use super::nodes::View;
use crate::shapes::{CellBrick, CUBE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// cells as boxes and nothing else
    Plain,
    /// boxes, and sloped pieces in the notches of stairs
    Wedged,
}

/// The most microbricks a cell may be across.
pub const MOST_SCALE: i32 = 20;

pub struct Bricks {
    pub bricks: Vec<CellBrick>,
    /// for each brick, the value of the grid it is made of: a number into the grid's letters
    pub shades: Vec<u8>,
    /// how many of the bricks are sloped pieces
    pub wedges: usize,
}

const SIDES: [[i64; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];

/// The shape that keeps the corners of its box `keep` says, each given as 0 or 1 along x, y and z (see `shapes`).
fn mask_of(keep: impl Fn(i64, i64, i64) -> bool) -> u8 {
    (0..8u8).filter(|c| keep((c & 1) as i64, (c >> 1 & 1) as i64, (c >> 2) as i64)).fold(0, |m, c| m | 1 << c)
}

/// What an empty cell is to be filled with, if anything.
#[derive(Clone, Copy, PartialEq)]
enum Notch {
    None,
    /// a ramp rising towards one side (a number into `SIDES`)
    Ramp(usize),
    /// a valley between two sides
    Inner(usize, usize),
}

pub fn bricks(view: &View, scale: i32, finish: Finish, longest: i32, limits: &Limits) -> Result<Bricks, Fault> {
    within(Thing::Scale, scale.max(0) as u64, MOST_SCALE as u64, "", 0)?;
    bricks_at(view, scale, finish, longest, limits)
}

/// [`bricks`], at any scale: for a grid drawn larger than its cells are
/// asked to be, as the plan of a building is before the building is made
/// (see `form::built`), which is as large as the building and no larger.
pub fn bricks_at(view: &View, scale: i32, finish: Finish, longest: i32, limits: &Limits) -> Result<Bricks, Fault> {
    let scale = scale.max(1);
    let [mx, my, mz] = view.size.map(|s| s as i64);
    let value = |x: i64, y: i64, z: i64| if x < 0 || y < 0 || z < 0 || x >= mx || y >= my || z >= mz { 0 } else { view.cells[(x + y * mx + z * mx * my) as usize] };
    let solid = |x: i64, y: i64, z: i64| value(x, y, z) != 0;
    let mut loose: Vec<(CellBrick, u8)> = Vec::new();
    // the cells a long ramp has taken into itself (see below): not built as cubes as well
    let mut taken = vec![false; view.cells.len()];
    let at = |x: i64, y: i64, z: i64| (x + y * mx + z * mx * my) as usize;
    let mut wedges = 0;
    if finish == Finish::Wedged {
        // once for the tops of flights (`up` 1: a step is held from below) and once for their undersides (`up` -1)
        for up in [1i64, -1] {
            // A side is a step if it is solid beside the cell and open past it, with the cell open on its far side and past itself.
            let notch = |x: i64, y: i64, z: i64| -> Notch {
                if solid(x, y, z) || !solid(x, y, z - up) || solid(x, y, z + up) {
                    return Notch::None;
                }
                let step = |side: usize| {
                    let [dx, dy] = SIDES[side];
                    solid(x + dx, y + dy, z) && !solid(x + dx, y + dy, z + up) && !solid(x - dx, y - dy, z)
                };
                let found: Vec<usize> = (0..4).filter(|side| step(*side)).collect();
                match found[..] {
                    [side] => Notch::Ramp(side),
                    [a, b] => Notch::Inner(a, b),
                    _ => Notch::None,
                }
            };
            // Whether a cell is a notch of any kind: open, held from below, and with a step beside it or diagonally from it.
            let stepped = |x: i64, y: i64, z: i64| {
                !solid(x, y, z) && solid(x, y, z - up) && !solid(x, y, z + up) && (notch(x, y, z) != Notch::None || [[1, 1], [1, -1], [-1, 1], [-1, -1]].iter().any(|[dx, dy]| solid(x + dx, y + dy, z) && !solid(x + dx, y + dy, z + up)))
            };
            // A ramp belongs to a flight if the cell one step up or one step down the same way is a notch too.
            // (Any notch, not only a ramp: at the corner of a pyramid the next step up from a ramp is a corner piece.)
            let flight = |x: i64, y: i64, z: i64, side: usize| {
                let [dx, dy] = SIDES[side];
                notch(x, y, z) == Notch::Ramp(side) && (stepped(x + dx, y + dy, z + up) || stepped(x - dx, y - dy, z - up))
            };
            let low = if up == 1 { 0 } else { 1 };
            let toward = |side: usize, cx: i64, cy: i64| (SIDES[side][0] == 1 && cx == 1) || (SIDES[side][0] == -1 && cx == 0) || (SIDES[side][1] == 1 && cy == 1) || (SIDES[side][1] == -1 && cy == 0);
            // A flight's ramps, laid a row at a time from the foot of the
            // flight, and several rows as one brick where they can be.
            //
            // A row is the ramps side by side across a flight, on one
            // step. Rows one above the next up a flight, with the cells of
            // the steps under them, are one long ramp: a ramp over three
            // steps that stand on solid cells of their own colour is the
            // same solid as three ramps and the three cells beneath them,
            // in one brick for six. So a row takes the rows above it into
            // itself for as long as each is the same width in the same
            // place and stands, all the way down to the foot of the run,
            // on cells that are there, of the ramp's colour, and not taken
            // by another ramp. What is built then fills exactly what it
            // filled. A flight one cell thick has nothing under its
            // second step but that step, and still goes two steps to a
            // ramp. (A ramp to a step, a staircase of n steps was 2n
            // bricks and is 2.)
            //
            // By rows of one width, and not by each column of ramps for
            // itself: the side of a stepped pyramid is rows that shorten
            // as they rise, and taken by columns its middle became long
            // ramps and its ends short ones that join to nothing, twenty
            // bricks where twelve rows had been twelve.
            let mut ramps: Vec<(i64, i64, i64, usize)> = Vec::new();
            for z in 0..mz {
                for y in 0..my {
                    for x in 0..mx {
                        if let Notch::Ramp(side) = notch(x, y, z) {
                            if flight(x, y, z, side) {
                                ramps.push((x, y, z, side));
                            }
                        }
                    }
                }
            }
            // (from the foot of each flight upwards, which for an underside is from the top down)
            ramps.sort_by_key(|(x, y, z, _)| (z * up, *y, *x));
            let mut laid = vec![false; view.cells.len()];
            let most = (longest.max(1) / scale).max(1) as i64;
            for (x, y, z, side) in ramps.iter().copied() {
                if laid[at(x, y, z)] {
                    continue;
                }
                let ([dx, dy], shade) = (SIDES[side], value(x, y, z - up));
                // across the flight
                let (px, py) = (dy.abs(), dx.abs());
                let inside = |x: i64, y: i64, z: i64| x >= 0 && y >= 0 && z >= 0 && x < mx && y < my && z < mz;
                let ramp = |x: i64, y: i64, z: i64, laid: &[bool]| inside(x, y, z) && !laid[at(x, y, z)] && value(x, y, z - up) == shade && notch(x, y, z) == Notch::Ramp(side) && flight(x, y, z, side);
                // (the row from this ramp on: the ramps are met in order across, so those before it in its row are laid already)
                let (x0, y0) = (x, y);
                let width = ((0..).take_while(|w| ramp(x0 + w * px, y0 + w * py, z, &laid)).count() as i64).min(most);
                let mut length = 1;
                while length < most {
                    let (rx, ry, rz) = (x0 + length * dx, y0 + length * dy, z + length * up);
                    let same = (0..width).all(|w| ramp(rx + w * px, ry + w * py, rz, &laid)) && !ramp(rx - px, ry - py, rz, &laid) && !ramp(rx + width * px, ry + width * py, rz, &laid);
                    let stands = || (0..width).all(|w| (1..=length).all(|j| value(rx + w * px, ry + w * py, rz - j * up) == shade && !taken[at(rx + w * px, ry + w * py, rz - j * up)]));
                    if !same || !stands() {
                        break;
                    }
                    length += 1;
                }
                for i in 0..length {
                    for w in 0..width {
                        let (cx, cy, cz) = (x0 + w * px + i * dx, y0 + w * py + i * dy, z + i * up);
                        laid[at(cx, cy, cz)] = true;
                        for j in 1..=i {
                            taken[at(cx, cy, cz - j * up)] = true;
                        }
                    }
                }
                // the box round them: from the first ramp of the lowest row to the last of the highest
                let (ex, ey, ez) = (x0 + (width - 1) * px + (length - 1) * dx, y0 + (width - 1) * py + (length - 1) * dy, z + (length - 1) * up);
                let min = [x0.min(ex) as i32 * scale, y0.min(ey) as i32 * scale, z.min(ez) as i32 * scale];
                let ext = [((x0 - ex).abs() + 1) as i32 * scale, ((y0 - ey).abs() + 1) as i32 * scale, length as i32 * scale];
                loose.push((CellBrick { mask: mask_of(|cx, cy, cz| cz == low || toward(side, cx, cy)), min, ext }, shade));
                wedges += 1;
            }
            for z in 0..mz {
                for y in 0..my {
                    for x in 0..mx {
                        let mask = match notch(x, y, z) {
                            // (a flight's ramps are laid already, a row or several rows at a time: see above)
                            Notch::Ramp(side) if flight(x, y, z, side) => None,
                            // a valley, where the flights on its two open sides are flights: each rises towards the other's step
                            Notch::Inner(a, b) if flight(x - SIDES[a][0], y - SIDES[a][1], z, b) || flight(x - SIDES[b][0], y - SIDES[b][1], z, a) => {
                                // (an inner corner's mask is the corners the pyramid it lacks does not touch: see `shapes`)
                                Some(mask_of(|cx, cy, cz| cz == low && (toward(a, cx, cy) || toward(b, cx, cy))))
                            }
                            // the corner of a pyramid: nothing beside it, a step diagonally, and a flight on each side of it
                            Notch::None if !solid(x, y, z) && solid(x, y, z - up) && !solid(x, y, z + up) => (0..2).flat_map(|a| (2..4).map(move |b| (a, b))).find_map(|(a, b)| {
                                let ([ax, ay], [bx, by]) = (SIDES[a], SIDES[b]);
                                let diagonal = solid(x + ax + bx, y + ay + by, z) && !solid(x + ax + bx, y + ay + by, z + up);
                                (diagonal && flight(x + ax, y + ay, z, b) && flight(x + bx, y + by, z, a)).then(|| mask_of(|cx, cy, cz| cz == low || (toward(a, cx, cy) && toward(b, cx, cy))))
                            }),
                            _ => None,
                        };
                        if let Some(mask) = mask {
                            loose.push((CellBrick { mask, min: [x as i32 * scale, y as i32 * scale, z as i32 * scale], ext: [scale; 3] }, value(x, y, z - up)));
                            wedges += 1;
                        }
                    }
                }
            }
        }
    }
    // The cells, a run along x at a time. Joining puts cells into rows
    // along x first (`pattern::joined`), by sorting every cell; handed
    // rows, it has a tenth as many things to sort, and joins them into
    // the same slabs and blocks. Writing the grid out is most of what a
    // frame of a run being watched costs, and CarmaTower's grid part way
    // through its run, which is nearly all solid cells, took 10 ms a
    // frame a cell at a time and takes 3.6 so (measured: see
    // `how_long_a_frame_of_a_run_takes` in the editor's `state/grow.rs`).
    // (A row is no longer than the longest brick, as joining keeps it.)
    let most = (longest.max(1) / scale).max(1) as i64;
    for z in 0..mz {
        for y in 0..my {
            let mut x = 0;
            while x < mx {
                let v = value(x, y, z);
                if v == 0 || taken[at(x, y, z)] {
                    x += 1;
                    continue;
                }
                let run = (1..most.min(mx - x)).take_while(|k| value(x + k, y, z) == v && !taken[at(x + k, y, z)]).count() as i64 + 1;
                loose.push((CellBrick { mask: CUBE, min: [x as i32 * scale, y as i32 * scale, z as i32 * scale], ext: [run as i32 * scale, scale, scale] }, v));
                x += run;
            }
        }
    }
    let joined = crate::pattern::joined(loose, longest.max(1));
    within(Thing::Bricks, joined.len() as u64, limits.bricks, "", 0)?;
    let wedges = if wedges == 0 { 0 } else { joined.iter().filter(|(brick, _)| brick.mask != CUBE).count() };
    let (bricks, shades) = joined.into_iter().unzip();
    Ok(Bricks { bricks, shades, wedges })
}
