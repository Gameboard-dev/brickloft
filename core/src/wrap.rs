//! A pattern on a form that is not flat: a sphere or a dome, a profile spun
//! about an axis, a round tube or a torus, a pyramid.
//!
//! A pattern is tiles pushed straight through a flat form (see
//! [`crate::pattern`]). A curved form has no flat to push them through, and
//! the one rule a pattern keeps everywhere is that it colours a form and
//! does not reshape it. So here nothing is fitted and nothing is moved. The
//! form's own bricks are kept exactly where they are, and each is given the
//! number the pattern has at its place ([`colour`]).
//!
//! ## Where a brick is on the pattern
//!
//! A pattern is drawn on a flat sheet, so each kind of form is given a way
//! of laying a sheet over it (a [`Chart`]), chosen so that the sheet is
//! stretched as little as the form allows and a pattern with a middle keeps
//! its middle:
//!
//! - A sphere or a dome is seen from its pole for a rug or medallions: a
//!   place is as far from the sheet's middle as it is from the pole over
//!   the surface, and in the same direction. That is the azimuthal
//!   equidistant projection of a map of the world drawn about a pole
//!   (<https://en.wikipedia.org/wiki/Azimuthal_equidistant_projection>).
//!   Distances from the pole are true, so the medallion sits on the pole
//!   and is round, and a rug is its own mirror image four ways as on a
//!   floor. For bricks, boards and cobbles the same sphere is rings about
//!   its upright axis, as a dome of real masonry is laid.
//! - A profile spun about an axis is rings too: round the axis one way,
//!   along the profile the other.
//! - A round tube, and a torus, which is a tube along a circle: along its
//!   curve one way and round its section the other.
//! - A pyramid is rings round its four faces, and is seen from above for a
//!   rug or medallions.
//!
//! ## Rings that close
//!
//! A sheet wrapped round a ring has to meet itself where it comes back, or
//! there is a seam with half a brick on it. So every count round a ring is
//! made whole: a course of bricks round a ring is the whole number of
//! bricks nearest its length, each a little longer or shorter than a
//! brick on a floor to take up the difference. The count is made for each
//! course by itself, from that course's own length round. A course near
//! the pole of a dome is far shorter than one at its foot, and bricks of
//! one count all the way up would be slivers at the top: a real dome has
//! fewer bricks in each ring as the rings close in, and so has this.
//! Boards and cobbles are counted the same way, ring by ring.
//!
//! Medallions and a rug are one design and cannot change their count from
//! ring to ring. Away from a pole they are wrapped at the form's widest
//! ring and are squeezed where the form is narrower, as a label is on a
//! bottle's neck.
//!
//! ## Bricks cut smaller, never another shape
//!
//! A brick is one colour, so the pattern can be no finer than the bricks.
//! A brick is cut into smaller ones only where the smaller ones fill
//! exactly what it filled ([`parts`]): a block into cells, a ramp into
//! short lengths, and a sloped brick whose sides share a divisor into
//! smaller bricks of the same slope over a staircase of cells, as a
//! slanting edge of a floor is cut in `pattern::lay_tiles`. The surface is
//! the same surface afterwards, face for face.
//!
//! A sloped brick whose sides share no divisor cannot be cut that way, and
//! is left whole in the one colour the pattern has at its middle. A
//! sphere's own bricks are mostly such: corner pieces stretched to 2 by 3
//! by 10 cells and the like, since that is what makes a sphere smooth. So
//! a pattern on a sphere is coarser than the same pattern on a floor (see
//! [`tile_for`]). Fitting a patterned sphere cell by cell instead would
//! give bricks small enough for any pattern and a rougher sphere, and a
//! form that changed its surface when a pattern was chosen is the fault
//! the rule above is there to prevent.
//!
//! ## How many bricks
//!
//! Only bricks at the surface are cut: a block wholly inside the form is
//! left as it is, in the ground colour, since nothing of it shows. The
//! pieces are then joined again wherever neighbours of one shape have one
//! colour ([`crate::pattern::joined`]), so the count grows with the detail
//! of the pattern and not with the size of the form. A form whose surface
//! would still be more than [`MOST_PIECES`] pieces is coloured brick by
//! brick with nothing cut.

use crate::par::Threads;
use crate::pattern::{figured, joined, Drawn, Masonry, Mosaic, Numbers, STYLES};
use crate::shapes::{shape_info, CellBrick, Kind, CUBE};
use crate::spec::{Built, Form, Spec};
use glam::{DVec2, DVec3};
use std::f64::consts::{FRAC_PI_2, PI, SQRT_2, TAU};

/// The most pieces a form's surface is cut into. Past it the form's bricks
/// are coloured as they stand. A sheet of single cells this large is a
/// floor 700 cells square, more than the game places in one go without a
/// long wait.
const MOST_PIECES: usize = 500_000;

/// How far inside the form the middle of a block of cells may be, beyond
/// its own half diagonal, before none of it can show. The bricks follow
/// the form to within half a cell or so, and a cell whose nearest corner is
/// further in than that is covered by the cells over it.
const HIDDEN: f64 = 0.65;

/// How many bricks are worth a thread of their own to cut and colour: a
/// brick is a handful of pieces, each asked of the form's distance once or
/// twice and of the pattern once, a microsecond or two all told.
const BRICKS_A_THREAD: usize = 400;

/// A curve as straight pieces, for finding the place on it nearest another.
struct Line {
    points: Vec<DVec3>,
    /// how far along the curve each point is
    along: Vec<f64>,
}

impl Line {
    fn new(mut points: Vec<DVec3>, closed: bool) -> Line {
        if closed && points.first().zip(points.last()).is_some_and(|(a, b)| a.distance(*b) > 1e-6) {
            points.push(points[0]);
        }
        let mut along = Vec::with_capacity(points.len());
        let mut sum = 0.0;
        for (n, p) in points.iter().enumerate() {
            if n > 0 {
                sum += p.distance(points[n - 1]);
            }
            along.push(sum);
        }
        Line { points, along }
    }

    fn length(&self) -> f64 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// The place on the curve nearest `p`: how far along the curve it is,
    /// the place itself, and the way the curve runs there.
    ///
    /// Every piece is tried. A curve is a hundred or two pieces and a form
    /// asks for some tens of thousands of places, so a tube's pattern costs
    /// a few milliseconds this way; a grid of the pieces to look them up in
    /// would cost as much to build as it saved on any but the largest.
    fn nearest(&self, p: DVec3) -> (f64, DVec3, DVec3) {
        let mut best = (f64::INFINITY, 0.0, DVec3::ZERO, DVec3::X);
        for n in 1..self.points.len() {
            let (a, b) = (self.points[n - 1], self.points[n]);
            let run = b - a;
            let long = run.length_squared();
            if long < 1e-18 {
                continue;
            }
            let t = ((p - a).dot(run) / long).clamp(0.0, 1.0);
            let q = a + run * t;
            let far = q.distance_squared(p);
            if far < best.0 {
                best = (far, self.along[n - 1] + t * long.sqrt(), q, run / long.sqrt());
            }
        }
        (best.1, best.2, best.3)
    }

    /// The place so far along the curve.
    fn at(&self, along: f64) -> DVec3 {
        let n = self.along.partition_point(|s| *s < along).clamp(1, self.points.len() - 1);
        let (a, b) = (self.along[n - 1], self.along[n]);
        self.points[n - 1].lerp(self.points[n], if b > a { ((along - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 })
    }
}

/// How a sheet is laid over a form: where each place of the form is on it.
///
/// A place is two numbers. The first is the way a course of bricks runs:
/// round the form, as a share of the way round from 0 to 1, or along an
/// open tube in cells. The second is across the courses, in cells. (See
/// the top of this file for which form is laid which way, and why.)
enum Chart {
    /// A sphere, an ellipsoid or a part of one. `top` and `bottom` are the
    /// angles from straight up between which it has a surface.
    Ball { centre: DVec3, radii: DVec3, top: f64, bottom: f64 },
    /// A profile spun about the upright through `axis`: `profile` is the
    /// profile as (distance from the axis, height, 0).
    Turned { axis: DVec2, profile: Line },
    /// A round tube along a curve, `up` being the direction its top faces.
    Tube { curve: Line, closed: bool, radius: f64, up: DVec3 },
    /// A pyramid on a rectangle `half` to each side of `base`, which is the
    /// middle of its foot.
    Pyramid { base: DVec3, half: [f64; 2], height: f64 },
}

impl Chart {
    /// The chart of a form, if it is one of those a sheet is laid over.
    fn of(spec: &Spec, path: &[DVec3]) -> Option<Chart> {
        let first = *spec.points.first()?;
        match spec.form {
            Form::Sphere => {
                let radii = spec.stretch.max(DVec3::splat(0.05)) * spec.radius;
                let (lo, hi) = spec.cut.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
                Some(Chart::Ball { centre: first, radii, top: (hi / radii.z).clamp(-1.0, 1.0).acos(), bottom: (lo / radii.z).clamp(-1.0, 1.0).acos() })
            }
            Form::Revolve if path.len() >= 2 => {
                let axis = first.truncate();
                Some(Chart::Turned { axis, profile: Line::new(path.iter().map(|p| DVec3::new(p.truncate().distance(axis), p.z, 0.0)).collect(), false) })
            }
            Form::Pyramid => Some(Chart::Pyramid { base: first, half: [spec.radius * spec.stretch.x.max(0.05), spec.radius * spec.stretch.y.max(0.05)], height: spec.rise.unwrap_or(spec.radius).max(1.0) }),
            Form::Torus | Form::Circle | Form::Arc | Form::Arch(_) | Form::Spline | Form::Bezier if (spec.form == Form::Torus || (spec.tube && !spec.fill)) && path.len() >= 2 => {
                let closed = matches!(spec.form, Form::Torus | Form::Circle) || (spec.closed && matches!(spec.form, Form::Arc | Form::Spline));
                // the tube's top is across the plane its curve lies in, or straight up for a curve that lies in none
                let lies = (0..3).rev().find(|a| path.iter().all(|p| (p[*a] - path[0][*a]).abs() < 1e-6));
                let mut up = DVec3::ZERO;
                up[lies.unwrap_or(2)] = 1.0;
                Some(Chart::Tube { curve: Line::new(path.to_vec(), closed), closed, radius: spec.thick / 2.0, up })
            }
            _ => None,
        }
    }

    /// A sphere's two lengths: the radius its upright circles are drawn
    /// with, and the one its level circles are. They are one length for a
    /// sphere. For an ellipsoid they are the nearest a single length comes
    /// to each, which is as true as a sheet on an ellipsoid can be made.
    fn ball(radii: DVec3) -> (f64, f64) {
        let level = (radii.x + radii.y) / 2.0;
        ((radii.z + level) / 2.0, level)
    }

    /// Where a place of the form is on the sheet (see [`Chart`]).
    fn at(&self, p: DVec3) -> (f64, f64) {
        match self {
            Chart::Ball { centre, radii, .. } => {
                let d = ((p - *centre) / *radii).normalize_or(DVec3::Z);
                // (counted up from the equator, so that a dome's lowest course of bricks stands whole on its foot)
                (d.y.atan2(d.x).rem_euclid(TAU) / TAU, (FRAC_PI_2 - d.z.clamp(-1.0, 1.0).acos()) * Chart::ball(*radii).0)
            }
            Chart::Turned { axis, profile } => {
                let out = p.truncate() - *axis;
                (out.y.atan2(out.x).rem_euclid(TAU) / TAU, profile.nearest(DVec3::new(out.length(), p.z, 0.0)).0)
            }
            Chart::Tube { curve, closed, radius, up } => {
                let (along, on, way) = curve.nearest(p);
                // round the section from its under side, so that where the sheet meets itself is underneath
                let over = (*up - way * up.dot(way)).normalize_or(DVec3::Z);
                let aside = way.cross(over);
                let round = ((p - on).dot(aside).atan2(-(p - on).dot(over)) + PI).rem_euclid(TAU);
                (if *closed { (along / curve.length().max(1e-9)).rem_euclid(1.0) } else { along }, round * *radius)
            }
            Chart::Pyramid { base, half, height } => {
                // round the four faces from the middle of the face towards x, by the lengths of their feet
                let (a, b) = ((p.x - base.x) / half[0], (p.y - base.y) / half[1]);
                let most = a.abs().max(b.abs()).max(1e-9);
                let (a, b) = (a / most, b / most);
                let round = if a >= 1.0 - 1e-12 && b > -1.0 {
                    half[1] * (1.0 + b)
                } else if b >= 1.0 - 1e-12 {
                    2.0 * half[1] + half[0] * (1.0 - a)
                } else if a <= -1.0 + 1e-12 {
                    2.0 * half[1] + 2.0 * half[0] + half[1] * (1.0 - b)
                } else {
                    4.0 * half[1] + 2.0 * half[0] + half[0] * (1.0 + a)
                };
                ((round / (4.0 * (half[0] + half[1]))).rem_euclid(1.0), (p.z - base.z) * self.slope(*half, *height))
            }
        }
    }

    /// How far up a pyramid's face a cell of height is, the two pairs of
    /// faces taken as one slope between them.
    fn slope(&self, half: [f64; 2], height: f64) -> f64 {
        ((half[0] + half[1]) / 2.0).hypot(height) / height
    }

    /// How far it is all the way round the form along the course at
    /// `across`, in cells; `None` where a course does not come back to its
    /// start, as along an open tube.
    fn round(&self, across: f64) -> Option<f64> {
        match self {
            Chart::Ball { radii, .. } => {
                let (upright, level) = Chart::ball(*radii);
                Some(TAU * level * (across / upright).cos().max(0.0))
            }
            Chart::Turned { profile, .. } => Some(TAU * profile.at(across).x.max(0.0)),
            Chart::Tube { curve, closed, .. } => closed.then(|| curve.length()),
            Chart::Pyramid { half, height, .. } => Some(4.0 * (half[0] + half[1]) * (1.0 - across / (height * self.slope(*half, *height))).clamp(0.0, 1.0)),
        }
    }

    /// How far it is all the way round across the courses, where they come
    /// back to the first: round a tube's section.
    fn about(&self) -> Option<f64> {
        match self {
            Chart::Tube { radius, .. } => Some(TAU * radius),
            _ => None,
        }
    }

    /// Between which two numbers across the courses the form has a
    /// surface (for a tube, once round its section).
    fn rows(&self) -> (f64, f64) {
        match self {
            Chart::Ball { radii, top, bottom, .. } => ((FRAC_PI_2 - bottom) * Chart::ball(*radii).0, (FRAC_PI_2 - top) * Chart::ball(*radii).0),
            Chart::Turned { profile, .. } => (0.0, profile.length()),
            Chart::Tube { radius, .. } => (0.0, TAU * radius),
            Chart::Pyramid { half, height, .. } => (0.0, height * self.slope(*half, *height)),
        }
    }

    /// The longest way along a course: the form's widest ring, or the
    /// length of a tube.
    fn long(&self) -> f64 {
        match self {
            Chart::Ball { .. } | Chart::Turned { .. } | Chart::Pyramid { .. } => {
                let (lo, hi) = self.rows();
                (0..=32).map(|n| self.round(lo + (hi - lo) * n as f64 / 32.0).unwrap_or(0.0)).fold(0.0, f64::max)
            }
            Chart::Tube { curve, .. } => curve.length(),
        }
    }

    /// For a form with a pole to see it from, where a place is on a flat
    /// sheet laid about the pole, in cells from it each way; and how far
    /// the sheet reaches each way before it runs off the form.
    ///
    /// A whole sphere has two poles and two sheets, the lower the upper's
    /// mirror image in the equator, where the two meet. A part of a sphere
    /// is seen from whichever pole it has (from the upper, if it has both
    /// or neither). A pyramid is seen from straight above: its faces are
    /// flat, and from above each is the floor it stands on, drawn out up
    /// the slope.
    fn pole(&self, p: DVec3) -> Option<(DVec2, DVec2)> {
        match self {
            Chart::Ball { centre, radii, top, bottom } => {
                let d = ((p - *centre) / *radii).normalize_or(DVec3::Z);
                let angle = d.z.clamp(-1.0, 1.0).acos();
                let (whole, under) = (*top < 1e-9 && *bottom > PI - 1e-9, *top > 1e-9 && *bottom > PI - 1e-9);
                let (from_pole, furthest) = if whole { (angle.min(PI - angle), FRAC_PI_2) } else if under { (PI - angle, PI - *top) } else { (angle, *bottom) };
                let upright = Chart::ball(*radii).0;
                // (the square that fits inside the circle the sheet is cut off at)
                Some((DVec2::new(d.x, d.y).normalize_or_zero() * from_pole * upright, DVec2::splat(furthest * upright / SQRT_2)))
            }
            Chart::Pyramid { base, half, .. } => Some((DVec2::new(p.x - base.x, p.y - base.y), DVec2::new(half[0], half[1]))),
            Chart::Turned { .. } | Chart::Tube { .. } => None,
        }
    }
}

/// How many cells across a tile of the pattern is on a form.
///
/// On a floor a tile is a cell, and it is here wherever the form's bricks
/// at the surface are cells: on a pyramid, and on a straight tube. A
/// sphere laid out whole is stretched corner pieces that cannot be cut
/// (see the top of this file), each some 2 or 3 cells by 3 to 12, and a
/// turned profile is such pieces 1 or 2 cells by 2 or 3. A pattern drawn
/// finer than its bricks does not come out smaller, it comes out as
/// noise: each brick takes whatever the pattern happens to have at its
/// middle, and a line one tile wide is there on one brick and gone on the
/// next. So the tile is made as large as the bricks that have to carry
/// it.
///
/// `sizes` is, for each piece at the surface, how large it is and how
/// much it counts: for a sloped brick, the square root of its two longer
/// sides multiplied together, counted by that same length; for a block,
/// 1, counted by half its cells (a cell at the surface shows a face or
/// two of its six). The tile is the mean size so weighed, rounded up from
/// a quarter over a whole number, and 6 at the most: past that a brick of
/// the pattern is larger than most forms. Weighed by area instead, the
/// few longest bricks of a dome set its tile at 6, where its picture
/// reads well at 4; a turned profile comes to 2 and a dome 60 in radius
/// to 4.
fn tile_for(sizes: impl Iterator<Item = (f64, f64)>) -> f64 {
    let (sum, all) = sizes.fold((0.0, 0.0), |(sum, all), (size, share)| (sum + size * share, all + share));
    if all <= 0.0 { 1.0 } else { (sum / all - 0.25).ceil().clamp(1.0, 6.0) }
}

/// The rule of a style, as it is asked on a chart.
enum Rule {
    /// medallions or a rug: the number at a place in tiles from the middle,
    /// and how many tiles the sheet is each way (see [`Laid::sheet`])
    Figured(Box<dyn Fn(f64, f64) -> u8 + Send + Sync>, (f64, f64)),
    Bricks(Masonry),
    Cobbles(Masonry),
    Planks { broad: f64, seed: u64 },
}

/// A pattern laid over a form: asked for the number at any place of it.
struct Laid {
    chart: Chart,
    rule: Rule,
    kind: Drawn,
    /// cells to a tile (see [`tile_for`])
    tile: f64,
    /// the chart's longest course, and between which heights it has a surface
    long: f64,
    rows: (f64, f64),
}

impl Laid {
    fn new(chart: Chart, settings: &Mosaic, tile: f64) -> Laid {
        let kind = STYLES[settings.style.min(STYLES.len() - 1)].drawn;
        let rule = match kind {
            Drawn::Bricks => Rule::Bricks(Masonry::new(settings, false)),
            Drawn::Cobbles => Rule::Cobbles(Masonry::new(settings, true)),
            Drawn::Planks => Rule::Planks { broad: settings.size.clamp(1, 16) as f64, seed: settings.seed },
            Drawn::Rug | Drawn::Medallions => {
                // (a rug is set out to fill its sheet; medallions repeat, and are set out as on a floor too large to matter)
                let (wide, deep) = Laid::sheet(&chart, tile, 0.0);
                let (colour, period) = if kind == Drawn::Rug { figured(kind, (wide / 2.0).floor(), (deep / 2.0).floor(), settings.seed) } else { figured(kind, 1.0e6, 1.0e6, settings.seed) };
                Rule::Figured(colour, Laid::sheet(&chart, tile, period))
            }
        };
        let (long, rows) = (chart.long(), chart.rows());
        Laid { chart, rule, kind, tile, long, rows }
    }

    /// How many tiles a sheet for medallions or a rug is each way. Round a
    /// ring it is a whole number of tiles, and of repeats where the
    /// pattern repeats every `period` tiles, so that it meets itself.
    fn sheet(chart: &Chart, tile: f64, period: f64) -> (f64, f64) {
        let whole = |cells: f64, closes: bool| {
            let tiles = cells / tile;
            if !closes {
                tiles
            } else if period > 0.0 {
                (tiles / period).round().max(1.0) * period
            } else {
                // (an even number, so that a rug's middle is a line of the grid)
                (tiles / 2.0).round().max(2.0) * 2.0
            }
        };
        let (lo, hi) = chart.rows();
        (whole(chart.long(), chart.round(lo).is_some()), whole(hi - lo, chart.about().is_some()))
    }

    /// The pattern's number at a place of the form.
    fn shade(&self, p: DVec3) -> u8 {
        let (along, across) = self.chart.at(p);
        match &self.rule {
            Rule::Figured(colour, (wide, deep)) => {
                if let Some((at, reach)) = self.chart.pole(p) {
                    let (at, reach) = (at / self.tile, (reach / self.tile).floor());
                    // (a rug stops at its own edge; medallions go on to the form's)
                    return if self.kind == Drawn::Rug && (at.x.abs() >= reach.x || at.y.abs() >= reach.y) { 0 } else { colour(at.x, at.y) };
                }
                let (lo, hi) = self.rows;
                let x = if self.chart.round(lo).is_some() { along * wide } else { along / self.tile };
                let y = (across - lo) / (hi - lo).max(1e-9) * deep;
                let (u, v) = (x - (wide / 2.0).floor(), y - (deep / 2.0).floor());
                if self.kind == Drawn::Rug && (u.abs() >= (wide / 2.0).floor() || v.abs() >= (deep / 2.0).floor()) { 0 } else { colour(u, v) }
            }
            Rule::Bricks(wall) => {
                if wall.upright {
                    // Courses run the other way: across the rings, each
                    // course a slice from pole to pole like the gores of a
                    // globe. A slice narrows as the rings do, which no
                    // count can mend, so the courses are counted once, at
                    // the widest ring.
                    let lo = self.rows.0;
                    let (long, closes) = (self.long, self.chart.round(lo).is_some());
                    let about = self.chart.about();
                    self.bricks(wall, (across - lo) / about.unwrap_or(1.0), &|_| about, if closes { along * long } else { along }, closes.then_some(long))
                } else {
                    self.bricks(wall, along, &|across| self.chart.round(across), across, self.chart.about())
                }
            }
            Rule::Cobbles(wall) => self.cobble(wall, along, across),
            Rule::Planks { broad, seed } => self.board(*broad, *seed, along, across),
        }
    }

    /// A height across the courses in the pattern's own tiles, and after
    /// how many courses `deep` tiles deep the courses come round again
    /// (nothing if they do not): round a tube's section the courses are
    /// made a whole number, each a little deeper or shallower for it.
    fn across(&self, across: f64, about: Option<f64>, deep: f64) -> (f64, i64) {
        match about {
            Some(round) => {
                let rows = ((round / self.tile / deep).round() as i64).max(1);
                (across / round * rows as f64 * deep, rows)
            }
            None => (across / self.tile, 0),
        }
    }

    /// Bricks: the number of the brick at a place, or the mortar's.
    ///
    /// `along` is the way the courses run and `round` how far it is round
    /// the course at a height, where it comes back to its start; `across`
    /// is the height and `about` how far round that way is.
    fn bricks(&self, wall: &Masonry, along: f64, round: &dyn Fn(f64) -> Option<f64>, across: f64, about: Option<f64>) -> u8 {
        let (height, rows) = self.across(across, about, wall.deep);
        // (the wall's own sums turn a place over for upright courses, which has been done here already, by which way is called along)
        let wall = wall.closing(rows, 0);
        let row = wall.course_at(height - wall.mortar / 2.0);
        // the course's middle, back in cells, and how far round it is there
        let middle = (wall.course(row) + wall.course(row + 1) + wall.mortar) / 2.0;
        let middle = match about {
            Some(round) => middle / (rows as f64 * wall.deep) * round,
            None => middle * self.tile,
        };
        let (place, count) = match round(middle) {
            Some(round) => {
                let count = ((round / self.tile / wall.long).round() as i64).max(1);
                (along * count as f64 * wall.long, count)
            }
            None => (along / self.tile, 0),
        };
        wall.closing(rows, count).colour(DVec2::new(place, height))
    }

    /// Cobbles: the number of the stone at a place, or the mortar's.
    ///
    /// As on a floor, a stone is what is nearer its own point than any
    /// other, and the points stand one to each square of a grid, moved
    /// about (see [`Masonry`]). Here the grid is rings: a row of points
    /// round each, as many as the ring is long, so the stones stay one
    /// size from the foot of a dome to its pole. How near a point is, is
    /// measured on the sheet with every ring taken to be as long as the
    /// one the place is on. That is true for the stones about the place,
    /// which are all that matter to it.
    fn cobble(&self, wall: &Masonry, along: f64, across: f64) -> u8 {
        let (height, rows) = self.across(across, self.chart.about(), wall.deep);
        let (lo, hi) = self.rows;
        // how far round the place's own ring is, in tiles
        let here = self.chart.round(across).map(|round| round / self.tile);
        let row = (height / wall.deep).floor() as i64;
        // the nearest point and the next nearest: how far, where from the place, and whose
        let mut nearest = [(f64::INFINITY, DVec2::ZERO, (0i64, 0i64)); 2];
        for row in row - 2..=row + 2 {
            let own = if rows > 0 { row.rem_euclid(rows) } else { row };
            // the ring's middle in cells: a ring off the end of the form has no stones
            let middle = if rows > 0 { (own as f64 + 0.5) / rows as f64 * self.chart.about().unwrap_or(1.0) } else { (row as f64 + 0.5) * wall.deep * self.tile };
            if rows == 0 && (middle < lo - 0.5 * wall.deep * self.tile || middle > hi + 0.5 * wall.deep * self.tile) {
                continue;
            }
            let aside = |nth: i64, which: u64| (wall.number(own, nth, which) - 0.5) * 0.8 * wall.uneven[which as usize - 1];
            let mut point = |nth: i64, from: DVec2| {
                let mut from = from;
                if rows > 0 {
                    let all = rows as f64 * wall.deep;
                    from.y -= all * (from.y / all).round();
                }
                let far = from.length_squared();
                if far < nearest[0].0 - 1e-9 {
                    nearest = [(far, from, (own, nth)), nearest[0]];
                } else if far < nearest[1].0 && (own, nth) != nearest[0].2 {
                    nearest[1] = (far, from, (own, nth));
                }
            };
            match (self.chart.round(middle), here) {
                (Some(round), Some(here)) => {
                    let count = ((round / self.tile / wall.long).round() as i64).max(1);
                    // (every other ring is moved by half a stone, so that four stones never meet at one corner)
                    let moved = if own.rem_euclid(2) == 1 { 0.5 } else { 0.0 };
                    let about = (along * count as f64 - moved).floor() as i64;
                    let all: Vec<i64> = if count <= 5 { (0..count).collect() } else { (about - 2..=about + 2).map(|nth| nth.rem_euclid(count)).collect() };
                    for nth in all {
                        let share = (nth as f64 + 0.5 + moved + aside(nth, 1)) / count as f64;
                        let turn = along - share;
                        point(nth, DVec2::new((turn - turn.round()) * here, height - (row as f64 + 0.5 + aside(nth, 2)) * wall.deep));
                    }
                }
                _ => {
                    let place = along / self.tile;
                    let moved = wall.moved(own);
                    let about = ((place + moved) / wall.long).floor() as i64;
                    for nth in about - 2..=about + 2 {
                        point(nth, DVec2::new(place - ((nth as f64 + 0.5 + aside(nth, 1)) * wall.long - moved), height - (row as f64 + 0.5 + aside(nth, 2)) * wall.deep));
                    }
                }
            }
        }
        let [(near, from, (own, nth)), (next, other, _)] = nearest;
        // how far inside its stone the place is: from the line halfway to the next nearest point
        let inside = (next - near) / (2.0 * from.distance(other)).max(1e-9) - wall.mortar / 2.0;
        if !near.is_finite() || (wall.mortar > 0.0 && next.is_finite() && inside <= 1e-9) {
            0
        } else {
            1 + (wall.number(own, nth, 3) * 7.0) as u8 % 7
        }
    }

    /// Boards: the number of the board at a place.
    ///
    /// As on a floor (see `pattern::planks`), a row of boards is laid end
    /// to end, each of a length drawn from the seed and never the shade of
    /// the board before it. Round a ring the row is as long as the ring,
    /// and its last board runs on to meet the first where less than half a
    /// board would be left.
    fn board(&self, broad: f64, seed: u64, along: f64, across: f64) -> u8 {
        let (height, rows) = self.across(across, self.chart.about(), broad);
        let row = (height / broad).floor() as i64;
        let own = if rows > 0 { row.rem_euclid(rows) } else { row };
        let middle = match self.chart.about() {
            Some(round) if rows > 0 => (own as f64 + 0.5) / rows as f64 * round,
            _ => (row as f64 + 0.5) * broad * self.tile,
        };
        let (place, all) = match self.chart.round(middle) {
            Some(round) => {
                let all = (round / self.tile).round().max(1.0);
                (along * all, all)
            }
            None => (along / self.tile, f64::INFINITY),
        };
        let mut numbers = Numbers(seed ^ ((own + 1) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let (mut at, mut last) = (0.0, u8::MAX);
        loop {
            let long = if at == 0.0 { 1.0 + (numbers.unit() * 12.0 * broad).floor() } else { 4.0 * broad + (numbers.unit() * (8.0 * broad + 1.0)).floor() };
            let mut shade = (numbers.unit() * 8.0) as u8 % 8;
            if shade == last {
                shade = (shade + 1) % 8;
            }
            at += long;
            if place < at || all - at < 2.0 * broad {
                return shade;
            }
            last = shade;
        }
    }
}

/// The place a brick is asked of the pattern at: the middle of its box.
///
/// The middle of its solid part was used first, which for a sloped brick
/// is to one side of its box. Sloped bricks lie in rows with their solid
/// parts to one side and the other by turns, so along any line of the
/// pattern every other brick fell on the far side of it, and the line came
/// out as a row of teeth. The boxes' middles are in a row themselves.
fn middle(brick: &CellBrick) -> DVec3 {
    DVec3::new(brick.min[0] as f64, brick.min[1] as f64, brick.min[2] as f64) + DVec3::new(brick.ext[0] as f64, brick.ext[1] as f64, brick.ext[2] as f64) / 2.0
}

/// A block of cells as the pieces that can show and the rest: halved until
/// a half is either a single cell or wholly hidden inside the form (see
/// [`HIDDEN`]). Each piece is pushed with whether it is at the surface.
fn cells(min: [i32; 3], ext: [i32; 3], distance: &dyn Fn(DVec3) -> f64, out: &mut Vec<(CellBrick, bool)>) {
    let size = DVec3::new(ext[0] as f64, ext[1] as f64, ext[2] as f64);
    let hidden = distance(DVec3::new(min[0] as f64, min[1] as f64, min[2] as f64) + size / 2.0) < -(size.length() / 2.0 + HIDDEN);
    if hidden || ext == [1; 3] {
        out.push((CellBrick { mask: CUBE, min, ext }, !hidden));
        return;
    }
    let longest = (0..3).max_by_key(|k| ext[*k]).unwrap_or(0);
    let (mut near, mut far, mut from) = (ext, ext, min);
    near[longest] = ext[longest] / 2;
    far[longest] = ext[longest] - near[longest];
    from[longest] = min[longest] + near[longest];
    cells(min, near, distance, out);
    cells(from, far, distance, out);
}

/// Which cells of a form are filled by its blocks, a bit apiece, to tell
/// which cells of a block show: those with a side that no block covers.
///
/// Whether a cell shows was first asked of the form's distance alone, as
/// [`cells`] asks it: a cell deep inside the form is covered. That holds
/// where the bricks follow the form closely. A round tube built as flat
/// bands standing proud of one another does not: its section is a cross,
/// and the cells in the cross's inside corners are deep inside the round
/// tube and open to the air. They were left in the ground's colour, a
/// plain stripe along the tube. What covers a cell is the cells round it,
/// so that is what is asked.
struct Blocks {
    min: [i32; 3],
    size: [usize; 3],
    filled: Vec<u64>,
}

impl Blocks {
    /// The grid of a form's blocks, if the form is small enough for one:
    /// up to a quarter of a thousand million cells, which is 32 megabytes.
    fn of(bricks: &[CellBrick]) -> Option<Blocks> {
        let min = [0, 1, 2].map(|k| bricks.iter().map(|b| b.min[k]).min().unwrap_or(0));
        let size = [0, 1, 2].map(|k| (bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap_or(0) - min[k]).max(0) as usize);
        let all = size.iter().product::<usize>();
        if all > 1 << 28 {
            return None;
        }
        let mut blocks = Blocks { min, size, filled: vec![0; all / 64 + 1] };
        for brick in bricks.iter().filter(|b| b.mask == CUBE) {
            for z in 0..brick.ext[2] {
                for y in 0..brick.ext[1] {
                    for x in 0..brick.ext[0] {
                        if let Some(at) = blocks.place([brick.min[0] + x, brick.min[1] + y, brick.min[2] + z]) {
                            blocks.filled[at / 64] |= 1 << (at % 64);
                        }
                    }
                }
            }
        }
        Some(blocks)
    }

    fn place(&self, cell: [i32; 3]) -> Option<usize> {
        let at = [0, 1, 2].map(|k| cell[k] - self.min[k]);
        (0..3).all(|k| at[k] >= 0 && (at[k] as usize) < self.size[k]).then(|| (at[2] as usize * self.size[1] + at[1] as usize) * self.size[0] + at[0] as usize)
    }

    /// Whether a cell of a block has a side no block covers.
    fn shows(&self, cell: [i32; 3]) -> bool {
        (0..6).any(|side| {
            let mut next = cell;
            next[side / 2] += if side % 2 == 0 { -1 } else { 1 };
            self.place(next).is_none_or(|at| self.filled[at / 64] >> (at % 64) & 1 == 0)
        })
    }

    /// A block as the cells of it that show and what is left: halved, as
    /// [`cells`] halves one, until a half is a single cell or has no cell
    /// that shows, and is left one block in the ground's colour. (A cell
    /// that shows is in one of a block's six outer layers: inside it,
    /// every side of a cell is covered by the block itself. So only those
    /// layers are looked at.)
    ///
    /// Each outer layer that held a cell that shows was first taken off
    /// whole, as single cells. A block inside a solid sphere touches the
    /// surface at a patch of a few cells and is thousands of cells a
    /// layer, and a sphere 80 in radius came to a million pieces that way,
    /// nearly all of them hidden.
    fn peel(&self, min: [i32; 3], ext: [i32; 3], out: &mut Vec<(CellBrick, bool)>) {
        let layer = |min: [i32; 3], ext: [i32; 3]| (0..ext[2]).flat_map(move |z| (0..ext[1]).flat_map(move |y| (0..ext[0]).map(move |x| [min[0] + x, min[1] + y, min[2] + z])));
        let shows = (0..6).any(|side| {
            let (mut from, mut span) = (min, ext);
            (from[side / 2], span[side / 2]) = (if side % 2 == 1 { min[side / 2] + ext[side / 2] - 1 } else { min[side / 2] }, 1);
            layer(from, span).any(|cell| self.shows(cell))
        });
        if !shows || ext == [1; 3] {
            out.push((CellBrick { mask: CUBE, min, ext }, shows));
            return;
        }
        let longest = (0..3).max_by_key(|k| ext[*k]).unwrap_or(0);
        let (mut near, mut far, mut from) = (ext, ext, min);
        near[longest] = ext[longest] / 2;
        far[longest] = ext[longest] - near[longest];
        from[longest] = min[longest] + near[longest];
        self.peel(min, near, out);
        self.peel(from, far, out);
    }
}

/// A brick as the smallest bricks that fill exactly what it fills, each
/// pushed with whether it is at the surface (a sloped brick always is).
///
/// A block is its cells. A sloped brick is cut on a lattice of smaller
/// boxes: as many each way as the greatest number that divides all its
/// sides (a ramp is the same all along its length, so along that it is
/// cut into single cells whatever its other sides are). Each small box
/// holds whatever of the brick lies in it, and because the brick's sloped
/// faces pass through the lattice's corners that is always another
/// brick: the same sloped brick smaller, where the face cuts the box as
/// it cut the whole; a block, where the box is behind the face; nothing,
/// where it is in front. A pyramid's corner piece 30 cells each way is
/// then a staircase of blocks under ramps and corner pieces one cell
/// each way. Which corners of a small box the brick holds is asked of the
/// brick's own faces in whole numbers, so there is no rounding to go
/// wrong.
///
/// The two corner pieces with a twist in them (the half corners) and the
/// inner corner are left whole: they are rare in the forms a pattern is
/// laid on, and are not the hull of lattice points in the same simple way.
fn parts(brick: &CellBrick, distance: &dyn Fn(DVec3) -> f64, blocks: Option<&Blocks>, cut: bool, out: &mut Vec<(CellBrick, bool)>) {
    let Some(info) = shape_info(brick.mask).filter(|_| cut) else {
        // (uncut, a block wholly inside still keeps the ground's colour)
        let size = DVec3::new(brick.ext[0] as f64, brick.ext[1] as f64, brick.ext[2] as f64);
        out.push((*brick, brick.mask != CUBE || distance(middle(brick)) >= -(size.length() / 2.0 + HIDDEN)));
        return;
    };
    let kind = info.kind;
    if kind == Kind::Cube {
        return match blocks {
            Some(blocks) => blocks.peel(brick.min, brick.ext, out),
            None => cells(brick.min, brick.ext, distance, out),
        };
    }
    if !matches!(kind, Kind::Ramp | Kind::Tet | Kind::Cut | Kind::Hip) {
        return out.push((*brick, true));
    }
    let (corner, axis) = (info.corner as usize, info.axis as usize);
    let sloped = |k: usize| kind != Kind::Ramp || k != axis;
    let divisor = (0..3).filter(|k| sloped(*k)).fold(0, |mut a, k| {
        let mut b = brick.ext[k];
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    });
    // how many small boxes each way, and how many cells each is
    let count = [0, 1, 2].map(|k| if sloped(k) { divisor } else { brick.ext[k] });
    let size = [0, 1, 2].map(|k| brick.ext[k] / count[k]);
    // how far a lattice corner is from the brick's given corner along an axis, or from the corner opposite, out of `divisor`
    let from = |at: [i32; 3], k: usize, opposite: bool| if (corner >> k & 1 == 1) != opposite { divisor - at[k] } else { at[k] };
    let holds = |at: [i32; 3]| match kind {
        // solid away from the edge it lacks
        Kind::Ramp => (0..3).filter(|k| *k != axis).map(|k| from(at, k, true)).sum::<i32>() <= divisor,
        // solid within one box's worth of its square corner
        Kind::Tet => (0..3).map(|k| from(at, k, false)).sum::<i32>() <= divisor,
        // solid within two boxes' worth of the corner opposite the one it lacks
        Kind::Cut => (0..3).map(|k| from(at, k, true)).sum::<i32>() <= 2 * divisor,
        // solid where it is no further from its apex's edge, either way across, than it is below the apex
        _ => (0..3).filter(|k| *k != axis).all(|k| from(at, k, false) + (divisor - from(at, axis, false)) <= divisor),
    };
    for k in 0..count[2] {
        for j in 0..count[1] {
            // The whole boxes of a row are taken together as one block,
            // and that block is then halved down to the cells that show
            // (see `cells`). Taken a box at a time, the corner piece of a
            // large sphere, 16 boxes each way, was four thousand single
            // cells, nearly all of them hidden, and a sphere of them went
            // over the most pieces a form may be cut into.
            let mut run: Option<(i32, i32)> = None;
            for i in 0..=count[0] {
                let mask = if i == count[0] { 0 } else { (0..8u8).fold(0u8, |mask, c| mask | (holds([if sloped(0) { i + (c & 1) as i32 } else { 0 }, if sloped(1) { j + (c >> 1 & 1) as i32 } else { 0 }, if sloped(2) { k + (c >> 2 & 1) as i32 } else { 0 }]) as u8) << c) };
                let min = [brick.min[0] + i * size[0], brick.min[1] + j * size[1], brick.min[2] + k * size[2]];
                if mask == CUBE {
                    run = Some(run.map_or((i, 1), |(from, boxes)| (from, boxes + 1)));
                    continue;
                }
                if let Some((from, boxes)) = run.take() {
                    cells([brick.min[0] + from * size[0], min[1], min[2]], [boxes * size[0], size[1], size[2]], distance, out);
                }
                if mask.count_ones() >= 4 && shape_info(mask).is_some() {
                    out.push((CellBrick { mask, min, ext: size }, true));
                }
            }
        }
    }
}

/// Whether a form's pattern is laid over it by this file and not pushed
/// through its flat part by `spec::pattern_flat_faces`.
pub(crate) fn wraps(spec: &Spec) -> bool {
    spec.mosaic.is_some() && (matches!(spec.form, Form::Sphere | Form::Revolve | Form::Pyramid | Form::Torus) || (spec.tube && !spec.fill && matches!(spec.form, Form::Circle | Form::Arc | Form::Arch(_) | Form::Spline | Form::Bezier)))
}

/// Give the bricks of a form that is not flat the colours of its pattern:
/// every brick stays where it is, cut smaller where that leaves the
/// surface as it was, and `built.shades` is filled with the pattern's
/// number for each (see the top of this file). Does nothing to a form
/// with no pattern, one whose pattern is laid already, or one that has no
/// chart.
///
/// `distance` is the form's own, below nothing inside it, and `path` its
/// curve as it was built.
///
/// The bricks are cut and coloured on `threads`, a brick's pieces being
/// its own affair, and joined on one, in an order that does not depend
/// on how they were shared out.
pub(crate) fn colour(spec: &Spec, path: &[DVec3], distance: &(dyn Fn(DVec3) -> f64 + Sync), built: &mut Built, threads: Threads) {
    let Some(mosaic) = &spec.mosaic else { return };
    if !wraps(spec) || !built.shades.is_empty() || built.bricks.is_empty() {
        return;
    }
    let Some(chart) = Chart::of(spec, path) else { return };
    let blocks = Blocks::of(&built.bricks);
    let cut = |cut: bool| -> Vec<Vec<(CellBrick, bool)>> {
        threads.map(built.bricks.len(), BRICKS_A_THREAD, |n| {
            let mut out = Vec::new();
            parts(&built.bricks[n], distance, blocks.as_ref(), cut, &mut out);
            out
        })
    };
    let mut pieces = cut(true);
    if pieces.iter().map(Vec::len).sum::<usize>() > MOST_PIECES {
        pieces = cut(false);
    }
    let pieces: Vec<(CellBrick, bool)> = pieces.into_iter().flatten().collect();
    // how large the pieces at the surface are, which sets how fine the pattern may be
    let size = |brick: &CellBrick| {
        let mut sides = brick.ext;
        sides.sort();
        ((sides[1] * sides[2]) as f64).sqrt()
    };
    let tile = tile_for(pieces.iter().filter(|(_, shown)| *shown).map(|(brick, _)| if brick.mask == CUBE { (1.0, 0.5 * (brick.ext[0] * brick.ext[1] * brick.ext[2]) as f64) } else { (size(brick), size(brick)) }));
    let laid = Laid::new(chart, mosaic, tile);
    let shades = threads.map(pieces.len(), BRICKS_A_THREAD * 4, |n| {
        let (brick, shown) = &pieces[n];
        if !*shown {
            return 0;
        }
        // A brick longer than a tile holds several parts of the pattern
        // and can show one. It takes the number most of it has, asked at
        // 27 places through its box, and not the one at its middle: a
        // long brick whose middle fell on a line of mortar was a bar of
        // mortar across a stone, where most of what it covers is stone. Whether it is ground or not is settled first,
        // by which most of the places are, and then which part of the
        // pattern: counted all together, the ground won over bricks of
        // six shades that each had fewer places than it and together had
        // twice as many. (Of two numbers as common, the lower.)
        if brick.ext.iter().all(|side| (*side as f64) <= tile) {
            return laid.shade(middle(brick));
        }
        let mut count = [0u8; 256];
        for k in 0..27 {
            let at = DVec3::new((k % 3) as f64 + 0.5, (k / 3 % 3) as f64 + 0.5, (k / 9) as f64 + 0.5) / 3.0;
            count[laid.shade(DVec3::new(brick.min[0] as f64, brick.min[1] as f64, brick.min[2] as f64) + at * DVec3::new(brick.ext[0] as f64, brick.ext[1] as f64, brick.ext[2] as f64)) as usize] += 1;
        }
        if count[0] as usize * 2 > 27 {
            return 0;
        }
        (1..=255u8).max_by_key(|shade| (count[*shade as usize], std::cmp::Reverse(*shade))).unwrap_or(0)
    });
    let (bricks, shades): (Vec<CellBrick>, Vec<u8>) = joined(pieces.into_iter().map(|(brick, _)| brick).zip(shades).collect(), spec.max_ext.max(1)).into_iter().unzip();
    (built.bricks, built.shades) = (bricks, shades);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::inside_shape;
    use crate::spec::build_with;
    use std::collections::HashSet;

    fn at(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x + 0.5, y + 0.5, z + 0.5)
    }

    fn mosaic(style: &str) -> Option<Mosaic> {
        Some(Mosaic { style: STYLES.iter().position(|s| s.name == style).unwrap(), size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: 50, upright: false, uneven: [0; 2] })
    }

    fn forms() -> Vec<(&'static str, Spec)> {
        vec![
            ("a sphere", Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius: 20.0, fill: true, grain: 1, ..Spec::default() }),
            ("a dome", Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius: 24.0, fill: false, thick: 3.0, cut: Some((0.0, f64::INFINITY)), grain: 1, ..Spec::default() }),
            ("a torus", Spec { form: Form::Torus, points: vec![at(0.0, 0.0, 0.0)], radius: 30.0, thick: 12.0, grain: 1, ..Spec::default() }),
            ("a turned profile", Spec { form: Form::Revolve, points: vec![at(0.0, 0.0, 30.0), at(15.0, 0.0, 20.0), at(25.0, 0.0, 0.0)], thick: 3.0, grain: 1, ..Spec::default() }),
            ("a pyramid", Spec { form: Form::Pyramid, points: vec![at(0.0, 0.0, 0.0)], radius: 12.0, rise: Some(24.0), fill: true, grain: 1, ..Spec::default() }),
            ("a tube", Spec { form: Form::Arch(crate::curves::ArchType::Round), points: vec![at(-30.0, 0.0, 0.0), at(30.0, 0.0, 0.0)], thick: 8.0, tube: true, grain: 1, ..Spec::default() }),
        ]
    }

    /// What a build fills: three places to a cell each way. (The places
    /// are a different part of the way across a third of a cell each way,
    /// none of them a round number, so that none lies on a sloped face,
    /// where whether it is inside would turn on the last digit of a sum.)
    fn filled(bricks: &[CellBrick]) -> HashSet<[i32; 3]> {
        let mut filled = HashSet::new();
        for brick in bricks {
            let info = shape_info(brick.mask).expect("a brick is one of the shapes");
            for k in 0..brick.ext[2] * 3 {
                for j in 0..brick.ext[1] * 3 {
                    for i in 0..brick.ext[0] * 3 {
                        if inside_shape(info, (i as f64 + 0.31) / (brick.ext[0] * 3) as f64, (j as f64 + 0.47) / (brick.ext[1] * 3) as f64, (k as f64 + 0.61) / (brick.ext[2] * 3) as f64) {
                            filled.insert([brick.min[0] * 3 + i, brick.min[1] * 3 + j, brick.min[2] * 3 + k]);
                        }
                    }
                }
            }
        }
        filled
    }

    #[test]
    fn a_pattern_over_a_form_that_is_not_flat_colours_it_and_fills_exactly_what_it_filled() {
        for (name, plain) in forms() {
            let was = build_with(&plain, false, Threads::ONE).unwrap();
            for style in ["bricks", "rug"] {
                let now = build_with(&Spec { mosaic: mosaic(style), ..plain.clone() }, false, Threads::ONE).unwrap();
                assert_eq!(now.shades.len(), now.bricks.len(), "{name}: every brick has a number");
                assert!(now.shades.iter().any(|s| *s != 0) && now.shades.iter().any(|s| *s == 0), "{name}: {style} shows on it");
                assert!(filled(&was.bricks) == filled(&now.bricks), "{name}: {style} changed what the form fills");
                // nothing is longer than the longest brick allowed
                assert!(now.bricks.iter().all(|b| b.ext.iter().all(|e| *e >= 1 && *e <= plain.max_ext)));
            }
        }
    }

    #[test]
    fn a_sloped_brick_is_cut_into_smaller_ones_of_the_same_slope() {
        // each kind that is cut, in every pose, three boxes each way
        for mask in crate::shapes::all_masks() {
            let kind = shape_info(mask).unwrap().kind;
            if !matches!(kind, Kind::Ramp | Kind::Tet | Kind::Cut | Kind::Hip) {
                continue;
            }
            let whole = CellBrick { mask, min: [3, -2, 5], ext: [3, 6, 9] };
            let mut pieces = Vec::new();
            parts(&whole, &|_| 0.0, None, true, &mut pieces);
            let pieces: Vec<CellBrick> = pieces.into_iter().map(|(piece, _)| piece).collect();
            assert!(pieces.len() > 1, "{kind:?} was left whole");
            assert!(filled(&[whole]) == filled(&pieces), "{kind:?} {mask:02x} is not what its pieces are");
            // (no two pieces in one place: their cells add up to the whole's)
            assert_eq!(pieces.iter().map(|piece| filled(&[*piece]).len()).sum::<usize>(), filled(&[whole]).len(), "{kind:?} {mask:02x}: pieces overlap");
            assert!(pieces.iter().all(|piece| piece.mask == CUBE || shape_info(piece.mask).unwrap().kind == kind || matches!((kind, shape_info(piece.mask).unwrap().kind), (Kind::Tet, Kind::Cut) | (Kind::Cut, Kind::Tet) | (Kind::Hip, Kind::Ramp))), "{kind:?} was cut into another slope");
        }
        // sides with no divisor in common: left as it is
        let odd = CellBrick { mask: Kind::Tet.canonical(), min: [0; 3], ext: [2, 3, 5] };
        let mut pieces = Vec::new();
        parts(&odd, &|_| 0.0, None, true, &mut pieces);
        assert_eq!(pieces.len(), 1);
        assert_eq!((pieces[0].0.mask, pieces[0].0.ext), (odd.mask, odd.ext));
    }

    #[test]
    fn courses_round_a_ring_meet_themselves_and_keep_their_bricks_one_size() {
        let (_, spec) = forms().remove(3);
        let spec = spec.snapped();
        let built = build_with(&spec, false, Threads::ONE).unwrap();
        let chart = Chart::of(&spec, &built.path).unwrap();
        let (lo, hi) = chart.rows();
        let wall = Masonry::new(&mosaic("bricks").unwrap(), false);
        let laid = Laid::new(chart, &mosaic("bricks").unwrap(), 1.0);
        // Round each ring every brick is whole and one length: no part
        // brick where the sheet meets itself. The bricks are found as the
        // runs between joints, going round and on past the start.
        let mut rings = 0;
        for n in 0..200 {
            let across = lo + (hi - lo) * (n as f64 + 0.5) / 200.0;
            let round = laid.chart.round(across).unwrap();
            if round < 2.5 * wall.long {
                continue;
            }
            let shade = |along: f64| laid.bricks(&wall, along, &|across| laid.chart.round(across), across, None);
            const PLACES: usize = 4000;
            let shades: Vec<u8> = (0..PLACES).map(|k| shade(k as f64 / PLACES as f64)).collect();
            // (a height in a line of mortar between two courses has no bricks to count)
            if shades.iter().all(|shade| *shade == 0) {
                continue;
            }
            let starts: Vec<usize> = (0..PLACES).filter(|k| shades[*k] != 0 && shades[(*k + PLACES - 1) % PLACES] == 0).collect();
            assert!(starts.len() >= 2, "{} bricks round a ring {round} long", starts.len());
            let longs: Vec<f64> = (0..starts.len()).map(|k| ((starts[(k + 1) % starts.len()] + PLACES - starts[k]) % PLACES) as f64 / PLACES as f64 * round).collect();
            let (least, most) = (longs.iter().copied().fold(f64::INFINITY, f64::min), longs.iter().copied().fold(0.0, f64::max));
            assert!(most - least < 0.02 * round / starts.len() as f64 + 2.0 * round / PLACES as f64, "bricks from {least} to {most} long round one ring");
            // as long as a brick on a floor, to within a third
            assert!(least > wall.long * 0.66 && most < wall.long * 1.34, "bricks {least} to {most} long, where a brick is {}", wall.long);
            rings += 1;
        }
        assert!(rings > 100, "{rings} rings were looked at");
    }

    #[test]
    fn a_rug_on_a_pyramid_or_a_dome_is_centred_and_its_own_mirror_image() {
        for (name, spec) in [forms().remove(4), forms().remove(1)] {
            let spec = spec.snapped();
            let built = build_with(&spec, false, Threads::ONE).unwrap();
            let laid = Laid::new(Chart::of(&spec, &built.path).unwrap(), &mosaic("rug").unwrap(), 1.0);
            let middle = spec.points[0];
            let mut seen = HashSet::new();
            for n in 0..400 {
                // places on the form's own surface, by way of its bricks
                let p = super::middle(&built.bricks[n * built.bricks.len() / 400]) - middle;
                let shades = [1.0, -1.0].map(|x| [1.0, -1.0].map(|y| laid.shade(middle + DVec3::new(p.x * x, p.y * y, p.z))));
                assert!(shades.iter().flatten().all(|shade| *shade == shades[0][0]), "{name}: the rug is not its own mirror image at {p}");
                seen.insert(shades[0][0]);
            }
            assert!(seen.len() >= 3, "{name}: the rug has {} parts", seen.len());
        }
    }

    #[test]
    fn the_inside_of_a_solid_form_is_not_cut_up() {
        let (_, sphere) = forms().remove(0);
        let plain = build_with(&sphere, false, Threads::ONE).unwrap();
        let laid = build_with(&Spec { mosaic: mosaic("cobblestone"), ..sphere }, false, Threads::ONE).unwrap();
        // the surface of a sphere 20 in radius is some 5,000 cells: a pattern's bricks are of that order, not the 33,000 cells inside
        assert!(laid.bricks.len() < plain.bricks.len() + 6_000, "{} bricks from {}", laid.bricks.len(), plain.bricks.len());
        let ground = laid.bricks.iter().zip(&laid.shades).filter(|(_, shade)| **shade == 0).map(|(brick, _)| brick.volume()).fold(0.0, f64::max);
        assert!(ground > 1_000.0, "the largest brick inside it is {ground} cells");
        // and only the cells that show are coloured: some 5,000, and far from all 33,000
        let coloured: f64 = laid.bricks.iter().zip(&laid.shades).filter(|(brick, shade)| **shade != 0 && brick.mask == CUBE).map(|(brick, _)| brick.volume()).sum();
        assert!(coloured < 9_000.0, "{coloured} cells of blocks are coloured");
    }
}
