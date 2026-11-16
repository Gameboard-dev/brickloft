//! The styles furniture comes in, and what a style and a seed decide.
//!
//! A style is a set of choices that go together: what a leg is like, how
//! heavy the sections are, whether fronts are panelled, what a handle is.
//! It is not a number added to every size. A farmhouse table has turned
//! legs and a plank top because those belong together, and a seed that
//! varied each setting on its own would give an industrial frame under a
//! classical cornice as often as not.
//!
//! So the seed chooses only among what the style allows, and it chooses
//! the same way for every kind of furniture ([`Set::of`] is given no
//! kind). A dining table and a chair of one style and one seed therefore
//! have the same legs, the same weight and the same wood, and are plainly
//! a set. Had each kind drawn its own numbers from the seed, a table and
//! its chairs would have matched only by chance.

use super::{Furniture, Kind};
use crate::pattern::Numbers;

/// The styles there are. `Random` lets the seed pick one of the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// square sections, flush fronts, no mouldings, thin tops
    Modern,
    /// turned legs, panelled doors with knobs, plank tops
    Farmhouse,
    /// tapered or turned legs, panelled doors, a cornice and a skirting, upholstery
    Classical,
    /// heavy square sections, trestles and stretchers, thick tops, no polish
    Rustic,
    /// thin dark metal frames carrying slabs of wood
    Industrial,
    Random,
}

/// Every style, with the name a save or another program calls it by.
pub const STYLES: [(Style, &str); 6] = [(Style::Modern, "modern"), (Style::Farmhouse, "farmhouse"), (Style::Classical, "classical"), (Style::Rustic, "rustic"), (Style::Industrial, "industrial"), (Style::Random, "random")];

impl Style {
    /// The style itself, or for `Random` the one `seed` picks.
    pub fn resolved(self, seed: u64) -> Style {
        match self {
            Style::Random => STYLES[((Numbers(seed ^ 0x5EED_0F57_71E5_0001).unit() * 5.0) as usize).min(4)].0,
            other => other,
        }
    }
}

/// What a table or a chair stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leg {
    Square,
    /// narrowing to the floor on its two inner faces, by sloped pieces
    Tapered,
    /// a square leg with a neck, a ball and a foot, as a lathe leaves one
    Turned,
    /// two end frames and a rail between them, in place of four legs
    Trestle,
    /// one column on a base
    Pedestal,
}

/// What fills a chair's back between its posts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Back {
    Slats,
    Ladder,
    Panel,
    Spindles,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Knob,
    Bar,
    None,
}

/// What a cupboard stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stand {
    Plinth,
    Feet,
}

/// The shape of a lamp's shade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    Drum,
    Cone,
    Globe,
}

/// What one style and one seed come to: the choices every piece of a set
/// shares. A kind's own settings may overrule one of them (see
/// [`Set::for_piece`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Set {
    pub style: Style,
    pub leg: Leg,
    /// how heavy its sections are: -1 thin, 0, or 1 heavy
    pub heavy: i32,
    /// the colour number of legs and frames: wood, or metal
    pub frame: u8,
    /// how far a top stands out past what carries it, in cells
    pub overhang: i32,
    /// whether a top is laid in planks of two tones
    pub planks: bool,
    pub apron: bool,
    pub stretcher: bool,
    /// whether a small table's top is round
    pub round: bool,
    pub back: Back,
    pub cushion: bool,
    /// whether doors and drawer fronts have a raised frame round a panel
    pub panelled: bool,
    pub handle: Handle,
    pub stand: Stand,
    pub moulding: bool,
    pub shade: Shade,
    /// whether a sofa's arms are rolled (sloped outwards at the top)
    pub rolled: bool,
    /// a number from 0 up to, not including, 1, for whatever else a piece has to choose
    pub more: f64,
}

/// One of `from`, as `u` (from 0 to 1) picks.
fn one<T: Copy>(from: &[T], u: f64) -> T {
    from[((u * from.len() as f64) as usize).min(from.len() - 1)]
}

impl Set {
    /// The choices of `style` that `seed` picks. The numbers are drawn in
    /// one order whatever the style, so each choice has a number of its
    /// own, and adding a choice to a style's list moves no other choice.
    pub fn of(style: Style, seed: u64) -> Set {
        let style = style.resolved(seed);
        let mut numbers = Numbers(seed ^ 0xF0A1_17E5_5E7C_4A3D);
        let mut u = || numbers.unit();
        let (leg, back, handle, stand, shade, round, cushion, stretcher, more) = (u(), u(), u(), u(), u(), u(), u(), u(), u());
        use super::{METAL, WOOD};
        match style {
            Style::Modern => Set { style, leg: one(&[Leg::Square, Leg::Tapered, Leg::Square], leg), heavy: -1, frame: one(&[WOOD, WOOD, METAL], more), overhang: 0, planks: false, apron: false, stretcher: false, round: round < 0.5, back: one(&[Back::Panel, Back::Slats], back), cushion: cushion < 0.6, panelled: false, handle: one(&[Handle::None, Handle::Bar], handle), stand: one(&[Stand::Feet, Stand::Plinth], stand), moulding: false, shade: one(&[Shade::Drum, Shade::Globe], shade), rolled: false, more },
            Style::Farmhouse => Set { style, leg: one(&[Leg::Turned, Leg::Turned, Leg::Square], leg), heavy: 0, frame: WOOD, overhang: 1, planks: true, apron: true, stretcher: stretcher < 0.4, round: round < 0.3, back: one(&[Back::Ladder, Back::Spindles, Back::Slats], back), cushion: cushion < 0.4, panelled: true, handle: Handle::Knob, stand: one(&[Stand::Feet, Stand::Plinth], stand), moulding: false, shade: one(&[Shade::Cone, Shade::Drum], shade), rolled: false, more },
            Style::Classical => Set { style, leg: one(&[Leg::Tapered, Leg::Turned], leg), heavy: 0, frame: WOOD, overhang: 1, planks: false, apron: true, stretcher: false, round: round < 0.5, back: one(&[Back::Panel, Back::Spindles, Back::Slats], back), cushion: true, panelled: true, handle: one(&[Handle::Knob, Handle::Bar], handle), stand: Stand::Plinth, moulding: true, shade: one(&[Shade::Cone, Shade::Drum], shade), rolled: true, more },
            Style::Rustic => Set { style, leg: one(&[Leg::Square, Leg::Trestle], leg), heavy: 1, frame: WOOD, overhang: 1, planks: true, apron: true, stretcher: true, round: false, back: one(&[Back::Slats, Back::Ladder], back), cushion: false, panelled: false, handle: Handle::Bar, stand: one(&[Stand::Feet, Stand::Plinth], stand), moulding: false, shade: one(&[Shade::Drum, Shade::Cone], shade), rolled: false, more },
            _ => Set { style, leg: Leg::Square, heavy: -1, frame: METAL, overhang: 0, planks: false, apron: false, stretcher: true, round: round < 0.3, back: one(&[Back::Slats, Back::Panel, Back::Ladder], back), cushion: false, panelled: false, handle: Handle::Bar, stand: Stand::Feet, moulding: false, shade: one(&[Shade::Cone, Shade::Globe], shade), rolled: false, more },
        }
    }

    /// The set as one piece has it: what the piece's own settings say in
    /// place of the style's choice, wherever they say anything (0 leaves
    /// the style's). Which setting means what is each kind's own affair
    /// (see [`settings`]).
    pub fn for_piece(f: &Furniture) -> Set {
        let mut set = Set::of(f.style, f.seed);
        let pick = |at: i32| (at - 1).max(0) as usize;
        match f.kind {
            Kind::Dining | Kind::Coffee | Kind::Side | Kind::Desk => {
                if f.choice > 0 {
                    set.leg = [Leg::Square, Leg::Tapered, Leg::Turned, Leg::Trestle, Leg::Pedestal][pick(f.choice).min(4)];
                }
                if f.second > 0 {
                    set.round = f.second == 2;
                }
            }
            Kind::Chair | Kind::Bench => {
                if f.choice > 0 {
                    set.back = [Back::Slats, Back::Ladder, Back::Panel, Back::Spindles][pick(f.choice).min(3)];
                }
                if f.second > 0 {
                    set.cushion = f.second == 2;
                }
            }
            Kind::Stool => {
                if f.choice > 0 {
                    set.round = f.choice == 2;
                }
                if f.second > 0 {
                    set.cushion = f.second == 2;
                }
            }
            Kind::Armchair | Kind::Sofa => {
                if f.choice > 0 {
                    set.rolled = f.choice == 2;
                }
                if f.second > 0 {
                    set.stand = [Stand::Feet, Stand::Plinth][pick(f.second).min(1)];
                }
            }
            Kind::Cupboard | Kind::Drawers | Kind::Sideboard | Kind::Kitchen => {
                if f.choice > 0 {
                    set.handle = [Handle::Knob, Handle::Bar, Handle::None][pick(f.choice).min(2)];
                }
                if f.second > 0 {
                    set.stand = [Stand::Plinth, Stand::Feet][pick(f.second).min(1)];
                }
            }
            Kind::TableLamp | Kind::FloorLamp | Kind::Pendant | Kind::Sconce => {
                if f.choice > 0 {
                    set.shade = [Shade::Drum, Shade::Cone, Shade::Globe][pick(f.choice).min(2)];
                }
            }
            Kind::Bed => {
                if f.choice > 0 {
                    set.back = [Back::Panel, Back::Slats, Back::Ladder][pick(f.choice).min(2)];
                }
            }
            Kind::Bookcase | Kind::Chandelier | Kind::Candlestick => {}
        }
        set
    }
}

/// Which of a piece's settings a control changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Count,
    Choice,
    Second,
    Extra,
    Shine,
}

/// One setting of a kind, as the panel shows it: which field it is, what
/// it is called for this kind, and either the names of its choices (the
/// first being 0, the style's own) or, for a number, the most it may be
/// (0 being "as its size has it").
#[derive(Clone, Copy, Debug)]
pub struct Setting {
    pub field: Field,
    pub label: &'static str,
    pub choices: &'static [&'static str],
    pub most: i32,
}

const fn choice(field: Field, label: &'static str, choices: &'static [&'static str]) -> Setting {
    Setting { field, label, choices, most: choices.len() as i32 - 1 }
}

const fn number(label: &'static str, most: i32) -> Setting {
    Setting { field: Field::Count, label, choices: &[], most }
}

const fn tick(field: Field, label: &'static str) -> Setting {
    Setting { field, label, choices: &[], most: 1 }
}

const BY_STYLE: &str = "As the style has it";
const LEGS: Setting = choice(Field::Choice, "Legs", &[BY_STYLE, "Square", "Tapered", "Turned", "Trestle", "Pedestal"]);
const TOP: Setting = choice(Field::Second, "Top", &[BY_STYLE, "Rectangular", "Round"]);
const BACKS: Setting = choice(Field::Choice, "Back", &[BY_STYLE, "Slats", "Ladder", "Panel", "Spindles"]);
const SEAT: Setting = choice(Field::Second, "Seat", &[BY_STYLE, "Bare", "Cushion"]);
const ARMS: Setting = choice(Field::Choice, "Arms", &[BY_STYLE, "Square", "Rolled"]);
const UNDER: Setting = choice(Field::Second, "Stands on", &[BY_STYLE, "Legs", "A skirt to the floor"]);
const HANDLES: Setting = choice(Field::Choice, "Handles", &[BY_STYLE, "Knobs", "Bars", "None"]);
const STANDS: Setting = choice(Field::Second, "Stands on", &[BY_STYLE, "A plinth", "Feet"]);
const SHADES: Setting = choice(Field::Choice, "Shade", &[BY_STYLE, "Drum", "Cone", "Globe"]);
const SHINE: Setting = tick(Field::Shine, "Lights shine in the game");

/// The settings a kind has beyond its size, its style and its seed, in
/// the order the panel shows them. A bookcase's are its own (see the
/// panel), from before there were styles.
pub fn settings(kind: Kind) -> &'static [Setting] {
    match kind {
        Kind::Bookcase => const { &[] },
        Kind::Dining => const { &[LEGS, TOP, tick(Field::Extra, "Stretchers between the legs")] },
        Kind::Coffee => const { &[LEGS, TOP, tick(Field::Extra, "A shelf underneath")] },
        Kind::Side => const { &[LEGS, TOP, tick(Field::Extra, "A drawer")] },
        Kind::Desk => const { &[choice(Field::Choice, "Legs", &[BY_STYLE, "Square", "Tapered", "Turned"]), number("Drawers", 6), tick(Field::Extra, "Drawers at both ends")] },
        Kind::Chair => const { &[BACKS, SEAT, tick(Field::Extra, "Arms")] },
        Kind::Stool => const { &[choice(Field::Choice, "Seat shape", &[BY_STYLE, "Square", "Round"]), SEAT] },
        Kind::Bench => const { &[tick(Field::Extra, "A back"), BACKS, SEAT] },
        Kind::Armchair => const { &[ARMS, UNDER] },
        Kind::Sofa => const { &[number("Seats", 6), ARMS, UNDER] },
        Kind::Cupboard => const { &[number("Doors", 6), HANDLES, STANDS, tick(Field::Extra, "A drawer under the doors")] },
        Kind::Drawers => const { &[number("Drawers", 8), HANDLES, STANDS, tick(Field::Extra, "Two small drawers at the top")] },
        Kind::Sideboard => const { &[number("Doors", 6), HANDLES, STANDS, tick(Field::Extra, "Drawers in the middle")] },
        Kind::Kitchen => const { &[number("Units", 12), HANDLES, tick(Field::Extra, "A drawer over each door")] },
        Kind::TableLamp | Kind::FloorLamp | Kind::Pendant | Kind::Sconce => const { &[SHADES, SHINE] },
        Kind::Chandelier => const { &[choice(Field::Second, "Arms", &["As its size has it", "Two", "Four", "Eight candles on four"]), SHINE] },
        Kind::Candlestick => const { &[choice(Field::Second, "Candles", &["As its size has it", "One", "Three"]), SHINE] },
        Kind::Bed => const { &[choice(Field::Choice, "Headboard", &[BY_STYLE, "Panel", "Slats", "Rails"]), choice(Field::Second, "Pillows", &["As its width has it", "One", "Two"]), tick(Field::Extra, "A footboard")] },
    }
}
