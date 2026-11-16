//! A profile spun about an upright axis, as rings joined by sloped pieces.
//!
//! Fitted cell by cell, a turned form is a staircase wherever its profile
//! slopes, because no cell knows what its neighbours hold. Here the form is
//! laid out from its profile, the way a flat arch is laid out from its
//! outline:
//!
//! 1. The solid's section, in the half-plane of (distance from the axis,
//!    height), is fitted as an outline (`fit_planar`). Its sides are upright,
//!    level, or sloped, and a sloped side is one straight run across so many
//!    cells and up so many.
//! 2. Spun about the axis, an upright side is a wall standing on a circle, a
//!    level side is a flat face, and a sloped side is a ring whose circle
//!    shrinks from one radius to another as it rises.
//! 3. A circle is fitted as an outline too. A sloped ring is then the
//!    chamfer of `fit_stack`: the outline of its wide end is cut back to its
//!    narrow end with a ramp, a tetrahedron or a cut cube for each side and
//!    corner of the outline, all sharing whole faces.
//!
//! The sides of the section are followed in runs that go one way in height
//! (a *track*): the outside of a vase from foot to lip is one, the inside of
//! its bowl another. Each sloped ring's wide outline is fitted for that ring,
//! knowing how far in its narrow end is (`fit_planar_towards`), because how
//! deep the chamfer's pieces go is set by the outline's own triangles: an
//! outline fitted for one slope and reused for the next, or the outline one
//! chamfer leaves behind used as the start of the next, puts every sloped
//! piece that is not a ramp at the wrong depth, and a cone comes out bulging
//! between its four axes. (That was tried first.) The price is that where
//! one ring ends and the next begins there are two outlines of the same
//! circle, fitted for different slopes, which can differ by a sliver: a
//! ledge a fraction of a cell wide. A wall is built on the outline of the
//! slope that starts from it, so those two meet edge to edge.
//!
//! A track with solid inside its circle is the form's outer face; one with
//! air inside is the wall of a hollow. The same pieces serve both, since a
//! hollow narrowing is solid growing inwards.
//!
//! What is not done: a profile is not tried at several positions, and the
//! circles are plain outlines, so a very thin wall, whose inner and outer
//! pieces want the same cells, loses the inner ones there and stands a
//! little thick. Where the layout cannot be made at all the caller fits the
//! form cell by cell, as before.

use crate::fit_planar::{fit_planar, fit_planar_towards, Opts, Piece};
use crate::fit_round::rest_of_box;
use crate::fit_stack::{grow, Plan};
use crate::fit_volume::merge;
use crate::shapes::{corners, shape_info, CellBrick, CUBE};
use glam::DVec2;
use std::collections::HashMap;

/// A turned form to fit. Everything is in cells.
pub struct Lathe<'a> {
    /// where the upright axis stands
    pub axis: DVec2,
    /// the solid's section at (distance from the axis, height): negative inside
    pub section: &'a dyn Fn(f64, f64) -> f64,
    /// the furthest it reaches from the axis, and its lowest and highest
    pub far: f64,
    pub low: f64,
    pub high: f64,
    /// what a corner of an outline costs; see `fit_planar::Opts`
    pub brick_cost: f64,
    /// Whether the section's sloped sides are kept short as it is fitted, or
    /// fitted freely and cut short afterwards (see `fit_lathe`). Neither is
    /// better for every form, so the caller tries both.
    pub short_sides: bool,
    pub max_ext: i32,
}

/// A run of the section's outline going one way in height: a circle whose
/// radius changes from level to level.
struct Track {
    /// whether the inside of the circle is air: the wall of a hollow
    hollow: bool,
    /// (height, radius in whole cells beyond the least) from the bottom up
    nodes: Vec<(i32, i32)>,
}

/// One side of a track, spun: a wall or a sloped ring through layers
/// `low..high`.
struct Band {
    hollow: bool,
    low: i32,
    high: i32,
    /// its circle's radius at its bottom and at its top
    radius: (f64, f64),
    /// What is outside the circle where the circle is smallest. A cell this
    /// leaves empty is wholly inside the circle all the way up the band.
    outside: Plan,
    /// its sloped and three-cornered bricks
    pieces: Vec<CellBrick>,
    /// For each piece, the shape it takes where it shares its box with a
    /// piece of `partner`, the band it meets in a crease (see [`Crease`]).
    halves: Vec<Option<u8>>,
    partner: Option<usize>,
}

/// Where an outer face and the wall of a hollow end on the same circle: the
/// crest of a lip (a ridge, solid between the two faces) or the bottom of a
/// groove (air between them).
struct Crease {
    /// the side of each track that ends there, as (track, side): the outer
    /// face's, then the hollow's
    sides: [(usize, usize); 2],
    /// the circle they share, and the reach its outline is fitted for
    cells: i32,
    reach: i32,
    ridge: bool,
}

impl Track {
    /// Its top or bottom node, and the node next to that.
    fn end(&self, top: bool) -> ((i32, i32), (i32, i32)) {
        let n = self.nodes.len() - 1;
        if top { (self.nodes[n], self.nodes[n - 1]) } else { (self.nodes[0], self.nodes[1]) }
    }

    /// Which of its sides that end is.
    fn end_side(&self, top: bool) -> usize {
        if top { self.nodes.len() - 2 } else { 0 }
    }
}

/// Whether the ends of an outer face and of a hollow's wall, each as its
/// end node and the node next to it, make a crease: they end at one node,
/// and away from it one widens while the other narrows or stands upright.
/// Two that go the same way meet in a cusp, which no one outline serves.
fn meets(ends: [((i32, i32), (i32, i32)); 2]) -> bool {
    let wider = ends.map(|(at, next)| (next.1 - at.1).signum());
    ends[0].0 == ends[1].0 && wider[0] != wider[1] && wider[0] * wider[1] <= 0
}

/// Cut the taller of the two sides that meet in each crease, so that both
/// go through the same layers.
///
/// The pieces of the two rings share boxes along the crease (see
/// `fit_lathe`), and a box is shared only by pieces of one height. A wall is
/// cut anywhere. A sloped side taller than its neighbour is one cell across
/// (see `stepped`), so one part of it has to become a wall and the other
/// keep the cell: whichever leaves the side nearer where it was, but never
/// a wall against a wall, which would be a crease between two upright
/// faces with nothing to share.
fn level_creases(tracks: &mut [Track]) {
    for o in 0..tracks.len() {
        for h in 0..tracks.len() {
            for top in [false, true] {
                if tracks[o].hollow || !tracks[h].hollow {
                    continue;
                }
                let ends = [tracks[o].end(top), tracks[h].end(top)];
                let tall = ends.map(|(at, next)| (next.0 - at.0).abs());
                if !meets(ends) || tall[0] == tall[1] {
                    continue;
                }
                let taller = (tall[1] > tall[0]) as usize;
                let (at, next) = ends[taller];
                let (short, other_is_wall) = (tall[1 - taller], ends[1 - taller].1 .1 == at.1);
                let wall_at_crease = !other_is_wall && 2 * short < tall[taller];
                let cut = (at.0 + if top { -short } else { short }, if wall_at_crease { at.1 } else { next.1 });
                let nodes = &mut tracks[[o, h][taller]].nodes;
                nodes.insert(if top { nodes.len() - 1 } else { 1 }, cut);
            }
        }
    }
}

/// The bricks of `lathe`, or `None` where it cannot be laid out this way:
/// the axis off the half-cell grid or between the two, a piece longer than a
/// brick may be, or two faces of the form so close that their pieces cannot
/// be told apart.
pub fn fit_lathe(lathe: &Lathe) -> Option<Vec<CellBrick>> {
    let c = lathe.axis;
    // The axis on a lattice line both ways, or in the middle of a cell both
    // ways: then a circle about it can end on lattice lines all round. The
    // least radius is 0 for the first and half a cell, one cell, for the second.
    let least = (c.x - c.x.floor()).min(c.x.ceil() - c.x);
    let other = (c.y - c.y.floor()).min(c.y.ceil() - c.y);
    if least != other || (least != 0.0 && least != 0.5) {
        return None;
    }
    let limit = lathe.max_ext.max(1);
    let opts = Opts { brick_cost: lathe.brick_cost, max_ext: limit, wedges: true };

    // 1. the section as an outline, measured in whole cells out from the least radius
    let across = (lathe.far - least).ceil() as i32 + 2;
    let (floor, top) = (lathe.low.floor() as i32 - 1, lathe.high.ceil() as i32 + 1);
    // Every sloped side of it has to end up one cell across or one cell up
    // (see `stepped`), and there are two ways to get there.
    //
    // Fitted freely, a straight slope is one long side, which `stepped` then
    // cuts at the lattice points nearest it: as true to the slope as the
    // lattice allows. But a wall a few cells thick is then fitted with one
    // long triangle on one face, whose box takes the wall's whole width, and
    // a staircase on the other, whose triangles had nowhere to go; and each
    // tread of that staircase is a step all the way round the form.
    //
    // Or the sides are kept short in the fitting itself, by asking for an
    // outline that another, a cell inside it, is to be cut back from
    // (`fit_planar_towards`). Both faces of a thin wall then get slopes. But
    // the fitter will not alternate two slopes to follow a third (see
    // `fit_planar`'s WIGGLE), so a long straight slope that is neither comes
    // out bent.
    let across_fn = |out: f64, up: f64| (lathe.section)(out + least, up);
    let a_cell_inside = |out: f64, up: f64| across_fn(out, up) + 1.0;
    let section = if lathe.short_sides { fit_planar_towards(&across_fn, [0, floor], [across, top], &opts, Some((&a_cell_inside, 1))) } else { fit_planar(&across_fn, [0, floor], [across, top], &opts) };
    if section.pieces.is_empty() {
        return None;
    }
    let mut tracks = tracks(&Plan::new(&section.pieces, 1));
    level_creases(&mut tracks);
    let radius = |cells: i32| cells as f64 + least;
    let widest = tracks.iter().flat_map(|t| t.nodes.iter().map(|n| n.1)).max()?;

    // 2. each track's circles, from where it is widest
    let centre = |x: f64, y: f64| (x - c.x).hypot(y - c.y);
    // a circle's outline is the same wherever it is wanted with the same reach
    let mut fitted: HashMap<(i32, i32), Vec<Piece>> = HashMap::new();
    let mut circle = |cells: i32, reach: i32| -> Vec<Piece> {
        fitted
            .entry((cells, reach))
            .or_insert_with(|| {
                let r = radius(cells);
                let (lo, hi) = ([(c.x - r).floor() as i32 - 2, (c.y - r).floor() as i32 - 2], [(c.x + r).ceil() as i32 + 2, (c.y + r).ceil() as i32 + 2]);
                let disc = |x: f64, y: f64| centre(x, y) - r;
                // fitted knowing how far in the next circle is, where there is one: see `fit_planar_towards`
                let narrower = |x: f64, y: f64| centre(x, y) - (r - reach as f64);
                let fit = if reach > 0 { fit_planar_towards(&disc, lo, hi, &opts, Some((&narrower, reach))) } else { fit_planar(&disc, lo, hi, &opts) };
                fit.pieces
            })
            .clone()
    };
    // what is outside a circle, which is what grows as the circle shrinks
    let air = |pieces: &[Piece]| Plan::new(pieces, 2).complement();

    // The creases. Each ring's wide outline is fitted for that ring, so two
    // rings that end on one circle have two outlines of it, a sliver apart.
    // Along one face that is a ledge too small to see. But where an outer
    // face and a hollow's wall end on one circle, the pieces of the two
    // rings lie against each other all the way round, and a sliver between
    // their outlines puts a cell of one ring's boxes in the other's: every
    // such piece then had to be left out and built as whole cells, a ring
    // of steps round the crease. So the two rings share one outline there.
    // A ring that narrows away from the crease is cut back from that
    // outline as any ring is, a ring that widens away from it is grown
    // outwards from the same outline (`grow` works either way), and a wall
    // stands on it.
    //
    // The outline is fitted for the shallower of two sloped rings. Its
    // sloped sides are then no deeper than either ring goes, so both rings
    // can take every one of them; fitted for the deeper, the shallower
    // could not, and would stand upright along those sides.
    let mut creases: Vec<Crease> = Vec::new();
    // the sides of tracks built on a crease's outline, with the reach it was fitted for
    let (mut outwards, mut inwards, mut standing): (HashMap<(usize, usize), i32>, HashMap<(usize, usize), i32>, HashMap<(usize, usize), i32>) = Default::default();
    for (o, outer) in tracks.iter().enumerate().filter(|(_, t)| !t.hollow) {
        for (h, hollow) in tracks.iter().enumerate().filter(|(_, t)| t.hollow) {
            for top in [false, true] {
                let (ends, sides) = ([outer.end(top), hollow.end(top)], [(o, outer.end_side(top)), (h, hollow.end_side(top))]);
                let wider = ends.map(|(at, next)| next.1 - at.1);
                // A side is in one crease only: its other end is then
                // whatever the growing left. And a crease on the axis
                // itself has no circle.
                let taken = sides.iter().any(|side| creases.iter().any(|c| c.sides.contains(side)));
                if !meets(ends) || ends[0].1 .0 != ends[1].1 .0 || taken || (ends[0].0 .1 == 0 && least == 0.0) {
                    continue;
                }
                let reach = wider.iter().map(|w| w.abs()).filter(|w| *w > 0).min().unwrap_or(0);
                for (side, wider) in sides.iter().zip(wider) {
                    let built = if wider > 0 { &mut outwards } else if wider < 0 { &mut inwards } else { &mut standing };
                    built.insert(*side, reach);
                }
                // solid between the two faces if the outer face is the further out away from the crease
                creases.push(Crease { sides, cells: ends[0].0 .1, reach, ridge: wider[0] > wider[1] });
            }
        }
    }

    let mut bands: Vec<Band> = Vec::new();
    // which band each side of a track became
    let mut band_of: HashMap<(usize, usize), usize> = HashMap::new();
    for (number, track) in tracks.iter().enumerate() {
        let n = track.nodes.len() - 1;
        let r = |k: usize| track.nodes[k].1;
        for k in 0..n {
            let (low, high) = (track.nodes[k].0, track.nodes[k + 1].0);
            if r(k) == r(k + 1) {
                if r(k) == 0 && least == 0.0 {
                    continue; // about the axis itself: no circle, and nothing to build
                }
                // A wall, standing on the outline of its circle. If a slope
                // starts from either end of it, on the slope's outline, so
                // that the two meet edge to edge.
                let above = (k + 2 <= n && r(k + 2) < r(k + 1)).then(|| r(k + 1) - r(k + 2));
                let below = (k > 0 && r(k - 1) < r(k)).then(|| r(k) - r(k - 1));
                // Or on the outline of the crease it ends in.
                let outside = air(&circle(r(k), standing.get(&(number, k)).copied().or(above).or(below).unwrap_or(0)));
                // The circle's own triangles stand straight up: the solid is
                // the half of each one's box inside the circle, or for a
                // hollow the half outside it.
                let mut pieces = Vec::new();
                for p in outside.live_triangles() {
                    let inside_half = 15 & !(1 << ((!p.mask & 15).trailing_zeros() ^ 3));
                    let mask = if track.hollow { p.mask } else { inside_half };
                    upright(&mut pieces, [p.x, p.y, p.w, p.h], mask, mask, false, false, low, high, limit)?;
                }
                let halves = vec![None; pieces.len()];
                band_of.insert((number, k), bands.len());
                bands.push(Band { hollow: track.hollow, low, high, radius: (radius(r(k)), radius(r(k))), outside, pieces, halves, partner: None });
            } else {
                // A sloped ring, cut back from the outline of its wide end
                // to its narrow end. What `grow` makes are pieces of whatever
                // is outside the circle: air round a solid, of which the
                // brick is the rest of the box, and solid round a hollow.
                let wide_below = r(k) > r(k + 1);
                let (wide, narrow) = if wide_below { (r(k), r(k + 1)) } else { (r(k + 1), r(k)) };
                let to = radius(narrow);
                // From the outline of its narrow end outwards, where that
                // end is a crease whose outline the other ring there is cut
                // back from. What `grow` makes is then pieces of the inside
                // of the circle: solid, or for a hollow the air.
                let from_narrow = outwards.get(&(number, k)).copied();
                let (feet, smallest) = match from_narrow {
                    None => grow(&air(&circle(wide, inwards.get(&(number, k)).copied().unwrap_or(wide - narrow))), &|x, y| to - centre(x, y), wide - narrow),
                    Some(reach) => {
                        let shared = circle(narrow, reach);
                        let (feet, _) = grow(&Plan::new(&shared, wide - narrow + 2), &|x, y| centre(x, y) - radius(wide), wide - narrow);
                        (feet, air(&shared))
                    }
                };
                let mut pieces = Vec::new();
                for f in &feet {
                    // what it reaches where the circle is wide, and where it is narrow
                    let (at_wide, at_narrow) = if from_narrow.is_some() { (f.wide, f.narrow) } else { (f.narrow, f.wide) };
                    let (below, above) = if wide_below { (at_wide, at_narrow) } else { (at_narrow, at_wide) };
                    upright(&mut pieces, [f.x, f.y, f.w, f.h], below, above, track.hollow == from_narrow.is_some(), f.valley, low, high, limit)?;
                }
                let ends = if wide_below { (radius(wide), to) } else { (to, radius(wide)) };
                let halves = vec![None; pieces.len()];
                band_of.insert((number, k), bands.len());
                bands.push(Band { hollow: track.hollow, low, high, radius: ends, outside: smallest, pieces, halves, partner: None });
            }
        }
    }

    // A sloped side of a crease's outline lies across a box, and the pieces
    // of both rings are in that box: each is under its own face, and its
    // face comes to the outline from its own side. Neither can be left out
    // without a step. So the two share the box, each keeping to its own
    // side of the diagonal, as the two halves of a folded cell do in
    // `fit_ribbon`.
    //
    // In a groove the box holds what either ring holds, and the rings'
    // pieces are on their own sides of the diagonal as they are made: two
    // tetrahedra, or a tetrahedron and a wall's upright half. On a ridge
    // the box holds only what both rings hold, and each ring's piece takes
    // in the other's side: a cut cube, or a wall's upright half. So each is
    // cut down to what both hold on its own side of the diagonal, which is
    // a half outer corner under the ridge, or for a wall nothing at all.
    for crease in &creases {
        let (Some(&outer), Some(&hollow)) = (band_of.get(&crease.sides[0]), band_of.get(&crease.sides[1])) else { continue };
        (bands[outer].partner, bands[hollow].partner) = (Some(hollow), Some(outer));
        for p in circle(crease.cells, crease.reach).iter().filter(|p| p.mask != 15) {
            let held = |band: &Band| band.pieces.iter().position(|b| b.min[..2] == [p.x, p.y] && b.ext[..2] == [p.w, p.h]);
            let (Some(in_outer), Some(in_hollow)) = (held(&bands[outer]), held(&bands[hollow])) else { continue };
            let (whole_outer, whole_hollow) = (corners(bands[outer].pieces[in_outer].mask), corners(bands[hollow].pieces[in_hollow].mask));
            // the triangle inside the circle and the one outside it, each
            // through the box from bottom to top, and the diagonal between them
            let square = (!p.mask & 15).trailing_zeros() ^ 3;
            let (within, beyond) = (levels(p.mask, p.mask), levels(15 & !(1 << square), 15 & !(1 << square)));
            let diagonal = within & beyond;
            // The outer face is the inner of the two in a groove, and the
            // outer of the two on a ridge.
            let (of_outer, of_hollow) = if crease.ridge { (whole_outer & whole_hollow & beyond, whole_outer & whole_hollow & within) } else { (whole_outer, whole_hollow) };
            let kept = if crease.ridge { [(of_outer, beyond), (of_hollow, within)] } else { [(of_outer, within), (of_hollow, beyond)] };
            // nothing, if it has come down to the diagonal itself; and never a shape no brick has
            let shapes = kept.map(|(corners, side)| if corners & !diagonal == 0 { Some(0) } else { (corners & !side == 0 && shape_info(corners).is_some()).then_some(corners) });
            if let [Some(for_outer), Some(for_hollow)] = shapes {
                bands[outer].halves[in_outer] = Some(for_outer);
                bands[hollow].halves[in_hollow] = Some(for_hollow);
            }
        }
    }

    // 3. everything into one grid: the pieces first, then the whole cells
    let reach = radius(widest).ceil() as i32 + 3;
    let origin = [c.x.floor() as i32 - reach, c.y.floor() as i32 - reach, bands.iter().map(|b| b.low).min()?];
    let size = [2 * reach + 2, 2 * reach + 2, bands.iter().map(|b| b.high).max()? - origin[2]].map(|v| v as usize);
    let index = |x: i32, y: i32, z: i32| -> Option<usize> {
        let at = [x - origin[0], y - origin[1], z - origin[2]];
        (0..3).all(|k| at[k] >= 0 && (at[k] as usize) < size[k]).then(|| at[0] as usize + size[0] * (at[1] as usize + size[1] * at[2] as usize))
    };
    // which bands pass through each layer, the widest first
    let layers: Vec<Vec<usize>> = (0..size[2] as i32)
        .map(|layer| {
            let z = origin[2] + layer;
            let wide = |b: &Band| b.radius.0 + (b.radius.1 - b.radius.0) * (z as f64 + 0.5 - b.low as f64) / (b.high - b.low) as f64;
            let mut through: Vec<usize> = (0..bands.len()).filter(|&i| bands[i].low <= z && z < bands[i].high).collect();
            through.sort_by(|a, b| wide(&bands[*b]).total_cmp(&wide(&bands[*a])).then(a.cmp(b)));
            through
        })
        .collect();
    // From the outside in, the bands must be the outer face of something,
    // then the wall of a hollow in it, then the outer face of something
    // inside that, and so on. Any other order is two faces crossing.
    if layers.iter().any(|through| through.iter().enumerate().any(|(place, &band)| bands[band].hollow != (place % 2 == 1))) {
        return None;
    }
    let inside = |band: &Band, x: i32, y: i32| {
        let p = &band.outside;
        x >= p.min[0] && y >= p.min[1] && x < p.min[0] + p.w as i32 && y < p.min[1] + p.h as i32 && p.at(x, y) == 0
    };
    let cells = |b: &CellBrick| {
        let (min, ext) = (b.min, b.ext);
        (0..ext[2]).flat_map(move |z| (0..ext[1]).flat_map(move |y| (0..ext[0]).map(move |x| [min[0] + x, min[1] + y, min[2] + z])))
    };
    // 0 for nothing, 1 for a piece's box, 2 for a box holding the first of
    // the two pieces that share it, CUBE for a whole cell
    let mut grid = vec![0u8; size[0] * size[1] * size[2]];
    let mut bricks: Vec<CellBrick> = Vec::new();
    // Outer faces first, so that where an outer face and a hollow's wall
    // want the same cells it is the outer face that is built as fitted.
    //
    // A piece that cannot be placed, because another has a cell of its box
    // or because its box reaches past the face next out from it, is where
    // two faces of the form run into each other: the bottom of a groove,
    // the root of a lip. An outer face's piece is what the air leaves of
    // its box, so leaving it out would leave a hole; its box is built whole
    // instead, wherever it is free, which rounds the groove off with a
    // little too much. A hollow's piece is simply left out: its cells are
    // then not hollow, so they too are built whole where they are free.
    for hollow in [false, true] {
        for (number, band) in bands.iter().enumerate().filter(|(_, b)| b.hollow == hollow) {
            for (piece, half) in band.pieces.iter().zip(&band.halves) {
                // the second piece into a box two share
                if let Some(mask) = half.filter(|_| cells(piece).all(|[x, y, z]| index(x, y, z).is_some_and(|i| grid[i] == 2))) {
                    cells(piece).for_each(|[x, y, z]| grid[index(x, y, z).unwrap()] = 1);
                    if mask != 0 {
                        bricks.push(CellBrick { mask, ..*piece });
                    }
                    continue;
                }
                // Every cell of its box wholly inside the face next out from
                // it, at every layer. A box shared with that face's own
                // piece is on the outline the two faces share, not inside it.
                let enclosed = cells(piece).all(|[x, y, z]| {
                    let through = &layers[(z - origin[2]) as usize];
                    match through.iter().position(|b| *b == number) {
                        Some(0) => true,
                        Some(place) => (half.is_some() && band.partner == Some(through[place - 1])) || inside(&bands[through[place - 1]], x, y),
                        None => false,
                    }
                });
                let free = cells(piece).all(|[x, y, z]| index(x, y, z).is_some_and(|i| grid[i] == 0));
                if enclosed && free {
                    // The first into a shared box takes its half at once:
                    // the other ring has a piece for every such box, and
                    // nothing else can take a box so marked.
                    cells(piece).for_each(|[x, y, z]| grid[index(x, y, z).unwrap()] = if half.is_some() { 2 } else { 1 });
                    if *half != Some(0) {
                        bricks.push(CellBrick { mask: half.unwrap_or(piece.mask), ..*piece });
                    }
                } else if !hollow {
                    for [x, y, z] in cells(piece) {
                        if let Some(i) = index(x, y, z).filter(|i| grid[*i] == 0) {
                            grid[i] = CUBE;
                        }
                    }
                }
            }
        }
    }
    for (layer, through) in layers.iter().enumerate() {
        let z = origin[2] + layer as i32;
        for y in origin[1]..origin[1] + size[1] as i32 {
            for x in origin[0]..origin[0] + size[0] as i32 {
                let i = index(x, y, z).unwrap();
                // inside an odd number of the circles, counted from the outside in: solid
                if grid[i] == 0 && through.iter().take_while(|b| inside(&bands[**b], x, y)).count() % 2 == 1 {
                    grid[i] = CUBE;
                }
            }
        }
    }
    for cell in &mut grid {
        if *cell != CUBE {
            *cell = 0;
        }
    }
    for b in merge(grid, size, limit as usize) {
        bricks.push(CellBrick { mask: CUBE, min: [b.min[0] + origin[0], b.min[1] + origin[1], b.min[2] + origin[2]], ext: b.ext });
    }
    (!bricks.is_empty()).then_some(bricks)
}

/// The tracks of a section's outline: its sides in runs that go one way in
/// height. Level sides end a run (they are flat faces), and so do sides
/// along the edge of the section's own half-plane, which is the axis and no
/// face at all.
fn tracks(section: &Plan) -> Vec<Track> {
    let mut all = Vec::new();
    for outline in section.outlines() {
        // which way each side goes: up, down, or neither (a break)
        let way = |side: &((i32, i32), (i32, i32), Option<usize>)| {
            let (a, b) = (side.0, side.1);
            if a.0 == 0 && b.0 == 0 { 0 } else { (b.1 - a.1).signum() }
        };
        let n = outline.len();
        // begin where a run begins, so that none is split by where the outline happens to start
        let Some(first) = (0..n).find(|&i| way(&outline[i]) != way(&outline[(i + n - 1) % n])) else { continue };
        let mut i = 0;
        while i < n {
            let dir = way(&outline[(first + i) % n]);
            let run = (i..n).take_while(|&j| way(&outline[(first + j) % n]) == dir).count();
            if dir != 0 {
                let sides = (i..i + run).map(|j| outline[(first + j) % n]);
                let mut nodes: Vec<(i32, i32)> = sides.clone().map(|s| (s.0 .1, s.0 .0)).collect();
                let last = sides.last().unwrap().1;
                nodes.push((last.1, last.0));
                // The outline goes round with the solid on its left. Going up,
                // that is the side nearer the axis: an outer face. Going down,
                // the solid is further out: the wall of a hollow.
                if dir < 0 {
                    nodes.reverse();
                }
                all.push(Track { hollow: dir < 0, nodes: stepped(&nodes) });
            }
            i += run;
        }
    }
    all
}

/// A track's sloped sides cut into short ones: each one cell across and
/// some cells up, or one cell up and some across.
///
/// A sloped piece owns its whole box, and so do the pieces of the face on
/// the other side of a wall. One long side makes boxes as wide as the side
/// runs and as tall as it rises, and across a wall a few cells thick the
/// boxes of its two faces then want the same cells: one of each pair has to
/// go, and the wall is left with a step. Cut short, the boxes are a cell
/// deep and keep to their own face. The cuts are at the lattice points
/// nearest the side, so a side that passes through lattice points (2 across
/// and 4 up, say) is cut exactly, and any other is bent by under half a
/// cell: 3 across and 2 up becomes 2 and 1, then 1 and 1. That the pieces of
/// a long gentle side then lie at two slopes and not one is the price.
///
/// It also keeps a chamfer shallow next to a small circle, where ramps
/// running in from neighbouring sides of the outline would otherwise cross.
fn stepped(nodes: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = vec![nodes[0]];
    for pair in nodes.windows(2) {
        let ((z0, r0), (z1, r1)) = (pair[0], pair[1]);
        let (rise, run) = (z1 - z0, r1 - r0);
        let parts = rise.min(run.abs()).max(1);
        for part in 1..=parts {
            let along = part as f64 / parts as f64;
            out.push((z0 + (rise as f64 * along).round() as i32, r0 + (run as f64 * along).round() as i32));
        }
    }
    out
}

/// The corners of a box that are `below` at its bottom and `above` at its
/// top (each as bits `cx | cy << 1`).
fn levels(below: u8, above: u8) -> u8 {
    (0..8u8).fold(0u8, |m, corner| m | ((if corner >> 2 == 0 { below } else { above }) >> (corner & 3) & 1) << corner)
}

/// Add the brick that fills box `[x, y, w, h]` through layers `low..high`,
/// reaching the corners `below` of the box at its bottom and `above` at its
/// top (bits `cx | cy << 1`); or, with `rest`, the brick that fills what
/// that shape leaves of the box. `None` if it is longer than a brick may be.
#[allow(clippy::too_many_arguments)]
fn upright(pieces: &mut Vec<CellBrick>, [x, y, w, h]: [i32; 4], below: u8, above: u8, rest: bool, valley: bool, low: i32, high: i32, limit: i32) -> Option<()> {
    if w > limit || h > limit {
        return None;
    }
    let mut mask = levels(below, above);
    if valley {
        mask = crate::fit_stack::valley(mask, 2);
    }
    if rest {
        mask = rest_of_box(mask);
    }
    // nothing, for the rest of a box that was whole; never a shape no brick has
    if mask == 0 {
        return Some(());
    }
    shape_info(mask)?;
    // one that is the same at every layer may be cut to length; a sloped one may not
    let mut at = low;
    while at < high {
        let run = if below == above { (high - at).min(limit) } else { high - at };
        if run > limit {
            return None;
        }
        pieces.push(CellBrick { mask, min: [x, y, at], ext: [w, h, run] });
        at += run;
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{inside_shape, Kind};

    fn count(bricks: &[CellBrick], kind: Kind) -> usize {
        bricks.iter().filter(|b| shape_info(b.mask).unwrap().kind == kind).count()
    }

    /// Whether `p` is in the bricks, and that no two share a cell.
    fn solid(bricks: &[CellBrick], p: [f64; 3]) -> bool {
        bricks.iter().any(|b| {
            let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
            t.iter().all(|v| (0.0..1.0).contains(v)) && inside_shape(shape_info(b.mask).unwrap(), t[0], t[1], t[2])
        })
    }

    fn no_overlap(bricks: &[CellBrick]) {
        let mut seen = std::collections::HashSet::new();
        for b in bricks {
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        assert!(seen.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]), "{b:?} shares a cell");
                    }
                }
            }
        }
    }

    #[test]
    fn a_cone_is_sloped_rings_and_no_steps() {
        // a cone 12 across its base and 12 high, standing on a cell's centre
        let cone = |out: f64, up: f64| (out - (12.0 - up) * 12.5 / 12.0).max(-up).max(up - 12.0);
        let lathe = Lathe { axis: DVec2::new(0.5, 0.5), section: &cone, far: 12.5, low: 0.0, high: 12.0, brick_cost: 0.0, short_sides: false, max_ext: 80 };
        let bricks = fit_lathe(&lathe).unwrap();
        no_overlap(&bricks);
        assert!(count(&bricks, Kind::Ramp) >= 4 && count(&bricks, Kind::Tet) >= 4 && count(&bricks, Kind::Cut) >= 4);
        // close to the cone everywhere, and the same in all four quarters
        let (mut wrong, mut total) = (0, 0);
        for i in 0..40 * 40 * 26 {
            let p = [(i % 40) as f64 * 0.7 - 13.37, (i / 40 % 40) as f64 * 0.7 - 13.37, (i / 1600) as f64 * 0.5 - 0.3];
            let d = cone((p[0] - 0.5).hypot(p[1] - 0.5), p[2]);
            let built = solid(&bricks, p);
            assert!(!(built && d > 1.2) && !(!built && d < -1.2), "{p:?} is {d} from the surface and {}", if built { "built" } else { "not built" });
            wrong += (built != (d < 0.0)) as usize;
            total += (d < 0.0) as usize;
            assert_eq!(built, solid(&bricks, [1.0 - p[0], p[1], p[2]]), "not the same mirrored in x at {p:?}");
            assert_eq!(built, solid(&bricks, [p[1], p[0], p[2]]), "not the same with x and y exchanged at {p:?}");
        }
        assert!((wrong as f64) < 0.08 * total as f64, "{wrong} of {total} samples wrong");
    }

    #[test]
    fn a_bowl_has_a_hollow_and_a_wall_all_round() {
        // a wall 3 thick: a cone frustum's shell, wider at the top
        let wall = |out: f64, up: f64| ((out - (8.0 + up * 0.5)).abs() - 1.5).max(-up).max(up - 10.0);
        let lathe = Lathe { axis: DVec2::new(0.0, 0.0), section: &wall, far: 14.5, low: 0.0, high: 10.0, brick_cost: 0.0, short_sides: false, max_ext: 80 };
        let bricks = fit_lathe(&lathe).unwrap();
        no_overlap(&bricks);
        for level in [0.5, 4.5, 9.5] {
            let middle = 8.0 + level * 0.5;
            for turn in 0..16 {
                let (s, c) = (turn as f64 * std::f64::consts::PI / 8.0 + 0.1).sin_cos();
                assert!(solid(&bricks, [c * middle, s * middle, level]), "no wall at level {level}, turn {turn}");
                assert!(!solid(&bricks, [c * (middle - 3.0), s * (middle - 3.0), level]), "the hollow is filled at level {level}, turn {turn}");
                assert!(!solid(&bricks, [c * (middle + 3.0), s * (middle + 3.0), level]), "something outside the wall at level {level}, turn {turn}");
            }
        }
    }

    #[test]
    fn an_axis_off_the_grid_is_left_to_the_other_fitter() {
        let cone = |out: f64, up: f64| (out - (12.0 - up)).max(-up).max(up - 12.0);
        for axis in [DVec2::new(0.5, 0.0), DVec2::new(0.3, 0.3)] {
            assert!(fit_lathe(&Lathe { axis, section: &cone, far: 12.0, low: 0.0, high: 12.0, brick_cost: 0.0, short_sides: false, max_ext: 80 }).is_none());
        }
    }
}
