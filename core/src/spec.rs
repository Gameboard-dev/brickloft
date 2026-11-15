//! From a description of a form to its bricks.
//!
//! A [`Spec`] is everything the editor lets a player change. [`build`] turns
//! it into a curve, the curve into a solid, and the solid into bricks, picking
//! the planar fitter whenever the form is a flat outline pushed straight
//! through the grid (it can stretch wedges; the volume fitter cannot yet).

use crate::curves::{arch, catmull_rom, circle3, rational_bezier, ArchType, Circle};
use crate::fit_lathe::{fit_lathe, Lathe};
use crate::fit_planar::{self, fit_planar, fit_planar_towards};
use crate::fit_pyramid::{fit_pyramid, Pyramid};
use crate::fit_round::{fit_round, Round};
use crate::fit_volume::{self, fit_volume};
use crate::sdf::{Field, Fill, Region, Stroke};
use crate::shapes::CellBrick;
use glam::{DVec2, DVec3};
use std::f64::consts::PI;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Form {
    /// 3 points: the circular arc through them (`closed`: the whole circle)
    Arc,
    /// 1 point: a circle of `radius` about it, facing axis `plane`
    Circle,
    /// 2 points: an arch springing from them
    Arch(ArchType),
    /// 2 or more points: a smooth curve through all of them
    Spline,
    /// 2 or more points: a curve pulled towards them by `weights`
    Bezier,
    /// 1 point: a sphere of `radius` about it, scaled by `stretch`
    Sphere,
    /// 1 point: a ring of `radius` about it, `thick` across
    Torus,
    /// 2 or more points: a profile spun about the vertical through the first
    Revolve,
    /// 1 point, the middle of its base: a pyramid `radius` to each side of
    /// it (scaled by `stretch` along x and y) and `rise` tall
    Pyramid,
}

/// A form to build. Lengths are in cells; points sit in cell space, where
/// whole numbers are cell corners and halves are cell centres.
#[derive(Clone, PartialEq, Debug)]
pub struct Spec {
    pub form: Form,
    pub points: Vec<DVec3>,
    /// Bezier: how hard each point pulls the curve (1 where missing)
    pub weights: Vec<f64>,
    pub radius: f64,
    /// Sphere: per-axis scale of `radius`, for ellipsoids
    pub stretch: DVec3,
    /// Arch: crown height above the feet; `None` takes the type's natural one
    pub rise: Option<f64>,
    /// Width of a curve's band, or of a hollow form's wall
    pub thick: f64,
    /// how far a flat curve is pushed through the grid
    pub depth: i32,
    /// A curve: fill the area it encloses. A 3D form: solid, not a shell.
    pub fill: bool,
    pub closed: bool,
    /// a curve as a round tube instead of a flat band
    pub tube: bool,
    /// A round tube lying flat, and a torus: built as flat bands of
    /// different widths laid side by side, each standing proud of the next,
    /// instead of as a pipe with its edges chamfered round.
    pub protrude: bool,
    /// Circle and Torus: the axis (0, 1, 2) the ring faces
    pub plane: usize,
    /// Sphere: keep only heights `lo..hi` relative to its centre. A dome is
    /// `(0, ∞)`; a segment has both ends finite.
    pub cut: Option<(f64, f64)>,
    pub wedges: bool,
    /// Mirror the finished form along x, y or z, about the middle of its
    /// points: a dome becomes a bowl, an arc bends the other way.
    pub flip: [bool; 3],
    /// Where the form is its own mirror image, make its bricks so too (see
    /// [`symmetrise`]).
    pub symmetric: bool,
    /// What a brick costs in cells of disagreement; see the fitters'
    /// `brick_cost`. 0 fits as closely as possible, whatever the count.
    pub simplify: f64,
    /// Brick size: the form is fitted on a lattice this many cells coarse, so
    /// every brick is a multiple of it. 0 picks the finest size whose brick
    /// count is within `budget`.
    pub grain: i32,
    /// most bricks wanted when `grain` is 0
    pub budget: usize,
    pub max_ext: i32,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            form: Form::Arch(ArchType::Round),
            points: Vec::new(),
            weights: Vec::new(),
            radius: 8.0,
            stretch: DVec3::ONE,
            rise: None,
            thick: 2.0,
            depth: 2,
            fill: false,
            closed: false,
            tube: false,
            protrude: false,
            plane: 2,
            cut: None,
            wedges: true,
            flip: [false; 3],
            symmetric: false,
            simplify: 0.0,
            grain: 1,
            budget: 2500,
            max_ext: 80,
        }
    }
}

/// Bricks and how well they fit.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub bricks: Vec<CellBrick>,
    /// volume of the true form, in cells cubed
    pub solid: f64,
    /// volume where bricks and form disagree
    pub error: f64,
    /// the form's curve, for drawing; empty for forms without one
    pub path: Vec<DVec3>,
    /// the brick size it was fitted at, in cells
    pub grain: i32,
    /// `solid` and `error` are still to be measured (see `build_at`)
    pub unmeasured: bool,
}

/// The furthest apart, in cells, a form's points may be, and the largest its
/// radius or rise may be. Far beyond anything buildable (a brick is at most
/// 80 cells long), and checked before anything is computed: a point dragged
/// off towards the horizon once asked for a curve millions of cells long,
/// and the search structures for it exhausted memory before the size check
/// that would have refused it.
pub const MAX_SPAN: f64 = 1000.0;

/// Most cells a build may span. The volume fitter keeps a byte per cell, so
/// this bounds its memory at 32 MB, which a browser tab allows comfortably.
pub const MAX_CELLS: i64 = 32_000_000;

/// Round a length in cells to whole game units (half cells).
pub fn snap(v: f64) -> f64 {
    (v * 2.0).round() / 2.0
}

impl Spec {
    /// The same form with every point and length on the game's unit grid.
    ///
    /// Brick positions are whole units, so a form placed between units can
    /// only be approximated by bricks that are all slightly off it: the fit
    /// along a curve then drifts in and out instead of following it. On the
    /// grid, a form's own symmetries and straight edges line up with the
    /// bricks' and come out exact.
    pub fn snapped(&self) -> Spec {
        let mut s = self.clone();
        for p in &mut s.points {
            *p = DVec3::new(snap(p.x), snap(p.y), snap(p.z));
        }
        s.radius = snap(s.radius).max(0.5);
        s.thick = snap(s.thick).max(0.5);
        s.rise = s.rise.map(|r| snap(r).max(0.5));
        s.cut = s.cut.map(|(lo, hi)| (snap(lo), snap(hi)));
        s
    }
}

/// Build a form. The spec is snapped to the unit grid first (see
/// [`Spec::snapped`]), so nothing off the grid can reach the fitters.
pub fn build(spec: &Spec) -> Result<Built, String> {
    build_as(spec, true)
}

/// [`build`], with the choice of leaving the fit unmeasured.
///
/// Forms laid out whole (spheres, pipes, pyramids) are measured against the
/// form after they are built, and for those the measuring takes several
/// times as long as the building: four fifths of a large sphere's time.
/// While a form is being dragged only its bricks are wanted, many times a
/// second, so the editor builds unmeasured then and measures once when the
/// mouse is let go. An unmeasured build has `unmeasured` set and its `solid`
/// and `error` at 0.
pub fn build_as(spec: &Spec, measured: bool) -> Result<Built, String> {
    if !(spec.thick > 0.0) || spec.depth < 1 || !(spec.radius > 0.0) || spec.max_ext < 1 {
        return Err("thickness, depth and radius must all be positive".into());
    }
    if spec.points.iter().any(|p| !p.is_finite()) {
        return Err("a point is not a number".into());
    }
    let lo = spec.points.iter().fold(DVec3::MAX, |a, p| a.min(*p));
    let hi = spec.points.iter().fold(DVec3::MIN, |a, p| a.max(*p));
    let too_far = !spec.points.is_empty() && (hi - lo).max_element() > MAX_SPAN;
    let too_big = spec.radius * spec.stretch.max_element() > MAX_SPAN
        || spec.rise.is_some_and(|r| r > MAX_SPAN)
        || spec.thick > MAX_SPAN
        || spec.depth as f64 > MAX_SPAN;
    if too_far || too_big {
        return Err(format!("the form is too large to build: nothing may be more than {MAX_SPAN} microbricks across; bring its points closer together"));
    }
    if spec.grain > 0 {
        return build_at(spec, spec.grain.min(GRAINS[GRAINS.len() - 1]), measured);
    }
    // Automatic brick size: from coarse to fine, keep the finest that is
    // within budget. Coarse to fine because a fit `g` times coarser has about
    // g cubed fewer cells to look at, so every size tried on the way down
    // costs less than the one finally kept; fine to coarse would start with
    // the most expensive fit and usually throw it away.
    let mut best: Option<Built> = None;
    let mut last_error = None;
    let limit = spec.snapped().coarsest_grain();
    for &grain in GRAINS.iter().rev().filter(|g| **g <= limit) {
        match build_at(spec, grain, measured) {
            Ok(built) if built.bricks.is_empty() => {} // too coarse to hold the form at all
            Ok(built) => {
                let over = built.bricks.len() > spec.budget;
                if over && best.is_some() {
                    break;
                }
                best = Some(built);
                if over {
                    break; // even the coarsest size allowed is over budget: it is the closest there is
                }
            }
            Err(e) => last_error = Some(e),
        }
    }
    best.ok_or_else(|| last_error.unwrap_or_else(|| "the form is too small to build".into()))
}

/// The brick sizes automatic mode chooses among.
pub const GRAINS: [i32; 6] = [1, 2, 3, 4, 6, 8];

impl Spec {
    /// The largest brick size automatic mode may use.
    ///
    /// A wall has to be at least two bricks thick: measured on a shell 2
    /// thick, bricks 2 across left a third of its volume wrong, because the
    /// wall then straddles the bricks and half-fills each. A round solid
    /// needs four across. And a flat form's depth must be a whole number of
    /// bricks, or coarsening would change how deep it is. Past these limits
    /// the brick count would fall only because the form was no longer being
    /// built, so automatic mode stops and reports that it is over budget.
    fn coarsest_grain(&self) -> i32 {
        let limit = match self.form {
            Form::Sphere if self.fill => self.radius * self.stretch.min_element() / 4.0,
            Form::Torus => self.thick / 4.0,
            // a pyramid is exact at any size: nothing is gained by coarser bricks
            Form::Pyramid => 1.0,
            Form::Revolve if self.fill => f64::INFINITY,
            Form::Sphere | Form::Revolve => self.thick / 2.0,
            // a curve
            _ if self.fill => f64::INFINITY,
            _ => self.thick / 2.0,
        };
        let flat = !matches!(self.form, Form::Sphere | Form::Torus | Form::Revolve | Form::Pyramid) && !self.tube;
        GRAINS
            .iter()
            .rev()
            .copied()
            .find(|&g| g as f64 <= limit && (!flat || self.depth % g == 0))
            .unwrap_or(1)
    }
}

/// `spec` shrunk by `grain` and snapped: the form as the coarse lattice sees it.
fn coarse(spec: &Spec, grain: i32) -> Spec {
    let g = grain.max(1) as f64;
    let mut coarse = spec.clone();
    for p in &mut coarse.points {
        *p /= g;
    }
    coarse.radius /= g;
    coarse.thick /= g;
    coarse.rise = coarse.rise.map(|r| r / g);
    coarse.cut = coarse.cut.map(|(lo, hi)| (lo / g, hi / g));
    coarse.depth = ((spec.depth as f64 / g).round() as i32).max(1);
    coarse.max_ext = (spec.max_ext / grain.max(1)).max(1);
    coarse.snapped()
}

/// Build `spec` with bricks `grain` cells coarse: shrink the form by `grain`,
/// fit it as usual, and scale the bricks back up. Every microbrick shape can
/// be stretched by whole cells, so a scaled-up fit is as valid as a fine one;
/// it has about `grain` squared fewer bricks on a surface and is `grain`
/// times rougher.
/// Make bricks the same on both sides of each plane the form itself is
/// symmetric about. Returns whether anything was changed.
///
/// The fitters are built to treat a form and its mirror image alike, and
/// tests hold them to it, but every case found so far where they did not was
/// found by eye, in a form a player had made. This is the guarantee for
/// those: along each axis, if the form is its own mirror image about the
/// middle of its bricks, the bricks on the low side are kept and the high
/// side is replaced with their mirror images.
///
/// An axis is left alone if the form is not symmetric along it (an arch is
/// not, top to bottom), or if a brick that lies across the middle is not its
/// own mirror image, since replacing one side would then leave that brick
/// overlapping or a hole beside it.
pub fn symmetrise(bricks: &mut Vec<CellBrick>, distance: &dyn Fn(DVec3) -> f64) -> bool {
    let mut changed = false;
    for k in 0..3 {
        if bricks.is_empty() {
            break;
        }
        let lo = bricks.iter().map(|b| b.min[k]).min().unwrap();
        let hi = bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap();
        // twice the mirror plane, to stay in whole numbers
        let twice = lo + hi;
        let mirror = |b: &CellBrick| {
            let mut m = *b;
            m.min[k] = twice - b.min[k] - b.ext[k];
            m.mask = (0..8u8).fold(0u8, |mask, c| mask | (b.mask >> c & 1) << (c ^ (1 << k)));
            m
        };
        // Is the form itself symmetric here? Asked at the middle of every
        // brick near its surface, and at the mirror image of each.
        let alike = bricks.iter().all(|b| {
            let mut p = DVec3::new(b.min[0] as f64 + b.ext[0] as f64 / 2.0, b.min[1] as f64 + b.ext[1] as f64 / 2.0, b.min[2] as f64 + b.ext[2] as f64 / 2.0);
            let here = distance(p);
            p[k] = twice as f64 - p[k];
            let there = distance(p);
            // far from the surface the distance is not exact, only its sign
            if here.abs() > 3.0 || there.abs() > 3.0 { (here < 0.0) == (there < 0.0) } else { (here - there).abs() < 1e-6 }
        });
        let across: Vec<&CellBrick> = bricks.iter().filter(|b| 2 * b.min[k] < twice && 2 * (b.min[k] + b.ext[k]) > twice).collect();
        if !alike || across.iter().any(|b| mirror(b) != **b) {
            continue;
        }
        let low: Vec<CellBrick> = bricks.iter().filter(|b| 2 * (b.min[k] + b.ext[k]) <= twice).copied().collect();
        let mut high: Vec<CellBrick> = bricks.iter().filter(|b| 2 * b.min[k] >= twice).copied().collect();
        let mut wanted: Vec<CellBrick> = low.iter().map(mirror).collect();
        let order = |b: &CellBrick| (b.min, b.ext, b.mask);
        high.sort_by_key(order);
        wanted.sort_by_key(order);
        if high != wanted {
            bricks.retain(|b| 2 * b.min[k] < twice);
            bricks.extend(wanted);
            changed = true;
        }
    }
    changed
}

/// Where a spec's form is mirrored: for each axis it is flipped along, twice
/// the coordinate of the mirror plane, which is the middle of its points.
///
/// Twice, and rounded to a whole number, so that the plane is on a lattice
/// plane or half way between two: a mirror anywhere else would move the
/// bricks off the lattice. Mirroring the finished bricks, instead of the
/// points before fitting, makes a flipped form the exact mirror image of the
/// unflipped one, brick for brick.
fn mirrors(spec: &Spec) -> [Option<f64>; 3] {
    [0, 1, 2].map(|k| {
        let lo = spec.points.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
        let hi = spec.points.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
        (spec.flip[k] && lo.is_finite()).then(|| (lo + hi).round())
    })
}

fn build_at(spec: &Spec, grain: i32, measured: bool) -> Result<Built, String> {
    let g = grain.max(1) as f64;
    let coarse = coarse(spec, grain);
    let (solid, path) = solid(&coarse)?;
    let mut built = fit(&coarse, &solid, path, g);
    if coarse.symmetric && symmetrise(&mut built.bricks, &|p| solid.distance(p)) {
        // the fitter's own figures are for the bricks as it left them
        built.unmeasured = true;
    }
    if built.unmeasured && measured && !built.bricks.is_empty() {
        // Bricks laid out as a whole, not cell by cell against the form, come
        // with no figure for how well they fit it. Measure them against the
        // form itself. A build too large to measure that way is fitted cell
        // by cell after all, which does have its own figures.
        let (field, min, max) = match &solid {
            Solid::Round { field, min, max, .. } | Solid::Pyramid { field, min, max, .. } | Solid::Lathe { field, min, max, .. } => (Some(field.clone()), *min, *max),
            _ => (None, [0; 3], [0; 3]),
        };
        match crate::report::measure(&Probe::of(solid), &built) {
            Ok((volume, wrong)) => {
                built.solid = volume;
                built.error = wrong;
            }
            Err(e) => match field {
                Some(field) => {
                    built = fit(
                        &coarse,
                        &Solid::Volume { field, min, max },
                        std::mem::take(&mut built.path),
                        g,
                    )
                }
                None => return Err(e),
            },
        }
        built.unmeasured = false;
    }
    for (k, twice) in mirrors(&coarse).into_iter().enumerate() {
        let Some(twice) = twice else { continue };
        for b in &mut built.bricks {
            b.min[k] = twice as i32 - b.min[k] - b.ext[k];
            // each corner of the box changes ends along this axis
            b.mask = (0..8u8).fold(0u8, |m, c| m | (b.mask >> c & 1) << (c ^ (1 << k)));
        }
        for p in &mut built.path {
            p[k] = twice - p[k];
        }
    }
    for b in &mut built.bricks {
        b.min = b.min.map(|v| v * grain);
        b.ext = b.ext.map(|v| v * grain);
    }
    for p in &mut built.path {
        *p *= g;
    }
    built.solid *= g * g * g;
    built.error *= g * g * g;
    built.grain = grain;
    Ok(built)
}

/// A form ready to fit: a flat outline pushed straight through the grid, or
/// a volume. Kept apart from the fitting so that the same form can be asked
/// afterwards where its surface is (see [`Probe`]).
pub enum Solid {
    /// `region` lies across `axis`, in the plane of the other two axes, and
    /// fills cells `w0..w0 + depth` along it.
    Flat {
        region: Region,
        axis: usize,
        min: [i32; 2],
        max: [i32; 2],
        w0: i32,
        depth: i32,
    },
    Volume {
        field: Field,
        min: [i32; 3],
        max: [i32; 3],
    },
    /// A sphere or ellipsoid, which has a fitter of its own (`fit_round`).
    /// `field` is the same solid as a distance function, for measuring the
    /// result and for the cell-by-cell fitter where that one does not apply.
    Round {
        round: Round,
        field: Field,
        min: [i32; 3],
        max: [i32; 3],
    },
    /// A round tube along a curve lying across `axis`, built as a stack of
    /// flat outlines: each of `layers` is an outline, the first cell it
    /// fills along the axis, and how many. `field` is the true round tube,
    /// which the result is measured against.
    Stack {
        layers: Vec<(Region, i32, i32)>,
        axis: usize,
        min: [i32; 2],
        max: [i32; 2],
        field: Field,
    },
    /// A profile spun about an upright axis, which has a fitter of its own
    /// (`fit_lathe`): `profile` is the solid's section in the half-plane of
    /// (distance from the axis, height), reaching `far` from the axis and
    /// from `low` to `high`. `field` is the same solid as a distance function.
    Lathe { profile: Region, axis: DVec2, far: f64, low: f64, high: f64, field: Field, min: [i32; 3], max: [i32; 3] },
    /// A pyramid, whose bricks follow from its size with no fitting (see
    /// `fit_pyramid`); `None` where it cannot be laid out that way. `field`
    /// is the same solid as a distance function.
    Pyramid { bricks: Option<Vec<CellBrick>>, field: Field, min: [i32; 3], max: [i32; 3] },
    /// A round tube along a curve lying across `axis`, built as a pipe: an
    /// octagon in section (see `fit_stack`). `top` is the band of its flat
    /// top and `full` the band of its whole width; it fills `layers` cells
    /// along the axis from `first`, the outer `chamfer` of them at each end
    /// sloping. `field` is the true round tube.
    Pipe {
        top: Region,
        full: Region,
        chamfer: i32,
        /// how many layers each chamfer takes
        rise: i32,
        axis: usize,
        first: i32,
        layers: i32,
        min: [i32; 2],
        max: [i32; 2],
        field: Field,
    },
}

/// The two axes a flat form's outline is drawn in, given the one it lies across.
pub fn plane_axes(axis: usize) -> (usize, usize) {
    [(1, 2), (0, 2), (0, 1)][axis.min(2)]
}

impl Solid {
    /// Negative inside the form, positive outside; near the surface, the
    /// distance to it in cells.
    pub fn distance(&self, p: DVec3) -> f64 {
        match self {
            Solid::Volume { field, .. }
            | Solid::Stack { field, .. }
            | Solid::Round { field, .. }
            | Solid::Pyramid { field, .. }
            | Solid::Lathe { field, .. }
            | Solid::Pipe { field, .. } => field.eval(p),
            Solid::Flat {
                region,
                axis,
                w0,
                depth,
                ..
            } => {
                let (ua, va) = plane_axes(*axis);
                let w = p[*axis];
                region
                    .eval(DVec2::new(p[ua], p[va]))
                    .max(*w0 as f64 - w)
                    .max(w - (*w0 + *depth) as f64)
            }
        }
    }
}

/// The form a spec describes, to ask where it is: for checking a fit against
/// the form itself, independently of the fitter.
pub struct Probe {
    solid: Solid,
    grain: f64,
    /// see [`mirrors`]
    mirror: [Option<f64>; 3],
}

impl Probe {
    /// The form as it was fitted at brick size `grain` (a `Built`'s `grain`).
    pub fn new(spec: &Spec, grain: i32) -> Result<Probe, String> {
        let coarse = coarse(spec, grain);
        let (solid, _) = solid(&coarse)?;
        Ok(Probe { solid, grain: grain.max(1) as f64, mirror: mirrors(&coarse) })
    }

    /// Signed distance from the form's surface at `p`, in cells: negative inside.
    pub fn distance(&self, p: DVec3) -> f64 {
        let mut q = p / self.grain;
        for (k, twice) in self.mirror.into_iter().enumerate() {
            if let Some(twice) = twice {
                q[k] = twice - q[k];
            }
        }
        self.solid.distance(q) * self.grain
    }

    /// For a flat form, the axis it lies across and the first cell and depth
    /// it fills along that axis.
    pub fn flat(&self) -> Option<(usize, i32, i32)> {
        match &self.solid {
            Solid::Flat {
                axis, w0, depth, ..
            } => {
                // mirrored along its own axis, the form fills the cells opposite
                let first = self.mirror[*axis].map_or(*w0, |twice| twice as i32 - *w0 - *depth);
                Some((*axis, first * self.grain as i32, *depth * self.grain as i32))
            }
            Solid::Volume { .. }
            | Solid::Stack { .. }
            | Solid::Round { .. }
            | Solid::Pyramid { .. }
            | Solid::Lathe { .. }
            | Solid::Pipe { .. } => None,
        }
    }

    /// A probe of a form at hand, for measuring bricks fitted to it at brick size 1.
    fn of(solid: Solid) -> Probe {
        Probe { solid, grain: 1.0, mirror: [None; 3] }
    }
}

/// The form of an already snapped spec, and its curve for drawing.
fn solid(spec: &Spec) -> Result<(Solid, Vec<DVec3>), String> {
    let need = |n: usize| {
        if spec.points.len() < n {
            Err(format!(
                "this form needs {n} point{}, and has {}",
                if n == 1 { "" } else { "s" },
                spec.points.len()
            ))
        } else {
            Ok(&spec.points[..])
        }
    };
    match spec.form {
        Form::Arc => {
            let p = need(3)?;
            let c = circle3(p[0], p[1], p[2]).ok_or("the three points are in a straight line")?;
            let path = c.path(0.0, if spec.closed { 2.0 * PI } else { c.a2 });
            curve(spec, path, spec.closed, Some(c.normal))
        }
        Form::Circle => {
            let centre = need(1)?[0];
            let c = Circle::axis_aligned(centre, spec.radius, spec.plane.min(2));
            // stretched within its own plane, a circle is an ellipse; its
            // stretch across the plane is the depth of a round tube (see `stacked`)
            let mut within = spec.stretch.max(DVec3::splat(0.05));
            within[spec.plane.min(2)] = 1.0;
            let path = c.path(0.0, 2.0 * PI).into_iter().map(|p| centre + (p - centre) * within).collect();
            curve(spec, path, true, Some(c.normal))
        }
        Form::Arch(kind) => {
            let p = need(2)?;
            let rise = spec
                .rise
                .unwrap_or_else(|| kind.default_rise(p[0].distance(p[1]) / 2.0));
            let (path, normal) = arch(p[0], p[1], kind, rise, DVec3::Z)?;
            curve(spec, path, false, Some(normal))
        }
        Form::Spline => curve(spec, catmull_rom(need(2)?, spec.closed)?, spec.closed, None),
        Form::Bezier => curve(spec, rational_bezier(need(2)?, &spec.weights)?, false, None),
        Form::Sphere => {
            let center = need(1)?[0];
            let radii = spec.stretch.max(DVec3::splat(0.05)) * spec.radius;
            let (lo, hi) = spec
                .cut
                .map_or((f64::NEG_INFINITY, f64::INFINITY), |(lo, hi)| {
                    (center.z + lo, center.z + hi)
                });
            let field = Field::Ellipsoid {
                center,
                radii,
                shell: (!spec.fill).then_some(spec.thick),
                lo,
                hi,
            };
            let (mut min, mut max) = (center - radii, center + radii);
            min.z = min.z.max(lo);
            max.z = max.z.min(hi);
            let (solid, path) = volume(field.clone(), min, max, Vec::new())?;
            let Solid::Volume { min, max, .. } = solid else {
                unreachable!()
            };
            let round = Round {
                center,
                radii,
                shell: (!spec.fill).then_some(spec.thick),
                lo,
                hi,
                max_ext: spec.max_ext,
            };
            Ok((
                Solid::Round {
                    round,
                    field,
                    min,
                    max,
                },
                path,
            ))
        }
        // A torus is a circle as a round tube, and is built as one. It was a
        // form of its own while it was fitted cell by cell from a formula.
        Form::Torus => solid(&Spec { form: Form::Circle, tube: true, fill: false, ..spec.clone() }),
        Form::Pyramid => {
            let base = need(1)?[0];
            let half = [spec.radius * spec.stretch.x.max(0.05), spec.radius * spec.stretch.y.max(0.05)];
            let pyramid = Pyramid { base, half, height: spec.rise.unwrap_or(spec.radius), max_ext: spec.max_ext };
            // measured against the pyramid as the lattice has it; off the
            // half-cell grid it has no such place and is fitted as it stands
            let (field, lo, hi) = match pyramid.snapped() {
                Some(at) => {
                    let floor = DVec3::new(base.x, base.y, at.floor as f64);
                    (Field::Pyramid { base: floor, half: at.half, top: at.top, height: at.height as f64 }, floor - DVec3::new(at.half[0], at.half[1], 0.0), floor + DVec3::new(at.half[0], at.half[1], at.height as f64))
                }
                None => (Field::Pyramid { base, half, top: [0.0; 2], height: pyramid.height.max(1.0) }, base - DVec3::new(half[0], half[1], 0.0), base + DVec3::new(half[0], half[1], pyramid.height.max(1.0))),
            };
            let (solid, path) = volume(field.clone(), lo, hi, Vec::new())?;
            let Solid::Volume { min, max, .. } = solid else { unreachable!() };
            Ok((Solid::Pyramid { bricks: fit_pyramid(&pyramid), field, min, max }, path))
        }
        Form::Revolve => {
            let p = need(2)?;
            let axis = p[0].truncate();
            let profile: Vec<DVec3> = p
                .iter()
                .map(|q| DVec3::new(q.truncate().distance(axis), q.z, 0.0))
                .collect();
            let path = catmull_rom(&profile, false)?;
            let region = if spec.fill {
                // close the profile along the axis
                let mut poly: Vec<DVec2> = path.iter().map(|q| q.truncate()).collect();
                poly.push(DVec2::new(0.0, path[path.len() - 1].y));
                poly.push(DVec2::new(0.0, path[0].y));
                Region::Fill(Fill::new(poly))
            } else {
                Region::Stroke(Stroke::new(&path, spec.thick / 2.0, false, Some(0.0)))
            };
            let far = path.iter().fold(0.0, |m: f64, q| m.max(q.x)) + spec.thick / 2.0;
            let (z0, z1) = path
                .iter()
                .fold((f64::MAX, f64::MIN), |(a, b), q| (a.min(q.y), b.max(q.y)));
            let half = spec.thick / 2.0;
            let min = DVec3::new(axis.x - far, axis.y - far, z0 - half);
            let max = DVec3::new(axis.x + far, axis.y + far, z1 + half);
            // the curve drawn in the editor: the profile, back in space
            let shown = catmull_rom(p, false)?;
            let (solid, path) = volume(Field::Revolve { axis, region: region.clone() }, min, max, shown)?;
            let Solid::Volume { field, min, max } = solid else { unreachable!() };
            Ok((Solid::Lathe { profile: region, axis, far, low: z0 - half, high: z1 + half, field, min, max }, path))
        }
    }
}

/// A curve as a flat band, a filled outline or a tube.
fn curve(
    spec: &Spec,
    path: Vec<DVec3>,
    closed: bool,
    normal: Option<DVec3>,
) -> Result<(Solid, Vec<DVec3>), String> {
    if path.len() < 2 {
        return Err("the curve has no length".into());
    }
    // the axis the curve lies flat across, if any: every point shares that coordinate
    let flat = (0..3).rev().find(|&a| {
        path.iter().all(|p| (p[a] - path[0][a]).abs() < 1e-6)
            && normal.is_none_or(|n| n[a].abs() > 1.0 - 1e-9)
    });
    let half = spec.thick / 2.0;
    if let Some(axis) = flat {
        return if spec.tube {
            stacked(spec, path, closed, axis)
        } else {
            planar(spec, path, closed, axis)
        };
    }
    if spec.fill {
        return Err(
            "only a curve lying flat along the grid can be filled; this one is tilted".into(),
        );
    }
    let reach = if spec.tube {
        half
    } else {
        half.max(spec.depth as f64 / 2.0)
    };
    let lo = path.iter().fold(DVec3::MAX, |a, p| a.min(*p)) - reach;
    let hi = path.iter().fold(DVec3::MIN, |a, p| a.max(*p)) + reach;
    let field = match normal {
        Some(normal) if !spec.tube => Field::Band {
            stroke: Stroke::new(&path, half, closed, Some(0.5)),
            origin: path[0],
            normal,
            half_depth: spec.depth as f64 / 2.0,
        },
        // a curve that does not lie in one plane has no flat band; a tube needs no plane
        _ => Field::Tube(Stroke::new(&path, half, closed, Some(0.5))),
    };
    volume(field, lo, hi, path)
}

/// Whether a band `width` cells wide along `path` would have its edges half
/// way between lattice lines, where no brick can end.
///
/// Judged where the path sets out: an arch's foot. A closed path has no such
/// place, so it is judged at its furthest reach along the axis, which for a
/// ring is where its edge runs straight. Left half a cell off there, a
/// ring's outline fits that straight run as a row of unit triangles, each
/// half a cell, which is right by area and looks like the teeth of a saw.
fn off_lattice(path: &[DVec3], closed: bool, axis: usize, width: i32) -> bool {
    let at = if closed { path.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max) } else { path[0][axis] };
    let edge = at + width as f64 / 2.0;
    (edge - edge.round()).abs() > 0.25
}

fn planar(
    spec: &Spec,
    path: Vec<DVec3>,
    closed: bool,
    axis: usize,
) -> Result<(Solid, Vec<DVec3>), String> {
    let (ua, va) = plane_axes(axis);
    let half = spec.thick / 2.0;
    // Points sit at cell centres, which centres a band an odd number of cells
    // wide: its ends are then run half a cell on, to the faces of the end
    // cells. A band an even number wide has to be centred on a lattice line
    // instead, or each side would cover half a row. Moving it half a cell
    // towards the origin does that, and puts its ends on lattice lines too
    // (an arch's feet land on the same floor either way), so it is not extended.
    //
    // That is the usual case, of points at cell centres. What decides is
    // whether the band's edge falls on a lattice line or half way between
    // two (see `off_lattice`): a ring of radius 38.5 about a cell's centre
    // runs along lattice lines, and it is then the odd widths that need the
    // half cell.
    let shifted = !spec.fill && off_lattice(&path, closed, ua, (spec.thick.round() as i32).max(1));
    let (shift, extend) = if shifted { (-0.5, 0.0) } else { (0.0, 0.5) };
    let flat: Vec<DVec3> = path
        .iter()
        .map(|p| DVec3::new(p[ua] + shift, p[va] + shift, 0.0))
        .collect();
    let region = if spec.fill {
        let mut poly: Vec<DVec2> = flat.iter().map(|p| p.truncate()).collect();
        if closed {
            poly.pop(); // a closed path repeats its first point
        }
        if poly.len() < 3 {
            return Err("there is no area to fill".into());
        }
        Region::Fill(Fill::new(poly))
    } else {
        Region::Stroke(Stroke::new(&flat, half, closed, Some(extend)))
    };
    let pad = if spec.fill { 1.0 } else { half + 1.5 };
    let lo = flat.iter().fold(DVec3::MAX, |a, p| a.min(*p)) - pad;
    let hi = flat.iter().fold(DVec3::MIN, |a, p| a.max(*p)) + pad;
    let (min, max) = (
        [lo.x.floor() as i32, lo.y.floor() as i32],
        [hi.x.ceil() as i32, hi.y.ceil() as i32],
    );
    check_size(&[(max[0] - min[0]) as i64, (max[1] - min[1]) as i64])?;
    // pushed through the grid, centred on the cell the curve is in
    let w0 = path[0][axis].floor() as i32 - (spec.depth - 1) / 2;
    Ok((
        Solid::Flat {
            region,
            axis,
            min,
            max,
            w0,
            depth: spec.depth,
        },
        path,
    ))
}

/// A round tube along a curve that lies flat across the grid, as a stack of
/// flat outlines.
///
/// Fitted as a volume, such a tube is hundreds of unit ramps and corner
/// pieces, since nothing in the volume fitter can follow a curve with a long
/// brick: a thin arc measured 265 bricks and 30% discrepancy, against 28
/// bricks as a flat band. But a tube lying flat is, in each layer of cells
/// across it, just a band of some width: widest through the middle, narrower
/// towards the two sides. Each layer is fitted as an outline, with long ramps
/// following the curve, and the layers are stacked. The tube's cross-section
/// comes out stepped, not chamfered; along its length, which is what the eye
/// follows, it is smooth.
fn stacked(
    spec: &Spec,
    path: Vec<DVec3>,
    closed: bool,
    axis: usize,
) -> Result<(Solid, Vec<DVec3>), String> {
    let (ua, va) = plane_axes(axis);
    let radius = spec.thick / 2.0;
    // as for a flat band: centred on a lattice line if an even number of
    // cells across, on a row of cells if odd (see `planar`)
    // How many cells across. For bands laid side by side, the thickness to
    // the nearest cell. For a pipe, whichever of the whole numbers either
    // side of the thickness gives the octagon nearer the circle in area: a
    // tube 6.5 across is far closer to an octagon 6 across (28 cells against
    // the circle's 33) than to one 7 across (41).
    let chamfer_of = |count: i32| ((count as f64 * (1.0 - std::f64::consts::FRAC_1_SQRT_2)).round() as i32).min((count - 1) / 2);
    let round_area = PI * radius * radius;
    let octagon = |count: i32| ((count * count - 2 * chamfer_of(count).pow(2)) as f64 - round_area).abs();
    let (below, above) = ((spec.thick.floor() as i32).max(1), (spec.thick.ceil() as i32).max(1));
    let pipe = spec.wedges && !spec.protrude;
    let count = if !pipe { (spec.thick.round() as i32).max(1) } else if octagon(below) <= octagon(above) { below } else { above };
    let (shift, extend) = if off_lattice(&path, closed, ua, count) { (-0.5, 0.0) } else { (0.0, 0.5) };
    let flat: Vec<DVec3> = path
        .iter()
        .map(|p| DVec3::new(p[ua] + shift, p[va] + shift, 0.0))
        .collect();
    // A circle may be stretched across its plane, which makes its tube that
    // many times as deep as it is wide: an oval in section.
    let deep = if matches!(spec.form, Form::Circle | Form::Torus) { spec.stretch[axis].max(0.05) } else { 1.0 };
    let depth = ((count as f64 * deep).round() as i32).max(1);
    let w0 = path[0][axis].floor() as i32 - (depth - 1) / 2;
    let centre = w0 as f64 + depth as f64 / 2.0;
    // a closed curve may be filled: each band then takes in all it encloses
    let outline: Vec<DVec2> = flat.iter().take(flat.len() - 1).map(|p| p.truncate()).collect();
    let filled = spec.fill && closed && outline.len() >= 3;
    let band = |half_width: f64| {
        let stroke = Stroke::new(&flat, half_width, closed, Some(extend));
        if filled { Region::Both(stroke, Fill::new(outline.clone())) } else { Region::Stroke(stroke) }
    };

    // each layer's half-width, from the circle, in whole cells less than the
    // radius so that every layer shares the tube's centre line
    let widths: Vec<f64> = (0..depth)
        .map(|i| {
            let off = (w0 as f64 + i as f64 + 0.5 - centre) / deep;
            let reach = (radius * radius - off * off).max(0.0).sqrt();
            (radius - (radius - reach).round()).max(0.5)
        })
        .collect();
    // neighbouring layers of one width are one deeper layer
    let mut layers: Vec<(Region, i32, i32)> = Vec::new();
    let mut at = 0;
    while at < widths.len() {
        let run = widths[at..]
            .iter()
            .take_while(|w| **w == widths[at])
            .count();
        layers.push((band(widths[at]), w0 + at as i32, run as i32));
        at += run;
    }

    let pad = radius + 1.5;
    let lo = flat.iter().fold(DVec3::MAX, |a, p| a.min(*p)) - pad;
    let hi = flat.iter().fold(DVec3::MIN, |a, p| a.max(*p)) + pad;
    let (min, max) = (
        [lo.x.floor() as i32, lo.y.floor() as i32],
        [hi.x.ceil() as i32, hi.y.ceil() as i32],
    );
    check_size(&[(max[0] - min[0]) as i64, (max[1] - min[1]) as i64, depth as i64])?;

    // the round tube itself, along the same shifted centre line
    let centre_line: Vec<DVec3> = flat
        .iter()
        .map(|p| {
            let mut q = DVec3::ZERO;
            (q[ua], q[va], q[axis]) = (p.x, p.y, centre);
            q
        })
        .collect();
    let mut field = Field::Tube(Stroke::new(&centre_line, radius, closed, Some(extend)));
    if deep != 1.0 {
        field = Field::Squash { inner: Box::new(field), axis, centre, factor: deep };
    }
    if filled {
        // filled, it is a plate with a rounded rim
        let plate = Field::Plate { fill: Fill::new(outline.clone()), axis, lo: w0 as f64, hi: (w0 + depth) as f64 };
        field = Field::Either(Box::new(field), Box::new(plate));
    }
    // As a pipe: a regular octagon, the roundest section bricks allow. Its
    // sloping sides are as long as its flat ones when each chamfer is
    // 1 - 1/sqrt(2) of the whole width. The flat top must be a band with an
    // inside, so the chamfer stops at least half a cell short of the middle;
    // a tube too thin for any chamfer is a square bar either way.
    // https://en.wikipedia.org/wiki/Octagon#Regular_octagon
    let half = count as f64 / 2.0;
    let chamfer = chamfer_of(count);
    // how many layers the chamfer takes: as many as it is wide, times the
    // stretch, and always leaving a wall between the two chamfers
    let rise = ((chamfer as f64 * deep).round() as i32).min((depth - 1) / 2);
    if pipe && chamfer > 0 && rise > 0 {
        let (top, full) = (band(half - chamfer as f64), band(half));
        return Ok((Solid::Pipe { top, full, chamfer, rise, axis, first: w0, layers: depth, min, max, field }, path));
    }
    Ok((
        Solid::Stack {
            layers,
            axis,
            min,
            max,
            field,
        },
        path,
    ))
}

fn volume(
    field: Field,
    lo: DVec3,
    hi: DVec3,
    path: Vec<DVec3>,
) -> Result<(Solid, Vec<DVec3>), String> {
    let min = (lo - 1.0).floor().to_array().map(|v| v as i32);
    let max = (hi + 1.0).ceil().to_array().map(|v| v as i32);
    check_size(&[0, 1, 2].map(|i| (max[i] - min[i]) as i64))?;
    Ok((Solid::Volume { field, min, max }, path))
}

/// Fit bricks to a form that has been shrunk by `g` (see [`build_at`]).
///
/// The brick price applies to flat forms only (see `fit_volume::Opts` for
/// why a volume has none). It is given in fine cells and a flat form is
/// priced in its outline, by area, so on the coarse lattice it is divided by
/// `g` squared: deliberately not by the volume its depth gives each piece,
/// since depth only pushes the outline through the grid and making the form
/// deeper must not change how its outline is fitted.
fn fit(spec: &Spec, solid: &Solid, path: Vec<DVec3>, g: f64) -> Built {
    match solid {
        Solid::Volume { field, min, max } => {
            let opts = fit_volume::Opts {
                max_ext: spec.max_ext,
                wedges: spec.wedges,
                stretch: true,
            };
            let fit = fit_volume(&|p| field.eval(p), *min, *max, &opts);
            Built {
                bricks: fit.bricks,
                solid: fit.solid,
                error: fit.error,
                path,
                grain: 1,
                unmeasured: false,
            }
        }
        Solid::Lathe { profile, axis, far, low, high, field, min, max } => {
            let section = |out: f64, up: f64| profile.eval(DVec2::new(out, up));
            // Laid out both ways `fit_lathe` knows, and the closer kept: which
            // way suits depends on the form (a straight cone one, a thin
            // curved wall the other), and measuring both is cheaper than a
            // rule that would have to guess.
            let ways = [false, true].map(|short_sides| {
                let lathe = Lathe { axis: *axis, section: &section, far: *far, low: *low, high: *high, brick_cost: spec.simplify / (g * g), short_sides, max_ext: spec.max_ext };
                let bricks = fit_lathe(&lathe).filter(|_| spec.wedges)?;
                let mut built = Built { bricks, solid: 0.0, error: 0.0, path: Vec::new(), grain: 1, unmeasured: true };
                let form = Probe::of(Solid::Volume { field: field.clone(), min: *min, max: *max });
                if let Ok(measured) = crate::report::analyse_probe(&form, &built) {
                    (built.solid, built.error, built.unmeasured) = (measured.form_volume, measured.missing + measured.extra, false);
                }
                Some(built)
            });
            let closer = ways.into_iter().flatten().min_by(|a, b| a.error.total_cmp(&b.error));
            match closer {
                Some(built) => Built { path, ..built },
                None => fit(spec, &Solid::Volume { field: field.clone(), min: *min, max: *max }, path, g),
            }
        }
        Solid::Pyramid { bricks, field, min, max } => match bricks.clone().filter(|_| spec.wedges) {
            Some(bricks) => Built { bricks, solid: 0.0, error: 0.0, path, grain: 1, unmeasured: true },
            None => fit(spec, &Solid::Volume { field: field.clone(), min: *min, max: *max }, path, g),
        },
        Solid::Round {
            round,
            field,
            min,
            max,
        } => match fit_round(round).filter(|_| spec.wedges) {
            Some(bricks) => Built {
                bricks,
                solid: 0.0,
                error: 0.0,
                path,
                grain: 1,
                unmeasured: true,
            },
            None => fit(
                spec,
                &Solid::Volume {
                    field: field.clone(),
                    min: *min,
                    max: *max,
                },
                path,
                g,
            ),
        },
        Solid::Flat {
            region,
            axis,
            min,
            max,
            w0,
            depth,
        } => {
            let mut bricks = Vec::new();
            let fit = fit_layer(spec, region, *axis, *min, *max, *w0, *depth, g, &mut bricks);
            let depth = *depth as f64;
            Built {
                bricks,
                solid: fit.solid * depth,
                error: fit.error * depth,
                path,
                grain: 1,
                unmeasured: false,
            }
        }
        Solid::Pipe { top, full, chamfer, rise, axis, first, layers, min, max, .. } => {
            let opts = fit_planar::Opts { brick_cost: spec.simplify / (g * g), max_ext: spec.max_ext, wedges: true };
            let narrower = |x: f64, y: f64| top.eval(DVec2::new(x, y));
            let fit = fit_planar_towards(&|x, y| full.eval(DVec2::new(x, y)), *min, *max, &opts, Some((&narrower, *chamfer)));
            let pipe = crate::fit_stack::Pipe { wall: &fit.pieces, top: &narrower, chamfer: *chamfer, rise: *rise, axis: *axis, first: *first, layers: *layers, max_ext: spec.max_ext };
            // measured against the round tube by the caller
            Built { bricks: crate::fit_stack::pipe(&pipe), solid: 0.0, error: 0.0, path, grain: 1, unmeasured: true }
        }
        Solid::Stack {
            layers,
            axis,
            min,
            max,
            ..
        } => {
            let mut bricks = Vec::new();
            for (region, w0, depth) in layers {
                fit_layer(spec, region, *axis, *min, *max, *w0, *depth, g, &mut bricks);
            }
            // measured against the round tube by the caller
            Built {
                bricks,
                solid: 0.0,
                error: 0.0,
                path,
                grain: 1,
                unmeasured: true,
            }
        }
    }
}

/// Fit one flat outline and push it through cells `w0..w0 + depth` across `axis`.
#[allow(clippy::too_many_arguments)]
fn fit_layer(
    spec: &Spec,
    region: &Region,
    axis: usize,
    min: [i32; 2],
    max: [i32; 2],
    w0: i32,
    depth: i32,
    g: f64,
    bricks: &mut Vec<CellBrick>,
) -> fit_planar::Fit {
    let (ua, va) = plane_axes(axis);
    let opts = fit_planar::Opts {
        brick_cost: spec.simplify / (g * g),
        max_ext: spec.max_ext,
        wedges: spec.wedges,
    };
    let fit = fit_planar(&|x, y| region.eval(DVec2::new(x, y)), min, max, &opts);
    for piece in &fit.pieces {
        let mask = (0..8u8).fold(0u8, |m, c| {
            m | (piece.mask >> ((c >> ua & 1) | (c >> va & 1) << 1) & 1) << c
        });
        let mut at = 0;
        while at < depth {
            let run = (depth - at).min(spec.max_ext);
            let (mut min, mut ext) = ([0; 3], [0; 3]);
            (min[ua], min[va], min[axis]) = (piece.x, piece.y, w0 + at);
            (ext[ua], ext[va], ext[axis]) = (piece.w, piece.h, run);
            bricks.push(CellBrick { mask, min, ext });
            at += run;
        }
    }
    fit
}

fn check_size(sides: &[i64]) -> Result<(), String> {
    let cells: i64 = sides.iter().map(|s| (*s).max(1)).product();
    if sides.iter().any(|s| !(0..=100_000).contains(s)) || cells > MAX_CELLS {
        return Err(format!("the form is too large to build: it spans {cells} cells and the limit is {MAX_CELLS}; make it smaller"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orient::{Models, Solver};
    use crate::shapes::{shape_info, Kind, CUBE};
    use std::collections::HashSet;

    fn at(x: f64, y: f64, z: f64) -> DVec3 {
        DVec3::new(x + 0.5, y + 0.5, z + 0.5)
    }

    /// What every build must satisfy to be placeable in the game.
    fn check(spec: &Spec) -> Built {
        let built = &build(spec).unwrap_or_else(|e| panic!("{spec:?}: {e}"));
        assert!(!built.bricks.is_empty());
        let solver = Solver::new(&Models::default());
        let mut seen = HashSet::new();
        for b in &built.bricks {
            assert!(b.ext.iter().all(|&e| (1..=80).contains(&e)), "{b:?}");
            assert!(solver.place(b).is_some(), "{b:?} has no orientation");
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        assert!(
                            seen.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]),
                            "{b:?} overlaps another brick"
                        );
                    }
                }
            }
        }
        // A sanity bound, not a target: these forms are a few cells thick, so
        // they are mostly surface, which is where all the disagreement is.
        assert!(
            built.error / built.solid < 0.25,
            "{:?}: error {} of {}",
            spec.form,
            built.error,
            built.solid
        );
        built.clone()
    }

    #[test]
    fn a_straight_band_is_exactly_its_thickness() {
        for thick in [1.0, 2.0, 3.0] {
            let spec = Spec {
                form: Form::Spline,
                points: vec![at(0.0, 0.0, 0.0), at(9.0, 0.0, 0.0)],
                thick,
                depth: 3,
                ..Spec::default()
            };
            let built = build(&spec).unwrap();
            assert_eq!(built.bricks.len(), 1, "thick {thick}: {:?}", built.bricks);
            let b = built.bricks[0];
            assert_eq!(b.mask, CUBE);
            // an odd band runs from the first cell's near face to the last
            // cell's far face; an even one sits half a cell back and ends on
            // the last cell's near face
            assert_eq!(b.ext[0], if thick == 2.0 { 9 } else { 10 });
            assert_eq!(b.min[0], 0);
            assert_eq!(b.ext[1], thick as i32);
            assert_eq!(b.ext[2], 3);
            assert_eq!(built.error, 0.0);
        }
    }

    #[test]
    fn a_round_arch_uses_stretched_ramps_and_stands_upright() {
        let spec = Spec {
            form: Form::Arch(ArchType::Round),
            points: vec![at(-12.0, 0.0, 0.0), at(12.0, 0.0, 0.0)],
            thick: 3.0,
            depth: 4,
            ..Spec::default()
        };
        let built = check(&spec);
        // flat across y: every brick is 4 deep in y and none is a corner piece
        assert!(built.bricks.iter().all(|b| b.ext[1] == 4));
        assert!(built
            .bricks
            .iter()
            .all(|b| matches!(shape_info(b.mask).unwrap().kind, Kind::Cube | Kind::Ramp)));
        assert!(built
            .bricks
            .iter()
            .any(|b| b.mask != CUBE && b.ext[0].max(b.ext[2]) >= 3));
        // the crown reaches a semicircle's height, and nothing dips below the feet
        let top = built
            .bricks
            .iter()
            .map(|b| b.min[2] + b.ext[2])
            .max()
            .unwrap();
        assert!((13..=15).contains(&top), "top {top}");
        assert!(built.bricks.iter().all(|b| b.min[2] >= 0));
    }

    #[test]
    fn every_form_builds_placeable_bricks() {
        let three = vec![at(-10.0, 0.0, 0.0), at(0.0, 0.0, 7.0), at(10.0, 0.0, 0.0)];
        let tilted = vec![at(-10.0, -3.0, 0.0), at(0.0, 2.0, 7.0), at(10.0, 6.0, 1.0)];
        let wavy = vec![
            at(-10.0, -3.0, 0.0),
            at(-3.0, 4.0, 5.0),
            at(4.0, -2.0, 9.0),
            at(10.0, 6.0, 1.0),
        ];
        let one = vec![at(0.0, 0.0, 0.0)];
        let base = Spec {
            thick: 3.0,
            depth: 3,
            radius: 9.0,
            ..Spec::default()
        };
        let specs = [
            Spec {
                form: Form::Arc,
                points: three.clone(),
                ..base.clone()
            },
            Spec {
                form: Form::Arc,
                points: three.clone(),
                closed: true,
                fill: true,
                ..base.clone()
            },
            Spec {
                form: Form::Arc,
                points: tilted.clone(),
                ..base.clone()
            },
            Spec {
                form: Form::Circle,
                points: one.clone(),
                ..base.clone()
            },
            Spec {
                form: Form::Circle,
                points: one.clone(),
                fill: true,
                plane: 0,
                ..base.clone()
            },
            Spec {
                form: Form::Arch(ArchType::Pointed),
                points: vec![three[0], three[2]],
                ..base.clone()
            },
            Spec {
                form: Form::Arch(ArchType::Triangular),
                points: vec![three[0], three[2]],
                fill: true,
                ..base.clone()
            },
            Spec {
                form: Form::Arch(ArchType::Catenary),
                points: vec![tilted[0], at(10.0, 6.0, 0.0)],
                ..base.clone()
            },
            Spec {
                form: Form::Spline,
                points: wavy.clone(),
                ..base.clone()
            },
            Spec {
                form: Form::Bezier,
                points: three.clone(),
                weights: vec![1.0, 3.0, 1.0],
                ..base.clone()
            },
            // 5 across: at 3 a round tube's nearest brick form is a square bar,
            // which is right but a quarter larger than the tube it stands for
            Spec {
                form: Form::Bezier,
                points: three.clone(),
                tube: true,
                thick: 5.0,
                ..base.clone()
            },
            Spec {
                form: Form::Sphere,
                points: one.clone(),
                fill: true,
                ..base.clone()
            },
            Spec {
                form: Form::Sphere,
                points: one.clone(),
                cut: Some((-0.5, f64::INFINITY)),
                ..base.clone()
            },
            Spec {
                form: Form::Sphere,
                points: one.clone(),
                fill: true,
                cut: Some((1.5, 5.5)),
                stretch: DVec3::new(1.0, 1.5, 0.8),
                ..base.clone()
            },
            Spec {
                form: Form::Torus,
                points: one.clone(),
                ..base.clone()
            },
            Spec {
                form: Form::Revolve,
                points: vec![at(0.0, 0.0, 10.0), at(6.0, 0.0, 7.0), at(8.0, 0.0, 0.0)],
                ..base.clone()
            },
            Spec {
                form: Form::Revolve,
                points: vec![at(0.0, 0.0, 10.0), at(6.0, 0.0, 7.0), at(8.0, 0.0, 0.0)],
                fill: true,
                ..base.clone()
            },
        ];
        for spec in specs {
            check(&spec);
        }
    }

    #[test]
    fn a_hollow_dome_is_empty_inside_and_flat_underneath() {
        let spec = Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: 10.0,
            thick: 2.0,
            cut: Some((-0.5, f64::INFINITY)),
            ..Spec::default()
        };
        let built = check(&spec);
        assert!(
            built.bricks.iter().all(|b| b.min[2] >= 0),
            "something is below the base"
        );
        let covers = |c: [i32; 3]| {
            built
                .bricks
                .iter()
                .any(|b| (0..3).all(|i| c[i] >= b.min[i] && c[i] < b.min[i] + b.ext[i]))
        };
        assert!(!covers([0, 0, 3]), "the middle is not hollow");
        assert!(covers([0, 0, 9]), "the top is missing");
    }

    #[test]
    fn points_off_the_grid_build_the_same_as_their_snapped_selves() {
        let on = Spec {
            form: Form::Arc,
            points: vec![at(-10.0, 0.0, 0.0), at(0.0, 0.0, 7.0), at(10.0, 0.0, 0.0)],
            ..Spec::default()
        };
        let mut off = on.clone();
        for (p, d) in off.points.iter_mut().zip([0.2, -0.24, 0.13]) {
            *p += DVec3::new(d, -d, d / 2.0);
        }
        off.thick += 0.2;
        assert_eq!(off.snapped(), on);
        assert_eq!(build(&off).unwrap().bricks, build(&on).unwrap().bricks);
        // whole units: half cells stay where they are
        assert_eq!(snap(3.5), 3.5);
        assert_eq!(snap(3.3), 3.5);
        assert_eq!(snap(-0.2), 0.0);
    }

    #[test]
    fn a_coarser_grain_gives_fewer_larger_bricks() {
        let sphere = |grain: i32| {
            build(&Spec {
                form: Form::Sphere,
                points: vec![at(0.0, 0.0, 0.0)],
                radius: 16.0,
                fill: true,
                grain,
                ..Spec::default()
            })
            .unwrap()
        };
        let (fine, coarse) = (sphere(1), sphere(4));
        assert_eq!((fine.grain, coarse.grain), (1, 4));
        assert!(
            coarse.bricks.len() * 2 < fine.bricks.len(),
            "{} vs {}",
            coarse.bricks.len(),
            fine.bricks.len()
        );
        assert!(coarse
            .bricks
            .iter()
            .all(|b| b.min.iter().chain(&b.ext).all(|v| v % 4 == 0)));
        // the same form: its volume agrees, and the coarse fit is the rougher
        assert!(
            (coarse.solid / fine.solid - 1.0).abs() < 0.02,
            "{} vs {}",
            coarse.solid,
            fine.solid
        );
        assert!(coarse.error > fine.error);
        check(&Spec {
            form: Form::Arch(ArchType::Round),
            points: vec![at(-24.0, 0.0, 0.0), at(24.0, 0.0, 0.0)],
            thick: 6.0,
            depth: 6,
            grain: 3,
            ..Spec::default()
        });
    }

    #[test]
    fn automatic_grain_keeps_within_the_budget() {
        let sphere = |budget: usize| {
            build(&Spec {
                form: Form::Sphere,
                points: vec![at(0.0, 0.0, 0.0)],
                radius: 30.0,
                fill: true,
                grain: 0,
                budget,
                ..Spec::default()
            })
            .unwrap()
        };
        let roomy = sphere(1_000_000);
        assert_eq!(roomy.grain, 1, "with room to spare, the finest size");
        let tight = sphere(300);
        assert!(
            tight.grain > 1 && tight.bricks.len() <= 300,
            "grain {} gave {} bricks",
            tight.grain,
            tight.bricks.len()
        );
        // the finest that fits: one size finer would be over
        let finer = GRAINS[GRAINS.iter().position(|g| *g == tight.grain).unwrap() - 1];
        let over = build(&Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: 30.0,
            fill: true,
            grain: finer,
            ..Spec::default()
        })
        .unwrap();
        assert!(over.bricks.len() > 300);
        // a budget nothing can meet still builds, at the coarsest size allowed:
        // a solid sphere of radius 30 is kept at least 4 bricks to its radius
        assert_eq!(sphere(1).grain, 6);
    }

    #[test]
    fn automatic_grain_never_exceeds_the_thinnest_part() {
        // a shell 4 thick: its wall is to be at least two bricks thick
        let shell = Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: 30.0,
            thick: 4.0,
            fill: false,
            grain: 0,
            budget: 10,
            ..Spec::default()
        };
        let built = build(&shell).unwrap();
        assert_eq!(built.grain, 2);
        assert!(
            built.error / built.solid < 0.2,
            "error {} of {}",
            built.error,
            built.solid
        );
        assert_eq!(
            build(&Spec {
                thick: 2.0,
                ..shell.clone()
            })
            .unwrap()
            .grain,
            1
        );
        // and it is still a shell: hollow in the middle
        assert!(!built
            .bricks
            .iter()
            .any(|b| (0..3).all(|i| b.min[i] <= 0 && 0 < b.min[i] + b.ext[i])));
        // a band 1 thick stays at single microbricks however small the budget
        let band = Spec {
            form: Form::Arch(ArchType::Round),
            points: vec![at(-20.0, 0.0, 0.0), at(20.0, 0.0, 0.0)],
            thick: 1.0,
            depth: 6,
            grain: 0,
            budget: 1,
            ..Spec::default()
        };
        assert_eq!(build(&band).unwrap().grain, 1);
        // a band 6 thick could take bricks 3 across, but not at a depth of 4:
        // 4 is not a whole number of 3s, so it takes 2
        let deep = |depth: i32| {
            build(&Spec {
                thick: 6.0,
                depth,
                ..band.clone()
            })
            .unwrap()
        };
        assert_eq!(deep(6).grain, 3);
        assert_eq!(deep(4).grain, 2);
        assert!(
            deep(4).bricks.iter().all(|b| b.ext[1] == 4),
            "the depth is unchanged"
        );
    }

    #[test]
    fn simplify_thins_flat_forms_and_leaves_volumes_alone() {
        let count = |form: Form, points: Vec<DVec3>, simplify: f64| {
            build(&Spec {
                form,
                points,
                radius: 12.0,
                thick: 3.0,
                fill: true,
                simplify,
                ..Spec::default()
            })
            .unwrap()
            .bricks
            .len()
        };
        let feet = vec![at(-12.0, 0.0, 0.0), at(12.0, 0.0, 0.0)];
        assert!(
            count(Form::Arch(ArchType::Round), feet.clone(), 0.5)
                < count(Form::Arch(ArchType::Round), feet, 0.0)
        );
        // a volume has no brick price: corner pieces are never worth one, and
        // without them a surface is pitted
        let centre = vec![at(0.0, 0.0, 0.0)];
        assert_eq!(
            count(Form::Sphere, centre.clone(), 0.5),
            count(Form::Sphere, centre, 0.0)
        );
    }

    /// From a saved report: where one long ramp ends and the next begins a
    /// cell away diagonally, the cell between them was left as a square step,
    /// because the form fills only a fifth of it.
    #[test]
    fn a_step_between_two_ramps_is_bridged() {
        let spec = Spec {
            form: Form::Arch(ArchType::Round),
            points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)],
            rise: Some(8.0),
            thick: 2.0,
            depth: 2,
            simplify: 0.25,
            ..Spec::default()
        };
        let built = build(&spec).unwrap();
        for (x, mask) in [(-7, 0xaf), (6, 0x5f)] {
            let found = built
                .bricks
                .iter()
                .find(|b| b.min == [x, 0, 6] && b.ext == [1, 2, 1]);
            assert_eq!(
                found.map(|b| b.mask),
                Some(mask),
                "no ramp bridging the step at x = {x}: {:?}",
                built.bricks
            );
        }
        check(&spec);
    }

    #[test]
    fn depth_does_not_change_a_flat_forms_outline() {
        let outline = |depth: i32| {
            let spec = Spec {
                form: Form::Arch(ArchType::Round),
                points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)],
                rise: Some(8.0),
                thick: 2.0,
                depth,
                simplify: 0.25,
                ..Spec::default()
            };
            let built = build(&spec).unwrap();
            assert!(built.bricks.iter().all(|b| b.ext[1] == depth));
            let mut pieces: Vec<_> = built
                .bricks
                .iter()
                .map(|b| (b.mask, b.min[0], b.min[2], b.ext[0], b.ext[2]))
                .collect();
            pieces.sort();
            pieces
        };
        let shallow = outline(1);
        for depth in [2, 5, 11, 40] {
            assert_eq!(outline(depth), shallow, "depth {depth}");
        }
    }

    /// From a saved report (rise 11.5): a symmetric arch came out with
    /// different bricks on its two sides.
    #[test]
    fn a_symmetric_arch_is_fitted_symmetrically() {
        use std::collections::HashSet;
        let flip = |mask: u8| (0..8u8).fold(0u8, |m, c| m | (mask >> c & 1) << (c ^ 1));
        for kind in crate::curves::ARCH_TYPES {
            for rise in (12..=40).map(|r| r as f64 / 2.0) {
                for simplify in [0.0, 0.25] {
                    let spec = Spec {
                        form: Form::Arch(kind),
                        points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)],
                        rise: Some(rise),
                        thick: 2.0,
                        depth: 1,
                        simplify,
                        ..Spec::default()
                    };
                    let built = build(&spec).unwrap();
                    // ramps must pair up exactly; cubes are merged in scan
                    // order, so for those it is the cells they cover that must
                    let ramps: HashSet<(u8, [i32; 3], [i32; 3])> = built
                        .bricks
                        .iter()
                        .filter(|b| b.mask != 0xff)
                        .map(|b| (b.mask, b.min, b.ext))
                        .collect();
                    let mut cubes = HashSet::new();
                    for b in built.bricks.iter().filter(|b| b.mask == 0xff) {
                        cubes.extend((0..b.ext[0]).flat_map(|x| {
                            (0..b.ext[2]).map(move |z| (b.min[0] + x, b.min[2] + z))
                        }));
                    }
                    for &(mask, min, ext) in &ramps {
                        let mirrored = (flip(mask), [-min[0] - ext[0], min[1], min[2]], ext);
                        assert!(ramps.contains(&mirrored), "{kind:?} rise {rise} simplify {simplify}: ramp {mask:#x} at {min:?} size {ext:?} has no mirror image");
                    }
                    for &(x, z) in &cubes {
                        assert!(cubes.contains(&(-x - 1, z)), "{kind:?} rise {rise} simplify {simplify}: cube cell ({x}, {z}) has no mirror image");
                    }
                }
            }
        }
    }

    /// From saved reports 8 to 10: a thin round tube along an arc came out
    /// as hundreds of unit corner pieces.
    #[test]
    fn a_flat_tube_is_stacked_outlines_not_a_cloud_of_corner_pieces() {
        let arc = vec![
            at(-25.0, 0.0, -2.0),
            at(-1.0, 0.0, 17.0),
            at(19.0, 0.0, 1.0),
        ];
        let tube = |thick: f64| Spec {
            form: Form::Arc,
            points: arc.clone(),
            thick,
            tube: true,
            ..Spec::default()
        };
        let band = build(&Spec {
            tube: false,
            thick: 2.0,
            depth: 2,
            ..tube(2.0)
        })
        .unwrap();

        // 2 across, a round tube is as near a square band as bricks allow
        let thin = build(&tube(2.0)).unwrap();
        assert_eq!(thin.bricks.len(), band.bricks.len(), "was 265 bricks");
        assert!(thin
            .bricks
            .iter()
            .all(|b| matches!(shape_info(b.mask).unwrap().kind, Kind::Cube | Kind::Ramp)));
        assert!(
            thin.bricks.iter().all(|b| b.ext[1] == 2 && b.min[1] == 0),
            "{:?}",
            thin.bricks[0]
        );

        // 6 across: narrower layers outside, wider within, all in whole cells
        let thick = check(&Spec { protrude: true, ..tube(6.0) });
        // the six layers across y, cells -2 to 3: one narrow, four wide as one, one narrow
        let depths: HashSet<(i32, i32)> =
            thick.bricks.iter().map(|b| (b.min[1], b.ext[1])).collect();
        assert_eq!(depths, HashSet::from([(-2, 1), (-1, 4), (3, 1)]));
        assert!(
            thick.bricks.len() < 250,
            "{} bricks; the volume fit was 785",
            thick.bricks.len()
        );
        assert!(
            thick
                .bricks
                .iter()
                .any(|b| b.mask != CUBE && b.ext[0].max(b.ext[2]) >= 3),
            "no long ramps along the curve"
        );
        // measured against the true round tube, not against its own layers
        assert!(
            thick.solid > 0.0 && thick.error / thick.solid < 0.2,
            "error {} of {}",
            thick.error,
            thick.solid
        );

        // a tube that does not lie flat is still fitted as a volume
        let tilted = vec![at(-10.0, -3.0, 0.0), at(0.0, 2.0, 7.0), at(10.0, 6.0, 1.0)];
        assert!(build(&Spec {
            points: tilted,
            ..tube(4.0)
        })
        .unwrap()
        .bricks
        .iter()
        .any(|b| shape_info(b.mask).unwrap().kind == Kind::Tet));
    }

    /// From saved reports 13 and 15: a round tube built as bands side by
    /// side reads as narrow arches stuck on a wide one, and a torus fitted
    /// cell by cell is a third treads and risers.
    #[test]
    fn a_round_tube_is_a_pipe_with_chamfered_edges() {
        let stepped = |spec: &Spec| {
            let built = check(spec);
            let survey = crate::surface::survey(&Probe::new(spec, built.grain).unwrap(), &built).unwrap();
            (built, survey.stepped / survey.area)
        };
        let arch = Spec { form: Form::Arch(ArchType::Round), points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)], thick: 6.5, tube: true, simplify: 0.25, ..Spec::default() };
        let (pipe, pipe_steps) = stepped(&arch);
        let (_, band_steps) = stepped(&Spec { protrude: true, ..arch.clone() });
        assert!(pipe_steps < 0.06 && band_steps > 0.25, "pipe {pipe_steps}, bands {band_steps}");
        // an octagon across: 6 cells wide, the outer 2 each side sloping
        let across: HashSet<(i32, i32)> = pipe.bricks.iter().map(|b| (b.min[1], b.ext[1])).collect();
        assert_eq!(across, HashSet::from([(-2, 2), (0, 2), (2, 2), (-2, 6)]), "{across:?}");
        for kind in [Kind::Ramp, Kind::Tet, Kind::Cut] {
            assert!(pipe.bricks.iter().any(|b| shape_info(b.mask).unwrap().kind == kind && b.ext[1] == 2 && b.min[1] != 0), "no {kind:?} in the chamfer");
        }
        // the same seen from either side, and from front and back
        let cells: HashSet<[i32; 3]> = pipe.bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).flat_map(move |y| (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1] + y, b.min[2] + z])))).collect();
        assert!(cells.iter().all(|c| cells.contains(&[-c[0] - 1, c[1], c[2]]) && cells.contains(&[c[0], 1 - c[1], c[2]])), "the pipe is lopsided");
        // a wall with no holes: every cell of the middle layers that is well inside the tube is built
        let probe = Probe::new(&arch, pipe.grain).unwrap();
        for z in 0..12 {
            for x in -12..12 {
                let centre = DVec3::new(x as f64 + 0.5, 0.5, z as f64 + 0.5);
                assert!(probe.distance(centre) > -1.0 || cells.contains(&[x, 0, z]), "a hole at {x}, {z}");
            }
        }

        // From saved report 18: a wide ring. Its top band is two cells wide
        // and runs diagonally for much of the way round, where it cannot be
        // fitted with slopes; built outwards from that band the chamfer was
        // lost there and the ring was 29% steps.
        let ring = Spec { form: Form::Torus, points: vec![at(-1.0, 0.0, 3.0)], radius: 19.0, thick: 6.5, ..Spec::default() };
        let (_, wide_steps) = stepped(&ring);
        assert!(wide_steps < 0.03, "wide ring {wide_steps}");

        // a torus is a pipe along a circle
        let torus = Spec { form: Form::Torus, points: vec![at(0.0, 0.0, 0.0)], radius: 18.5, thick: 5.5, ..Spec::default() };
        let (_, ring_steps) = stepped(&torus);
        let (_, cell_steps) = stepped(&Spec { wedges: false, ..torus.clone() });
        assert!(ring_steps < 0.06 && ring_steps < cell_steps / 2.0, "pipe {ring_steps}, cubes {cell_steps}");
    }

    #[test]
    fn a_flipped_form_is_the_mirror_image_brick_for_brick() {
        let dome = Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 4.0)], radius: 9.5, thick: 2.0, cut: Some((-0.5, f64::INFINITY)), ..Spec::default() };
        let arc = Spec { form: Form::Arc, points: vec![at(-9.0, 0.0, 0.0), at(-2.0, 0.0, 7.0), at(10.0, 0.0, 1.0)], thick: 2.0, ..Spec::default() };
        for (spec, axis) in [(dome, 2), (arc.clone(), 0), (arc, 2)] {
            let mut flip = [false; 3];
            flip[axis] = true;
            let flipped_spec = Spec { flip, ..spec.clone() };
            let (plain, flipped) = (build(&spec).unwrap(), check(&flipped_spec));
            assert_ne!(plain.bricks, flipped.bricks);
            // the dome's base is now its top; the arc's low end has changed sides
            let lo = spec.points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
            let hi = spec.points.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
            let twice = (lo + hi).round() as i32;
            let mut back: Vec<CellBrick> = flipped.bricks.clone();
            for b in &mut back {
                b.min[axis] = twice - b.min[axis] - b.ext[axis];
                b.mask = (0..8u8).fold(0u8, |m, c| m | (b.mask >> c & 1) << (c ^ (1 << axis)));
            }
            assert_eq!(back, plain.bricks);
            // and measured against the mirrored form, it fits exactly as well
            let measured = crate::report::analyse(&flipped_spec, &flipped).unwrap();
            let before = crate::report::analyse(&spec, &plain).unwrap();
            assert!((measured.missing + measured.extra - before.missing - before.extra).abs() < 1e-9);
        }
    }

    /// From saved report 22: an arch taller than a semicircle ends on feet
    /// cut at a slant, and its two feet came out as different bricks.
    #[test]
    fn a_horseshoe_arch_has_two_feet_alike() {
        let flip = |mask: u8| (0..8u8).fold(0u8, |m, c| m | (mask >> c & 1) << (c ^ 1));
        for (thick, tube) in [(7.0, false), (7.5, true), (3.0, false), (5.0, true)] {
            for rise in [20.0, 24.0, 27.0] {
                let spec = Spec { form: Form::Arch(ArchType::Round), points: vec![at(7.0, 0.0, 0.0), at(37.0, 0.0, 0.0)], rise: Some(rise), thick, depth: 1, tube, simplify: 0.25, ..Spec::default() };
                let built = check(&spec);
                let sloped: HashSet<(u8, [i32; 3], [i32; 3])> = built.bricks.iter().filter(|b| b.mask != CUBE).map(|b| (b.mask, b.min, b.ext)).collect();
                assert!(sloped.len() > 10);
                for &(mask, min, ext) in &sloped {
                    // the arch's middle is the plane x = 22.5
                    let mirrored = (flip(mask), [45 - min[0] - ext[0], min[1], min[2]], ext);
                    assert!(sloped.contains(&mirrored), "thick {thick} tube {tube} rise {rise}: {mask:#x} at {min:?} size {ext:?} has no mirror image");
                }
            }
        }
    }

    /// From saved report 23 and a screenshot of it: a torus of radius 38.5,
    /// 5.5 thick, had a row of unit triangles like saw teeth where its
    /// outline crosses each axis, and elsewhere alternated between two
    /// slopes instead of turning steadily.
    #[test]
    fn a_ring_turns_steadily_all_the_way_round() {
        for (radius, thick) in [(38.5, 5.5), (19.0, 6.5), (19.0, 3.0), (26.0, 4.0)] {
            let spec = Spec { form: Form::Torus, points: vec![at(0.0, 0.0, 1.0)], radius, thick, ..Spec::default() };
            let built = check(&spec);
            // the wall's upright sloped pieces: one quarter of the outer outline,
            // in order round it, as the angle each side makes with the y axis
            let mid = built.bricks.iter().map(|b| b.min[2]).min().unwrap() + (thick as i32) / 2;
            let centre = built.bricks.iter().fold([i32::MAX, i32::MIN, i32::MAX, i32::MIN], |m, b| [m[0].min(b.min[0]), m[1].max(b.min[0] + b.ext[0]), m[2].min(b.min[1]), m[3].max(b.min[1] + b.ext[1])]);
            let (cx, cy) = ((centre[0] + centre[1]) as f64 / 2.0, (centre[2] + centre[3]) as f64 / 2.0);
            let mut sides: Vec<(f64, f64)> = built
                .bricks
                .iter()
                .filter(|b| b.min[2] <= mid && mid < b.min[2] + b.ext[2] && shape_info(b.mask).unwrap().kind == Kind::Ramp)
                .filter_map(|b| {
                    let (x, y) = (b.min[0] as f64 + b.ext[0] as f64 / 2.0 - cx, b.min[1] as f64 + b.ext[1] as f64 / 2.0 - cy);
                    (x > 0.0 && y > 0.0 && x.hypot(y) > radius).then(|| (y.atan2(x), (b.ext[0] as f64).atan2(b.ext[1] as f64).to_degrees()))
                })
                .collect();
            sides.sort_by(|a, b| a.0.total_cmp(&b.0));
            let slopes: Vec<f64> = sides.iter().map(|s| s.1.round()).collect();
            assert!(slopes.len() >= 5, "radius {radius}: only {} sloped sides in a quarter", slopes.len());
            assert!(slopes.windows(2).all(|w| w[0] <= w[1]), "radius {radius} thick {thick}: the outline's slopes go back and forth: {slopes:?}");
            // nearly upright where it leaves the x axis, nearly level at the y axis
            assert!(slopes[0] < 30.0 && slopes[slopes.len() - 1] > 60.0, "radius {radius} thick {thick}: {slopes:?}");
        }
    }

    #[test]
    fn a_circle_stretches_fills_and_stands_in_for_the_torus() {
        let ring = Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 14.0, thick: 6.0, tube: true, simplify: 0.25, ..Spec::default() };
        let size = |built: &Built, k: usize| built.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap() - built.bricks.iter().map(|b| b.min[k]).min().unwrap();
        let plain = check(&ring);
        assert_eq!([size(&plain, 0), size(&plain, 1), size(&plain, 2)], [34, 34, 6]);
        // a torus is the same thing under its old name
        assert_eq!(build(&Spec { form: Form::Torus, tube: false, ..ring.clone() }).unwrap().bricks, plain.bricks);
        // stretched within its plane it is an ellipse, and across it a deeper or shallower tube
        let wide = check(&Spec { stretch: DVec3::new(1.5, 1.0, 1.0), ..ring.clone() });
        assert_eq!([size(&wide, 0), size(&wide, 1), size(&wide, 2)], [48, 34, 6]);
        let deep = check(&Spec { stretch: DVec3::new(1.0, 1.0, 2.0), ..ring.clone() });
        assert_eq!([size(&deep, 0), size(&deep, 1), size(&deep, 2)], [34, 34, 12]);
        let shallow = check(&Spec { stretch: DVec3::new(1.0, 1.0, 0.5), ..ring.clone() });
        assert_eq!(size(&shallow, 2), 3);
        for built in [&deep, &shallow] {
            assert!(built.bricks.iter().any(|b| shape_info(b.mask).unwrap().kind == Kind::Tet), "no chamfer left");
        }
        // facing x, its plane is y and z, and x is its depth
        let turned = check(&Spec { plane: 0, stretch: DVec3::new(2.0, 1.0, 1.5), ..ring.clone() });
        assert_eq!([size(&turned, 0), size(&turned, 1), size(&turned, 2)], [12, 34, 48]);
        // filled, the hole is gone and the rim is still rounded
        let filled = check(&Spec { fill: true, ..ring.clone() });
        let covers = |built: &Built, c: [i32; 3]| built.bricks.iter().any(|b| (0..3).all(|i| c[i] >= b.min[i] && c[i] < b.min[i] + b.ext[i]));
        assert!(covers(&filled, [0, 0, 0]) && !covers(&plain, [0, 0, 0]));
        assert_eq!([size(&filled, 0), size(&filled, 2)], [34, 6]);
        assert!(filled.bricks.iter().any(|b| shape_info(b.mask).unwrap().kind == Kind::Tet));
        assert!(filled.error / filled.solid < 0.05, "error {} of {}", filled.error, filled.solid);
    }

    #[test]
    fn a_pyramid_is_built_exactly_from_a_handful_of_bricks() {
        let spec = Spec { form: Form::Pyramid, points: vec![at(0.0, 0.0, 0.0)], radius: 8.5, rise: Some(8.0), ..Spec::default() };
        let built = check(&spec);
        let kind = |k: Kind| built.bricks.iter().filter(|b| shape_info(b.mask).unwrap().kind == k).count();
        // centred in a cell: a hip on each ridge, a ramp on each side, a column up the middle
        assert_eq!((built.bricks.len(), kind(Kind::Hip), kind(Kind::Ramp), kind(Kind::Cube)), (9, 4, 4, 1));
        assert_eq!(built.bricks.iter().map(|b| b.min[2]).min(), Some(0));
        assert_eq!(built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max(), Some(8));
        // exact: what little is measured is the sampling of sloped faces
        assert!(built.error / built.solid < 0.03, "error {} of {}", built.error, built.solid);
        let survey = crate::surface::survey(&Probe::new(&spec, built.grain).unwrap(), &built).unwrap();
        assert!(survey.stepped == 0.0 && survey.mean_angle < 2.0, "stepped {}, mean angle {}", survey.stepped, survey.mean_angle);
        // oblong, and flipped into a funnel
        let oblong = check(&Spec { stretch: DVec3::new(2.0, 1.0, 1.0), flip: [false, false, true], ..spec.clone() });
        let size = |k: usize| oblong.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap() - oblong.bricks.iter().map(|b| b.min[k]).min().unwrap();
        assert_eq!([size(0), size(1), size(2)], [35, 17, 8]);
        // with cubes alone it is still a pyramid, in steps
        let steps = check(&Spec { wedges: false, ..spec.clone() });
        assert!(steps.bricks.iter().all(|b| b.mask == CUBE) && steps.bricks.len() >= 6);
    }

    /// From saved report 24: a hollow turned form, fitted cell by cell, was
    /// a quarter treads and risers.
    #[test]
    fn a_turned_form_is_sloped_rings_not_a_staircase() {
        let stepped = |spec: &Spec| {
            let built = check(spec);
            let survey = crate::surface::survey(&Probe::new(spec, built.grain).unwrap(), &built).unwrap();
            (built, survey.stepped / survey.area)
        };
        let bell = Spec { form: Form::Revolve, points: vec![at(0.0, 0.0, 29.0), at(17.0, 0.0, 9.0), at(24.0, 0.0, 16.0)], thick: 3.5, fill: false, simplify: 0.25, ..Spec::default() };
        let (built, steps) = stepped(&bell);
        assert!(steps < 0.15, "the bell is {steps} steps");
        for kind in [Kind::Ramp, Kind::Tet, Kind::Cut] {
            assert!(built.bricks.iter().any(|b| shape_info(b.mask).unwrap().kind == kind), "no {kind:?}");
        }
        // the same all the way round: mirrored in x and in y, and with the two exchanged
        let cells: HashSet<[i32; 3]> = built.bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).flat_map(move |y| (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1] + y, b.min[2] + z])))).collect();
        assert!(cells.iter().all(|c| cells.contains(&[-c[0], c[1], c[2]]) && cells.contains(&[c[0], -c[1], c[2]]) && cells.contains(&[c[1], c[0], c[2]])), "the bell is not the same all round");
        // solid, and a plain dome's profile
        let (_, solid) = stepped(&Spec { fill: true, ..bell.clone() });
        let (_, dome) = stepped(&Spec { points: vec![at(0.0, 0.0, 14.0), at(9.0, 0.0, 11.0), at(14.0, 0.0, 0.0)], fill: true, ..bell.clone() });
        assert!(solid < 0.1 && dome < 0.06, "solid {solid}, dome {dome}");
        // cubes alone are still the staircase, and an axis between the two grids is still fitted cell by cell
        let (_, cubes) = stepped(&Spec { wedges: false, ..bell.clone() });
        assert!(cubes > 0.4, "cubes {cubes}");
        check(&Spec { points: vec![DVec3::new(0.5, 0.0, 14.5), DVec3::new(9.5, 0.0, 11.5), DVec3::new(14.5, 0.0, 0.5)], fill: true, ..bell });
    }

    #[test]
    fn fix_symmetry_makes_the_high_side_the_mirror_of_the_low() {
        // a ball of radius 6 about the origin, in eight like corners
        let ball = |p: DVec3| p.length() - 6.0;
        let whole: Vec<CellBrick> = (0..8)
            .map(|c| CellBrick { mask: CUBE, min: [0, 1, 2].map(|k| if c >> k & 1 == 1 { 0 } else { -4 }), ext: [4; 3] })
            .collect();
        // already symmetric: nothing to do
        let mut bricks = whole.clone();
        assert!(!symmetrise(&mut bricks, &ball));
        assert_eq!(bricks, whole);
        // one high corner turned into a ramp, another lost: both put right, from the low side
        let mut bricks = whole.clone();
        bricks[7].mask = 0x77;
        bricks.remove(3);
        assert!(symmetrise(&mut bricks, &ball));
        let mut found = bricks.clone();
        found.sort_by_key(|b| b.min);
        let mut expected = whole.clone();
        expected.sort_by_key(|b| b.min);
        assert_eq!(found, expected);
        // a form that is not symmetric along an axis is left alone along it
        let lopsided = |p: DVec3| (p - DVec3::new(0.0, 0.0, 1.5)).length() - 6.0;
        let mut bricks = whole.clone();
        for b in bricks.iter_mut().filter(|b| b.min[2] == 0) {
            b.ext[2] = 2; // the top half as thick, all four corners alike
        }
        let short = bricks.clone();
        assert!(!symmetrise(&mut bricks, &lopsided));
        assert_eq!(bricks, short);
        // and through a whole build it changes nothing where the fit is already symmetric
        let arch = Spec { form: Form::Arch(ArchType::Round), points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)], thick: 3.0, ..Spec::default() };
        let mut plain = build(&arch).unwrap().bricks;
        let mut fixed = check(&Spec { symmetric: true, ..arch.clone() }).bricks;
        plain.sort_by_key(|b| (b.min, b.ext, b.mask));
        fixed.sort_by_key(|b| (b.min, b.ext, b.mask));
        assert_eq!(plain, fixed);
    }

    #[test]
    fn problems_are_reported_not_built() {
        let err = |spec: Spec| build(&spec).unwrap_err();
        assert!(err(Spec {
            form: Form::Arc,
            points: vec![at(0.0, 0.0, 0.0)],
            ..Spec::default()
        })
        .contains("3 points"));
        let line = vec![at(0.0, 0.0, 0.0), at(1.0, 0.0, 0.0), at(2.0, 0.0, 0.0)];
        assert!(err(Spec {
            form: Form::Arc,
            points: line,
            ..Spec::default()
        })
        .contains("straight line"));
        assert!(err(Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: 5000.0,
            ..Spec::default()
        })
        .contains("too large"));
        // a point flung far away is refused at once, whatever the form: these
        // used to run out of memory building the curve before any size check
        let far = vec![at(0.0, 0.0, 0.0), at(3.0e7, 0.0, 2.0e6)];
        for (form, tube) in [
            (Form::Arch(ArchType::Round), false),
            (Form::Spline, false),
            (Form::Spline, true),
            (Form::Bezier, true),
            (Form::Revolve, false),
        ] {
            let started = std::time::Instant::now();
            assert!(
                err(Spec {
                    form,
                    tube,
                    points: far.clone(),
                    ..Spec::default()
                })
                .contains("too large"),
                "{form:?}"
            );
            assert!(
                started.elapsed().as_millis() < 200,
                "{form:?} took {:?} to refuse",
                started.elapsed()
            );
        }
        assert!(err(Spec {
            form: Form::Arch(ArchType::Round),
            points: vec![at(-8.0, 0.0, 0.0), at(8.0, 0.0, 0.0)],
            rise: Some(1.0e6),
            ..Spec::default()
        })
        .contains("too large"));
        assert!(err(Spec {
            form: Form::Sphere,
            points: vec![at(0.0, 0.0, 0.0)],
            radius: f64::NAN,
            ..Spec::default()
        })
        .contains("positive"));
    }
}
