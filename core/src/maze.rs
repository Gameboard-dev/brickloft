//! A maze: its cells, the walls between them, which walls are taken out,
//! and the way through.
//!
//! Two kinds are laid out here. A theta maze is rings of cells about a
//! round room, the walls arcs of circles and straight spokes. A grid maze
//! is square cells in rows, about a square room. Both are the same thing
//! underneath (cells, and wall slots that each stand between two cells or
//! between a cell and the outside), so one routine carves both, and what
//! differs is only where the walls are.
//!
//! This file knows nothing of bricks. It answers three questions about a
//! place on the ground: how far it is from the nearest wall
//! ([`Layout::wall`]), which the outline fitter asks; where it lies along
//! that wall ([`Layout::along`]), which a pattern of bricks or stones laid
//! on the walls' faces asks; and which lines the walls run along
//! ([`Layout::lines`]), which a hedge is grown along. [`walls`] turns the
//! answers into bricks.
//!
//! The maze this replaces drew its walls into a picture, a pixel to a
//! stud, walked the picture's black pixels and cut what it found into
//! rectangles and triangles. Here the walls are never a picture: a ring
//! wall is the band between two circles and a spoke a rectangle, as
//! formulas, and the fitter that lays a circle or an arch lays them, with
//! its long sloped bricks along every curve.

use crate::pattern::Numbers;
use glam::DVec2;
use std::f64::consts::{FRAC_PI_2, TAU};

pub mod walls;

/// Which maze.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// rings of cells about a round room
    Theta,
    /// square cells about a square room
    Grid,
}

pub const KINDS: [(Kind, &str); 2] = [(Kind::Theta, "theta"), (Kind::Grid, "grid")];

/// How a maze is made: the settings a form carries for it. Its size is the
/// form's `radius` (to the middle of its outside wall), its walls are the
/// form's `thick` through and its `rise` tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Maze {
    pub kind: Kind,
    /// How many rings, the room in the middle counted as the first. The
    /// corridors are the radius divided by this wide, wall and all.
    pub rings: i32,
    /// theta: how many cells the first ring round the room is divided into
    pub divisions: i32,
    /// Theta: how the number of cells grows outwards. 0 doubles it
    /// wherever the cells would otherwise be wider than they are deep;
    /// any other number doubles it every that many rings.
    pub grow: i32,
    /// which maze: any number, each a different one
    pub seed: u64,
    /// where on the rim the way in is, in degrees round from the x axis
    pub entrance: i32,
    /// how many cells thick the floor under it is; 0 for none
    pub floor: i32,
    /// whether the way through is marked, as a strip along the ground
    pub solution: bool,
    /// whether the walls are hedge (see [`crate::tree`]) and not built
    pub hedge: bool,
}

impl Default for Maze {
    fn default() -> Self {
        Maze { kind: Kind::Theta, rings: 6, divisions: 4, grow: 0, seed: 1, entrance: 270, floor: 0, solution: false, hedge: false }
    }
}

/// The fewest and the most rings. Two is a room and one ring round it.
/// Forty rings at the largest radius there is are corridors 25 cells wide,
/// about what a player walks down, so more rings than that could only be
/// narrower than a player.
pub const RINGS: (i32, i32) = (2, 40);
/// The most cells the first ring may be asked to have.
pub const MOST_DIVISIONS: i32 = 64;
/// The most rings between two doublings that may be asked for.
pub const MOST_GROW: i32 = 8;
/// The thickest a floor may be.
pub const MOST_FLOOR: i32 = 8;
/// The height of a wall when none is given, in cells: a little over a
/// player's own, so that the maze cannot be seen over.
pub const USUAL_HEIGHT: f64 = 30.0;
/// The radius and wall thickness a maze starts at.
pub const USUAL_SIZE: (f64, f64) = (120.0, 4.0);
/// The least open width a corridor, or a cell between two spokes, may
/// have, in cells. Under this the walls on either side are within a cell
/// or two of touching and the fit of one runs into the other.
pub const LEAST_PASSAGE: f64 = 3.0;
/// The thinnest a hedge wall may be: twice the least spread a hedge has
/// (see [`crate::tree::LEAST_SPREAD`]).
pub const LEAST_HEDGE: f64 = 2.0 * crate::tree::LEAST_SPREAD;

impl Maze {
    /// The same settings, each within what it may be: a maze read from a
    /// file or sent by another program may say anything.
    pub fn clamped(self) -> Maze {
        Maze { rings: self.rings.clamp(RINGS.0, RINGS.1), divisions: self.divisions.clamp(1, MOST_DIVISIONS), grow: self.grow.clamp(0, MOST_GROW), entrance: self.entrance.rem_euclid(360), floor: self.floor.clamp(0, MOST_FLOOR), ..self }
    }
}

/// A maze laid out on the ground: where its walls are, which stand, and
/// the way through.
///
/// A theta maze of `rings` rings has ring walls at the radii `gap`,
/// `2 gap`, and so on out to `rings * gap`, which is the rim. Ring 0 is
/// the room inside the first wall. Ring `k` lies between walls `k` and
/// `k + 1` and is `counts[k]` cells, cell `d` between the angles `d` and
/// `d + 1` of that many equal parts of a turn. A cell has an arc of wall
/// on its inner side, one on the rim if it is in the outermost ring, and a
/// spoke at its far end round the ring.
///
/// A grid maze is `side` cells each way, `gap` apart, with walls on the
/// lines between them.
#[derive(Clone, Debug)]
pub struct Layout {
    kind: Kind,
    /// the middle of the maze
    centre: DVec2,
    /// half a wall's thickness
    half: f64,
    rings: usize,
    /// theta: from one ring wall to the next; grid: from one line of walls to the next
    gap: f64,
    /// theta: how many cells each ring is (`counts[0]` is 1, the room)
    counts: Vec<usize>,
    /// theta: the number of each ring's first cell
    first: Vec<usize>,
    /// theta: the slot of each ring wall's first arc (`arcs[j - 1]` for wall `j`), and of each ring's first spoke
    arcs: Vec<usize>,
    spokes: Vec<usize>,
    /// grid: how many cells each way
    side: usize,
    /// whether each wall slot still has its wall
    standing: Vec<bool>,
    /// the cells of the way through, from the one inside the entrance to the room
    solution: Vec<usize>,
    /// where the way in crosses the rim, for the strip that marks the way through
    door: DVec2,
    /// where along the unrolled walls each ring wall's and each ring's spokes' part begins (see [`Layout::along`])
    strips: Vec<usize>,
    long: usize,
}

/// A point as its distance from the middle and its angle round it, from 0 up to a whole turn.
fn polar(q: DVec2) -> (f64, f64) {
    let angle = q.y.atan2(q.x);
    (q.length(), if angle < 0.0 { angle + TAU } else { angle })
}

/// How far a point is outside the box from `(low, -half)` to `(high, half)`: negative inside.
fn bar(along: f64, across: f64, low: f64, high: f64, half: f64) -> f64 {
    let (a, b) = ((low - along).max(along - high), across.abs() - half);
    if a > 0.0 && b > 0.0 { a.hypot(b) } else { a.max(b) }
}

impl Layout {
    /// Lay out the maze `maze` describes about `centre`, `radius` to the
    /// middle of its outside wall, with walls `thick` through.
    ///
    /// Refused, with what to change, if its corridors would be too narrow
    /// to walk or to build.
    pub fn new(maze: &Maze, centre: DVec2, radius: f64, thick: f64) -> Result<Layout, String> {
        let maze = maze.clamped();
        let rings = maze.rings as usize;
        if !(radius.is_finite() && thick.is_finite() && radius > 0.0 && thick > 0.0) {
            return Err("a maze needs a size and a wall thickness".into());
        }
        let thick = thick.round().max(1.0);
        let half = thick / 2.0;
        // A wall an even number of cells through has its middle on a
        // lattice line and an odd one down the middle of a row of cells,
        // as a band's is (see `spec::planar`): the spokes along the axes,
        // and every wall of a grid, are then whole cells. The middle is
        // moved half a cell towards the origin where it is not so.
        let on_lattice = |at: f64| if ((at + half) - (at + half).round()).abs() > 0.25 { at - 0.5 } else { at };
        let mut layout = Layout { kind: maze.kind, centre, half, rings, gap: 0.0, counts: Vec::new(), first: Vec::new(), arcs: Vec::new(), spokes: Vec::new(), side: 0, standing: Vec::new(), solution: Vec::new(), door: DVec2::ZERO, strips: Vec::new(), long: 0 };
        let too_narrow = |open: f64| format!("the maze's corridors would be {open:.1} microbricks wide between their walls, and the least is {LEAST_PASSAGE}: make it larger, its walls thinner, or give it fewer rings");
        let turn = (maze.entrance as f64).to_radians();
        let (entrance, goal, way_in);
        match maze.kind {
            Kind::Theta => {
                layout.centre = DVec2::new(on_lattice(centre.x), on_lattice(centre.y));
                let gap = radius / rings as f64;
                if gap - thick < LEAST_PASSAGE {
                    return Err(too_narrow(gap - thick));
                }
                layout.gap = gap;
                // How many cells each ring has. A ring's cells are as wide
                // as its inner wall is long divided among them, so with one
                // number for every ring the cells would be slivers by the
                // room and halls at the rim. The number doubles where the
                // rule says, and only doubles: every wall of a ring then
                // lines up with a wall of the ring outside it, and a cell
                // has one cell inside it and one or two outside.
                // (The adaptive rule is the one in Jamis Buck, "Mazes for
                // Programmers", 2015, ch. 7, for a polar grid, in the form
                // that only ever doubles.)
                let open = |k: usize, n: usize| TAU * k as f64 * gap / n as f64 - thick;
                let mut counts = vec![1usize];
                let mut n = (maze.divisions as usize).max(1);
                while n > 1 && open(1, n) < LEAST_PASSAGE {
                    n -= 1;
                }
                counts.push(n);
                for k in 2..rings {
                    let twice = 2 * counts[k - 1];
                    let wanted = match maze.grow {
                        // doubled, its cells are still as wide as the corridor is
                        0 => TAU * k as f64 * gap / twice as f64 >= gap,
                        every => k % every as usize == 0,
                    };
                    counts.push(if wanted && open(k, twice) >= LEAST_PASSAGE { twice } else { counts[k - 1] });
                }
                layout.first = counts.iter().scan(0, |at, n| Some(std::mem::replace(at, *at + n))).collect();
                let mut slots = 0;
                for j in 1..=rings {
                    layout.arcs.push(slots);
                    slots += counts[j.min(rings - 1)];
                }
                layout.spokes.push(slots); // (the room has none)
                for count in &counts[1..] {
                    layout.spokes.push(slots);
                    slots += count;
                }
                layout.standing = vec![true; slots];
                layout.counts = counts;
                // The way in: the cell of the outermost ring the angle falls in.
                let outer = layout.counts[rings - 1];
                let d = ((turn / (TAU / outer as f64)) as usize).min(outer - 1);
                (entrance, goal, way_in) = (layout.first[rings - 1] + d, 0, layout.arc(rings, d));
                let middle = (d as f64 + 0.5) * TAU / outer as f64;
                layout.door = layout.centre + DVec2::from_angle(middle) * (radius + half);
            }
            Kind::Grid => {
                // The room is a cell, with `rings - 1` cells to each side of it.
                let side = 2 * rings - 1;
                let gap = (radius / (rings as f64 - 0.5)).round();
                if gap - thick < LEAST_PASSAGE {
                    return Err(too_narrow(gap - thick));
                }
                (layout.gap, layout.side) = (gap, side);
                // (its lowest corner is what must be on the lattice, a whole number of cells from every line of walls)
                let low = centre - DVec2::splat(side as f64 * gap / 2.0);
                layout.centre = DVec2::new(on_lattice(low.x), on_lattice(low.y)) + DVec2::splat(side as f64 * gap / 2.0);
                layout.standing = vec![true; 2 * side * (side + 1)];
                // The way in: where a line from the middle at the angle
                // leaves the square, and the cell just inside there.
                let way = DVec2::from_angle(turn);
                let reach = side as f64 * gap / 2.0;
                let out = way * (reach / way.x.abs().max(way.y.abs()));
                let cell = |v: f64| (((v + reach) / gap) as usize).min(side - 1);
                let (i, j) = (cell(out.x), cell(out.y));
                let (at, slot) = if way.x.abs() >= way.y.abs() {
                    (DVec2::new(out.x.signum() * (reach + half), (j as f64 + 0.5) * gap - reach), layout.upright(if out.x > 0.0 { side } else { 0 }, j))
                } else {
                    (DVec2::new((i as f64 + 0.5) * gap - reach, out.y.signum() * (reach + half)), layout.level(i, if out.y > 0.0 { side } else { 0 }))
                };
                (entrance, goal, way_in) = (i + side * j, (side / 2) * (side + 1), slot);
                layout.door = layout.centre + at;
            }
        }
        let between = layout.between();
        let cells = match maze.kind {
            Kind::Theta => layout.counts.iter().sum(),
            Kind::Grid => layout.side * layout.side,
        };
        let (open, solution) = carve(&between, cells, entrance, goal, maze.seed);
        for (slot, open) in open.into_iter().enumerate() {
            layout.standing[slot] = !open;
        }
        layout.standing[way_in] = false;
        layout.solution = solution;
        // where each part of the walls begins along the roll they are all unrolled into (see `along`)
        let mut at = 0;
        match maze.kind {
            Kind::Theta => {
                for j in 1..=rings {
                    layout.strips.push(at);
                    at += (TAU * j as f64 * layout.gap).ceil() as usize;
                }
                for k in 1..rings {
                    layout.strips.push(at);
                    at += layout.counts[k] * layout.spoke_long();
                }
            }
            Kind::Grid => at = 2 * (layout.side + 1) * layout.spoke_long(),
        }
        layout.long = at;
        Ok(layout)
    }

    /// theta: the slot of the arc of ring wall `j` (from 1, the room's, to `rings`, the rim) that is cell `d`'s of the ring outside it, or for the rim of the ring inside it
    fn arc(&self, j: usize, d: usize) -> usize {
        self.arcs[j - 1] + d
    }

    /// theta: how many arcs ring wall `j` is
    fn arc_count(&self, j: usize) -> usize {
        self.counts[j.min(self.rings - 1)]
    }

    /// theta: the slot of the spoke at the far end of cell `d` of ring `k`
    fn spoke(&self, k: usize, d: usize) -> usize {
        self.spokes[k] + d
    }

    /// grid: the slot of the wall on upright line `i` (0 to `side`) beside row `j`
    fn upright(&self, i: usize, j: usize) -> usize {
        i + (self.side + 1) * j
    }

    /// grid: the slot of the wall on level line `j` (0 to `side`) under or over column `i`
    fn level(&self, i: usize, j: usize) -> usize {
        self.side * (self.side + 1) + i + self.side * j
    }

    /// How long a spoke's part of the unrolled walls is, or for a grid a whole line's.
    fn spoke_long(&self) -> usize {
        match self.kind {
            Kind::Theta => (self.gap + 2.0 * self.half).ceil() as usize + 1,
            Kind::Grid => (self.side as f64 * self.gap + 2.0 * self.half).ceil() as usize + 1,
        }
    }

    /// For each wall slot, the cells it stands between: `None` for the
    /// outside, and for a wall with the same cell on both sides (the one
    /// spoke of a ring that is a single cell), which is never taken out.
    fn between(&self) -> Vec<Option<(usize, usize)>> {
        let mut all = vec![None; self.standing.len()];
        match self.kind {
            Kind::Theta => {
                for j in 1..self.rings {
                    let (outer, inner) = (self.counts[j], self.counts[j - 1]);
                    for d in 0..outer {
                        all[self.arc(j, d)] = Some((self.first[j] + d, self.first[j - 1] + d * inner / outer));
                    }
                }
                for k in 1..self.rings {
                    let n = self.counts[k];
                    for d in 0..n {
                        all[self.spoke(k, d)] = (n > 1).then_some((self.first[k] + d, self.first[k] + (d + 1) % n));
                    }
                }
            }
            Kind::Grid => {
                let side = self.side;
                for j in 0..side {
                    for i in 1..side {
                        all[self.upright(i, j)] = Some((i - 1 + side * j, i + side * j));
                        all[self.level(j, i)] = Some((j + side * (i - 1), j + side * i));
                    }
                }
            }
        }
        all
    }

    /// How many cells the maze has, the room among them.
    pub fn cells(&self) -> usize {
        match self.kind {
            Kind::Theta => self.counts.iter().sum(),
            Kind::Grid => self.side * self.side,
        }
    }

    /// The cells either side of each wall that has been taken out, the way in apart.
    pub fn doors(&self) -> Vec<(usize, usize)> {
        self.between().into_iter().zip(&self.standing).filter(|(_, standing)| !**standing).filter_map(|(cells, _)| cells).collect()
    }

    /// The cells of the way through, from the one inside the entrance to the room, which is [`Layout::goal`].
    pub fn solution(&self) -> &[usize] {
        &self.solution
    }

    pub fn goal(&self) -> usize {
        *self.solution.last().unwrap_or(&0)
    }

    /// theta: how many cells each ring is, from the room outwards
    pub fn counts(&self) -> &[usize] {
        &self.counts
    }

    /// From one ring wall to the next, or one line of a grid's walls to the next.
    pub fn gap(&self) -> f64 {
        self.gap
    }

    /// The middle of the maze as it is built, which is half a cell from where it was asked for if its walls would otherwise lie across cells.
    pub fn centre(&self) -> DVec2 {
        self.centre
    }

    /// How far out from the middle the maze reaches along x and y, wall and all.
    pub fn reach(&self) -> f64 {
        match self.kind {
            Kind::Theta => self.rings as f64 * self.gap + self.half,
            Kind::Grid => self.side as f64 * self.gap / 2.0 + self.half,
        }
    }

    /// How far `p` is from the nearest wall: negative inside one. Near a
    /// wall's face it is no more than the distance to it; further off it
    /// is only known to be far.
    ///
    /// Each wall is a formula, and the walls near a place are found by
    /// arithmetic: the nearest ring wall from the distance to the middle,
    /// the nearest spoke from the angle. The other choice was the walls as
    /// polylines in a search grid, as a spline's band is (`sdf::Stroke`),
    /// which would have made the circles into chords and each wall a
    /// solid of its own with rounded joints to the next.
    pub fn wall(&self, p: DVec2) -> f64 {
        let q = p - self.centre;
        match self.kind {
            Kind::Theta => self.theta_wall(q),
            Kind::Grid => self.grid_wall(q),
        }
    }

    fn theta_wall(&self, q: DVec2) -> f64 {
        let (r, angle) = polar(q);
        let (gap, half, rings) = (self.gap, self.half, self.rings);
        let mut best = f64::INFINITY;
        // The nearest ring wall. Corridors are at least `LEAST_PASSAGE`
        // wide, so no other is within a cell and a half of a place this
        // one is not nearer to.
        let j = ((r / gap).round() as usize).clamp(1, rings);
        let across = (r - j as f64 * gap).abs() - half;
        if across < 2.0 {
            let m = self.arc_count(j);
            let slot = TAU / m as f64;
            let d = ((angle / slot) as usize).min(m - 1);
            // how far round from each end of the arc the place is, as a length square to the end
            let chord = |turn: f64| if turn < FRAC_PI_2 { r * turn.sin() } else { r };
            let (from, to) = (chord(angle - d as f64 * slot), chord((d + 1) as f64 * slot - angle));
            let (before, after) = (self.standing[self.arc(j, (d + m - 1) % m)], self.standing[self.arc(j, (d + 1) % m)]);
            if self.standing[self.arc(j, d)] {
                // (an end is an end only where the next arc is gone: two arcs side by side are one wall with no seam)
                best = across.max(if before { f64::NEG_INFINITY } else { -from }).max(if after { f64::NEG_INFINITY } else { -to });
            } else {
                if before {
                    best = best.min(across.max(0.0).hypot(from));
                }
                if after {
                    best = best.min(across.max(0.0).hypot(to));
                }
            }
        }
        // The nearest spoke of the ring the place is in, and of the ring
        // beside it where it is near that one: a spoke runs on half a
        // wall's thickness past each ring wall it meets, so that it closes
        // against the wall's far face and not its middle.
        let k = (r / gap) as usize;
        let near = half + 2.0;
        for ring in [Some(k), (r - k as f64 * gap < near && k > 0).then(|| k - 1), ((k + 1) as f64 * gap - r < near).then_some(k + 1)].into_iter().flatten() {
            if ring == 0 || ring >= rings {
                continue;
            }
            let m = self.counts[ring];
            let slot = TAU / m as f64;
            let nearest = (angle / slot).round();
            if !self.standing[self.spoke(ring, (nearest as usize + m - 1) % m)] {
                continue;
            }
            let off = angle - nearest * slot;
            best = best.min(bar(r * off.cos(), r * off.sin(), ring as f64 * gap - half, (ring + 1) as f64 * gap + half, half));
        }
        best
    }

    fn grid_wall(&self, q: DVec2) -> f64 {
        let (gap, half, side) = (self.gap, self.half, self.side);
        // from the lowest corner, where line 0 each way is
        let p = q + DVec2::splat(side as f64 * gap / 2.0);
        let mut best = f64::INFINITY;
        // each way in turn: the nearest line of walls, and its walls beside the place and one along each way
        for upright in [true, false] {
            let (across, along) = if upright { (p.x, p.y) } else { (p.y, p.x) };
            let line = ((across / gap).round().max(0.0) as usize).min(side);
            let off = across - line as f64 * gap;
            if off.abs() - half >= 2.0 {
                continue;
            }
            let row = (along / gap).floor();
            for n in [row - 1.0, row, row + 1.0] {
                if n < 0.0 || n >= side as f64 {
                    continue;
                }
                let slot = if upright { self.upright(line, n as usize) } else { self.level(n as usize, line) };
                if self.standing[slot] {
                    // (run on half a thickness at each end, so that two walls meeting at a corner fill it)
                    best = best.min(bar(along, off, n * gap - half, (n + 1.0) * gap + half, half));
                }
            }
        }
        best
    }

    /// How far `p` is outside the ground the maze stands on: the disc or
    /// the square out to the outside face of its outside wall.
    pub fn ground(&self, p: DVec2) -> f64 {
        let q = p - self.centre;
        match self.kind {
            Kind::Theta => q.length() - self.reach(),
            Kind::Grid => bar(q.x, q.y, -self.reach(), self.reach(), self.reach()),
        }
    }

    /// How long the walls are when all of them are unrolled end to end into one strip (see [`Layout::along`]).
    pub fn long(&self) -> usize {
        self.long
    }

    /// Where `p`, a place in a wall, lies along the walls unrolled into
    /// one strip: a whole number of cells from the strip's start.
    ///
    /// A pattern laid on a wall's face is drawn flat and wrapped onto it.
    /// Each ring wall is unrolled from the x axis round, a cell of the
    /// strip to a cell of its middle circle, so the same place on the
    /// strip is behind every point of a line through the wall towards the
    /// middle: a course of bricks runs along the wall and bends with it,
    /// and a joint between two bricks goes straight through. A spoke is
    /// unrolled outwards from its inner end, and a grid's wall along its
    /// line. Each has a part of the strip to itself, so no two walls
    /// show the same bricks.
    ///
    /// Where a spoke meets a ring wall the place is the ring wall's, if it
    /// is within the wall's thickness of that circle.
    pub fn along(&self, p: DVec2) -> usize {
        let q = p - self.centre;
        let spoke_long = self.spoke_long();
        match self.kind {
            Kind::Theta => {
                let (r, angle) = polar(q);
                let (gap, rings) = (self.gap, self.rings);
                let j = ((r / gap).round() as usize).clamp(1, rings);
                let m = self.arc_count(j);
                let d = ((angle / (TAU / m as f64)) as usize).min(m - 1);
                let ring = ((r / gap) as usize).clamp(1, rings - 1);
                let n = self.counts[ring];
                let nearest = (angle / (TAU / n as f64)).round();
                let spoke = (nearest as usize + n - 1) % n;
                let on_arc = (r - j as f64 * gap).abs() <= self.half && self.standing[self.arc(j, d)];
                if on_arc || !self.standing[self.spoke(ring, spoke)] {
                    let round = (TAU * j as f64 * gap).ceil() as usize;
                    self.strips[j - 1] + ((angle * j as f64 * gap) as usize).min(round - 1)
                } else {
                    let out = r * (angle - nearest * TAU / n as f64).cos() - (ring as f64 * gap - self.half);
                    self.strips[rings + ring - 1] + spoke * spoke_long + (out.max(0.0) as usize).min(spoke_long - 1)
                }
            }
            Kind::Grid => {
                let (gap, side) = (self.gap, self.side);
                let p = q + DVec2::splat(side as f64 * gap / 2.0);
                let line = |v: f64| ((v / gap).round().max(0.0) as usize).min(side);
                let place = |v: f64| ((v + self.half).max(0.0) as usize).min(spoke_long - 1);
                // (an upright wall's if it is within one, and otherwise a level one's)
                if (p.x - line(p.x) as f64 * gap).abs() <= self.half {
                    line(p.x) * spoke_long + place(p.y)
                } else {
                    (side + 1 + line(p.y)) * spoke_long + place(p.x)
                }
            }
        }
    }

    /// The lines the walls run along, each with whether it comes round to
    /// its own start: every run of arcs standing side by side as one line,
    /// every spoke as one, and for a grid every run of walls along a line.
    /// For whatever is grown or laid along a wall and not fitted to it.
    pub fn lines(&self) -> Vec<(Vec<DVec2>, bool)> {
        let mut all = Vec::new();
        let (gap, half) = (self.gap, self.half);
        match self.kind {
            Kind::Theta => {
                for j in 1..=self.rings {
                    let (m, radius) = (self.arc_count(j), j as f64 * gap);
                    let slot = TAU / m as f64;
                    let stands = |d: usize| self.standing[self.arc(j, d % m)];
                    // (a point every three cells round: the line then leaves the circle by under a tenth of a cell at the smallest ring there may be)
                    let curve = |from: usize, to: usize| {
                        let steps = (((to - from) as f64 * slot * radius / 3.0).ceil() as usize).max(2);
                        (0..=steps).map(|i| self.centre + DVec2::from_angle((from as f64 + (to - from) as f64 * i as f64 / steps as f64) * slot) * radius).collect::<Vec<_>>()
                    };
                    let Some(gone) = (0..m).find(|d| !stands(*d)) else {
                        let mut ring = curve(0, m);
                        ring.pop(); // (a closed line does not repeat its first point)
                        all.push((ring, true));
                        continue;
                    };
                    // from just past a gap, each run of arcs up to the next gap
                    let mut d = gone + 1;
                    while d < gone + m {
                        if !stands(d) {
                            d += 1;
                            continue;
                        }
                        let from = d;
                        while stands(d) {
                            d += 1;
                        }
                        all.push((curve(from, d), false));
                    }
                }
                for k in 1..self.rings {
                    let n = self.counts[k];
                    for d in (0..n).filter(|d| self.standing[self.spoke(k, *d)]) {
                        let way = DVec2::from_angle((d + 1) as f64 * TAU / n as f64);
                        all.push((vec![self.centre + way * (k as f64 * gap - half), self.centre + way * ((k + 1) as f64 * gap + half)], false));
                    }
                }
            }
            Kind::Grid => {
                let side = self.side;
                let low = self.centre - DVec2::splat(side as f64 * gap / 2.0);
                for upright in [true, false] {
                    for line in 0..=side {
                        let stands = |n: usize| n < side && self.standing[if upright { self.upright(line, n) } else { self.level(n, line) }];
                        let mut n = 0;
                        while n < side {
                            if !stands(n) {
                                n += 1;
                                continue;
                            }
                            let from = n;
                            while stands(n) {
                                n += 1;
                            }
                            let at = |along: f64| low + if upright { DVec2::new(line as f64 * gap, along) } else { DVec2::new(along, line as f64 * gap) };
                            all.push((vec![at(from as f64 * gap - half), at(n as f64 * gap + half)], false));
                        }
                    }
                }
            }
        }
        all
    }

    /// The narrowest opening anywhere: between two ring walls, and between two spokes at the inner end of a cell.
    pub fn narrowest(&self) -> f64 {
        let across = self.gap - 2.0 * self.half;
        match self.kind {
            Kind::Theta => (1..self.rings).map(|k| TAU * k as f64 * self.gap / self.counts[k] as f64 - 2.0 * self.half).fold(across, f64::min),
            Kind::Grid => across,
        }
    }

    /// The way through as a line on the ground: from where the way in
    /// crosses the rim, through the middle of each cell on the way, to
    /// the middle of the room.
    ///
    /// In a theta maze it keeps to the middle of each corridor, going
    /// round with the ring, and crosses from one ring to the next along a
    /// line towards the middle of the maze through the middle of the
    /// outer of the two cells. That line is inside the gap the two share,
    /// which is as wide as the outer cell and no wider.
    pub fn route(&self) -> Vec<DVec2> {
        let mut line = vec![self.door];
        match self.kind {
            Kind::Grid => {
                let (gap, side) = (self.gap, self.side);
                let low = self.centre - DVec2::splat(side as f64 * gap / 2.0);
                line.extend(self.solution.iter().map(|cell| low + DVec2::new((cell % side) as f64 + 0.5, (cell / side) as f64 + 0.5) * gap));
            }
            Kind::Theta => {
                // a cell as its ring, and the angle of its middle
                let place = |cell: usize| {
                    let k = self.first.partition_point(|first| *first <= cell) - 1;
                    (k, (cell - self.first[k]) as f64 + 0.5, self.counts[k])
                };
                let at = |k: usize, angle: f64| self.centre + DVec2::from_angle(angle) * ((k as f64 + 0.5) * self.gap);
                // round ring `k` from one angle to another, the shorter way
                let round = |line: &mut Vec<DVec2>, k: usize, from: f64, to: f64| {
                    let turn = (to - from + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0;
                    let steps = ((turn.abs() * (k as f64 + 0.5) * self.gap / 2.0).ceil() as usize).max(1);
                    line.extend((1..=steps).map(|i| at(k, from + turn * i as f64 / steps as f64)));
                };
                let angle_of = |cell: usize| {
                    let (_, d, n) = place(cell);
                    d * TAU / n as f64
                };
                let Some(&start) = self.solution.first() else { return line };
                line.push(at(place(start).0, angle_of(start)));
                for pair in self.solution.windows(2) {
                    let ((from, to), (a, b)) = ((place(pair[0]).0, place(pair[1]).0), (angle_of(pair[0]), angle_of(pair[1])));
                    if to == 0 {
                        line.push(self.centre);
                    } else if to == from {
                        round(&mut line, from, a, b);
                    } else if to > from {
                        round(&mut line, from, a, b);
                        line.push(at(to, b));
                    } else {
                        line.push(at(to, a));
                        round(&mut line, to, a, b);
                    }
                }
            }
        }
        line.dedup_by(|b, a| a.distance(*b) < 1e-9);
        line
    }
}

/// Take walls out until every cell can be reached from every other by one
/// way only: which slots are opened, and the cells of the way from
/// `entrance` to `goal`.
///
/// `between` gives, for each wall slot, the two cells it stands between,
/// or `None` for a wall that is never taken out.
///
/// ## The method
///
/// A walk from the entrance that always steps into a cell not yet visited,
/// chosen at random among those beside it, taking out the wall it steps
/// through, and goes back the way it came when there is none: the
/// "recursive backtracker", a depth-first search of the cells, kept on a
/// list here and not on the call stack, which a large maze would overrun.
/// <https://en.wikipedia.org/wiki/Maze_generation_algorithm#Randomized_depth-first_search>
///
/// Every cell is stepped into once, through one wall, so the opened walls
/// are a spanning tree of the cells: every cell is reached, and between
/// any two there is exactly one way. That much any spanning tree gives.
/// What the walk adds is the shape of the tree. It goes on as far as it
/// can before it turns back, so corridors are long and winding and the
/// branches off them few. The other methods weighed were Wilson's, which
/// draws a tree with every tree equally likely, and Prim's and Kruskal's
/// with random weights. Those give many short dead ends: about three
/// cells in ten of a uniform tree on a square grid are dead ends, against
/// one in ten of this walk's, and the way through is far shorter (see
/// Buck, "Maze Generation: Algorithm Recap", 2011, and the test
/// `corridors_are_long_and_dead_ends_few`, which measures this walk
/// against a tree grown the Prim way on the same cells).
/// <https://weblog.jamisbuck.org/2011/2/7/maze-generation-algorithm-recap>
///
/// ## The room
///
/// The goal is left out of the walk and given one door afterwards. Walked
/// into like any other cell, the room in the middle of a theta maze has
/// every cell of the first ring beside it and the walk would leave it by
/// several: corridors would run through the goal. Its door is put in the
/// cell beside it that is furthest from the entrance by the corridors, so
/// the way through is as long as this maze allows.
fn carve(between: &[Option<(usize, usize)>], cells: usize, entrance: usize, goal: usize, seed: u64) -> (Vec<bool>, Vec<usize>) {
    // for each cell, the slots round it and the cell beyond each: one flat list, as `sdf::PolyDist` keeps its buckets
    let mut start = vec![0usize; cells + 1];
    for (a, b) in between.iter().flatten() {
        start[a + 1] += 1;
        start[b + 1] += 1;
    }
    for cell in 0..cells {
        start[cell + 1] += start[cell];
    }
    let mut beside = vec![(0usize, 0usize); start[cells]];
    let mut next = start.clone();
    for (slot, pair) in between.iter().enumerate() {
        let Some((a, b)) = *pair else { continue };
        beside[next[a]] = (slot, b);
        beside[next[b]] = (slot, a);
        next[a] += 1;
        next[b] += 1;
    }
    let round = |cell: usize| &beside[start[cell]..start[cell + 1]];
    let mut numbers = Numbers(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x3A2E_0001);
    let mut open = vec![false; between.len()];
    let mut visited = vec![false; cells];
    // how each cell was come to, for the way through
    let mut from = vec![usize::MAX; cells];
    (visited[entrance], visited[goal]) = (true, true);
    let mut path = vec![entrance];
    // how many cells along the corridors from the entrance each cell is: the length of the list when it was stepped into
    let mut far = vec![0usize; cells];
    let mut fresh: Vec<(usize, usize)> = Vec::new();
    while let Some(&at) = path.last() {
        fresh.clear();
        fresh.extend(round(at).iter().filter(|(_, cell)| !visited[*cell]));
        if fresh.is_empty() {
            path.pop();
            continue;
        }
        let (slot, cell) = fresh[((numbers.unit() * fresh.len() as f64) as usize).min(fresh.len() - 1)];
        (open[slot], visited[cell], from[cell], far[cell]) = (true, true, at, path.len());
        path.push(cell);
    }
    // the room's one door: in the cell beside it furthest from the entrance (the first of them, of several as far)
    let mut door: Option<(usize, usize)> = None;
    for &(slot, cell) in round(goal) {
        if door.is_none_or(|(_, best)| far[cell] > far[best]) {
            door = Some((slot, cell));
        }
    }
    let mut solution = vec![goal];
    if let Some((slot, cell)) = door {
        open[slot] = true;
        let mut at = cell;
        while at != usize::MAX {
            solution.push(at);
            at = from[at];
        }
    }
    solution.reverse();
    (open, solution)
}

#[cfg(test)]
mod tests;
