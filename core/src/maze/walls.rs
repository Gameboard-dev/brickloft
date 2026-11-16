//! A maze's bricks: its walls, its floor and the strip that marks the way
//! through.
//!
//! The walls are one flat outline, every ring wall and spoke of it
//! together, fitted by the outline fitter as a circle's band or an arch
//! is (`fit_planar`) and pushed up to the walls' height. Fitted together,
//! a spoke and the ring wall it meets are one piece with no seam, and a
//! ring wall is long sloped bricks all the way round. The ground is
//! fitted a square of it at a time, each on a thread of its own (see
//! [`Walls::fitted`]).
//!
//! What the walls are made of only colours that fit, or replaces it:
//!
//! - plain: the fit as it stands, in one colour;
//! - a tile pattern (bricks, cobblestone, planks, medallions): the same
//!   fit, cut into courses and coloured by the pattern wrapped along each
//!   wall (see [`Walls::patterned`]);
//! - hedge: no fit of the outline at all, but the hedge of
//!   [`crate::tree`] grown along every wall's line.

use super::{Layout, Maze, LEAST_HEDGE, USUAL_HEIGHT};
use crate::fit_planar::{fit_planar, Fit, Opts, Piece};
use crate::par::Threads;
use crate::pattern::Mosaic;
use crate::sdf::Stroke;
use crate::shapes::{CellBrick, CUBE};
use crate::tree::{Lay, Model, Tree};
use glam::{DVec2, DVec3};
use std::sync::Arc;

/// The colour number of the floor, and of whatever of a wall's pattern is
/// its ground: the mortar between bricks, the stems of a hedge.
pub const FLOOR: u8 = 0;
/// The colour number of a plain wall.
pub const WALL: u8 = 1;
/// The colour number of the strip that marks the way through a maze of
/// hedge or of patterned walls: the last there is. A wall's pattern never
/// has it: its own seventh number is given the first's colour (see
/// [`sheet`]).
pub const SOLUTION: u8 = 7;
/// The colour number of the strip in a maze of plain walls, which then
/// needs three colours and not eight: the floor's, the walls' and this.
/// (With three colours the last number of eight would share the walls'
/// colour, see `pattern::colour_of`, and the strip would not show.)
pub const PLAIN_SOLUTION: u8 = 2;

/// The most hedge a maze may be, as the length of its walls times their
/// thickness times their height, in cells cubed.
///
/// A hedge is bricks all over its surface and through its tufts: measured
/// on straight hedges from 6 to 24 thick and 12 to 36 tall, between a
/// tenth and a quarter of a brick to each cell of that volume. A maze of
/// 6 rings and radius 120 with hedges 6 thick and 30 tall is 300,000
/// cells of it: 55,000 bricks, built in two and a half seconds on one
/// thread and half a second on sixteen. This much hedge is therefore
/// 100,000 to 250,000 bricks, and eight seconds on one thread or under
/// two on sixteen. Past it the build is refused with what to change,
/// where a larger one would have stopped the editor for a minute and
/// made a prefab the game is slow to place.
pub const MOST_HEDGE: f64 = 1_000_000.0;

/// The most tiles the walls of a patterned maze may unroll to: their
/// length times their height. Each tile is drawn by the pattern's rule,
/// at up to four microseconds for cobblestone, so this is a few seconds
/// at the worst.
pub const MOST_TILES: usize = 1_500_000;

/// The longest the walls of any maze may be, all together, in cells.
/// The outline fitter's time goes by the length of outline it follows:
/// measured, a maze of 16 rings and radius 320, with 14,000 cells of
/// wall, is fitted in a second and a half on one thread. This much wall
/// is therefore four or five seconds on one thread and under one on
/// eight, and some 20,000 bricks before any pattern cuts them up.
pub const MOST_WALL: usize = 40_000;

/// How many cells each way a square of ground fitted on its own is (see
/// [`Walls::fitted`]).
///
/// Measured on mazes of 6, 12 and 16 rings with corridors 20 wide: at 96
/// the whole fit was quickest or near it (1.4 seconds for the largest,
/// against 2.1 at 64, 2.5 at 192 and 17.7 as one outline) and laid 4%
/// more pieces than one outline did.
const TILE: i32 = 96;

/// How many of a patterned wall's pieces are worth a thread of their own
/// (see `par::Threads::map_ranges`): a piece is a lookup for each of its
/// cells and each cell of its height, about a microsecond in all for a
/// sloped piece and ten for a block.
const PIECES_A_THREAD: usize = 200;

/// A maze ready to be built: its layout, and how tall it stands.
pub struct Walls {
    pub layout: Layout,
    /// the cell its floor, or with no floor its walls, start in
    low: i32,
    floor: i32,
    height: i32,
    /// the hedge its walls are, if they are hedge
    hedge: Option<Arc<Model>>,
    /// the strip that marks the way through, if it is marked
    strip: Option<Stroke>,
    /// the cells of the ground it covers
    min: [i32; 2],
    max: [i32; 2],
}

impl Walls {
    /// The maze `maze` describes, standing on the floor of the cell
    /// `centre` is in, `radius` to the middle of its outside wall, with
    /// walls `thick` through and `height` tall (the usual height if none
    /// is given). `tree` is how its hedge is grown if its walls are hedge,
    /// and `patterned` whether a pattern is to be wrapped along them.
    ///
    /// Refused, with what to change, if it is more than may be built.
    pub fn new(maze: &Maze, centre: DVec3, radius: f64, thick: f64, height: Option<f64>, tree: &Tree, patterned: bool) -> Result<Walls, String> {
        let maze = maze.clamped();
        let thick = if maze.hedge { thick.max(LEAST_HEDGE) } else { thick };
        let mut height = height.unwrap_or(USUAL_HEIGHT).round().max(1.0);
        if maze.hedge {
            height = height.max(Tree::least_height(crate::tree::Kind::Hedge));
        }
        let layout = Layout::new(&maze, centre.truncate(), radius, thick)?;
        let lines = layout.lines();
        let long: f64 = lines.iter().map(|(line, _)| line.windows(2).map(|pair| pair[0].distance(pair[1])).sum::<f64>()).sum();
        let fewer = "give it fewer rings, or make it smaller";
        if long > MOST_WALL as f64 {
            return Err(format!("the maze is too large to build: its walls are {long:.0} microbricks long in all, and the limit is {MOST_WALL}; {fewer}"));
        }
        if patterned && !maze.hedge && layout.long() * height as usize > MOST_TILES {
            return Err(format!("the maze is too large to build with a texture on its walls: they are {} tiles, and the limit is {MOST_TILES}; {fewer}, or its walls lower", layout.long() * height as usize));
        }
        let reach = layout.reach() + 2.0;
        let (lo, hi) = (layout.centre() - reach, layout.centre() + reach);
        let (min, max) = ([lo.x.floor() as i32, lo.y.floor() as i32], [hi.x.ceil() as i32, hi.y.ceil() as i32]);
        let low = centre.z.floor() as i32;
        let foot = DVec3::new(layout.centre().x, layout.centre().y, (low + maze.floor) as f64);
        let hedge = if maze.hedge {
            if long * thick * height > MOST_HEDGE {
                return Err(format!("the maze is too much hedge to build: {:.0} microbricks of wall, {thick} thick and {height} tall, is {:.0} cubic microbricks of hedge, and the limit is {MOST_HEDGE}; {fewer}, or its walls thinner or lower", long, long * thick * height));
            }
            // (every wall its own line, from the maze's middle as a plant's line is from its foot)
            let lays: Vec<Lay> = lines.iter().map(|(line, closed)| Lay { run: line.iter().map(|p| (*p - layout.centre()).extend(0.0)).collect(), closed: *closed, mounds: Vec::new() }).collect();
            // as many plants as a hedge may be, shared among the walls by their length
            let grown = Tree { kind: crate::tree::Kind::Hedge, count: Tree::count_range(crate::tree::Kind::Hedge).1, ..*tree };
            Some(Arc::new(Model::laid_all(&grown, foot, height, thick / 2.0, &lays)))
        } else {
            None
        };
        // The strip: a quarter as wide as the narrowest gap it goes
        // through, a cell at the least and six at the most, so that it
        // stays clear of the walls either side of every gap.
        let strip = maze.solution.then(|| {
            let half = ((layout.narrowest() / 4.0).round() / 2.0).clamp(0.5, 3.0);
            let line: Vec<DVec3> = layout.route().into_iter().map(|p| p.extend(0.0)).collect();
            Stroke::new(&line, half, false, Some(0.0))
        });
        Ok(Walls { layout, low, floor: maze.floor, height: height as i32, hedge, strip, min, max })
    }

    /// The box the maze fills, in cells.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        let (lo, hi) = (DVec3::new(self.min[0] as f64, self.min[1] as f64, self.low as f64), DVec3::new(self.max[0] as f64, self.max[1] as f64, (self.low + self.floor + self.height) as f64));
        match &self.hedge {
            Some(hedge) => (lo.min(hedge.bounds().0), hi.max(hedge.bounds().1)),
            None => (lo, hi),
        }
    }

    /// The walls' outline fitted: the ground cut into squares, each fitted
    /// as an outline of its own, and the pieces put end to end in the
    /// squares' order.
    ///
    /// ## Why not one outline
    ///
    /// The outline fitter searches each stretch of an outline for its
    /// corners, and the walls of a maze are one outline or a few, a
    /// hundred times as long as a ring's and joined at every spoke. Its
    /// time rose faster than the outline's length: a maze of 12 rings
    /// was fitted in 1.3 seconds and one of 13 in 10.8. Cut into squares
    /// of [`TILE`] cells, no outline is longer than a square holds, the
    /// time goes by the area, and each square is its own piece of work
    /// for a thread (see [`crate::par`]): the squares do not depend on
    /// one another, and their order is fixed by where they are.
    ///
    /// ## What the cuts cost
    ///
    /// A cut is along lattice lines, so it is exact: the pieces either
    /// side of it are whole cells there and fill the wall between them
    /// with no gap. A sloped brick cannot cross a cut, so a wall is two
    /// bricks where it crosses one and might have been one, and beside a
    /// cut the fitter may choose its corners differently. Measured, 4%
    /// more pieces and 3.5% of the walls' area amiss against 3.4%.
    ///
    /// The squares are set out from the maze's middle, half a square to
    /// each side of it, so that no cut runs along the spokes that lie on
    /// the axes. For a grid the squares are an even number of cells of
    /// the maze across, which puts every cut down the middle of a row of
    /// cells and never along a wall.
    fn fitted(&self, opts: &Opts) -> Fit {
        let tile = match self.layout.kind {
            super::Kind::Theta => TILE,
            super::Kind::Grid => {
                let gap = self.layout.gap().round().max(1.0) as i32;
                gap * (2 * ((TILE as f64 / 2.0 / gap as f64).round() as i32)).max(2)
            }
        };
        let middle = self.layout.centre();
        let from = [(middle.x - tile as f64 / 2.0).floor() as i32, (middle.y - tile as f64 / 2.0).floor() as i32];
        // the squares that have any of the ground in them, row by row
        let first = [0, 1].map(|k| (self.min[k] - from[k]).div_euclid(tile));
        let last = [0, 1].map(|k| (self.max[k] - 1 - from[k]).div_euclid(tile));
        let tiles: Vec<([i32; 2], [i32; 2])> = (first[1]..=last[1])
            .flat_map(|n| (first[0]..=last[0]).map(move |m| [m, n]))
            .map(|at| ([0, 1].map(|k| (from[k] + at[k] * tile).max(self.min[k])), [0, 1].map(|k| (from[k] + (at[k] + 1) * tile).min(self.max[k]))))
            .collect();
        // (each square on one thread: a square is tens of milliseconds, far more than a thread costs to start, so every square is worth one)
        let alone = Opts { threads: Threads::ONE, ..*opts };
        let fits = opts.threads.map(tiles.len(), 1, |n| fit_planar(&|x, y| self.layout.wall(DVec2::new(x, y)), tiles[n].0, tiles[n].1, &alone));
        let mut whole = Fit::default();
        // (the areas added up here, in the squares' order: a sum of fractions is not the same in every grouping)
        for fit in fits {
            whole.pieces.extend(fit.pieces);
            whole.solid += fit.solid;
            whole.error += fit.error;
        }
        whole
    }

    /// Negative inside the maze's walls, floor or strip; near a face, no
    /// more than the distance to it.
    pub fn distance(&self, p: DVec3) -> f64 {
        let flat = p.truncate();
        let slab = |d: f64, from: i32, deep: i32| d.max(from as f64 - p.z).max(p.z - (from + deep) as f64);
        let top = self.low + self.floor;
        let mut best = match &self.hedge {
            Some(hedge) => hedge.eval(p),
            None => slab(self.layout.wall(flat), top, self.height),
        };
        if self.floor > 0 {
            best = best.min(slab(self.layout.ground(flat), self.low, self.floor));
        }
        if let Some(strip) = &self.strip {
            best = best.min(slab(strip.eval(flat.extend(0.0)), top, 1));
        }
        best
    }

    /// The maze's bricks, each with its colour number, and the volume of
    /// the maze and of where the bricks and the maze disagree.
    ///
    /// `mosaic` is the pattern on its walls' faces, if they have one;
    /// hedge walls take no notice of it. The bricks are the same on any
    /// number of threads: the threads are given whole squares of the
    /// walls to fit, the cells of a hedge to look at and the tiles of a
    /// pattern to draw, and each of those gives its results in the order
    /// one thread would (see [`crate::par`]).
    pub fn built(&self, mosaic: Option<&Mosaic>, wedges: bool, max_ext: i32, threads: Threads) -> (Vec<CellBrick>, Vec<u8>, f64, f64) {
        let opts = Opts { brick_cost: 0.0, max_ext, wedges, threads };
        let (mut bricks, mut shades) = (Vec::new(), Vec::new());
        let (mut solid, mut error) = (0.0, 0.0);
        let top = self.low + self.floor;
        // an outline pushed up through `deep` cells from `from`, in one colour
        let mut lay = |fit: &Fit, from: i32, deep: i32, shade: u8| {
            for piece in &fit.pieces {
                let mut at = 0;
                while at < deep {
                    let run = (deep - at).min(max_ext.max(1));
                    bricks.push(CellBrick { mask: prism(piece.mask), min: [piece.x, piece.y, from + at], ext: [piece.w, piece.h, run] });
                    shades.push(shade);
                    at += run;
                }
            }
            solid += fit.solid * deep as f64;
            error += fit.error * deep as f64;
        };
        if self.floor > 0 {
            lay(&fit_planar(&|x, y| self.layout.ground(DVec2::new(x, y)), self.min, self.max, &opts), self.low, self.floor, FLOOR);
        }
        // (the strip before the walls, which may be laid another way: the order of a maze's bricks is floor, strip, walls)
        let mosaic = mosaic.filter(|mosaic| wrapped(mosaic));
        if let Some(strip) = &self.strip {
            let shade = if self.hedge.is_some() || mosaic.is_some() { SOLUTION } else { PLAIN_SOLUTION };
            lay(&fit_planar(&|x, y| strip.eval(DVec3::new(x, y, 0.0)), self.min, self.max, &opts), top, 1, shade);
        }
        match &self.hedge {
            Some(hedge) => {
                let (lo, hi) = hedge.bounds();
                let (min, max) = ((lo - 1.0).floor().to_array().map(|v| v as i32), (hi + 1.0).ceil().to_array().map(|v| v as i32));
                let fit = crate::fit_volume::fit_volume(&|p| hedge.eval(p), min, max, &crate::fit_volume::Opts { max_ext, wedges, stretch: true, threads });
                let (grown, colours) = hedge.colour(fit.bricks, max_ext);
                bricks.extend(grown);
                // (a hedge has nothing of the last number, which is the strip's)
                shades.extend(colours.into_iter().map(|shade| if shade == SOLUTION { WALL } else { shade }));
                solid += fit.solid;
                error += fit.error;
            }
            None => {
                let fit = self.fitted(&opts);
                match mosaic {
                    Some(mosaic) => {
                        for (brick, shade) in self.patterned(&fit, mosaic, &opts) {
                            bricks.push(brick);
                            shades.push(shade);
                        }
                        solid += fit.solid * self.height as f64;
                        error += fit.error * self.height as f64;
                    }
                    None => lay(&fit, top, self.height, WALL),
                }
            }
        }
        (bricks, shades, solid, error)
    }

    /// The walls' fit, cut into courses and coloured by a pattern wrapped
    /// along every wall.
    ///
    /// ## What is done
    ///
    /// The pattern is drawn once, flat, by the code that draws it on a
    /// floor (`pattern::lay_mosaic_on`), on a sheet as long as all the
    /// walls unrolled end to end and as tall as they stand. Every piece
    /// of the fit is then a column from the ground up, and each cell of
    /// its height takes the sheet's colour at that height and at the
    /// place along its wall the piece lies ([`Layout::along`]). A block
    /// is taken a cell of the ground at a time, so a joint between two
    /// bricks is a cell wide and goes through the wall where the pattern
    /// has it. A sloped piece is one brick across its slope and takes the
    /// colour where the middle of its solid half lies, as a sloped brick
    /// at the edge of a patterned floor does; but where that place is on
    /// a joint a cell wide, it takes the colour of the brick beside the
    /// joint. A sloped piece is often a dozen cells long, and one that
    /// happened to have its middle on a joint was a slab of mortar that
    /// long in the wall's face. Cells of one colour are then joined into
    /// bricks again.
    ///
    /// So a course of bricks runs along a wall and bends round a ring
    /// with it, and the line between two courses is level and unbroken
    /// all the way round, because every piece is cut at the same heights.
    ///
    /// ## What it is not
    ///
    /// The fit is not changed: each piece keeps its outline and is only
    /// cut across its height. The curved faces of a ring wall are long
    /// sloped bricks, each of one colour to a course, so the upright
    /// joints of the pattern show on a curved face only where one sloped
    /// brick ends and the next begins. Cutting them where the pattern
    /// has its joints would have needed the sloped bricks shortened to a
    /// brick's length, which fits a curve less closely: the form would
    /// change its outline when a pattern was chosen (see
    /// `pattern::lay_mosaic` on the same choice for a floor's edge).
    ///
    /// The pass that patterns the sides of a box (`pattern_side_faces` in
    /// `spec.rs`) was not used. It takes the form's outermost layer of
    /// whole cells on each of its four sides, and a maze's faces are
    /// inside it, hundreds of them, and curved.
    fn patterned(&self, fit: &Fit, mosaic: &Mosaic, opts: &Opts) -> Vec<(CellBrick, u8)> {
        let (long, tall) = (self.layout.long().max(1), self.height.max(1) as usize);
        let sheet = sheet(mosaic, long, tall, opts);
        let top = self.low + self.floor;
        let limit = opts.max_ext.max(1) as usize;
        // the sheet's colour at a place, and for a sloped piece the colour beside a joint it lies on: the nearest within two cells along that is not the ground's
        let shade = |along: usize, z: usize, sloped: bool| {
            let here = sheet[along * tall + z];
            if here != FLOOR || !sloped {
                return here;
            }
            [1, -1, 2, -2].into_iter().filter_map(|by: isize| along.checked_add_signed(by).filter(|s| *s < long)).map(|s| sheet[s * tall + z]).find(|shade| *shade != FLOOR).unwrap_or(FLOOR)
        };
        // a column of the sheet as bricks: a run of cells of one colour is one brick
        let column = |mask: u8, x: i32, y: i32, w: i32, h: i32, along: usize, out: &mut Vec<(CellBrick, u8)>| {
            let mut from = 0;
            while from < tall {
                let first = shade(along, from, mask != CUBE);
                let run = (from..tall).take(limit).take_while(|z| shade(along, *z, mask != CUBE) == first).count();
                out.push((CellBrick { mask, min: [x, y, top + from as i32], ext: [w, h, run as i32] }, first));
                from += run;
            }
        };
        let parts = opts.threads.map_ranges(fit.pieces.len(), PIECES_A_THREAD, |range| {
            let mut out = Vec::new();
            for piece in &fit.pieces[range] {
                if piece.mask == 15 {
                    for y in piece.y..piece.y + piece.h {
                        for x in piece.x..piece.x + piece.w {
                            column(CUBE, x, y, 1, 1, self.layout.along(DVec2::new(x as f64 + 0.5, y as f64 + 0.5)), &mut out);
                        }
                    }
                } else {
                    column(prism(piece.mask), piece.x, piece.y, piece.w, piece.h, self.layout.along(solid_middle(piece)), &mut out);
                }
            }
            out
        });
        crate::pattern::joined(parts.into_iter().flatten().collect(), opts.max_ext.max(1))
    }
}

/// Whether a pattern is one that is wrapped along a maze's walls. A rug
/// is not: it is one figure with a middle and a border, made for one
/// floor, and cut into the strips of a hundred walls it is none.
pub fn wrapped(mosaic: &Mosaic) -> bool {
    crate::pattern::STYLES.get(mosaic.style).is_some_and(|style| style.name != "rug")
}

/// A flat piece of an outline as the shape of the brick it is when pushed
/// straight up: each corner of the box filled if the corner under it is.
fn prism(mask: u8) -> u8 {
    (0..8u8).fold(0u8, |m, c| m | (mask >> (c & 3) & 1) << c)
}

/// The middle of the solid half of a sloped piece: a third of the way in
/// from its square corner, each way.
fn solid_middle(piece: &Piece) -> DVec2 {
    let missing = (!piece.mask & 15).trailing_zeros();
    DVec2::new(piece.x as f64 + piece.w as f64 * if missing & 1 == 1 { 1.0 / 3.0 } else { 2.0 / 3.0 }, piece.y as f64 + piece.h as f64 * if missing & 2 == 2 { 1.0 / 3.0 } else { 2.0 / 3.0 })
}

/// A pattern drawn flat on a sheet `long` cells by `tall`: the colour
/// number of each cell, a column of the sheet after another.
///
/// It is drawn by laying the pattern on a floor of that size
/// (`pattern::lay_mosaic_on`) and reading the bricks' colours back, so
/// that a wall's bricks are the very ones a floor's are, from the same
/// settings. Asking the pattern's rule for each cell directly would have
/// been less work at run time, but the rules are private to
/// `pattern.rs`, each with its own way of setting itself out, and a
/// second way into them here would be one more thing to keep in step.
///
/// A tile the pattern cuts along a diagonal (the slanting edge of a
/// cobblestone) is two sloped bricks of two colours in one cell. A cell
/// of a wall's face is one colour, so it takes the first of the two.
fn sheet(mosaic: &Mosaic, long: usize, tall: usize, opts: &Opts) -> Vec<u8> {
    const UNSET: u8 = u8::MAX;
    // (rows in order, as the pattern's own callers give them)
    let cells: Vec<(i32, i32)> = (0..tall as i32).flat_map(|z| (0..long as i32).map(move |s| (s, z))).collect();
    // A wall's face has no border of its own and no bevel: a border is the
    // edge of a floor, and a bevel takes a cell off a brick's top, which
    // on a wall's face would be the wall's shape changed.
    let flat = Mosaic { bevel: false, border: 0, ..*mosaic };
    let (mut bricks, mut shades) = (Vec::new(), Vec::new());
    // (laid across y, so that the sheet's two ways are x and z)
    crate::pattern::lay_mosaic_on(&cells, &flat, 1, 0, 1, opts, &mut bricks, &mut shades);
    let mut sheet = vec![UNSET; long * tall];
    for (brick, shade) in bricks.iter().zip(shades) {
        // (the last number is the strip's: a pattern's own seventh feature shares the first's colour)
        let shade = if shade >= SOLUTION { WALL } else { shade };
        for s in brick.min[0].max(0) as usize..((brick.min[0] + brick.ext[0]).max(0) as usize).min(long) {
            for z in brick.min[2].max(0) as usize..((brick.min[2] + brick.ext[2]).max(0) as usize).min(tall) {
                let cell = &mut sheet[s * tall + z];
                if brick.mask == CUBE || *cell == UNSET {
                    *cell = shade;
                }
            }
        }
    }
    for cell in &mut sheet {
        if *cell == UNSET {
            *cell = FLOOR;
        }
    }
    sheet
}

/// The height a maze of this kind and these walls is built to (see [`Walls::new`]).
pub fn height_of(maze: &Maze, height: Option<f64>) -> f64 {
    let tall = height.unwrap_or(USUAL_HEIGHT).round().max(1.0);
    if maze.hedge { tall.max(Tree::least_height(crate::tree::Kind::Hedge)) } else { tall }
}
