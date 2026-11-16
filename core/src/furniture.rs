//! Furniture: forms that are boards and boxes, laid out and not fitted.
//!
//! A piece of furniture is given a box to fill (its width, depth and
//! height in cells, as a cube's are given: see `spec::cube_sides`) and a
//! few settings, and its parts follow from those by rule: so many shelves
//! for the height, so many uprights for the width, each board where a
//! joiner would put it. Every part is a box on the lattice, or one of the
//! game's sloped pieces for a moulding or a leaning book, so the build is
//! the form itself, brick for brick: there is nothing to fit and no
//! discrepancy to measure.
//!
//! It is laid out directly for that reason. The other way was to describe
//! the furniture as a solid (`sdf::Field`) and let the cell-by-cell fitter
//! find its bricks, as a tree's are found. That fitter joins cells into
//! the largest boxes it can, which is the right thing for a trunk and the
//! wrong thing here: two books standing side by side would be joined into
//! one brick wherever their tops were level, and a shelf into the side it
//! is housed in. A book has to be a brick of its own to be a colour of its
//! own, and a board one brick to be a board. Laid out, a library wall is
//! built in a millisecond or two; fitted, it would be millions of cells
//! looked at to find what was known before the looking began.
//!
//! One kind so far, the bookcase. A kind is a function from the box and
//! the settings to parts ([`Lay`]), and the settings ([`Furniture`]) are
//! named for what most furniture has (boards of a thickness, a count, a
//! spacing, a seed), so that a table or a chair is another function and
//! another entry in [`KINDS`], with no change to how a form carries them.

use crate::pattern::Numbers;
use crate::shapes::{shape_info, CellBrick, CUBE};
use glam::DVec3;
use std::sync::OnceLock;

// Each group of kinds in a file of its own, and what they share in two more.
mod bed;
mod lights;
mod parts;
mod pictures;
mod pots;
mod seating;
mod storage;
mod style;
mod tables;
mod trellis;
pub use lights::MOST_LIGHTS;
pub use pictures::{subject, SUBJECTS};
pub use pots::{FLOWER, FLOWER_TOO, LEAF, LEAF_DARK, PLANTS};
pub use trellis::CLIMBERS;
pub use style::{settings, Field, Set, Setting, Style, STYLES};

/// The kinds of furniture there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bookcase,
    Cupboard,
    Drawers,
    Sideboard,
    Kitchen,
    Dining,
    Coffee,
    Side,
    Desk,
    Chair,
    Armchair,
    Stool,
    Bench,
    Sofa,
    TableLamp,
    FloorLamp,
    Pendant,
    Chandelier,
    Sconce,
    Candlestick,
    Bed,
    Potted,
    Basket,
    WindowBox,
    Trellis,
    Painting,
    Portrait,
}

/// Every kind, in the order of [`Kind`], with the name a save or another
/// program calls it by.
pub const KINDS: [(Kind, &str); 27] = [
    (Kind::Bookcase, "bookcase"),
    (Kind::Cupboard, "cupboard"),
    (Kind::Drawers, "drawers"),
    (Kind::Sideboard, "sideboard"),
    (Kind::Kitchen, "kitchen"),
    (Kind::Dining, "dining"),
    (Kind::Coffee, "coffee"),
    (Kind::Side, "side"),
    (Kind::Desk, "desk"),
    (Kind::Chair, "chair"),
    (Kind::Armchair, "armchair"),
    (Kind::Stool, "stool"),
    (Kind::Bench, "bench"),
    (Kind::Sofa, "sofa"),
    (Kind::TableLamp, "tablelamp"),
    (Kind::FloorLamp, "floorlamp"),
    (Kind::Pendant, "pendant"),
    (Kind::Chandelier, "chandelier"),
    (Kind::Sconce, "sconce"),
    (Kind::Candlestick, "candlestick"),
    (Kind::Bed, "bed"),
    (Kind::Potted, "potted"),
    (Kind::Basket, "basket"),
    (Kind::WindowBox, "windowbox"),
    (Kind::Trellis, "trellis"),
    (Kind::Painting, "painting"),
    (Kind::Portrait, "portrait"),
];

/// The kinds in the groups a list of them is shown in, each kind with
/// what it is called there.
pub const GROUPS: [(&str, &[(Kind, &str)]); 7] = [
    ("Storage", &[(Kind::Bookcase, "Bookcase"), (Kind::Cupboard, "Cupboard"), (Kind::Drawers, "Chest of drawers"), (Kind::Sideboard, "Sideboard"), (Kind::Kitchen, "Kitchen cabinet")]),
    ("Tables", &[(Kind::Dining, "Dining table"), (Kind::Coffee, "Coffee table"), (Kind::Side, "Side table"), (Kind::Desk, "Desk")]),
    ("Seating", &[(Kind::Chair, "Chair"), (Kind::Armchair, "Armchair"), (Kind::Stool, "Stool"), (Kind::Bench, "Bench"), (Kind::Sofa, "Sofa")]),
    ("Lights", &[(Kind::TableLamp, "Table lamp"), (Kind::FloorLamp, "Floor lamp"), (Kind::Pendant, "Pendant"), (Kind::Chandelier, "Chandelier"), (Kind::Sconce, "Wall sconce"), (Kind::Candlestick, "Candlestick")]),
    ("Beds", &[(Kind::Bed, "Bed")]),
    ("Plants in pots", &[(Kind::Potted, "Potted plant"), (Kind::Basket, "Hanging basket"), (Kind::WindowBox, "Window box"), (Kind::Trellis, "Trellis with a climber")]),
    ("Pictures", &[(Kind::Painting, "Framed painting"), (Kind::Portrait, "Framed portrait")]),
];

/// The colour numbers of every kind but the bookcase, whose numbers
/// above 1 are its books. They are numbers for what a thing is made of
/// and not for its parts, so that a set of furniture is one set of
/// colours: a table's top and a cupboard's door are the same wood in the
/// same number. [`WOOD`] is 0 for all of them.
/// The second wood, or the paint of a panel, a door or a drawer front.
pub const PANEL: u8 = 1;
/// Metal: handles, a lamp's stem, an industrial frame. Written to the game as its metal.
pub const METAL: u8 = 2;
/// Upholstery, and a bed's blanket.
pub const FABRIC: u8 = 3;
/// A second cloth: loose cushions, pillows.
pub const ACCENT: u8 = 4;
/// A lamp's bulb or a candle's flame. Written to the game as a glowing brick.
pub const LIGHT: u8 = 5;
/// Pale cloth and wax: a lampshade, a mattress, a candle.
pub const LINEN: u8 = 6;
/// A top of another stuff than the piece: a worktop, a slab on a metal frame, the second tone of a plank top.
pub const TOP: u8 = 7;

/// The colour number of the carcass: sides, top, shelves, mouldings.
pub const WOOD: u8 = 0;
/// Of the back panel, a second tone of the wood.
pub const BACK: u8 = 1;
/// The first of the numbers the books and ornaments share, and how many
/// there are of them. With the two woods that is eight, which is as many
/// colours as a form may be painted in (`Built::shades`, and the editor's
/// `paint::MOST_COLOURS`).
pub const FIRST_BOOK: u8 = 2;
pub const BOOK_COLOURS: u8 = 6;

/// The thickest a board may be, in cells, and the tallest a plinth.
pub const MOST_BOARD: i32 = 8;
pub const MOST_PLINTH: i32 = 20;
/// The most shelves that may be asked for by number.
pub const MOST_SHELVES: i32 = 60;
/// The least and most clear height that may be asked for between shelves.
pub const LEAST_SPACING: i32 = 3;
pub const MOST_SPACING: i32 = 200;
/// The narrowest and widest a bay may be asked to be kept to (0 asks for no uprights at all).
pub const LEAST_BAY: i32 = 4;
pub const MOST_BAY: i32 = 400;

/// The most openings (a bay between two shelves) a bookcase may have, and
/// the most books and ornaments. A cube may be asked to be 2,000 cells
/// each way, which with the closest spacing is a wall of four hundred
/// thousand openings and a million books. That would be built, slowly,
/// and then could not be drawn. Past these it is refused with a message
/// that says what to change. They are several times what the largest
/// bookcase the editor's own dots can drag out comes to (800 cells each
/// way at the usual spacing: some 2,000 openings).
pub const MOST_OPENINGS: i64 = 12_000;
pub const MOST_BOOKS: i64 = 60_000;

/// The settings a form carries for its furniture. Its size is the form's
/// own, kept as a cube's is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Furniture {
    pub kind: Kind,
    /// which arrangement of books: any number, each a different one
    pub seed: u64,
    /// how thick its boards are, in cells
    pub board: i32,
    /// How many shelves it has between its bottom and its top, in each
    /// bay; 0 for as many as the height has room for at `spacing`.
    pub shelves: i32,
    /// the clear height wanted between one shelf and the next, in cells
    pub spacing: i32,
    /// The widest a bay may be before an upright is put in to divide it,
    /// in cells; 0 for no uprights however wide it is.
    pub bay: i32,
    /// how far the bottom board is lifted off the floor, on a plinth; 0 for standing on it
    pub plinth: i32,
    /// whether it has a back panel
    pub back: bool,
    /// whether it has a cornice under its top and a skirting round its foot
    pub moulding: bool,
    /// how full of books it is, from 0 (bare shelves) to 100 (every shelf packed)
    pub books: i32,
    /// Which style it is in (see [`Style`]). A bookcase takes no notice
    /// of it: its mouldings and boards have settings of their own, from
    /// before there were styles, and a saved bookcase has to stay as it was.
    pub style: Style,
    /// A number of the kind's own, 0 for as many as its size has room
    /// for: a cupboard's doors, a chest's drawers, a sofa's seats.
    pub count: i32,
    /// A choice of the kind's own, 0 for whatever the style has: a
    /// table's legs, a chair's back, a cupboard's handles, a lamp's shade.
    pub choice: i32,
    /// A second such choice: a table's top, a seat's cushion, what a cupboard stands on.
    pub second: i32,
    /// Something the kind may have or not: a chair's arms, a side table's drawer, a bed's footboard.
    pub extra: bool,
    /// Whether its lights shine in the game, or only glow.
    pub shine: bool,
}

/// The most bricks the box of a piece may come to when cut to the longest brick allowed (see `Model::new`).
pub const MOST_PIECES: i64 = 400_000;

/// The most a kind's own number or choice may be, whatever the kind.
pub const MOST_COUNT: i32 = 12;

impl Furniture {
    /// A kind as it starts out.
    pub fn of(kind: Kind) -> Furniture {
        // (what a kind has unless it is taken off: a side table's drawer, the drawers of a sideboard and of a kitchen's units)
        // (and a picture's mount)
        let extra = matches!(kind, Kind::Side | Kind::Sideboard | Kind::Kitchen | Kind::Painting | Kind::Portrait);
        // (a trellis's `books` is how much of it its climber covers: a plant a few years old, with lattice still to climb)
        let books = if kind == Kind::Trellis { 60 } else { 85 };
        Furniture { kind, seed: 1, board: 2, shelves: 0, spacing: 11, bay: 34, plinth: 3, back: true, moulding: true, books, style: Style::Classical, count: 0, choice: 0, second: 0, extra, shine: true }
    }

    /// Whether it is a plant in a pot: a container with something growing in it.
    pub fn planted(&self) -> bool {
        matches!(self.kind, Kind::Potted | Kind::Basket | Kind::WindowBox | Kind::Trellis)
    }

    /// Whether it is a picture, whose colour numbers are its picture's and not what it is made of.
    pub fn pictured(&self) -> bool {
        matches!(self.kind, Kind::Painting | Kind::Portrait)
    }

    /// The name its sets of colours go by: a bookcase's own, or its
    /// style's (the one its seed picks, for a random style).
    pub fn colours(&self) -> &'static str {
        match (self.kind, self.style.resolved(self.seed)) {
            (Kind::Bookcase, _) => "bookcase",
            // (a picture's colours are its subject's: a sunset's sky is no landscape's)
            (Kind::Painting, _) => ["sunset painting", "landscape painting", "abstract painting"][subject(self)],
            (Kind::Portrait, _) => "portrait",
            // (a plant in a pot has a pot's colours and a plant's, which are other numbers than a table's: see `pots`)
            (_, Style::Modern) if self.planted() => "modern pots",
            (_, Style::Farmhouse) if self.planted() => "farmhouse pots",
            (_, Style::Rustic) if self.planted() => "rustic pots",
            (_, Style::Industrial) if self.planted() => "industrial pots",
            _ if self.planted() => "classical pots",
            (_, Style::Modern) => "modern furniture",
            (_, Style::Farmhouse) => "farmhouse furniture",
            (_, Style::Rustic) => "rustic furniture",
            (_, Style::Industrial) => "industrial furniture",
            _ => "classical furniture",
        }
    }

    /// The size a kind starts at: its width, depth and height in cells.
    /// A figure in the game stands about thirty cells tall, so this is a
    /// bookcase to above head height, in boards a little heavier than
    /// life: at true scale a board is under half a cell.
    ///
    /// The other kinds are at one scale with each other, about four
    /// centimetres to a cell, so that a chair set beside a table fits
    /// under it: a table 19 tall, a seat 11 off the floor. That is one
    /// and a half times life beside a figure, which is what leaves a
    /// chair's leg room to be a leg and its back room for slats.
    pub fn usual_size(kind: Kind) -> [f64; 3] {
        match kind {
            Kind::Bookcase => [36.0, 10.0, 60.0],
            Kind::Cupboard => [30.0, 14.0, 50.0],
            Kind::Drawers => [24.0, 12.0, 26.0],
            Kind::Sideboard => [44.0, 12.0, 22.0],
            Kind::Kitchen => [30.0, 15.0, 23.0],
            Kind::Dining => [44.0, 24.0, 19.0],
            Kind::Coffee => [28.0, 16.0, 10.0],
            Kind::Side => [12.0, 12.0, 14.0],
            Kind::Desk => [36.0, 18.0, 19.0],
            Kind::Chair => [11.0, 11.0, 23.0],
            Kind::Armchair => [22.0, 20.0, 21.0],
            Kind::Stool => [9.0, 9.0, 12.0],
            Kind::Bench => [34.0, 10.0, 11.0],
            Kind::Sofa => [50.0, 21.0, 21.0],
            Kind::TableLamp => [9.0, 9.0, 15.0],
            Kind::FloorLamp => [11.0, 11.0, 40.0],
            Kind::Pendant => [9.0, 9.0, 17.0],
            Kind::Chandelier => [21.0, 21.0, 17.0],
            Kind::Sconce => [5.0, 5.0, 9.0],
            Kind::Candlestick => [5.0, 5.0, 11.0],
            Kind::Bed => [30.0, 52.0, 24.0],
            Kind::Potted => [26.0, 26.0, 46.0],
            Kind::Basket => [28.0, 28.0, 44.0],
            Kind::WindowBox => [52.0, 14.0, 26.0],
            Kind::Trellis => [36.0, 10.0, 56.0],
            Kind::Painting => [48.0, 3.0, 36.0],
            Kind::Portrait => [34.0, 3.0, 44.0],
        }
    }

    /// That size as a form keeps it (see `spec::cube_sides`): half a side,
    /// and how far that is stretched along each axis. The half side is
    /// half the depth, the least of the three sizes. A stretch is not
    /// taken below a twentieth, so with half the height as the half side a
    /// bookcase 240 tall could not be made shallower than 12; from half
    /// its depth each size can be brought down to a cell.
    pub fn usual_form(kind: Kind) -> (f64, DVec3) {
        let [wide, deep, tall] = Furniture::usual_size(kind);
        (deep / 2.0, DVec3::new(wide / deep, 1.0, tall / deep))
    }

    /// The same settings, each within what it may be: furniture read from
    /// a file or sent by another program may say anything.
    pub fn clamped(self) -> Furniture {
        Furniture {
            board: self.board.clamp(1, MOST_BOARD),
            shelves: self.shelves.clamp(0, MOST_SHELVES),
            spacing: self.spacing.clamp(LEAST_SPACING, MOST_SPACING),
            bay: if self.bay <= 0 { 0 } else { self.bay.clamp(LEAST_BAY, MOST_BAY) },
            plinth: self.plinth.clamp(0, MOST_PLINTH),
            books: self.books.clamp(0, 100),
            count: self.count.clamp(0, MOST_COUNT),
            choice: self.choice.clamp(0, MOST_COUNT),
            second: self.second.clamp(0, MOST_COUNT),
            ..self
        }
    }
}

impl Default for Furniture {
    fn default() -> Self {
        Furniture::of(Kind::Bookcase)
    }
}

/// The parts of a piece of furniture as they are laid: the bricks, and
/// the colour number of each.
struct Lay {
    bricks: Vec<CellBrick>,
    shades: Vec<u8>,
    /// the longest a brick may be, along any axis
    longest: i32,
}

impl Lay {
    /// A box from `lo` to `hi`, as one brick if it is no longer than a
    /// brick may be, or else cut into as few as that allows.
    fn block(&mut self, lo: [i32; 3], hi: [i32; 3], shade: u8) {
        if (0..3).any(|k| hi[k] <= lo[k]) {
            return;
        }
        let step = self.longest.max(1) as usize;
        for x in (lo[0]..hi[0]).step_by(step) {
            for y in (lo[1]..hi[1]).step_by(step) {
                for z in (lo[2]..hi[2]).step_by(step) {
                    let min = [x, y, z];
                    self.bricks.push(CellBrick { mask: CUBE, min, ext: [0, 1, 2].map(|k| (hi[k] - min[k]).min(self.longest.max(1))) });
                    self.shades.push(shade);
                }
            }
        }
    }

    /// A sloped piece filling the box from `lo` to `hi` as `mask` says,
    /// cut into lengths along `along` only: a strip of moulding is the
    /// same section all the way, so its lengths are the same piece, where
    /// a cut across the slope would need two other shapes. Whoever asks
    /// sees that its other two sizes are within what a brick may be.
    fn piece(&mut self, mask: u8, lo: [i32; 3], hi: [i32; 3], shade: u8, along: usize) {
        if (0..3).any(|k| hi[k] <= lo[k]) {
            return;
        }
        let step = self.longest.max(1);
        let mut at = lo[along];
        while at < hi[along] {
            let (mut min, mut ext) = (lo, [0, 1, 2].map(|k| hi[k] - lo[k]));
            (min[along], ext[along]) = (at, (hi[along] - at).min(step));
            self.bricks.push(CellBrick { mask, min, ext });
            self.shades.push(shade);
            at += step;
        }
    }
}

/// The shape that keeps the corners of its box `keep` says, each given as
/// 0 or 1 along x, y and z (see `shapes`).
fn mask_of(keep: impl Fn(u8, u8, u8) -> bool) -> u8 {
    (0..8u8).filter(|c| keep(c & 1, c >> 1 & 1, c >> 2)).fold(0, |m, c| m | 1 << c)
}

/// `total` cells shared among `n` parts as evenly as whole cells allow.
/// Where they cannot be equal, the parts that are a cell larger are the
/// first ones if `first`, and otherwise those nearest the middle.
///
/// The first are the lowest openings of a bookcase, which is where its
/// tall books go. The middle is for its bays: a wider bay at one end only
/// would look like a mistake, and in the middle it looks meant.
fn shared(total: i32, n: i32, first: bool) -> Vec<i32> {
    let n = n.max(1);
    let (each, over) = (total / n, total % n);
    let mut order: Vec<i32> = (0..n).collect();
    if !first {
        order.sort_by_key(|i| ((2 * i - (n - 1)).abs(), *i));
    }
    let mut parts = vec![each; n as usize];
    for i in order.into_iter().take(over as usize) {
        parts[i as usize] += 1;
    }
    parts
}

/// One opening of a bookcase: the room between two shelves in one bay,
/// which books stand in. `x` and `y` are its low and high sides across and
/// from front to back, `z` the top of the board under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    pub x: (i32, i32),
    pub y: (i32, i32),
    pub z: i32,
    pub clear: i32,
}

/// What a bookcase came to: for the panel to say, and for tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Count {
    /// shelves between the bottom board and the top, in each bay
    pub shelves: usize,
    /// bays: one more than its uprights
    pub bays: usize,
    /// books and ornaments
    pub books: usize,
    /// the room between its shelves, bay by bay from the bottom up
    pub openings: Vec<Opening>,
}

/// A bookcase filling cells `lo..hi`, its front on the low y side.
///
/// The carcass is built as a joiner builds one. Two sides stand on the
/// floor for the whole height. The top lies over them. The bottom board
/// is housed between them, on the floor or lifted on a plinth with a kick
/// board under its front edge, set back a cell so that the toe of it is in
/// shadow. A back panel one cell thick closes it behind, between the
/// sides. Shelves and uprights are housed inside that: the uprights run
/// the whole height inside and the shelves between them, so each shelf of
/// each bay is one board.
///
/// How many shelves and uprights there are follows from the size. The
/// room inside is divided into openings as near `spacing` tall as whole
/// cells allow, or into one more than `shelves` if a number is asked for;
/// and into the fewest bays that are no wider than `bay`. Cells left over
/// go to the lowest openings and the middle bays (see [`shared`]). So a
/// bookcase dragged taller gains a shelf each time there is room for one,
/// and dragged wider gains an upright, and in between its openings grow a
/// cell at a time.
///
/// A size too small for a carcass (under three cells wide or tall) is one
/// block of wood: something is built at every size a box can be dragged
/// to, as a cube is, and nothing is refused for being small. Boards are
/// made thinner where the size is too small for them as asked.
fn bookcase(f: &Furniture, lo: [i32; 3], hi: [i32; 3], lay: &mut Lay, slopes: bool) -> Result<Count, String> {
    let (w, d, h) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    if w < 3 || h < 3 || d < 1 {
        lay.block(lo, hi, WOOD);
        return Ok(Count::default());
    }
    let f = f.clamped();
    // Boards no thicker than a twelfth of the width or the height. The
    // thickness asked for is right for the size it was chosen at, and a
    // bookcase is dragged from that size down to a bedside shelf: there,
    // boards of 2 in a carcass 20 tall were a tenth of it each, and with
    // a top, a bottom and a plinth half its height was wood. (And never so
    // thick that no room is left between them.)
    let t = f.board.min(w.min(h) / 12).min((w - 1) / 2).min((h - 1) / 2).max(1);
    // A moulding stands out from the carcass by half a board, and three
    // cells at most: more is a ledge. It is left off where the size has
    // no room for it and a carcass inside it: the skirting is as tall as
    // it stands out and a cell more, and the cornice must clear it.
    let m = ((t + 1) / 2).clamp(1, 3);
    let moulded = f.moulding && w >= 2 * m + 2 * t + 1 && d >= m + 2 && h >= 2 * m + 2 * t + 2;
    let m = if moulded { m } else { 0 };
    // sloped pieces, if they are wanted and a brick may be as long as one is wide
    let slopes = slopes && lay.longest >= m;
    // the carcass: inside the mouldings at the front and the sides, and flush with the back
    let (cx, cy) = ((lo[0] + m, hi[0] - m), (lo[1] + m, hi[1]));
    // (a plinth no taller than an eighth of the height, for the boards' reason; and under a skirting at least as tall as the skirting, which hides it)
    let plinth = if moulded { f.plinth.min(h / 8).max(m + 1) } else { f.plinth.min(h / 8) }.min(h - 2 * t - 1).max(0);
    let (floor, under, top) = (lo[2], lo[2] + plinth, hi[2] - t);
    // inside: between the sides, above the bottom board and under the top
    let (ix, iz) = ((cx.0 + t, cx.1 - t), (under + t, top));
    let back = f.back && cy.1 - cy.0 >= 2;
    // (where the boards inside end at the back: against the panel, or at the back itself)
    let behind = cy.1 - back as i32;

    // How many bays and openings: worked out before anything is laid, so
    // that a bookcase too large to build is refused before it is built.
    let inside = (ix.1 - ix.0, iz.1 - iz.0);
    let bays = if f.bay == 0 { 1 } else { ((inside.0 + t + f.bay + t - 1) / (f.bay + t)).min((inside.0 + t) / (2 + t)).max(1) };
    let wanted = if f.shelves > 0 { f.shelves + 1 } else { (2 * (inside.1 + t) + f.spacing + t) / (2 * (f.spacing + t)) };
    // (an opening is two cells clear at least, if there is room for more than one)
    let levels = wanted.min((inside.1 + t) / (2 + t)).max(1);
    if bays as i64 * levels as i64 > MOST_OPENINGS {
        return Err(format!("the bookcase is too large to build: it would have {} openings between its shelves and uprights and the limit is {MOST_OPENINGS}; make it smaller, or its shelves and bays further apart", bays as i64 * levels as i64));
    }
    // (a book is a cell thick at least, so a shelf holds no more books than it is cells wide)
    let most = inside.0 as i64 * levels as i64 * f.books as i64 / 100;
    if most > MOST_BOOKS {
        return Err(format!("the bookcase is too large to build: its shelves have room for {most} books and the limit is {MOST_BOOKS}; make it smaller, its shelves further apart, or set Books lower"));
    }

    // the sides, the top over them (and over the cornice, if there is one), the bottom board and the back
    lay.block([cx.0, cy.0, floor], [cx.0 + t, cy.1, top], WOOD);
    lay.block([cx.1 - t, cy.0, floor], [cx.1, cy.1, top], WOOD);
    lay.block([lo[0], lo[1], top], [hi[0], hi[1], hi[2]], WOOD);
    lay.block([ix.0, cy.0, under], [ix.1, behind, under + t], WOOD);
    if back {
        lay.block([ix.0, behind, floor], [ix.1, cy.1, top], BACK);
    }
    if plinth > 0 && !moulded {
        // the kick board, a cell back from the front where the depth allows
        let thick = t.min(behind - cy.0);
        let set = (behind - cy.0 > thick) as i32;
        lay.block([ix.0, cy.0 + set, floor], [ix.1, cy.0 + set + thick, under], WOOD);
    }

    // uprights between the bays, and the shelves of each bay
    let widths = shared(inside.0 - (bays - 1) * t, bays, false);
    let clears = shared(inside.1 - (levels - 1) * t, levels, true);
    let mut count = Count { shelves: (levels - 1) as usize, bays: bays as usize, ..Count::default() };
    let mut x = ix.0;
    for (bay, width) in widths.iter().enumerate() {
        if bay > 0 {
            lay.block([x, cy.0, iz.0], [x + t, behind, iz.1], WOOD);
            x += t;
        }
        let mut z = iz.0;
        for (level, clear) in clears.iter().enumerate() {
            if level > 0 {
                lay.block([x, cy.0, z], [x + width, behind, z + t], WOOD);
                z += t;
            }
            count.openings.push(Opening { x: (x, x + width), y: (cy.0, behind), z, clear: *clear });
            z += clear;
        }
        x += width;
    }

    if moulded {
        moulding(lay, lo, hi, m, cx, cy, plinth, top, slopes);
    }

    // The books of each opening come from numbers of its own, which its
    // bay and its level give with the seed. One run of numbers for the
    // whole bookcase would have had every book after a change follow
    // from it: dragging the top up by a cell would have rearranged every
    // shelf below. This way a shelf that has not changed size keeps its
    // books while the bookcase is resized around it.
    let before = lay.bricks.len();
    let fullness = f.books as f64 / 100.0;
    for (n, opening) in count.openings.iter().enumerate() {
        let (bay, level) = (n / levels as usize, n % levels as usize);
        let mut numbers = Numbers(f.seed ^ (bay as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93) ^ (level as u64 + 1).wrapping_mul(0xA076_1D64_78BD_642F));
        count.books += books(lay, &mut numbers, opening, fullness, slopes);
    }
    debug_assert!(lay.bricks.len() - before >= count.books);
    Ok(count)
}

/// A cornice under the top and a skirting round the foot, on the front
/// and both sides, standing `m` out from the carcass (`cx` across, `cy`
/// from front to back).
///
/// Each is the plainest profile there is, a chamfer: under the top a
/// strip that widens upwards at 45 degrees to meet the overhang, and at
/// the foot a band with its top edge sloped back to the carcass. They are
/// the game's ramp, laid along each side, with a hip piece (a quarter of
/// a pyramid) where two meet at a corner, so the mitre is two bricks'
/// faces meeting and no step shows. An ogee or a cove would be the usual
/// profile in wood, and neither can be had: the game's sloped pieces are
/// flat faces from corner to corner of a box, and a curve stepped out of
/// single cells is the roughness a moulding is there to avoid.
///
/// Where sloped pieces are not wanted (`slopes`), each is a square band.
#[allow(clippy::too_many_arguments)]
fn moulding(lay: &mut Lay, lo: [i32; 3], hi: [i32; 3], m: i32, cx: (i32, i32), cy: (i32, i32), plinth: i32, top: i32, slopes: bool) {
    // the footprints: the front, with both corners; the two sides behind it; and the corners alone
    let front = ([lo[0], lo[1]], [hi[0], cy.0]);
    let sides = [([lo[0], cy.0], [cx.0, cy.1]), ([cx.1, cy.0], [hi[0], cy.1])];
    let corners = [([lo[0], lo[1]], [cx.0, cy.0]), ([cx.1, lo[1]], [hi[0], cy.0])];
    let band = |lay: &mut Lay, z: (i32, i32)| {
        lay.block([front.0[0], front.0[1], z.0], [front.1[0], front.1[1], z.1], WOOD);
        for (a, b) in sides {
            lay.block([a[0], a[1], z.0], [b[0], b[1], z.1], WOOD);
        }
    };
    if !slopes {
        band(lay, (top - m, top));
        band(lay, (lo[2], lo[2] + plinth));
        return;
    }
    // `up` is 1 for the cornice, which is widest at its top, and 0 for the skirting's chamfer, widest at its foot
    let mut chamfer = |z: (i32, i32), up: u8| {
        // the front strip between the corners: all of its box but the outer edge at the narrow end
        lay.piece(mask_of(|_, y, z| !(y == 0 && z != up)), [cx.0, lo[1], z.0], [cx.1, cy.0, z.1], WOOD, 0);
        for (side, (a, b)) in sides.into_iter().enumerate() {
            let outer = side as u8; // the low x side of the left strip, the high x side of the right
            lay.piece(mask_of(|x, _, z| !(x == outer && z != up)), [a[0], a[1], z.0], [b[0], b[1], z.1], WOOD, 1);
        }
        for (side, (a, b)) in corners.into_iter().enumerate() {
            // the whole face at the wide end, and of the narrow end only the corner that touches the carcass
            let inner = 1 - side as u8;
            lay.piece(mask_of(|x, y, z| z == up || (x == inner && y == 1)), [a[0], a[1], z.0], [b[0], b[1], z.1], WOOD, 2);
        }
    };
    chamfer((top - m, top), 1);
    chamfer((lo[2] + plinth - m, lo[2] + plinth), 0);
    band(lay, (lo[2], lo[2] + plinth - m));
}

/// The sizes the books of one opening are drawn from, which its clear
/// height and depth give: a book is as tall as its shelf allows, less a
/// finger's room, down to a little over half of that; a cell thick, or
/// more where the shelves are far enough apart for the scale to show it;
/// and as deep as the shelf, less a cell so that the board's edge shows,
/// down to two thirds of that.
#[derive(Clone, Copy)]
struct Sizes {
    tall: (i32, i32),
    thick: i32,
    deep: (i32, i32),
}

impl Sizes {
    fn of(clear: i32, depth: i32) -> Sizes {
        let tallest = if clear >= 4 { clear - 1 } else { clear };
        let deepest = if depth >= 3 { depth - 1 } else { depth };
        Sizes { tall: (((clear * 11 + 19) / 20).clamp(2, tallest), tallest), thick: ((clear + 1) / 4).clamp(1, 3), deep: (((deepest * 2 + 2) / 3).max(1), deepest) }
    }

    /// A whole number from `low` to `high`, both included.
    fn between(numbers: &mut Numbers, low: i32, high: i32) -> i32 {
        low + ((numbers.unit() * (high - low + 1).max(1) as f64) as i32).min((high - low).max(0))
    }

    /// How thick a book is: mostly thin, since most books are.
    fn thickness(&self, numbers: &mut Numbers) -> i32 {
        let u = numbers.unit();
        (1 + (u * u * self.thick as f64) as i32).min(self.thick)
    }
}

/// A colour number for a book, never one that a book beside it has: two
/// neighbours of one colour read as one thick book. `beside` is the
/// colours its neighbours have, a bit for each, and is left as this
/// book's colour alone, for the next book along.
fn spine(numbers: &mut Numbers, beside: &mut u8) -> u8 {
    let every = (1 << BOOK_COLOURS) - 1;
    let mut n = ((numbers.unit() * BOOK_COLOURS as f64) as u8).min(BOOK_COLOURS - 1);
    // (the next colour round that no neighbour has, if any is left)
    while *beside & every != every && *beside >> n & 1 == 1 {
        n = (n + 1) % BOOK_COLOURS;
    }
    *beside = 1 << n;
    FIRST_BOOK + n
}

/// Fill one opening with books, and say how many things were put in it.
///
/// A shelf of books is not a row of random boxes. Books stand in runs,
/// pushed up against a side of the bay, and what room is left over is
/// left in one place, beside the run or between two runs, as it is when
/// books are taken out and the rest pushed along. So `fullness` decides
/// how many cells of the shelf's width have something standing on them
/// (all of them at 1), and those cells are one run at the left, or one at
/// the right, or one at each end. Within a run a book is near the height
/// of the one before it more often than not, since books are shelved in
/// sets and by size.
///
/// In the room left over, some of the time: a book that has fallen
/// against the end of a run and leans there; a few books lying flat in a
/// pile; an ornament. Their widths are taken out of the cells the runs
/// would have had, so a shelf is as full as it was asked to be whatever
/// stands on it. (Added on top of the runs, as they first were, they made
/// a shelf asked to be a fifth full two fifths full.) A run may have a
/// pile in it too, so that a full bookcase is not only uprights.
///
/// Everything is placed by the cell and by what is beside it, so nothing
/// needs testing against anything else afterwards: a book's foot is on
/// the shelf, a lying book is on the one under it and no larger, and each
/// thing has its own stretch of the shelf's width.
fn books(lay: &mut Lay, numbers: &mut Numbers, opening: &Opening, fullness: f64, slopes: bool) -> usize {
    let (width, depth) = (opening.x.1 - opening.x.0, opening.y.1 - opening.y.0);
    if width < 1 || depth < 1 || opening.clear < 2 || fullness <= 0.0 {
        return 0;
    }
    let sizes = Sizes::of(opening.clear, depth);
    // (a little more or less than asked, shelf by shelf: no two shelves of a real bookcase are as full as each other)
    let asked = if fullness >= 1.0 { width } else { ((width as f64 * fullness * (0.8 + 0.4 * numbers.unit())).round() as i32).clamp(0, width) };
    if asked == 0 {
        return 0;
    }
    let spare = width - asked;
    // one run or two, and if one, at which side
    let (split, share, side) = (numbers.unit(), numbers.unit(), numbers.unit());
    let two = spare >= 2 && asked >= 6 && split < 0.45;
    let (at_left, at_right) = (two || spare == 0 || side < 0.7, two || !(spare == 0 || side < 0.7));
    // What is to stand in the room left over, each bought with cells of
    // the runs while two are left for a run: a book leaning on the free
    // end of each run (how thick, and how far its top has slid, which is
    // no further than it is thick, so that it is three pieces at most); a
    // pile, by its length; an ornament, by its width.
    let mut runs = asked;
    let mut buy = |wanted: bool, cost: i32| {
        let bought = wanted && spare > 0 && runs - cost >= 2;
        runs -= if bought { cost } else { 0 };
        bought
    };
    let mut leaners = [None, None];
    for (leaner, has_run) in leaners.iter_mut().zip([at_left, at_right]) {
        let chance = numbers.unit();
        let thick = sizes.thickness(numbers).min(2).min(lay.longest);
        let lean = Sizes::between(numbers, 1, thick);
        *leaner = buy(has_run && slopes && chance < 0.35, thick + lean).then_some((thick, lean));
    }
    let (chance, long) = (numbers.unit(), Sizes::between(numbers, sizes.tall.0.max(3), sizes.tall.1 + 1));
    let piled = buy(chance < 0.2, long).then_some(long);
    let (chance, wide) = (numbers.unit(), Sizes::between(numbers, 2, 3));
    let stood = buy(chance < 0.4 && opening.clear >= 3, wide).then_some(wide);

    let left = if two { ((runs as f64 * (0.35 + 0.4 * share)).round() as i32).clamp(1, runs - 1) } else if at_left { runs } else { 0 };
    let right = runs - left;
    // the room left over, between the runs
    let mut room = (opening.x.0 + left, opening.x.1 - right);
    let mut made = 0;
    let ends_left = run(lay, numbers, opening, &sizes, (opening.x.0, room.0), &mut made);
    let ends_right = run(lay, numbers, opening, &sizes, (room.1, opening.x.1), &mut made);
    // the colours of whatever stands at each end of the room, which what is put there next must not have
    let mut beside = [ends_left.1.map_or(0, |end| end.1), ends_right.0.map_or(0, |end| end.1)];

    // The leaning books: each against the book at the end of its run, and
    // no taller than it, or there is nothing for its top to rest on. (A
    // run that ends in a pile, or in a book too short, has none.)
    for ((leaner, against), to_left) in leaners.into_iter().zip([ends_left.1, ends_right.0]).zip([true, false]) {
        let (Some((thick, lean)), Some((Some(height), _))) = (leaner, against) else { continue };
        let tall = (height - (numbers.unit() < 0.5) as i32).min(lay.longest);
        let deep = Sizes::between(numbers, sizes.deep.0, sizes.deep.1);
        if tall < 3 {
            continue;
        }
        let x = if to_left { room.0 } else { room.1 - thick - lean };
        leaning(lay, [x, opening.y.1 - deep, opening.z], thick, lean, deep, tall, to_left, spine(numbers, &mut beside[1 - to_left as usize]));
        if to_left {
            room.0 += thick + lean;
        } else {
            room.1 -= thick + lean;
        }
        made += 1;
    }
    // a pile of books lying flat, at one end of the room left
    if let Some(long) = piled {
        let x = if at_left { room.0 } else { room.1 - long };
        made += pile(lay, numbers, opening, &sizes, (x, x + long), &mut beside[1 - at_left as usize]);
        if at_left {
            room.0 += long;
        } else {
            room.1 -= long;
        }
    }
    // an ornament standing somewhere in what room is left: a vase, a box, or a picture in a frame
    if let Some(wide) = stood {
        let x = Sizes::between(numbers, room.0, room.1 - wide);
        let mut either = beside[0] | beside[1];
        ornament(lay, numbers, opening, (x, x + wide), opening.z, opening.clear, &mut either);
        made += 1;
    }
    made
}

/// What stands at one end of a run: how tall it is if it is a book
/// standing upright, for a leaning book to rest against, and the colours
/// it shows on that side (see [`spine`]).
type End = (Option<i32>, u8);

/// A run of books standing upright from `x.0` to `x.1`, filling it to the
/// cell. Says what stands at each end of it; `None` for a run of nothing.
fn run(lay: &mut Lay, numbers: &mut Numbers, opening: &Opening, sizes: &Sizes, x: (i32, i32), made: &mut usize) -> (Option<End>, Option<End>) {
    let mut ends = (None, None);
    let (mut at, mut beside) = (x.0, 0u8);
    // the height the books of a set share, until the next set
    let mut set = Sizes::between(numbers, sizes.tall.0, sizes.tall.1);
    let mut piled = false;
    while at < x.1 {
        let left = x.1 - at;
        // A pile within the run, once at most, where there is room for a
        // lying book's length. Seldom: the chance is taken at every book,
        // and at one in fourteen every other shelf had a pile.
        let long = Sizes::between(numbers, sizes.tall.0.max(3), sizes.tall.1 + 1);
        let (end, wide) = if !piled && numbers.unit() < 0.015 && left >= long && opening.clear >= 3 {
            *made += pile(lay, numbers, opening, sizes, (at, at + long), &mut beside);
            piled = true;
            ((None, beside), long)
        } else {
            if numbers.unit() < 0.25 {
                set = Sizes::between(numbers, sizes.tall.0, sizes.tall.1);
            }
            let thick = sizes.thickness(numbers).min(left);
            let tall = (set + [-1, 0, 0, 0, 1][Sizes::between(numbers, 0, 4) as usize]).clamp(sizes.tall.0, sizes.tall.1);
            let deep = Sizes::between(numbers, sizes.deep.0, sizes.deep.1);
            // pushed to the back of the shelf, so the spines stand at the depths the books have
            lay.block([at, opening.y.1 - deep, opening.z], [at + thick, opening.y.1, opening.z + tall], spine(numbers, &mut beside));
            *made += 1;
            ((Some(tall), beside), thick)
        };
        if at == x.0 {
            ends.0 = Some(end);
        }
        ends.1 = Some(end);
        at += wide;
    }
    ends
}

/// Books lying flat in a pile over `x`, each on the one under it and no
/// longer or deeper than it, so that none overhangs. Sometimes with an
/// ornament on top. Says how many things it came to.
fn pile(lay: &mut Lay, numbers: &mut Numbers, opening: &Opening, sizes: &Sizes, x: (i32, i32), beside: &mut u8) -> usize {
    let (mut x, mut z, mut made) = (x, opening.z, 0);
    // (each is beside whatever the pile is beside, and on the one under it; and what comes after the pile is beside all of them)
    let (neighbour, mut all) = (*beside, 0u8);
    let mut deep = Sizes::between(numbers, sizes.deep.0, sizes.deep.1);
    let most = Sizes::between(numbers, 2, 4);
    // (as high as an upright book may stand, so a hand still goes over it)
    let ceiling = opening.z + sizes.tall.1;
    for _ in 0..most {
        let thick = sizes.thickness(numbers);
        if z + thick > ceiling {
            break;
        }
        *beside |= neighbour;
        lay.block([x.0, opening.y.1 - deep, z], [x.1, opening.y.1, z + thick], spine(numbers, beside));
        (z, made, all) = (z + thick, made + 1, all | *beside);
        // the next is the same size or a cell shorter at one end, and a cell shallower some of the time
        let shorter = numbers.unit();
        if x.1 - x.0 > 3 && shorter < 0.6 {
            if shorter < 0.3 { x.0 += 1 } else { x.1 -= 1 }
        }
        if deep > 1 && numbers.unit() < 0.4 {
            deep -= 1;
        }
    }
    if made > 0 && numbers.unit() < 0.3 && ceiling - z >= 2 && x.1 - x.0 >= 2 {
        // (on the pile's top book, which ends at `x` and is at least `deep` deep)
        let wide = (x.1 - x.0).min(2);
        let from = x.0 + (x.1 - x.0 - wide) / 2;
        let room = Opening { y: (opening.y.1 - deep, opening.y.1), ..*opening };
        *beside |= neighbour;
        ornament(lay, numbers, &room, (from, from + wide), z, opening.z + opening.clear - z, beside);
        (made, all) = (made + 1, all | *beside);
    }
    *beside = all;
    made
}

/// An ornament over `x`, standing on whatever has its top at `z`, with
/// `clear` of height to stand in: a vase (a body and a neck), a box, or a
/// picture frame.
fn ornament(lay: &mut Lay, numbers: &mut Numbers, opening: &Opening, x: (i32, i32), z: i32, clear: i32, beside: &mut u8) {
    let (wide, depth) = (x.1 - x.0, opening.y.1 - opening.y.0);
    let shade = spine(numbers, beside);
    // as deep as it is wide, in the middle of the shelf's depth
    let deep = wide.min(depth);
    let y = opening.y.0 + (depth - deep + 1) / 2;
    let which = numbers.unit();
    let tall = Sizes::between(numbers, 2, 4).min(clear - 1).max(1);
    if which < 0.4 && wide == 3 && deep == 3 && clear >= 4 {
        // a vase: a square body, and a neck one cell across standing on the middle of it
        let body = (tall - 1).max(1);
        lay.block([x.0, y, z], [x.1, y + deep, z + body], shade);
        lay.block([x.0 + 1, y + 1, z + body], [x.0 + 2, y + 2, z + body + Sizes::between(numbers, 1, 2).min(clear - body)], shade);
    } else if which < 0.7 && depth >= 2 {
        // a frame: one cell deep, standing a cell off the back
        let y = (opening.y.1 - 2).max(opening.y.0);
        lay.block([x.0, y, z], [x.1, y + 1, z + (tall + 1).min(clear - 1).max(1)], shade);
    } else {
        // a box
        lay.block([x.0, y, z], [x.1, y + deep, z + tall.min(wide)], shade);
    }
}

/// A book leaning: `thick` through and `tall` high, its foot on the shelf
/// at `at` and its top slid `lean` cells along, to the left if `to_left`
/// (against a book standing at its left) or else to the right.
///
/// It is the outline of the book sheared, a parallelogram, which is a
/// box with a ramp at each side: one under the overhanging face and one
/// over the face that has leant away. No box at all where it leans as far
/// as it is thick. A leaning book drawn true is a rectangle turned, and
/// no brick is that: turned, its corners leave the lattice. Sheared, it
/// keeps its foot flat on the shelf and its top level, and is exactly
/// three bricks; stepped out of boxes instead it was a stair of four or
/// five, and read as a stack of pamphlets.
#[allow(clippy::too_many_arguments)]
fn leaning(lay: &mut Lay, at: [i32; 3], thick: i32, lean: i32, deep: i32, tall: i32, to_left: bool, shade: u8) {
    let [x, y, z] = at;
    let lean = lean.min(thick);
    // Along x: the ramp under the overhang, the box, the ramp over the
    // far face; in that order for a book leaning left, and the other way
    // round for one leaning right. The overhang's ramp lacks the edge at
    // its foot on the side the book leans to, and the other its top edge
    // on the side it leans from.
    let towards = if to_left { 0 } else { 1 };
    let (over, body, away) = if to_left { (x, x + lean, x + thick) } else { (x + thick, x + lean, x) };
    lay.piece(mask_of(|x, _, z| !(x == towards && z == 0)), [over, y, z], [over + lean, y + deep, z + tall], shade, 1);
    lay.block([body, y, z], [body + thick - lean, y + deep, z + tall], shade);
    lay.piece(mask_of(|x, _, z| !(x != towards && z == 1)), [away, y, z], [away + lean, y + deep, z + tall], shade, 1);
}

/// How far `p` is from one brick: negative inside it. Exact for a box.
/// For a sloped piece it is the furthest `p` is beyond any of the piece's
/// faces, which is never more than the true distance and has its sign.
fn brick_distance(b: &CellBrick, p: DVec3) -> f64 {
    let (lo, ext) = (DVec3::new(b.min[0] as f64, b.min[1] as f64, b.min[2] as f64), DVec3::new(b.ext[0] as f64, b.ext[1] as f64, b.ext[2] as f64));
    let q = (p - lo - ext / 2.0).abs() - ext / 2.0;
    let mut d = q.max(DVec3::ZERO).length() + q.max_element().min(0.0);
    let Some(info) = shape_info(b.mask).filter(|i| i.mask != CUBE) else { return d };
    // where `p` is across the box along axis `k`, measured from the side `from` (0 or 1) of it, as 0 to 1
    let across = |k: usize, from: u8| if from == 0 { (p[k] - lo[k]) / ext[k] } else { (lo[k] + ext[k] - p[k]) / ext[k] };
    // beyond the sloped face that cuts the box from the edge `i` = 1 to the edge `j` = 1, by how far
    let slope = |i: (usize, u8), j: (usize, u8)| (across(i.0, i.1) + across(j.0, j.1) - 1.0) / (1.0 / (ext[i.0] * ext[i.0]) + 1.0 / (ext[j.0] * ext[j.0])).sqrt();
    let (axis, bit) = (info.axis as usize, |k: usize| info.corner >> k & 1);
    match info.kind {
        // a ramp lacks the edge its corner is on: it is solid where the two distances from the sides opposite that edge come to under 1
        crate::shapes::Kind::Ramp => {
            let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
            d = d.max(slope((i, 1 - bit(i)), (j, 1 - bit(j))));
        }
        // a hip narrows from its whole face to its apex: along each other axis it is solid within as far from the apex's side as it is from the apex's end
        crate::shapes::Kind::Hip => {
            for i in [(axis + 1) % 3, (axis + 2) % 3] {
                d = d.max(slope((i, bit(i)), (axis, 1 - bit(axis))));
            }
        }
        // (no other piece is laid here)
        _ => {}
    }
    d
}

/// How far out from a brick the index looks: a place further than this
/// from every brick is said to be this far from them.
const REACH: f64 = 2.0;
/// The side of one square of the index, in cells.
const SQUARE: i32 = 8;

/// The bricks near each square of the furniture's face (x across, z up),
/// for asking how far a place is from the nearest of them without asking
/// every brick. Squares on the face and not cubes in space: furniture is
/// shallow from front to back, and nearly every brick reaches most of the
/// way through it.
struct Index {
    across: i32,
    squares: Vec<Vec<u32>>,
}

/// A piece of furniture laid out: its bricks, their colours, and where it is.
pub struct Model {
    pub bricks: Vec<CellBrick>,
    /// which colour each brick is ([`WOOD`], [`BACK`], or a book's)
    pub shades: Vec<u8>,
    /// the volume of all its bricks, in cells cubed
    pub volume: f64,
    /// The volume where its bricks and its shape disagree: none, but for a plant in a pot, which is fitted (see `pots::grow`).
    pub error: f64,
    pub count: Count,
    /// Which bricks carry a light in the game, by their place in
    /// `bricks`: a lamp's bulb, each flame of a chandelier. No more than
    /// [`MOST_LIGHTS`] of them.
    pub lamps: Vec<usize>,
    lo: [i32; 3],
    hi: [i32; 3],
    index: OnceLock<Index>,
}

impl Model {
    /// The furniture `f` describes, filling the box whose low and high
    /// sides along each axis are `sides` (see `spec::cube_sides`), in
    /// bricks no longer than `longest`, with sloped pieces if `slopes`.
    pub fn new(f: &Furniture, sides: [(f64, f64); 3], longest: i32, slopes: bool) -> Result<Model, String> {
        if sides.iter().any(|(low, high)| !(low.is_finite() && high.is_finite()) || high - low > 2.0 * crate::spec::MAX_SPAN || low.abs() > 1e6) {
            return Err("the furniture is too large to build: make it smaller".into());
        }
        let (lo, hi) = (sides.map(|s| s.0.round() as i32), sides.map(|s| s.1.round() as i32));
        let hi = [0, 1, 2].map(|k| hi[k].max(lo[k] + 1));
        // A box cut into bricks of the longest length allowed is this many
        // of them, and a solid piece (a cupboard's body, a sofa's base) is
        // its whole box. Two thousand cells each way in bricks one cell
        // long would be eight thousand million; refused here, before a
        // brick is laid, it costs nothing.
        let pieces: i64 = (0..3).map(|k| ((hi[k] - lo[k]) as i64 + longest.max(1) as i64 - 1) / longest.max(1) as i64).product();
        if pieces > MOST_PIECES {
            return Err(format!("the furniture is too large to build in bricks this short: its box alone would be {pieces} of them and the limit is {MOST_PIECES}; make it smaller, or the longest brick longer"));
        }
        let mut lay = Lay { bricks: Vec::new(), shades: Vec::new(), longest: longest.max(1) };
        let mut count = Count::default();
        let mut lamps = Vec::new();
        let mut grown = (0.0, 0.0, 0.0);
        if f.kind == Kind::Bookcase {
            count = bookcase(f, lo, hi, &mut lay, slopes)?;
        } else {
            let f = f.clamped();
            let mut room = parts::Room { lay: &mut lay, lo, hi, set: Set::for_piece(&f), f, slopes, lamps: Vec::new(), grown: (0.0, 0.0, 0.0) };
            match f.kind {
                Kind::Cupboard | Kind::Drawers | Kind::Sideboard | Kind::Kitchen => storage::cupboard(&mut room, f.kind),
                Kind::Dining | Kind::Coffee | Kind::Side | Kind::Desk => tables::table(&mut room, f.kind),
                Kind::Chair | Kind::Stool | Kind::Bench => seating::chair(&mut room, f.kind),
                Kind::Armchair | Kind::Sofa => seating::sofa(&mut room, f.kind),
                Kind::TableLamp | Kind::FloorLamp => lights::standing(&mut room, f.kind),
                Kind::Pendant => lights::pendant(&mut room),
                Kind::Chandelier => lights::chandelier(&mut room),
                Kind::Sconce => lights::sconce(&mut room),
                Kind::Candlestick => lights::candlestick(&mut room),
                Kind::Bed => bed::bed(&mut room),
                Kind::Potted => pots::potted(&mut room),
                Kind::Basket => pots::basket(&mut room),
                Kind::WindowBox => pots::window_box(&mut room),
                Kind::Trellis => trellis::trellis(&mut room),
                Kind::Painting | Kind::Portrait => pictures::picture(&mut room, f.kind),
                Kind::Bookcase => {}
            }
            grown = room.grown;
            lamps = room.lamps;
            lamps.truncate(MOST_LIGHTS);
        }
        // (a fitted plant's volume is its shape's, as any fitted form's is, and the rest is its bricks')
        let volume = lay.bricks.iter().map(CellBrick::volume).sum::<f64>() - grown.2 + grown.0;
        Ok(Model { bricks: lay.bricks, shades: lay.shades, volume, error: grown.1, count, lamps, lo, hi, index: OnceLock::new() })
    }

    /// The box it fills: its low corner and its high one.
    pub fn bounds(&self) -> (DVec3, DVec3) {
        (DVec3::new(self.lo[0] as f64, self.lo[1] as f64, self.lo[2] as f64), DVec3::new(self.hi[0] as f64, self.hi[1] as f64, self.hi[2] as f64))
    }

    /// The index, made the first time it is asked for: a build does not
    /// need it, only whoever measures the build or joins it to another form.
    fn index(&self) -> &Index {
        self.index.get_or_init(|| {
            let across = (self.hi[0] - self.lo[0]) / SQUARE + 1;
            let up = (self.hi[2] - self.lo[2]) / SQUARE + 1;
            let mut squares = vec![Vec::new(); (across * up) as usize];
            let reach = REACH.ceil() as i32;
            for (n, b) in self.bricks.iter().enumerate() {
                // every square within reach of the brick
                let span = |k: usize, most: i32| ((b.min[k] - reach - self.lo[k]).div_euclid(SQUARE).clamp(0, most - 1), (b.min[k] + b.ext[k] + reach - self.lo[k]).div_euclid(SQUARE).clamp(0, most - 1));
                let ((x0, x1), (z0, z1)) = (span(0, across), span(2, up));
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        squares[(z * across + x) as usize].push(n as u32);
                    }
                }
            }
            Index { across, squares }
        })
    }

    /// How far `p` is from the furniture, in cells: negative inside a
    /// brick of it. Never more than the true distance, and no more than
    /// [`REACH`] however far off `p` is within the furniture's own box;
    /// its sign is exact.
    pub fn distance(&self, p: DVec3) -> f64 {
        let (lo, hi) = self.bounds();
        let q = (p - (lo + hi) / 2.0).abs() - (hi - lo) / 2.0;
        let outside = q.max(DVec3::ZERO).length();
        if outside > 0.0 {
            // (outside its box, at least as far from any brick as from the box)
            return outside;
        }
        let index = self.index();
        let square = |k: usize| ((p[k] - self.lo[k] as f64).floor() as i32).div_euclid(SQUARE);
        let Some(near) = index.squares.get((square(2) * index.across + square(0).min(index.across - 1)) as usize) else { return REACH };
        near.iter().map(|n| brick_distance(&self.bricks[*n as usize], p)).fold(REACH, f64::min)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_kinds;
