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
    for z in 0..mz {
        for y in 0..my {
            for x in 0..mx {
                let v = value(x, y, z);
                if v != 0 {
                    loose.push((CellBrick { mask: CUBE, min: [x as i32 * scale, y as i32 * scale, z as i32 * scale], ext: [scale; 3] }, v));
                }
            }
        }
    }
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
            for z in 0..mz {
                for y in 0..my {
                    for x in 0..mx {
                        let toward = |side: usize, cx: i64, cy: i64| (SIDES[side][0] == 1 && cx == 1) || (SIDES[side][0] == -1 && cx == 0) || (SIDES[side][1] == 1 && cy == 1) || (SIDES[side][1] == -1 && cy == 0);
                        let mask = match notch(x, y, z) {
                            Notch::Ramp(side) if flight(x, y, z, side) => Some(mask_of(|cx, cy, cz| cz == low || toward(side, cx, cy))),
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
    let joined = crate::pattern::joined(loose, longest.max(1));
    within(Thing::Bricks, joined.len() as u64, limits.bricks, "", 0)?;
    let wedges = if wedges == 0 { 0 } else { joined.iter().filter(|(brick, _)| brick.mask != CUBE).count() };
    let (bricks, shades) = joined.into_iter().unzip();
    Ok(Bricks { bricks, shades, wedges })
}
