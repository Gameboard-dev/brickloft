//! A single flower head: one bloom with no plant under it, large enough to
//! be a centrepiece, that faces whichever way it is tilted and turned.
//!
//! The other flowers here are plants: stems, leaves, and at their tops
//! heads a dozen cells across. At that size a rose is a ball with a heart,
//! which is why the rose bush was taken out. A bloom forty cells across has
//! room for what a rose is recognised by: separate petals, each a curved
//! sheet, furled tight at the heart and opening outwards in a spiral.
//!
//! Every flower in the list is built by one piece of machinery, [`petal`]:
//! a sheet two cells thick that springs from near the bloom's axis, leans
//! out, cups back in, may roll its lip out again, and is curved across its
//! width. A flower is a table of those numbers for each petal from the
//! innermost to the outermost, and **How open it is** moves every petal
//! between its numbers in the bud and full blown. The petals are set by the
//! golden angle, as the peony's are (see [`super::peony`]): no petal stands
//! behind another in any round, and the eye joins them into the spirals a
//! real bloom shows. Rings of petals, the other way, stack them in spokes.
//!
//! A petal is ribs side by side, each a chain of round cones, close enough
//! to be one sheet. A new kind of part, a true curved sheet with a distance
//! of its own, was the alternative. It was not taken: a sheet bent three
//! ways has no exact distance in closed form, an approximate one has to be
//! scaled down to stay under the truth (see `sdf`, and what smooth noise
//! cost an oak's leaves in [`super::Grown::leaves`]), and a union of round
//! cones is exact with nothing to make up for.
//!
//! The bloom is grown along its own axis and then turned as a whole, part
//! by part, so the fitter still lays every brick on the grid. Turning the
//! finished bricks instead would be the 24 poses a form already has
//! (`Spec::turned`), which cannot lay a rose down at a slant; and turning
//! the grid turns everything else on it too.

use super::{vogel, Grown, Kind, Part, Shape, Tree, EXTRA, GOLDEN_ANGLE, LEAF, LEAF_DARK, LEAF_LIGHT, LIMB, STAR, TOP, TRUNK};
use glam::DVec3;
use std::f64::consts::{FRAC_PI_2, TAU};

/// Which flower a single flower head is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flower {
    /// a high-centred double rose, the florist's rose
    Rose,
    /// a water lily: a star of pointed petals round a heart of stamens
    WaterLily,
    /// a gerbera, the large daisy: straps round a disc
    Gerbera,
}

/// The flowers on offer, with the names they are written and read by.
pub const FLOWERS: [(Flower, &str); 3] = [(Flower::Rose, "rose"), (Flower::WaterLily, "waterlily"), (Flower::Gerbera, "gerbera")];

/// What a single flower head has beyond what every plant has: which
/// flower it is, which way it faces, and what lies with it.
///
/// Its petals' number is the plant's `fullness`, how open it is the
/// plant's `bend`, and its sepals the plant's `extras`: those three mean
/// for a bloom what they mean for a peony. These five are kept apart from
/// them and not in the pine's fields (`girth`, `baubles`, `hang`), which a
/// bloom has no use for: those are held to a pine's ranges where they are
/// read, and a turn of 359 degrees is no trunk width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bloom {
    pub flower: Flower,
    /// how far its face is tipped over from looking straight up, in degrees: 0 to [`MOST_TILT`]
    pub tilt: i32,
    /// which way round it is tipped, in degrees about the upright: 0 to 359, 0 being towards +x
    pub turn: i32,
    /// whether it has a short length of stem
    pub stem: bool,
    /// whether a few fallen petals lie beside it
    pub fallen: bool,
}

/// The most a bloom tilts: on its side. Past that it would face the floor,
/// which the form's own quarter turns give.
pub const MOST_TILT: i32 = 90;

impl Default for Bloom {
    fn default() -> Self {
        Bloom { flower: Flower::Rose, tilt: 0, turn: 0, stem: false, fallen: false }
    }
}

impl Bloom {
    /// The same settings, each within what it may be.
    pub fn clamped(self) -> Bloom {
        Bloom { tilt: self.tilt.clamp(0, MOST_TILT), turn: self.turn.rem_euclid(360), ..self }
    }

    /// As a save keeps it: `flower,tilt,turn,stem,fallen`.
    pub fn text(&self) -> String {
        format!("{},{},{},{},{}", FLOWERS[self.flower as usize].1, self.tilt, self.turn, self.stem as u8, self.fallen as u8)
    }

    /// Read from `flower,tilt,turn,stem,fallen` or the first few of them:
    /// what is not given stays as it starts out.
    pub fn read(value: &str) -> Result<Bloom, String> {
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        if parts.len() > 5 {
            return Err("expected `flower,tilt,turn,stem,fallen`, or the first few of them".into());
        }
        let flower = FLOWERS.iter().find(|f| f.1 == parts[0]).ok_or("unknown flower: rose, waterlily or gerbera")?.0;
        // (numbers from a file, which may be anyone's: a turn of any size is the same turn within one circle)
        let whole = |text: &str| text.parse::<i64>().map_err(|_| format!("`{text}` is not a whole number"));
        let mut bloom = Bloom { flower, ..Bloom::default() };
        if let Some(tilt) = parts.get(1) {
            bloom.tilt = whole(tilt)?.clamp(0, MOST_TILT as i64) as i32;
        }
        if let Some(turn) = parts.get(2) {
            bloom.turn = whole(turn)?.rem_euclid(360) as i32;
        }
        if let Some(stem) = parts.get(3) {
            bloom.stem = whole(stem)? != 0;
        }
        if let Some(fallen) = parts.get(4) {
            bloom.fallen = whole(fallen)? != 0;
        }
        Ok(bloom)
    }
}

// The colour numbers of a bloom. They are the plant's eight under other
// names: a bloom has no trunk or leaves, and four tones of petal are what
// make its layers read.
/// its stem
const STEM: u8 = TRUNK;
/// its sepals
const SEPAL: u8 = LEAF;
/// the throat of a petal: its lowest third, seen only down inside the bloom
const THROAT: u8 = LEAF_DARK;
/// the lip of a petal: its last fifth, which is what shows from above
const LIP: u8 = LEAF_LIGHT;
/// the swelling the petals spring from
const HIP: u8 = LIMB;
/// the outer petals
const OUTER: u8 = TOP;
/// the inner petals
const INNER: u8 = EXTRA;
/// stamens, or a daisy's disc
const HEART: u8 = STAR;

/// Half the thickness of a petal, in cells: each rib of it is a round cone
/// this thick, so a petal is a sheet of 1.9, a brick or two at any angle.
/// Thinner than a peony's ribs (2.5 through) on purpose: a rose's petals
/// stand three or four cells apart where they are furthest from one
/// another, and the gap between two of them is what shows them as two.
/// It is not thinner still because a round shoot much under two cells
/// through fills no cell by half where it crosses the grid at a slant
/// (see [`super::pine`]). Other thicknesses were not tried.
const THICK: f64 = 0.95;
/// How far apart the ribs of a petal lie, and how long each cone of a rib
/// is. The ribs must overlap to be one sheet: at 1.25 apart the hollows
/// between them are a quarter of a cell deep, under the half cell that is
/// the least the fitter shows.
const APART: f64 = 1.25;
const STEP: f64 = 1.7;

/// One petal, in the bloom's own frame: its axis is +z and it springs from
/// near the origin.
struct Petal {
    /// the angle round the axis its middle lies at
    at: f64,
    /// how far from the axis it springs, and how high
    root: f64,
    foot: f64,
    /// its length, and half its width at its widest
    long: f64,
    wide: f64,
    /// where along it it is widest (0 to 1), and how pointed its end: 0.5 a round end, 1 a straight taper
    widest: f64,
    point: f64,
    /// the angle from the axis it leaves at, and the angle it has turned to by half way along: the first is the bottom of the cup and the second its side
    lean: f64,
    rise: f64,
    /// how far it turns in towards the axis over its whole length, on top of that: what closes a bud
    cup: f64,
    /// how far its last third rolls out again, and how much more at its edges than its middle
    lip: f64,
    edges: f64,
    /// how far its edges stand out from its middle, as an angle at its tip: what lifts a petal's sides off the petal behind
    flare: f64,
    /// The radius it is curved to across its width. `None` wraps it round
    /// the bloom's axis at whatever distance it stands, which furls the
    /// innermost petals into tubes and lays the outer ones round the cup.
    bow: Option<f64>,
    /// how far one edge stands out and the other in, in cells: each petal tucked under the next
    tuck: f64,
    /// what the height of everything is multiplied by
    deep: f64,
    /// the colours of its throat, its middle and its lip
    shades: [u8; 3],
}

/// How wide a petal is at the share `s` of the way along it, as a share of
/// its widest: a narrow claw at the root, widening to `widest`, then closing
/// to the tip by a curve that `point` straightens.
fn outline(s: f64, widest: f64, point: f64) -> f64 {
    if s < widest {
        let u = s / widest;
        0.3 + 0.7 * (u * (2.0 - u))
    } else {
        let u = (s - widest) / (1.0 - widest);
        (1.0 - u * u).max(0.0).powf(point)
    }
}

/// Grow one petal as ribs of round cones.
///
/// A rib is followed out from the root by its heading, the angle between
/// it and the bloom's axis. The heading starts at `lean`, has turned to
/// `rise` by two fifths of the length, falls by `cup` over the whole
/// length, and over the last third rises again by `lip`. The ribs differ
/// in how far to the side they lie, in how much of the lip's roll they
/// have and in how far they flare, so the edges of a rose's outer petal
/// turn back further than its middle, which is what draws its end to
/// points (see [`rose`]).
fn petal(g: &mut Grown, p: &Petal) {
    let steps = ((p.long / STEP).ceil() as usize).max(2);
    let half = ((p.wide / APART).ceil() as i32).max(1);
    let ds = p.long / steps as f64;
    let (out, round) = (DVec3::new(p.at.cos(), p.at.sin(), 0.0), DVec3::new(-p.at.sin(), p.at.cos(), 0.0));
    let heading = |s: f64, w: f64| {
        let (turned, rolled) = (1.0 - (s / 0.4).min(1.0), ((s - 0.66) / 0.34).max(0.0));
        p.rise + (p.lean - p.rise) * turned * turned - p.cup * s * s + p.lip * rolled * rolled * (1.0 + p.edges * w * w) + p.flare * w * w * s
    };
    // each rib's distance from the axis and height, at each step
    let rib = |w: f64| -> Vec<(f64, f64)> {
        let (mut r, mut z) = (p.root, p.foot);
        let mut path = vec![(r, z)];
        for i in 0..steps {
            let heading = heading((i as f64 + 0.5) / steps as f64, w);
            r += heading.sin() * ds;
            z += heading.cos() * ds * p.deep;
            path.push((r, z));
        }
        path
    };
    let middle = rib(0.0);
    // Across its width the petal is an arc about a line through its
    // hollow side: the side towards the axis where it stands upright,
    // and the upper side where it lies out flat. Curving it about the
    // bloom's axis whatever its heading, tried first, hollowed a petal
    // lying flat sideways and not upwards: a water lily's were hooks.
    let place = |w: f64, path: &[(f64, f64)], i: usize| -> DVec3 {
        let s = i as f64 / steps as f64;
        let x = w * p.wide * outline(s, p.widest, p.point);
        let (stands, high) = middle[i];
        let facing = heading(s, 0.0);
        // along the petal, and out of its hollow side
        let (along, hollow) = (out * facing.sin() + DVec3::Z * facing.cos(), DVec3::Z * facing.sin() - out * facing.cos());
        // (wrapped round the axis, it is curved as a cone about the axis is there: to its distance from the axis over the cosine of its slope, so that a petal lying out flat is flat)
        let radius = p.bow.unwrap_or(stands / facing.cos().max(0.15)).max(1.2);
        // (never more than most of the way round: a petal that met itself behind the axis would be a tube twice as thick there)
        let angle = (x / radius).clamp(-2.7, 2.7);
        let back = round * angle.sin() - hollow * angle.cos();
        // where this rib has gone its own way from the middle one, by its lip
        let own = out * (path[i].0 - stands) + DVec3::Z * (path[i].1 - high);
        out * stands + DVec3::Z * high + hollow * radius + back * (radius - own.dot(hollow) + p.tuck * w * s) + along * own.dot(along)
    };
    let mut before: Vec<DVec3> = Vec::new();
    // from the middle rib outwards on each side, so that each rib knows the one inside it
    for side in [1.0, -1.0] {
        for j in 0..=half {
            if j == 0 && side < 0.0 {
                before = (0..=steps).map(|i| place(0.0, &middle, i)).collect();
                continue;
            }
            let w = side * j as f64 / half as f64;
            let path = rib(w);
            let points: Vec<DVec3> = (0..=steps).map(|i| place(w, &path, i)).collect();
            for i in 0..steps {
                // towards the tip the ribs close on one another: one that lies within its neighbour is left out
                if j > 0 && points[i].distance(before[i]) < 0.45 && points[i + 1].distance(before[i + 1]) < 0.45 {
                    continue;
                }
                let s = (i as f64 + 0.5) / steps as f64;
                // its margin is its lip's colour all round, not at its end alone: seen from above a petal is known by its edge
                let margin = s > 0.93 || (j == half && s > 0.5);
                let shade = if margin { p.shades[2] } else if s < 0.24 { p.shades[0] } else { p.shades[1] };
                g.cone(points[i], points[i + 1], THICK, THICK, shade);
            }
            before = points;
        }
    }
}

/// A rose: the high-centred double bloom of a hybrid tea.
///
/// What a rose is recognised by, and what is built:
///
/// - Many petals, the inner ones made over from stamens and smaller: a
///   double rose has 17 to 25 petals and a full one 26 to 40 by the American
///   Rose Society's classes, and a hybrid tea's are few and large.
///   https://www.marinrose.org/?p=487
///   https://doi.org/10.1371/journal.pone.0009288 (Dubois et al. 2010)
/// - A high centre: the innermost petals stay furled in a pointed spiral
///   that stands above the open ones round it.
///   https://garden.org/ideas/view/Calif_Sue/1187/An-Explanation-of-Rose-Shapes-and-Types/
/// - Outer petals that open into a cup and roll their lips back, more at
///   the edges than the middle, which draws each lip to a point. The roll
///   and its points are how a growing rose petal relieves the strain in its
///   margin (Zhang, Cohen, Moshe and Sharon 2025, "Geometrically frustrated
///   rose petals"). https://doi.org/10.1126/science.adt0672
/// - Five sepals under it, which lie against the bud and turn back as it
///   opens.
///
/// A petal's place `t` runs from 0 for the innermost to 1 for the
/// outermost. Its size does not change as the bloom opens, as a real
/// petal's does not: the innermost are narrow and the outermost as wide as
/// they are long. Wrapped round the axis where they stand, the narrow ones
/// near it go most of the way round and are the furled heart, and in the
/// bud the wide ones, held close, wrap half way round and over the top.
/// Opening changes four angles: how far a petal leans out where it
/// springs, what it has turned to by the side of the cup, how far it
/// turns in over the top, and how far its lip rolls out.
///
/// The golden angle is a choice and not a measurement: a wild rose's five
/// petals overlap in the order of a two-fifths spiral, and the many of a
/// double rose are close enough to a spiral for the eye. It is the one
/// turn that keeps every petal out from behind the last at any number.
fn rose(g: &mut Grown, full: f64, open: f64, size: f64, deep: f64) {
    // (its outer petals, a little longer than the spread, lie out to 1.14 of it when full blown: measured, and taken off here so that the spread asked for is the spread built)
    let size = size * 0.88;
    let petals = (10.0 + 20.0 * full).round() as usize;
    let twist = g.between(0.0, TAU);
    for k in 0..petals {
        let t = (k as f64 + 0.5) / petals as f64;
        let at = twist + k as f64 * GOLDEN_ANGLE + g.between(-0.12, 0.12);
        let long = size * (0.94 + 0.2 * t) * g.between(0.95, 1.05);
        // the innermost open last and least
        let opened = (open * (0.3 + 0.7 * t)).min(1.0);
        // The bottom of the cup: the outer petals leave nearly level and
        // turn up, in a bud as in a bloom. Leaving at the slope they end
        // at, which they did at first, made the bloom a funnel.
        let lean = 0.3 + (1.0 + 0.1 * opened) * t + g.between(-0.05, 0.05);
        // (the outermost fifth open alike: one petal further out than the rest made the bloom lopsided from above)
        let outer = (t / 0.8).min(1.0);
        let rise = opened * (0.2 * outer + 0.7 * opened * outer.powf(1.6) - 0.05) + g.between(-0.07, 0.07) * t;
        // in the bud every petal turns in over the top
        let cup = 0.08 + (1.0 - opened).powf(1.5) * (0.5 + 0.6 * t);
        let lip = opened.powf(0.8) * 1.0 * ((t - 0.45) / 0.55).max(0.0).powf(1.2);
        // Petals in turn are of two tones. Each petal overlaps the third and the fifth after it and not the next, so those that touch differ, and the eye parts them as it does by their shadows on a real bloom. One tone for the inner half and one for the outer, tried first, parted nothing: every petal lay against others of its own tone.
        let body = if k % 2 == 0 { INNER } else { OUTER };
        let shape = Petal {
            at,
            root: 0.9 + 0.12 * size * t,
            foot: 0.0,
            long,
            wide: size * (0.17 + 0.4 * t),
            widest: 0.62,
            point: 0.55,
            lean,
            rise,
            cup,
            lip,
            edges: 0.8,
            flare: 0.45 * opened * t,
            bow: None,
            tuck: 1.3,
            deep,
            shades: [if t < 0.7 { THROAT } else { body }, body, LIP],
        };
        petal(g, &shape);
    }
    // full blown, the heart has opened on its stamens
    if open > 0.88 {
        let (u, v) = (DVec3::X, DVec3::Y);
        for k in 1..=(size * 0.5) as usize {
            let (far, at) = vogel(k, 0.9);
            g.blob((u * at.cos() + v * at.sin()) * far + DVec3::Z * size * 0.34 * deep, DVec3::splat(1.1), HEART, false);
        }
    }
}

/// The sepals of a rose: five, a fifth of a turn apart, pointed, lying
/// against the bud and turned back flat under the open bloom.
fn sepals(g: &mut Grown, count: usize, open: f64, size: f64, deep: f64, hip: f64) {
    let twist = g.between(0.0, TAU);
    for k in 0..count {
        let shape = Petal {
            at: twist + TAU * k as f64 / count as f64,
            root: hip * 0.8 + size * 0.08,
            foot: -hip * 0.3,
            long: size * (0.78 - 0.2 * open),
            wide: (size * 0.095).max(1.3),
            widest: 0.3,
            point: 1.0,
            // (no further than level: it would be under the floor the bloom sits on)
            lean: 1.45,
            rise: (0.3 + 1.15 * open).min(1.45),
            cup: 1.0 * (1.0 - open),
            lip: 0.0,
            edges: 0.0,
            flare: 0.0,
            bow: Some(size * 0.5),
            tuck: 0.0,
            deep,
            shades: [SEPAL; 3],
        };
        petal(g, &shape);
    }
}

/// A water lily, *Nymphaea*: a star of pointed petals, the outer lying
/// out on the water and each round inside standing a little more upright,
/// about a heart of yellow stamens. Its petals are set in a spiral and
/// its sepals are four. https://en.wikipedia.org/wiki/Nymphaea
///
/// The rose's machinery with other numbers: narrow petals widest low down
/// and tapering straight to a point, hollowed across like a boat and not
/// wrapped round the axis, with no lip.
fn water_lily(g: &mut Grown, full: f64, open: f64, size: f64, deep: f64) {
    let petals = (14.0 + 22.0 * full).round() as usize;
    let twist = g.between(0.0, TAU);
    for k in 0..petals {
        let t = (k as f64 + 0.5) / petals as f64;
        let wide = (size * (0.11 + 0.07 * t)).max(1.3);
        let shape = Petal {
            at: twist + k as f64 * GOLDEN_ANGLE + g.between(-0.06, 0.06),
            // (outside the stamens, which an open lily shows)
            root: size * (0.17 + 0.1 * t),
            foot: size * 0.1 * (1.0 - t),
            long: size * (0.42 + 0.44 * t) * g.between(0.95, 1.03),
            wide,
            widest: 0.38,
            point: 0.95,
            lean: 0.3 + (0.3 + 0.7 * open) * t,
            rise: 0.05 + 0.1 * t + open * (0.4 + 0.95 * t) + g.between(-0.04, 0.04),
            cup: 0.25 + (1.0 - open) * (0.4 + 0.5 * t),
            lip: 0.0,
            edges: 0.0,
            flare: 0.0,
            bow: Some(wide * 1.5),
            tuck: 0.0,
            deep,
            shades: [THROAT, if k % 2 == 0 { INNER } else { OUTER }, LIP],
        };
        petal(g, &shape);
    }
    // the stamens: a low cushion in the heart
    let heart = size * 0.2;
    for k in 1..=((heart / 1.1).powi(2)) as usize {
        let (far, at) = vogel(k, 1.1);
        let rise = (1.0 - (far / heart).powi(2)).max(0.0);
        g.blob(DVec3::new(at.cos(), at.sin(), 0.0) * far + DVec3::Z * (size * 0.2 + 2.0 * rise) * deep, DVec3::splat(1.15), HEART, false);
    }
    // (and what they stand on)
    g.cone(DVec3::ZERO, DVec3::Z * size * 0.16 * deep, heart * 0.6, heart, HEART);
}

/// A gerbera, the Transvaal daisy: two rows of straps round a disc.
/// https://en.wikipedia.org/wiki/Gerbera_jamesonii
///
/// Its disc is seeds set by Vogel's model, as a sunflower's is, darker in
/// the middle. Its rays are counted in Fibonacci numbers as a daisy's are,
/// evenly round and not by the golden angle, because they are one row and
/// a row has no round behind it to stay out from behind.
fn gerbera(g: &mut Grown, full: f64, open: f64, size: f64, deep: f64) {
    let disc = (size * 0.27).max(2.0);
    g.blob(DVec3::Z * (1.0 + disc * 0.45) * deep, DVec3::splat(2.2), THROAT, false);
    for k in 1..=((disc / 1.2).powi(2)) as usize {
        let (far, at) = vogel(k, 1.2);
        let rise = (1.0 - (far / disc).powi(2)).max(0.0).sqrt();
        g.blob(DVec3::new(at.cos(), at.sin(), 0.0) * far + DVec3::Z * (1.6 + disc * 0.45 * rise) * deep, DVec3::splat(1.2), if far < disc * 0.5 { THROAT } else { HEART }, false);
    }
    let rays = if full < 0.34 { 13 } else if full < 0.67 { 21 } else { 34 };
    let twist = g.between(0.0, TAU);
    for (row, share) in [(0, 1.0), (1, 0.62)] {
        let count = rays;
        for k in 0..count {
            // (narrow enough to leave air between one ray's end and the next: without it the rays were a dish)
            let wide = (TAU * size * 0.8 / rays as f64 * 0.29).max(1.2);
            let shape = Petal {
                at: twist + TAU * (k as f64 + 0.5 * row as f64) / count as f64,
                root: disc * (0.95 - 0.1 * row as f64),
                foot: 0.3 + 1.2 * row as f64,
                long: (size - disc) * share * g.between(0.93, 1.03),
                wide,
                widest: 0.6,
                point: 0.5,
                // (both rows lie out level when open, the inner on the outer: standing any steeper it was a bowl with the disc at the bottom of it)
                lean: 0.25 + open * 1.25,
                rise: 0.2 + open * (1.3 - 0.1 * row as f64) + g.between(-0.05, 0.05),
                cup: (1.0 - open) * 1.1 - open * 0.2,
                lip: 0.0,
                edges: 0.0,
                flare: 0.0,
                bow: Some(wide * 2.5),
                tuck: 0.0,
                deep,
                shades: if row == 0 { [OUTER, OUTER, LIP] } else { [INNER; 3] },
            };
            petal(g, &shape);
        }
    }
}

/// Turn everything grown since `parts` about `pivot`: tipped over by
/// `tilt` towards +x, then turned by `turn` about the upright. A round
/// cone or a ball turned is the same solid elsewhere, so its distance is
/// as exact as before. (A flattened ball would not be, which is why a
/// bloom has none.)
fn turn_all(g: &mut Grown, parts: usize, pivot: DVec3, tilt: f64, turn: f64) {
    let spin = |v: DVec3| {
        let tipped = DVec3::new(v.x * tilt.cos() + v.z * tilt.sin(), v.y, v.z * tilt.cos() - v.x * tilt.sin());
        DVec3::new(tipped.x * turn.cos() - tipped.y * turn.sin(), tipped.x * turn.sin() + tipped.y * turn.cos(), tipped.z)
    };
    for part in &mut g.parts[parts..] {
        match &mut part.shape {
            Shape::Cone { a, ba, .. } => (*a, *ba) = (pivot + spin(*a - pivot), spin(*ba)),
            Shape::Blob { c, .. } => *c = pivot + spin(*c - pivot),
            // (a bloom has none)
            Shape::Column { .. } => {}
        }
    }
}

/// Set everything grown since `parts` down on the floor: moved straight up
/// or down until its lowest point touches it.
fn rest(g: &mut Grown, parts: usize) {
    let lowest = lowest(&g.parts[parts..]);
    if lowest.is_finite() {
        g.shift(parts, g.limbs.len(), DVec3::Z * -lowest);
    }
}

fn lowest(parts: &[Part]) -> f64 {
    parts.iter().map(|part| part.shape.bounds().0.z).fold(f64::INFINITY, f64::min)
}

/// A single flower head, `spread` to each side when full blown and
/// `height` from its foot to the top of its petals in bud.
///
/// A bloom's size is its petals' length, so the spread sets it, and the
/// height makes the bloom deeper or shallower than its kind is by nature
/// (by no more than two and a half times either way: past that a petal is
/// a blade).
pub(super) fn flower(g: &mut Grown, tree: &Tree, height: f64, spread: f64) {
    let bloom = tree.bloom.clamped();
    let (full, open) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    let natural = Tree::usual_size(Kind::Flower);
    let deep = (height / (spread * natural.0 / natural.1)).clamp(0.4, 2.5);
    let hip = (spread * 0.13).max(1.6);
    // the swelling the petals spring from, which the bloom sits on
    g.blob(DVec3::ZERO, DVec3::splat(hip), if tree.extras { HIP } else { THROAT }, false);
    match bloom.flower {
        Flower::Rose => rose(g, full, open, spread, deep),
        Flower::WaterLily => water_lily(g, full, open, spread, deep),
        Flower::Gerbera => gerbera(g, full, open, spread, deep),
    }
    if tree.extras {
        match bloom.flower {
            Flower::Rose => sepals(g, 5, open, spread, deep, hip),
            Flower::WaterLily => sepals(g, 4, open, spread * 1.1, deep, hip),
            // (a composite's green cup is many small bracts)
            Flower::Gerbera => sepals(g, 13, open.max(0.3) * 0.8, spread * 0.5, deep, hip),
        }
    }
    let stalk = (spread * 0.7).max(6.0);
    if bloom.stem {
        g.cone(DVec3::Z * -hip * 0.5, DVec3::Z * -(hip + stalk), (hip * 0.5).clamp(1.2, 3.0), (hip * 0.42).clamp(1.2, 3.0), STEM);
    }
    // turned about the middle of the bloom, so that it stays over its foot, and set down on whatever is then lowest
    let middle = DVec3::Z * height * 0.4;
    let (tilt, turn) = ((bloom.tilt as f64).to_radians(), (bloom.turn as f64).to_radians());
    turn_all(g, 0, middle, tilt, turn);
    rest(g, 0);
    let axis = DVec3::new(tilt.sin() * turn.cos(), tilt.sin() * turn.sin(), tilt.cos());
    let base = DVec3::Z * -lowest(&g.parts).min(0.0);
    g.limb_record(vec![base, base + axis * height], vec![hip, 1.0], None, 0);
    if bloom.fallen {
        fallen(g, bloom, spread, tilt, turn);
    }
}

/// A few petals fallen beside the bloom: each lying on the floor on its
/// back, its edges and tip turned up a little.
///
/// They lie on the side the bloom faces when it is tipped over, clear of
/// where its stem would lie, and all round it when it stands upright.
fn fallen(g: &mut Grown, bloom: Bloom, size: f64, tilt: f64, turn: f64) {
    let count = 3 + (g.unit() * 3.0) as usize;
    let any = g.between(0.0, TAU);
    for k in 0..count {
        let way = if tilt > 0.35 { turn + (k as f64 + g.between(0.1, 0.9)) / count as f64 * 3.4 - 1.7 } else { any + (k as f64 + g.between(0.0, 0.7)) * TAU / count as f64 };
        let far = size * g.between(1.25, 1.75);
        let at = DVec3::new(way.cos(), way.sin(), 0.0) * far;
        let parts = g.parts.len();
        let (wide, pointed) = match bloom.flower {
            Flower::Rose => (size * 0.3, 0.55),
            Flower::WaterLily => (size * 0.15, 0.95),
            Flower::Gerbera => ((size * 0.07).max(1.2), 0.5),
        };
        let long = size * match bloom.flower {
            Flower::Rose => 0.62,
            Flower::WaterLily => 0.7,
            Flower::Gerbera => 0.6,
        };
        let shape = Petal {
            at: g.between(0.0, TAU),
            root: 0.0,
            foot: 0.0,
            long,
            wide,
            widest: if bloom.flower == Flower::Rose { 0.62 } else { 0.45 },
            point: pointed,
            lean: FRAC_PI_2 + 0.1,
            rise: (FRAC_PI_2 + 0.1) - 0.5 * (0.5),
            cup: 0.5 * (0.5),
            lip: 0.0,
            edges: 0.0,
            flare: 0.0,
            bow: Some(wide * 2.2),
            tuck: 0.0,
            deep: 1.0,
            shades: [OUTER, OUTER, LIP],
        };
        petal(g, &shape);
        g.shift(parts, g.limbs.len(), at);
        rest(g, parts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{build, Built, Form, Spec};

    fn spec(flower: Flower, open: i32) -> Spec {
        let (height, spread) = Tree::usual_size(Kind::Flower);
        Spec { form: Form::Tree, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: spread, rise: Some(height), grain: 0, tree: Tree { bend: open, extras: true, bloom: Bloom { flower, ..Bloom::default() }, ..Tree::of(Kind::Flower) }, ..Spec::default() }
    }

    /// How far the bricks of some colours reach along each axis, and the lowest cell they have.
    fn extent(built: &Built, wanted: &dyn Fn(u8) -> bool) -> (DVec3, i32) {
        let of: Vec<_> = built.bricks.iter().zip(&built.shades).filter(|(_, s)| wanted(**s)).map(|(b, _)| b).collect();
        let reach = |k: usize| (of.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap_or(0) - of.iter().map(|b| b.min[k]).min().unwrap_or(0)) as f64;
        (DVec3::new(reach(0), reach(1), reach(2)), of.iter().map(|b| b.min[2]).min().unwrap_or(0))
    }

    fn petals(shade: u8) -> bool {
        matches!(shade, THROAT | LIP | OUTER | INNER)
    }

    #[test]
    fn a_rose_opens_from_a_closed_bud_to_a_bloom_as_wide_as_its_spread() {
        let (height, spread) = Tree::usual_size(Kind::Flower);
        let (bud, half, full) = (build(&spec(Flower::Rose, 0)).unwrap(), build(&spec(Flower::Rose, 50)).unwrap(), build(&spec(Flower::Rose, 100)).unwrap());
        let (bud_size, half_size, full_size) = (extent(&bud, &petals).0, extent(&half, &petals).0, extent(&full, &petals).0);
        println!("bud {bud_size}, half open {half_size}, full blown {full_size}; {} / {} / {} bricks", bud.bricks.len(), half.bricks.len(), full.bricks.len());
        // the bud is an egg, taller than wide and under half the full bloom across; full blown it is the spread to each side
        assert!(bud_size.x < spread && bud_size.y < spread && bud_size.z > bud_size.x.max(bud_size.y) && bud_size.z <= height + 1.0, "{bud_size}");
        assert!(half_size.x > bud_size.x + 4.0 && half_size.x < full_size.x - 4.0, "{bud_size} {half_size} {full_size}");
        for across in [full_size.x, full_size.y] {
            assert!((across - 2.0 * spread).abs() < 0.2 * spread, "full blown it is {across} across, of {} asked", 2.0 * spread);
        }
        // (and no taller: its heart stands as high as the bud did)
        assert!(full_size.z <= bud_size.z + 1.0);
        // The bud is closed: nothing can be seen down into it from above.
        // Its topmost cells over the middle are all near its tip, where an
        // open bloom's middle is a hollow between its petals.
        let top_at = |built: &Built, x: i32, y: i32| built.bricks.iter().filter(|b| b.min[0] <= x && x < b.min[0] + b.ext[0] && b.min[1] <= y && y < b.min[1] + b.ext[1]).map(|b| b.min[2] + b.ext[2]).max().unwrap_or(0);
        let sunk = |built: &Built| (-2..=2).flat_map(|x| (-2..=2).map(move |y| (x, y))).filter(|(x, y)| top_at(built, *x, *y) < extent(built, &petals).0.z as i32 - 8).count();
        assert_eq!(sunk(&bud), 0, "the bud is open at the top");
        // And its outer petals are wrapped over the inner: it narrows to
        // its tip, under half as wide seven eighths of the way up as at
        // its widest. Petals that stood straight would be a tube.
        let wide_at = |built: &Built, z: i32| {
            let across: Vec<i32> = built.bricks.iter().filter(|b| b.min[2] <= z && z < b.min[2] + b.ext[2]).flat_map(|b| [b.min[0], b.min[0] + b.ext[0]]).collect();
            across.iter().max().unwrap_or(&0) - across.iter().min().unwrap_or(&0)
        };
        let (tip, belly) = (wide_at(&bud, (bud_size.z * 0.875) as i32), (0..bud_size.z as i32).map(|z| wide_at(&bud, z)).max().unwrap());
        assert!(2 * tip < belly, "the bud is {tip} across near its tip and {belly} at its widest");
        // it has its two tones of petal and its lip in every state, and stamens only full blown
        for (built, stamens) in [(&bud, false), (&half, false), (&full, true)] {
            let count = |shade: u8| built.shades.iter().filter(|s| **s == shade).count();
            assert!(count(OUTER) > 100 && count(INNER) > 100 && count(LIP) > 20 && count(SEPAL) > 20, "{} {} {} {}", count(OUTER), count(INNER), count(LIP), count(SEPAL));
            assert_eq!(count(HEART) > 0, stamens);
        }
    }

    #[test]
    fn a_bloom_faces_the_way_it_is_tilted_and_turned_and_rests_on_the_floor() {
        let facing = |tilt: i32, turn: i32, stem: bool| build(&Spec { tree: Tree { bloom: Bloom { tilt, turn, stem, ..Bloom::default() }, ..spec(Flower::Rose, 60).tree }, ..spec(Flower::Rose, 60) }).unwrap();
        let (up, _) = extent(&facing(0, 0, false), &petals);
        for (tilt, turn) in [(0, 0), (30, 0), (45, 137), (90, 0), (90, 90), (90, 270), (60, 359)] {
            for stem in [false, true] {
                let built = facing(tilt, turn, stem);
                // never under the floor it is set on, and always touching it
                assert_eq!(extent(&built, &|_| true).1, 0, "tilted {tilt}, turned {turn}");
            }
        }
        // on its side towards +x it is as long along x as it was tall, and as tall as it was wide; turned a quarter, the same along y
        let (side, _) = extent(&facing(90, 0, false), &petals);
        let (quarter, _) = extent(&facing(90, 90, false), &petals);
        println!("upright {up}, on its side {side}, and turned a quarter {quarter}");
        assert!((side.x - up.z).abs() <= 3.0 && (side.z - up.x).abs() <= 3.0 && (side.y - up.y).abs() <= 3.0, "{up} {side}");
        assert!((quarter.y - up.z).abs() <= 3.0 && (quarter.z - up.x).abs() <= 3.0 && (quarter.x - up.y).abs() <= 3.0, "{up} {quarter}");
        // And it faces that way: its green is behind it. Tipped towards +x, the sepals and stem lie on the -x side of the petals' middle.
        let middle = |built: &Built, wanted: fn(u8) -> bool| {
            let of: Vec<DVec3> = built.bricks.iter().zip(&built.shades).filter(|(_, s)| wanted(**s)).map(|(b, _)| DVec3::new(b.min[0] as f64, b.min[1] as f64, b.min[2] as f64)).collect();
            of.iter().sum::<DVec3>() / of.len() as f64
        };
        let lying = facing(90, 0, true);
        assert!(middle(&lying, |s| s == SEPAL || s == STEM).x < middle(&lying, petals).x - 5.0);
        let other = facing(90, 180, true);
        assert!(middle(&other, |s| s == SEPAL || s == STEM).x > middle(&other, petals).x + 5.0);
    }

    #[test]
    fn each_flower_has_its_parts_and_only_those_asked_for() {
        let spread = Tree::usual_size(Kind::Flower).1;
        for (flower, name) in FLOWERS {
            let plain = Spec { tree: Tree { extras: false, ..spec(flower, 85).tree }, ..spec(flower, 85) };
            let all = Spec { tree: Tree { extras: true, bloom: Bloom { flower, stem: true, fallen: true, tilt: 20, turn: 40 }, ..plain.tree }, ..plain.clone() };
            let (bare, dressed) = (build(&plain).unwrap(), build(&all).unwrap());
            let count = |built: &Built, shade: u8| built.shades.iter().filter(|s| **s == shade).count();
            println!("{name}: {} bricks, and {} with its sepals, stem and fallen petals; {} across", bare.bricks.len(), dressed.bricks.len(), extent(&bare, &petals).0);
            assert!(bare.bricks.len() > 800, "{name}: {}", bare.bricks.len());
            assert_eq!((count(&bare, SEPAL), count(&bare, HIP), count(&bare, STEM)), (0, 0, 0), "{name}");
            assert!(count(&dressed, SEPAL) > 15 && count(&dressed, STEM) > 3, "{name}: {} of sepal, {} of stem", count(&dressed, SEPAL), count(&dressed, STEM));
            // a lily and a gerbera show their hearts
            assert_eq!(count(&bare, HEART) > 20, flower != Flower::Rose, "{name}");
            // fallen petals lie on the floor outside the bloom
            let out = |built: &Built| built.bricks.iter().filter(|b| DVec3::new(b.min[0] as f64, b.min[1] as f64, 0.0).length() > spread * 1.3 && b.min[2] < 5).count();
            assert!(out(&dressed) > 30 && out(&bare) == 0, "{name}: {} and {}", out(&dressed), out(&bare));
            // another seed is another bloom of the same flower
            assert_ne!(build(&Spec { tree: Tree { seed: 2, ..plain.tree }, ..plain.clone() }).unwrap().bricks, bare.bricks, "{name}");
        }
    }

    #[test]
    fn a_blooms_settings_are_read_back_as_written_and_kept_within_bounds() {
        for (flower, _) in FLOWERS {
            for bloom in [Bloom { flower, ..Bloom::default() }, Bloom { flower, tilt: 90, turn: 359, stem: true, fallen: false }, Bloom { flower, tilt: 37, turn: 1, stem: false, fallen: true }] {
                assert_eq!(Bloom::read(&bloom.text()), Ok(bloom));
            }
        }
        // the first few alone, and numbers out of range
        assert_eq!(Bloom::read("waterlily"), Ok(Bloom { flower: Flower::WaterLily, ..Bloom::default() }));
        assert_eq!(Bloom::read("rose, 30"), Ok(Bloom { tilt: 30, ..Bloom::default() }));
        assert_eq!(Bloom::read("rose,500,-90,7,0"), Ok(Bloom { tilt: 90, turn: 270, stem: true, ..Bloom::default() }));
        assert_eq!(Bloom { tilt: -4, turn: 725, ..Bloom::default() }.clamped(), Bloom { tilt: 0, turn: 5, ..Bloom::default() });
        for bad in ["", "tulip", "rose,steep", "rose,1,2,3,4,5", "rose;1", "rose,1e9999"] {
            assert!(Bloom::read(bad).is_err(), "{bad:?} was read");
        }
    }
}

#[cfg(test)]
mod measured {
    use super::*;
    use crate::par::Threads;
    use crate::spec::{build_with, Form, Spec};

    /// How many parts and bricks each flower is and how long it takes,
    /// printed: run with `--nocapture`. Not a check of anything. The bound
    /// on a plant's time is `tree::tests::a_tree_a_hundred_tall_builds_in_about_a_second`.
    #[test]
    #[ignore = "a measurement, not a check: run it by name with --nocapture"]
    #[cfg(not(debug_assertions))]
    fn how_long_a_bloom_takes() {
        let (height, spread) = Tree::usual_size(Kind::Flower);
        for (flower, name) in FLOWERS {
            for open in [0, 60, 100] {
                let spec = Spec { form: Form::Tree, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: spread, rise: Some(height), grain: 0, tree: Tree { bend: open, extras: true, bloom: Bloom { flower, ..Bloom::default() }, ..Tree::of(Kind::Flower) }, ..Spec::default() };
                let model = crate::tree::Model::new(&spec.tree, spec.points[0], height, spread);
                let timed = |threads: Threads| (0..5).map(|_| { let at = std::time::Instant::now(); build_with(&spec, true, threads).unwrap(); at.elapsed() }).min().unwrap();
                let built = build_with(&spec, true, Threads::ONE).unwrap();
                let (lo, hi) = model.bounds();
                println!("{name} {open}% open: {} parts, {} bricks, {:.0} by {:.0} by {:.0}, {:?} on one thread, {:?} on {}", model.parts(), built.bricks.len(), hi.x - lo.x, hi.y - lo.y, hi.z - lo.z, timed(Threads::ONE), timed(Threads::available()), Threads::available().count());
            }
        }
    }
}
