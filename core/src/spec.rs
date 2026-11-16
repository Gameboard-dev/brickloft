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
use crate::shapes::{CellBrick, CUBE, UPRIGHT};
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
    /// Make the form itself symmetric along x, y or z: its low side along
    /// that axis is kept and the high side replaced by its mirror image,
    /// about the middle of the form's points (see [`mirrored`]). Unlike
    /// `symmetric`, which only tidies the bricks of a form that is already
    /// its own mirror image, this changes the form: an arch with one foot
    /// higher than the other becomes an arch with both feet as the low
    /// side's is, and a point added to one side of a circle appears on the
    /// other too.
    pub mirror: [bool; 3],
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
    /// How the finished form is turned, in quarter turns: one of the game's
    /// 24 orientations (`shapes::ORIENT`), [`UPRIGHT`] for not at all. The
    /// form is fitted as it stands and its bricks are turned afterwards,
    /// about the middle of its points (see [`Turn`]), so a form is the same
    /// bricks whichever way it faces, and forms that can only be fitted
    /// upright (a pyramid, a profile spun about the vertical) can lie on
    /// their sides.
    pub turned: u8,
    /// The angle the form's own brick grid stands at, as a quaternion
    /// (x, y, z, w); `[0, 0, 0, 1]` for none. The game keeps bricks on
    /// grids, and a grid other than the world's can stand at any angle. A
    /// form turned by anything but quarter turns is on such a grid of its
    /// own: fitted here exactly as if it were not turned at all, which is
    /// why nothing in this library reads this, and turned as a whole by
    /// whatever places it (the editor's view, the file it copies).
    pub grid: [f64; 4],
    /// Where that grid is: what is added to a point of the form, once
    /// turned by `grid`, to find it in the world. In cells. All zero for a
    /// grid that is not turned.
    pub grid_at: [f64; 3],
    /// Which grid the form is on. Forms with the same number share a grid:
    /// they are turned and moved together, and kept clear of each other.
    pub on_grid: u32,
    /// Places where a curve is pulled out of its shape: a point on it, and
    /// how far that point is moved. The curve to either side follows, less
    /// and less, out to `pull_reach` along it (see [`pulled`]). Curves only:
    /// a sphere, a pyramid or a turned form has none.
    pub pulls: Vec<(DVec3, DVec3)>,
    pub pull_reach: f64,
    /// A flat band or filled outline laid as stepping stones with gaps
    /// between them, in place of solid brick (see [`crate::pattern`]).
    /// `None` for solid. Forms that are not flat take no notice of it.
    pub stones: Option<crate::pattern::Stones>,
    /// A flat band or filled outline laid as a mosaic of coloured tiles
    /// (see [`crate::pattern`]). `None` for none. If `stones` is set too,
    /// the stones are laid.
    pub mosaic: Option<crate::pattern::Mosaic>,
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
            mirror: [false; 3],
            turned: UPRIGHT,
            grid: [0.0, 0.0, 0.0, 1.0],
            grid_at: [0.0; 3],
            on_grid: 0,
            pulls: Vec::new(),
            pull_reach: 8.0,
            stones: None,
            mosaic: None,
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
    /// For a form laid as a pattern of colours (a mosaic): which of the
    /// pattern's colours each brick is, in the order of `bricks`. Empty for
    /// every other form, whose colouring is the editor's own affair.
    pub shades: Vec<u8>,
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
        // (where a pull takes hold need not be on the grid: it is only looked for along the curve)
        for pull in &mut s.pulls {
            pull.1 = DVec3::new(snap(pull.1.x), snap(pull.1.y), snap(pull.1.z));
        }
        s.pull_reach = snap(s.pull_reach).max(1.0);
        s
    }
}

/// Build a form. The spec is snapped to the unit grid first (see
/// [`Spec::snapped`]), so nothing off the grid can reach the fitters.
pub fn build(spec: &Spec) -> Result<Built, String> {
    build_as(spec, true)
}

/// A turn of a form's bricks by quarter turns about a point of the lattice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    orientation: u8,
    about: [i32; 3],
}

impl Turn {
    /// The turn `spec` asks for: about the middle of its points, to the
    /// nearest lattice point, so that turned bricks are on the lattice too.
    pub fn of(spec: &Spec) -> Turn {
        let about = [0, 1, 2].map(|k| {
            let lo = spec.points.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
            let hi = spec.points.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
            if lo.is_finite() { ((lo + hi) / 2.0).round() as i32 } else { 0 }
        });
        Turn { orientation: spec.turned % 24, about }
    }

    pub fn is_none(&self) -> bool {
        self.orientation == UPRIGHT
    }

    fn parts(&self) -> ([usize; 3], [f64; 3], [f64; 3]) {
        let o = crate::shapes::ORIENT[self.orientation as usize];
        (o.perm.map(|p| p as usize), [0, 1, 2].map(|i| if o.neg >> i & 1 == 1 { -1.0 } else { 1.0 }), self.about.map(|v| v as f64))
    }

    /// Where a point of the form as fitted is once the form is turned.
    pub fn point(&self, p: DVec3) -> DVec3 {
        let (perm, sign, about) = self.parts();
        DVec3::from_array([0, 1, 2].map(|i| about[i] + sign[i] * (p[perm[i]] - about[perm[i]])))
    }

    /// The reverse: where a point of the turned form was as fitted.
    pub fn back(&self, p: DVec3) -> DVec3 {
        let (perm, sign, about) = self.parts();
        let mut q = DVec3::ZERO;
        for i in 0..3 {
            q[perm[i]] = about[perm[i]] + sign[i] * (p[i] - about[i]);
        }
        q
    }

    /// [`Turn::back`] for a direction.
    pub fn back_way(&self, d: DVec3) -> DVec3 {
        let (perm, sign, _) = self.parts();
        let mut q = DVec3::ZERO;
        for i in 0..3 {
            q[perm[i]] = sign[i] * d[i];
        }
        q
    }

    fn brick(&self, b: &CellBrick) -> CellBrick {
        let o = crate::shapes::ORIENT[self.orientation as usize];
        let (mut min, mut ext) = ([0; 3], [0; 3]);
        for i in 0..3 {
            let j = o.perm[i] as usize;
            ext[i] = b.ext[j];
            min[i] = if o.neg >> i & 1 == 1 { self.about[i] - (b.min[j] + b.ext[j] - self.about[j]) } else { self.about[i] + (b.min[j] - self.about[j]) };
        }
        CellBrick { mask: crate::shapes::transform_mask(b.mask, self.orientation as usize), min, ext }
    }
}

/// [`build`], with the choice of leaving the fit unmeasured.
pub fn build_as(spec: &Spec, measured: bool) -> Result<Built, String> {
    let mut built = build_upright(spec, measured)?;
    let turn = Turn::of(&spec.snapped());
    if !turn.is_none() {
        built.bricks = built.bricks.iter().map(|b| turn.brick(b)).collect();
        built.path = built.path.iter().map(|p| turn.point(*p)).collect();
    }
    Ok(built)
}

/// [`build_as`], before the form is turned.
///
/// Forms laid out whole (spheres, pipes, pyramids) are measured against the
/// form after they are built, and for those the measuring takes several
/// times as long as the building: four fifths of a large sphere's time.
/// While a form is being dragged only its bricks are wanted, many times a
/// second, so the editor builds unmeasured then and measures once when the
/// mouse is let go. An unmeasured build has `unmeasured` set and its `solid`
/// and `error` at 0.
fn build_upright(spec: &Spec, measured: bool) -> Result<Built, String> {
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
    // A pattern of tiles is laid at single microbricks whatever the budget.
    // A tile is one brick, so a pattern's detail is its brick count: a rug
    // 240 by 160 is some twenty thousand bricks, and kept within a budget
    // of 2,500 it was built three times coarser, with every flower a blot.
    // A larger brick can still be asked for by name (the case above). The
    // budget is there to keep a solid form from being more bricks than it
    // needs, and a pattern needs every one of its.
    if spec.mosaic.is_some() {
        return build_at(spec, 1, measured);
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
            // a curve pulled out of shape is fitted at the finest size: the
            // bump is the detail wanted, and a band raised to follow its
            // curve is built from the bricks of the finest flat fit
            _ if !self.pulls.is_empty() => 1.0,
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
    coarse.pulls = spec.pulls.iter().map(|(at, by)| (*at / g, *by / g)).collect();
    coarse.pull_reach /= g;
    coarse.stones = spec.stones.map(|s| crate::pattern::Stones { size: s.size / g, gap: s.gap / g, ..s });
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
    // A turned form is laid out two ways and the closer kept, which means
    // measuring both. Unmeasured, it is laid out one way only: what is on
    // screen during a drag may then change a little when the mouse is let
    // go, for a drag several times as quick.
    let quick = match &solid {
        Solid::Lathe { .. } if !measured => [false, true].into_iter().find_map(|short_sides| turned(&coarse, &solid, g, short_sides)),
        _ => None,
    };
    let mut built = match quick {
        Some(bricks) => Built { bricks, solid: 0.0, error: 0.0, path, grain: 1, unmeasured: true, shades: Vec::new() },
        None => fit(&coarse, &solid, path, g),
    };
    // (a mirrored form is its own mirror image, so its bricks are held to it too)
    // (not a build laid in a pattern's colours, which name its bricks one by one)
    if built.shades.is_empty() && (coarse.symmetric || coarse.mirror.contains(&true)) && symmetrise(&mut built.bricks, &|p| solid.distance(p)) {
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
    /// see `Spec::turned`
    turn: Turn,
}

impl Probe {
    /// The form as it was fitted at brick size `grain` (a `Built`'s `grain`).
    pub fn new(spec: &Spec, grain: i32) -> Result<Probe, String> {
        let coarse = coarse(spec, grain);
        let (solid, _) = solid(&coarse)?;
        Ok(Probe { solid, grain: grain.max(1) as f64, mirror: mirrors(&coarse), turn: Turn::of(&spec.snapped()) })
    }

    /// Signed distance from the form's surface at `p`, in cells: negative inside.
    pub fn distance(&self, p: DVec3) -> f64 {
        // asked about the form as it faces; answered about the form as fitted
        let p = if self.turn.is_none() { p } else { self.turn.back(p) };
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
        Probe { solid, grain: 1.0, mirror: [None; 3], turn: Turn { orientation: UPRIGHT, about: [0; 3] } }
    }
}

/// The form of an already snapped spec, and its curve for drawing.
///
/// A sphere or a pyramid with points added to it (`Spec::pulls`), or
/// mirrored after that, is no longer a sphere or a pyramid where it was
/// pulled. It keeps its own layout wherever the pulls do not reach, and
/// only what they change is fitted cell by cell (see [`patched`]).
fn solid(spec: &Spec) -> Result<(Solid, Vec<DVec3>), String> {
    let (plain, path) = solid_as_made(spec)?;
    if spec.pulls.is_empty() || !matches!(spec.form, Form::Sphere | Form::Pyramid) {
        return Ok((plain, path));
    }
    // the solid as it would be laid out with nothing added to it
    let laid = fit(spec, &plain, Vec::new(), 1.0).bricks;
    let (Solid::Round { field, min, max, .. } | Solid::Pyramid { field, min, max, .. } | Solid::Volume { field, min, max }) = plain else {
        return Ok((plain, path));
    };
    let before = field.clone();
    let reach = spec.pull_reach.max(0.5);
    let furthest = spec.pulls.iter().map(|(_, by)| by.length()).fold(0.0, f64::max);
    // each pull stretches space by up to its length over half its reach, a quarter turn of the cosine
    let stretch: f64 = spec.pulls.iter().map(|(_, by)| by.length() * PI / (2.0 * reach)).sum();
    let mut field = Field::Warp { inner: Box::new(field), pulls: folded_pulls(spec), reach, scale: 1.0 / (1.0 + stretch) };
    for axis in (0..3).filter(|a| spec.mirror[*a]) {
        if let Some(plane) = mirror_plane(spec, axis) {
            field = Field::Mirror { inner: Box::new(field), axis, plane };
        }
    }
    let (lo, hi) = (min.map(|v| v as f64), max.map(|v| v as f64));
    patched(spec, laid, &before, 1.0 / (1.0 + stretch), field, DVec3::from_array(lo) - furthest, DVec3::from_array(hi) + furthest, path)
}

/// A form laid out whole, then changed in places: its own bricks kept
/// wherever the change does not reach, and the changed part fitted cell by
/// cell in their place.
///
/// `laid` are the bricks of the form as it was, `before` its distance
/// function then and `after` its distance function now, which is `scale`
/// times `before` wherever nothing has changed.
///
/// One point added to a ring or a sphere used to hand the whole of it to
/// the cell-by-cell fitter, which is rougher than a layout everywhere and
/// not only where the point was added (report 30: a ring 70 across, a fifth
/// of its faces steps, for one bump on its side).
///
/// A cell is unchanged if the form's distance is as it was at the cell's
/// middle and one cell to each side of it. A brick is kept if all its cells
/// are unchanged; a cube is kept cell by cell. The fitter then fills the
/// changed cells and no others, so no cell is filled twice and none is left
/// out. Along the seam the two fits meet as two fits of the same solid do:
/// nearly, with here and there a sliver's difference.
#[allow(clippy::too_many_arguments)]
fn patched(spec: &Spec, laid: Vec<CellBrick>, before: &Field, scale: f64, after: Field, lo: DVec3, hi: DVec3, path: Vec<DVec3>) -> Result<(Solid, Vec<DVec3>), String> {
    let (solid, path) = volume(after.clone(), lo, hi, path)?;
    let Solid::Volume { min, max, .. } = solid else { unreachable!() };
    if laid.is_empty() || !spec.wedges {
        return Ok((Solid::Volume { field: after, min, max }, path));
    }
    let as_it_was = |p: DVec3| (before.eval(p) * scale - after.eval(p)).abs() < 1e-6;
    let unchanged = |c: [i32; 3]| {
        let middle = DVec3::new(c[0] as f64 + 0.5, c[1] as f64 + 0.5, c[2] as f64 + 0.5);
        as_it_was(middle) && [DVec3::X, DVec3::Y, DVec3::Z].iter().all(|step| as_it_was(middle + *step) && as_it_was(middle - *step))
    };
    let cells = |b: &CellBrick| {
        let (m, e) = (b.min, b.ext);
        (0..e[2]).flat_map(move |z| (0..e[1]).flat_map(move |y| (0..e[0]).map(move |x| [m[0] + x, m[1] + y, m[2] + z])))
    };
    let mut bricks: Vec<CellBrick> = Vec::new();
    // single cubes to be joined up again: those left of a cube cut by the
    // change, and those the fitter adds
    let mut loose: Vec<[i32; 3]> = Vec::new();
    // the box of everything that changed
    let (mut from, mut to) = ([i32::MAX; 3], [i32::MIN; 3]);
    let mut changed_at = |c: [i32; 3]| {
        for k in 0..3 {
            (from[k], to[k]) = (from[k].min(c[k]), to[k].max(c[k] + 1));
        }
    };
    for b in &laid {
        let same: Vec<bool> = cells(b).map(unchanged).collect();
        if same.iter().all(|s| *s) {
            bricks.push(*b);
        } else if b.mask == CUBE {
            for (cell, same) in cells(b).zip(same) {
                if same { loose.push(cell) } else { changed_at(cell) }
            }
        } else {
            cells(b).for_each(&mut changed_at);
        }
    }
    // and wherever a point was pulled to, which may be clear of every brick there was
    let margin = spec.pull_reach + 2.0;
    let mut places: Vec<DVec3> = folded_pulls(spec).iter().flat_map(|(at, by)| [*at, *at + *by]).collect();
    // and their mirror images, where the form is mirrored
    for axis in (0..3).filter(|a| spec.mirror[*a]) {
        if let Some(plane) = mirror_plane(spec, axis) {
            let images: Vec<DVec3> = places
                .iter()
                .map(|p| {
                    let mut q = *p;
                    q[axis] = 2.0 * plane - p[axis];
                    q
                })
                .collect();
            places.extend(images);
        }
    }
    for p in places {
        changed_at((p - margin).floor().to_array().map(|v| v as i32));
        changed_at((p + margin).ceil().to_array().map(|v| v as i32));
    }
    let from = [0, 1, 2].map(|k| from[k].max(min[k]) - 1);
    let to = [0, 1, 2].map(|k| to[k].min(max[k]) + 1);
    if (0..3).all(|k| to[k] > from[k]) {
        // every ramp one cell long, so that each can be taken or left by its own cell
        let opts = fit_volume::Opts { max_ext: spec.max_ext, wedges: true, stretch: false };
        for b in fit_volume(&|p| after.eval(p), from, to, &opts).bricks {
            if b.mask == CUBE {
                loose.extend(cells(&b).filter(|c| !unchanged(*c)));
            } else {
                // a sloped brick longer than a cell is a ramp joined along its length: cell by cell again
                bricks.extend(cells(&b).filter(|c| !unchanged(*c)).map(|c| CellBrick { mask: b.mask, min: c, ext: [1; 3] }).filter(|_| b.ext.iter().filter(|e| **e > 1).count() <= 1));
            }
        }
    }
    if !loose.is_empty() {
        let lo = [0, 1, 2].map(|k| loose.iter().map(|c| c[k]).min().unwrap());
        let n = [0, 1, 2].map(|k| (loose.iter().map(|c| c[k]).max().unwrap() + 1 - lo[k]) as usize);
        if (n[0] * n[1] * n[2]) as i64 <= MAX_CELLS {
            let mut grid = vec![0u8; n[0] * n[1] * n[2]];
            for c in &loose {
                grid[(c[0] - lo[0]) as usize + n[0] * ((c[1] - lo[1]) as usize + n[1] * (c[2] - lo[2]) as usize)] = CUBE;
            }
            bricks.extend(fit_volume::merge(grid, n, spec.max_ext.max(1) as usize).into_iter().map(|b| CellBrick { mask: CUBE, min: [b.min[0] + lo[0], b.min[1] + lo[1], b.min[2] + lo[2]], ext: b.ext }));
        } else {
            bricks.extend(loose.iter().map(|c| CellBrick { mask: CUBE, min: *c, ext: [1; 3] }));
        }
    }
    // laid out whole and measured afterwards, as a pyramid is
    Ok((Solid::Pyramid { bricks: Some(bricks).filter(|b| !b.is_empty()), field: after, min, max }, path))
}

/// [`solid`], before any points added to a sphere or pyramid are applied.
fn solid_as_made(spec: &Spec) -> Result<(Solid, Vec<DVec3>), String> {
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
            // not filled, it is hollow: a tent with walls `thick` through, measured straight up
            let pyramid = Pyramid { base, half, height: spec.rise.unwrap_or(spec.radius), max_ext: spec.max_ext, shell: (!spec.fill).then_some(spec.thick) };
            // measured against the pyramid as the lattice has it; off the
            // half-cell grid it has no such place and is fitted as it stands
            let (field, lo, hi) = match pyramid.snapped() {
                Some(at) => {
                    let floor = DVec3::new(base.x, base.y, at.floor as f64);
                    (Field::Pyramid { base: floor, half: at.half, top: at.top, height: at.height as f64 }, floor - DVec3::new(at.half[0], at.half[1], 0.0), floor + DVec3::new(at.half[0], at.half[1], at.height as f64))
                }
                None => (Field::Pyramid { base, half, top: [0.0; 2], height: pyramid.height.max(1.0) }, base - DVec3::new(half[0], half[1], 0.0), base + DVec3::new(half[0], half[1], pyramid.height.max(1.0))),
            };
            // less the pyramid of air inside it, where it is hollow and big enough to be
            let field = match pyramid.hollow() {
                Some(air) => {
                    // standing on the same floor, so the hollow is open underneath
                    let floor = DVec3::new(base.x, base.y, air.floor as f64);
                    Field::Without(Box::new(field), Box::new(Field::Pyramid { base: floor, half: air.half, top: air.top, height: air.height as f64 }))
                }
                None => field,
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
            // Points added to the profile pull it as they pull any curve,
            // within the half-plane it is drawn in: each by how far out
            // from the axis and how far up it was dragged.
            let pulls: Vec<(DVec3, DVec3)> = spec
                .pulls
                .iter()
                .map(|(at, by)| {
                    let out = at.truncate() - axis;
                    let outwards = if out.length() > 1e-9 { out.normalize() } else { DVec2::X };
                    (DVec3::new(out.length(), at.z, 0.0), DVec3::new(by.truncate().dot(outwards), by.z, 0.0))
                })
                .collect();
            let path: Vec<DVec3> = pulled(&path, false, &pulls, spec.pull_reach).into_iter().map(|q| DVec3::new(q.x.max(0.0), q.y, 0.0)).collect();
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

/// Where a form is mirrored along `axis` (see `Spec::mirror`): the middle
/// of its points, to the nearest half cell, so that a mirror image of a
/// brick is on the lattice too.
fn mirror_plane(spec: &Spec, axis: usize) -> Option<f64> {
    let lo = spec.points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
    let hi = spec.points.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
    lo.is_finite().then(|| snap((lo + hi) / 2.0))
}

/// A form's added points as they act once it is mirrored: one added on the
/// high side of a mirror is carried over to the low side, where it does
/// what it would have done, and the mirror then shows it on both.
///
/// Mirroring keeps the low side of a form and replaces the high side with
/// its image. A point added on the high side used to be replaced with the
/// rest of that side, and so did nothing at all (report 33): whoever adds a
/// point to a mirrored form means it to count, whichever side they added
/// it on.
pub fn folded_pulls(spec: &Spec) -> Vec<(DVec3, DVec3)> {
    let mut pulls = spec.pulls.clone();
    for axis in (0..3).filter(|a| spec.mirror[*a]) {
        let Some(plane) = mirror_plane(spec, axis) else { continue };
        for (at, by) in pulls.iter_mut().filter(|(at, _)| at[axis] > plane) {
            at[axis] = 2.0 * plane - at[axis];
            by[axis] = -by[axis];
        }
    }
    pulls
}

/// A curve made its own mirror image along `axis` about the plane at
/// `plane`: the part of it on the low side is kept, and followed by its
/// mirror image back the other way.
///
/// This is done to the curve and not to the bricks afterwards, as
/// [`symmetrise`] does for a form that is already symmetric: a brick that
/// lies across the middle of a lopsided form has no mirror image that
/// fits, and the two halves of the form would not meet. Mirrored first, the
/// form is fitted as any symmetric form is.
///
/// A curve that crosses the plane more than once (twice, if it is closed)
/// has no one low side to keep, and is returned as it is.
pub fn mirrored(path: &[DVec3], closed: bool, axis: usize, plane: f64) -> Vec<DVec3> {
    let n = path.len();
    if n < 2 {
        return path.to_vec();
    }
    let low = |p: DVec3| p[axis] <= plane + 1e-9;
    let image = |p: DVec3| {
        let mut q = p;
        q[axis] = 2.0 * plane - p[axis];
        q
    };
    // where the piece from `a` to `b` meets the plane
    let meets = |a: DVec3, b: DVec3| {
        let mut at = a + (b - a) * ((plane - a[axis]) / (b[axis] - a[axis]));
        at[axis] = plane;
        at
    };
    let pieces = if closed { n } else { n - 1 };
    // the pieces that go from one side to the other
    let crossings: Vec<usize> = (0..pieces).filter(|&i| low(path[i]) != low(path[(i + 1) % n])).collect();
    // the low side, from where it comes in to where it goes out
    let kept: Vec<DVec3> = match (closed, crossings.as_slice()) {
        (false, &[i]) if low(path[0]) => path[..=i].iter().copied().chain([meets(path[i], path[i + 1])]).collect(),
        // it ends on the low side: kept from the plane to its end, then turned round to start there
        (false, &[i]) => {
            let mut kept: Vec<DVec3> = [meets(path[i], path[i + 1])].into_iter().chain(path[i + 1..].iter().copied()).collect();
            kept.reverse();
            kept
        }
        (true, &[i, j]) => {
            // comes in over one crossing and goes out over the other
            let (into, out) = if low(path[(i + 1) % n]) { (i, j) } else { (j, i) };
            let mut kept = vec![meets(path[into], path[(into + 1) % n])];
            let mut at = (into + 1) % n;
            loop {
                kept.push(path[at]);
                if at == out {
                    break;
                }
                at = (at + 1) % n;
            }
            kept.push(meets(path[out], path[(out + 1) % n]));
            // from the plane, round the low side, back to the plane; the
            // image then comes back, less the two points on the plane
            let back: Vec<DVec3> = kept[1..kept.len() - 1].iter().rev().map(|p| image(*p)).collect();
            kept.extend(back);
            return kept;
        }
        _ => return path.to_vec(),
    };
    // out along the low side to the plane, and its image back from there
    let back: Vec<DVec3> = kept[..kept.len() - 1].iter().rev().map(|p| image(*p)).collect();
    kept.into_iter().chain(back).collect()
}

/// A curve with its pulls applied: each pull moves the point of the curve
/// nearest where it takes hold, and the curve within `reach` of there along
/// its length by less the further off it is, falling away as a cosine does
/// so that the bump joins the rest of the curve without a corner.
///
/// Distance is measured along the curve, not straight across: a pull on one
/// side of a narrow loop then leaves the other side alone. The curve is cut
/// finer first, to pieces of half a cell, since a straight run of it is one
/// long piece that could only move by its ends.
pub fn pulled(path: &[DVec3], closed: bool, pulls: &[(DVec3, DVec3)], reach: f64) -> Vec<DVec3> {
    if pulls.is_empty() || path.len() < 2 {
        return path.to_vec();
    }
    let mut fine = Vec::new();
    let ends = if closed { path.len() } else { path.len() - 1 };
    for i in 0..ends {
        let (a, b) = (path[i], path[(i + 1) % path.len()]);
        let pieces = (a.distance(b) / 0.5).ceil().max(1.0) as usize;
        fine.extend((0..pieces).map(|k| a + (b - a) * (k as f64 / pieces as f64)));
    }
    if !closed {
        fine.push(path[path.len() - 1]);
    }
    // how far along the curve each point is
    let mut along = vec![0.0; fine.len()];
    for i in 1..fine.len() {
        along[i] = along[i - 1] + fine[i].distance(fine[i - 1]);
    }
    let whole = along[fine.len() - 1] + if closed { fine[fine.len() - 1].distance(fine[0]) } else { 0.0 };
    let reach = reach.max(0.5);
    let mut moved = fine.clone();
    for (at, by) in pulls {
        let held = (0..fine.len()).min_by(|a, b| fine[*a].distance_squared(*at).total_cmp(&fine[*b].distance_squared(*at))).unwrap();
        for i in 0..fine.len() {
            let mut d = (along[i] - along[held]).abs();
            if closed {
                d = d.min(whole - d);
            }
            if d < reach {
                moved[i] += *by * (0.5 * (1.0 + (PI * d / reach).cos()));
            }
        }
    }
    moved
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
    // the axis it lies flat across before anything is done to it
    let lay_across = (0..3).rev().find(|&a| path.iter().all(|p| (p[a] - path[0][a]).abs() < 1e-6) && normal.is_none_or(|n| n[a].abs() > 1.0 - 1e-9));
    let lay_at = path[0];
    // Pulled out of its plane, a curve has no plane any more. A flat band
    // is then raised to follow it (below); anything else is built as a
    // curve in space is, a round tube fitted cell by cell.
    let (path, normal) = if spec.pulls.is_empty() {
        (path, normal)
    } else {
        let in_plane = normal.is_some_and(|n| spec.pulls.iter().all(|(_, by)| by.dot(n).abs() < 1e-9));
        (pulled(&path, closed, &folded_pulls(spec), spec.pull_reach), normal.filter(|_| in_plane))
    };
    // then made its own mirror image, along each axis asked for
    let mut path = path;
    for axis in (0..3).filter(|a| spec.mirror[*a]) {
        if let Some(plane) = mirror_plane(spec, axis) {
            path = mirrored(&path, closed, axis, plane);
        }
    }
    if let (Some(axis), false, false) = (lay_across, spec.tube, spec.fill) {
        if path.iter().any(|p| (p[axis] - lay_at[axis]).abs() > 1e-6) {
            if let Some(raised) = raised(spec, &path, closed, axis, lay_at[axis])? {
                return Ok(raised);
            }
        }
    }
    // A round tube pulled out of its plane keeps its pipe wherever the pull
    // does not reach, and is built in columns where it does (see
    // `lifted_tube`). Not one made oval, whose pipe is not the plain tube
    // the pulled curve is measured as.
    if let (Some(axis), true) = (lay_across, spec.tube) {
        let half = spec.thick / 2.0;
        let round = (spec.stretch[axis] - 1.0).abs() < 1e-9 && !spec.fill;
        if round && path.iter().any(|p| (p[axis] - lay_at[axis]).abs() > 1e-6) {
            let level: Vec<DVec3> = path
                .iter()
                .map(|p| {
                    let mut q = *p;
                    q[axis] = lay_at[axis];
                    q
                })
                .collect();
            let (flat, _) = stacked(spec, level.clone(), closed, axis)?;
            let laid = fit(spec, &flat, Vec::new(), 1.0).bricks;
            // The pipe may have been laid half a cell aside of its curve, to
            // land on the lattice (see `stacked`): by as much as its middle
            // is off the level curve's, across the plane. What it is
            // measured against is the curve moved the same way.
            let (ua, va) = plane_axes(axis);
            let aside = |k: usize| {
                let bricks = (laid.iter().map(|b| b.min[k]).min().unwrap_or(0) + laid.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap_or(0)) as f64 / 2.0;
                let curve = (level.iter().map(|p| p[k]).fold(f64::MAX, f64::min) + level.iter().map(|p| p[k]).fold(f64::MIN, f64::max)) / 2.0;
                if (bricks - curve + 0.5).abs() < 0.26 { -0.5 } else { 0.0 }
            };
            let mut shift = DVec3::ZERO;
            (shift[ua], shift[va]) = (aside(ua), aside(va));
            let path: Vec<DVec3> = path.iter().map(|p| *p + shift).collect();
            let after = Field::Tube(Stroke::new(&path, half, closed, Some(0.5)));
            let lo = path.iter().fold(DVec3::MAX, |a, p| a.min(*p)).min(level.iter().fold(DVec3::MAX, |a, p| a.min(*p))) - half - 1.0;
            let hi = path.iter().fold(DVec3::MIN, |a, p| a.max(*p)).max(level.iter().fold(DVec3::MIN, |a, p| a.max(*p))) + half + 1.0;
            let bricks = lifted_tube(spec, &laid, &path, axis, lay_at[axis], half);
            let (solid, path) = volume(after.clone(), lo, hi, path)?;
            let Solid::Volume { min, max, .. } = solid else { unreachable!() };
            // laid out whole and measured afterwards, as a pyramid is
            return Ok((Solid::Pyramid { bricks: Some(bricks).filter(|b| !b.is_empty() && spec.wedges), field: after, min, max }, path));
        }
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

/// A round tube whose curve has been pulled out of the plane it lay in, at
/// `plane` along `axis` (`path` being the curve where the pipe was laid): the
/// bricks of the flat pipe `laid` wherever the
/// curve is still in the plane, and columns between the tube's under side
/// and its top wherever it has left it (see `tube_columns`).
///
/// The ground is shared out a cell at a time: a cell under a lifted part of
/// the curve is rebuilt from top to bottom, and so is every cell under a
/// sloped brick of the pipe that reaches into one, since such a brick
/// cannot be cut. The rest of the pipe stands as it was.
///
/// The columns stand along `axis` wherever the curve climbs gently. Where it
/// climbs steeply they lie on their sides (see `steep_stretches`).
///
/// It was first fitted cell by cell where it was lifted, from the tube's
/// distance function, on the argument that bricks cannot make a round tube
/// at a slant. They can, near enough: the tube's top and its under side are
/// each a surface over the ground, and a surface is sloped bricks.
fn lifted_tube(spec: &Spec, laid: &[CellBrick], path: &[DVec3], axis: usize, plane: f64, radius: f64) -> Vec<CellBrick> {
    if laid.is_empty() {
        return Vec::new();
    }
    // the flat pipe's section: as tall as it was laid, its chamfer the
    // regular octagon's (see `chamfer_of` where the pipe is laid out)
    let count = (2.0 * radius).round() as i32;
    let chamfer = ((count as f64 * (1.0 - std::f64::consts::FRAC_1_SQRT_2)).round() as i32).min((count - 1) / 2) as f64;
    let half_tall = (laid.iter().map(|b| b.min[axis] + b.ext[axis]).max().unwrap() - laid.iter().map(|b| b.min[axis]).min().unwrap()) as f64 / 2.0;
    lifted(spec, laid, path, axis, plane, &Section::Pipe { radius, chamfer, half_tall })
}

/// A flat band whose curve has been pulled out of the plane it lay in, built
/// as a lifted tube is (see `lifted_tube`): the bricks `laid` of the flat
/// band wherever the curve is still in the plane, and columns between the
/// band's under side and its top wherever it has left it. `run_on` is how
/// far the flat band runs on past each end of an open curve.
///
/// It was first built from the flat band's own bricks, each moved to the
/// height of the curve where it stood, with a sloped brick on top and the
/// one that answers it underneath (`fit_ribbon::fit_ribbon`, since taken
/// out). That kept the outline the flat fitter had found, and failed in
/// two ways. A long three-cornered brick of the outline cannot be bent, so
/// where the curve climbed along one it was moved whole and stood out of
/// the band as a slab. And where the curve climbed more than a cell per
/// cell, a cell's corners were further apart than the band is deep and the
/// cell became a plain block: a staircase. Of a ring two cells deep with a
/// point pulled seven and a half cells up, a quarter of the outside was
/// stepped faces. Columns give up the fitted outline where the band is
/// lifted, for whole cells with their notches filled, and gain a top and an
/// under side that meet edge to edge and can be laid on their sides.
fn lifted_band(spec: &Spec, laid: &[CellBrick], path: &[DVec3], axis: usize, plane: f64, run_on: f64) -> Vec<CellBrick> {
    if laid.is_empty() {
        return Vec::new();
    }
    let half_deep = (laid.iter().map(|b| b.min[axis] + b.ext[axis]).max().unwrap() - laid.iter().map(|b| b.min[axis]).min().unwrap()) as f64 / 2.0;
    lifted(spec, laid, path, axis, plane, &Section::Band { across: axis, half_thick: spec.thick / 2.0, half_deep, run_on })
}

/// What `lifted_tube` and `lifted_band` share: the flat form's bricks kept
/// where the curve is in its plane, columns of `section` where it is not.
fn lifted(spec: &Spec, laid: &[CellBrick], path: &[DVec3], axis: usize, plane: f64, section: &Section) -> Vec<CellBrick> {
    use std::collections::HashSet;
    let (ua, va) = plane_axes(axis);
    let radius = section.reach();
    // the middle of the flat pipe along the axis, which its curve runs through
    let centre = (laid.iter().map(|b| b.min[axis]).min().unwrap() + laid.iter().map(|b| b.min[axis] + b.ext[axis]).max().unwrap()) as f64 / 2.0;
    // the curve where the bricks are: the pipe's middle may be half a cell off the plane
    let curve: Vec<DVec3> = path
        .iter()
        .map(|p| {
            let mut q = *p;
            q[axis] += centre - plane;
            q
        })
        .collect();
    let closed = path.len() > 2 && path[0].distance(path[path.len() - 1]) < 1e-6;
    let ground = |b: &CellBrick| {
        let (u, v, wu, wv) = (b.min[ua], b.min[va], b.ext[ua], b.ext[va]);
        (0..wv).flat_map(move |j| (0..wu).map(move |i| (u + i, v + j)))
    };
    // the ground under the lifted part, a cell beyond the tube's own width each way
    let reach = radius.ceil() as i32 + 2;
    let mut lifted: HashSet<(i32, i32)> = HashSet::new();
    // (and under every stretch built lying down, as far as its seams reach:
    // none of the pipe may be left standing where those columns go)
    let parts = stretches(&curve, axis, radius);
    let mut length = vec![0.0];
    for pair in curve.windows(2) {
        length.push(length.last().unwrap() + pair[0].distance(pair[1]));
    }
    let lying_at = |i: usize| parts.iter().any(|s| s.axis != axis && length[i] >= s.along.0 && length[i] <= s.along.1);
    for (i, pair) in path.windows(2).enumerate().filter(|(i, p)| (p[0][axis] - plane).abs() > 1e-6 || (p[1][axis] - plane).abs() > 1e-6 || lying_at(*i) || lying_at(*i + 1)) {
        let _ = i;
        for p in pair {
            let (u, v) = (p[ua].floor() as i32, p[va].floor() as i32);
            for dv in -reach..=reach {
                for du in -reach..=reach {
                    lifted.insert((u + du, v + dv));
                }
            }
        }
    }
    // and under every sloped brick of the pipe that reaches into it
    loop {
        let before = lifted.len();
        for b in laid.iter().filter(|b| b.mask != CUBE) {
            if ground(b).any(|c| lifted.contains(&c)) {
                lifted.extend(ground(b));
            }
        }
        if lifted.len() == before {
            break;
        }
    }
    let mut bricks: Vec<CellBrick> = Vec::new();
    let mut whole: Vec<[i32; 3]> = Vec::new();
    for b in laid {
        if b.mask != CUBE {
            if !ground(b).any(|c| lifted.contains(&c)) {
                bricks.push(*b);
            }
            continue;
        }
        if !ground(b).any(|c| lifted.contains(&c)) {
            bricks.push(*b);
            continue;
        }
        // a cube cut by the lifted ground: what is left of it, cell by cell
        for cell in ground(b).filter(|c| !lifted.contains(c)) {
            for w in 0..b.ext[axis] {
                let mut at = [0; 3];
                (at[ua], at[va], at[axis]) = (cell.0, cell.1, b.min[axis] + w);
                whole.push(at);
            }
        }
    }
    let mut lifted: Vec<(i32, i32)> = lifted.into_iter().collect();
    lifted.sort();
    // upright columns over the lifted ground, and columns lying along each
    // level axis over the ground they lie across, each for its own stretches
    for lying in 0..3 {
        if !parts.iter().any(|s| s.axis == lying) {
            continue;
        }
        let (ub, vb) = plane_axes(lying);
        let within = |cell: (i32, i32), at: f64| parts.iter().any(|s| s.axis == lying && s.holds(&|c| if c == ub { cell.0 } else { cell.1 }, at));
        let cells = if lying == axis {
            lifted.clone()
        } else {
            let mut cells: Vec<(i32, i32)> = Vec::new();
            // (every other point of the curve: its points are closer together than the reach)
            for (_, p) in curve.iter().enumerate().filter(|(i, _)| i % 2 == 0 && parts.iter().any(|s| s.axis == lying && length[*i] >= s.along.0 && length[*i] <= s.along.1)) {
                let (u, v) = (p[ub].floor() as i32, p[vb].floor() as i32);
                cells.extend((-reach..=reach).flat_map(|dv| (-reach..=reach).map(move |du| (u + du, v + dv))));
            }
            cells.sort();
            cells.dedup();
            cells
        };
        // a band standing upright has the flat band's own outline (see `tube_columns`)
        let plan: &[CellBrick] = if lying == axis && matches!(section, Section::Band { .. }) { laid } else { &[] };
        let (pieces, blocks) = tube_columns(&curve, closed, lying, section, &cells, plan, &within, spec.max_ext);
        bricks.extend(crate::fit_ribbon::in_rows(pieces, spec.max_ext));
        whole.extend(blocks);
    }
    // No room in two bricks. The stretches are each built to keep to their
    // own ground, but where columns lying one way meet columns standing, in
    // a tight turn of the curve, the two do not agree to the cell on where
    // the tube's surface is, and a few bricks of one were found inside the
    // other. Each brick is taken in turn, the pipe's own first, and one
    // that shares room with a brick already taken is left out. Two bricks may share a
    // cell and no room (the two halves of a cell folded along its diagonal,
    // one of them often taller than the other), so sharing a cell is not
    // the test: each cell two bricks share is sampled, and they share room
    // if a sample is inside both. What is lost is a brick's worth of the
    // surface in a place that was wrong either way; what the game would do
    // with two bricks in one place is not known.
    {
        let mut at: std::collections::HashMap<[i32; 3], Vec<usize>> = std::collections::HashMap::new();
        let cells = |b: &CellBrick| {
            let (min, ext) = (b.min, b.ext);
            (0..ext[0] * ext[1] * ext[2]).map(move |k| [min[0] + k % ext[0], min[1] + k / ext[0] % ext[1], min[2] + k / (ext[0] * ext[1])])
        };
        // whether a point is inside a brick
        let holds = |b: &CellBrick, p: [f64; 3]| {
            let u = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
            crate::shapes::shape_info(b.mask).is_some_and(|info| crate::shapes::inside_shape(info, u[0], u[1], u[2]))
        };
        // The whole cells are taken first. A sloped brick found inside one
        // is the one that is out of place: taking the whole cell out for it
        // instead opened a pocket in the tube's side on some forms.
        let solid: std::collections::HashSet<[i32; 3]> = whole.iter().copied().collect();
        let mut kept: Vec<CellBrick> = Vec::with_capacity(bricks.len());
        for b in bricks.drain(..) {
            if b.mask != CUBE && cells(&b).any(|c| solid.contains(&c) && (0..27).any(|k| holds(&b, [c[0] as f64 + (k % 3) as f64 / 3.0 + 0.17, c[1] as f64 + (k / 3 % 3) as f64 / 3.0 + 0.15, c[2] as f64 + (k / 9) as f64 / 3.0 + 0.19]))) {
                continue;
            }
            let clashes = cells(&b).any(|c| {
                at.get(&c).is_some_and(|others| {
                    // 27 points of the cell, clear of its faces
                    (0..27).any(|k| {
                        let p = [c[0] as f64 + (k % 3) as f64 / 3.0 + 0.17, c[1] as f64 + (k / 3 % 3) as f64 / 3.0 + 0.15, c[2] as f64 + (k / 9) as f64 / 3.0 + 0.19];
                        holds(&b, p) && others.iter().any(|o| holds(&kept[*o], p))
                    })
                })
            });
            if clashes {
                continue;
            }
            for c in cells(&b) {
                at.entry(c).or_default().push(kept.len());
            }
            kept.push(b);
        }
        let kept_is_cube: Vec<bool> = kept.iter().map(|b| b.mask == CUBE).collect();
        bricks = kept;
        // (a whole cell the pipe's own cube already fills is said once)
        whole.retain(|c| !at.get(c).is_some_and(|there| there.iter().any(|o| kept_is_cube[*o])));
    }
    if !whole.is_empty() {
        whole.sort();
        whole.dedup();
        let lo = [0, 1, 2].map(|k| whole.iter().map(|c| c[k]).min().unwrap());
        let n = [0, 1, 2].map(|k| (whole.iter().map(|c| c[k]).max().unwrap() + 1 - lo[k]) as usize);
        if (n[0] * n[1] * n[2]) as i64 <= MAX_CELLS {
            let mut grid = vec![0u8; n[0] * n[1] * n[2]];
            for c in &whole {
                grid[(c[0] - lo[0]) as usize + n[0] * ((c[1] - lo[1]) as usize + n[1] * (c[2] - lo[2]) as usize)] = CUBE;
            }
            bricks.extend(fit_volume::merge(grid, n, spec.max_ext.max(1) as usize).into_iter().map(|b| CellBrick { mask: CUBE, min: [b.min[0] + lo[0], b.min[1] + lo[1], b.min[2] + lo[2]], ext: b.ext }));
        } else {
            bricks.extend(whole.iter().map(|c| CellBrick { mask: CUBE, min: *c, ext: [1; 3] }));
        }
    }
    bricks
}

/// What a lifted form is across its curve.
enum Section {
    /// The eight-sided section of a pipe: half its width, how far its
    /// chamfer reaches in from each side, and half its height.
    Pipe { radius: f64, chamfer: f64, half_tall: f64 },
    /// A band's rectangle, standing square to the curve: `half_thick` to
    /// each side in the plane across `across` that the band lay in, and
    /// `half_deep` to each side the other way (see `sdf::Stroke::rail`).
    /// Past each end of an open curve it runs `run_on` further.
    Band { across: usize, half_thick: f64, half_deep: f64, run_on: f64 },
}

impl Section {
    /// How far from its curve the section reaches, at most.
    fn reach(&self) -> f64 {
        match *self {
            Section::Pipe { radius, .. } => radius,
            Section::Band { half_thick, half_deep, .. } => half_thick.hypot(half_deep),
        }
    }
}

/// A band's columns along the axis `lying`, over the point (u, v) of the
/// plane across it, from the part of the curve nearest that point as seen
/// along the axis: `at` on the curve, running `way`. Returns how far outside
/// the band's walls the point is (negative inside them), the middle of the
/// band along the axis there, half its length along the axis, and half its
/// length square to its faces.
///
/// A rectangle has two pairs of faces. The pair that faces more nearly
/// along the columns is their top and their under side; the other pair is
/// the walls the columns stop at. Standing upright the band's faces in its
/// plane are always walls, exactly upright, and its top and under side are
/// the two faces across the axis. Lying down it depends on which way the
/// curve runs: across the columns, the band's two faces in its plane are
/// top and under side and the other two are walls.
///
/// The walls are taken to run straight along the columns, which they do
/// only when the curve runs square to them. Otherwise the rectangle is
/// built as the slanted four-sided figure with the same top, under side
/// and width through its middle. The alternative, taking each column from
/// where it enters the true rectangle to where it leaves, ends the top and
/// the under side in an edge where they meet, and a solid thinner than it
/// climbs across a cell is a plain block (see `fit_ribbon::Ground::lay`).
fn band_over(across: usize, half_thick: f64, half_deep: f64, lying: usize, at: DVec3, way: DVec3, u: f64, v: f64) -> (f64, f64, f64, f64) {
    let (ub, vb) = plane_axes(lying);
    let (side, upward) = crate::sdf::rail_frame(way, across);
    let (roof, half_roof, wall, half_wall) = if upward[lying].abs() >= side[lying].abs() { (upward, half_deep, side, half_thick) } else { (side, half_thick, upward, half_deep) };
    let mut off = DVec3::ZERO;
    (off[ub], off[vb]) = (u, v);
    let off = off - at;
    // (a pair of faces is never taken as top and under side when it faces less than 45 degrees off square to the columns, unless the curve runs along them)
    let facing = roof[lying].abs().max(1e-3) * roof[lying].signum();
    let middle = -off.dot(roof) / facing;
    (((off.dot(wall) + middle * wall[lying]).abs() - half_wall), middle, half_roof / facing.abs(), half_roof)
}

/// Where two stretches of a tube built along different axes meet: a lattice
/// plane at `at` across the axis `across`, which the curve goes through
/// `length` along its own length. Within `reach` of that along the curve, a
/// stretch keeps to its own side of the plane.
#[derive(Clone, Copy)]
struct Seam {
    across: usize,
    at: i32,
    length: f64,
    reach: f64,
    /// whether the stretch this belongs to is on the low side of the plane
    below: bool,
}

/// A stretch of a lifted tube built in columns along `axis`: the part of
/// the curve from `along.0` to `along.1` of its length, cut off at each end
/// where the next stretch takes over.
struct Stretch {
    axis: usize,
    along: (f64, f64),
    ends: Vec<Seam>,
}

impl Stretch {
    /// Whether a column belongs to this stretch, given the cell it stands
    /// over (as its place along each axis of the ground) and how far along
    /// the curve's length the pass it is built from is.
    fn holds(&self, cell: &dyn Fn(usize) -> i32, length: f64) -> bool {
        length >= self.along.0 && length <= self.along.1 && self.ends.iter().all(|s| (length - s.length).abs() > s.reach || (cell(s.across) < s.at) == s.below)
    }
}

/// How much less of the curve's way must go along another axis than along
/// the one its columns lie along now before the columns are turned: a
/// change of axis is a seam, and is not worth making for a small gain.
const LEAN: f64 = 0.3;

/// How steeply the curve must climb along its columns, in cells up per
/// cell along, before a stretch is worth laying on its side: a little
/// under the one in one past which upright columns cannot follow it.
const STEEP: f64 = 0.85;

/// The curve of a lifted tube in stretches, each built in columns along the
/// axis the curve is most nearly square to there.
///
/// A tube built in columns is a top and an under side over the ground the
/// columns stand on. Where its curve climbs along the columns more than a
/// cell per cell, those two surfaces are steeper than any brick one cell
/// square can follow smoothly: on upright columns alone, a tube climbing
/// four cells in three came out as a comb of tall fins. But a curve cannot
/// run steeply along all three axes at once. Along the axis it runs least
/// along it climbs at most seven cells in ten, so the columns are laid
/// along that axis, and a tube rising in a plane of the grid is built from
/// the side, as the level tube it is seen from there.
///
/// Columns along two axes share the third. Where one stretch gives way to
/// the next, both are cut off square at a lattice plane across that third
/// axis, which the curve is then running mostly along, and each keeps to
/// its own side of it. That needs the tube to cross the plane once: a
/// seam the curve does not get clear of, or a stretch too short to get
/// clear of both of its seams, is given up and the two stretches built as
/// one.
fn stretches(curve: &[DVec3], axis: usize, radius: f64) -> Vec<Stretch> {
    let n = curve.len();
    let everything = |axis: usize| vec![Stretch { axis, along: (f64::NEG_INFINITY, f64::INFINITY), ends: Vec::new() }];
    if n < 3 {
        return everything(axis);
    }
    let mut length = vec![0.0];
    for pair in curve.windows(2) {
        length.push(length.last().unwrap() + pair[0].distance(pair[1]));
    }
    // which way the curve runs at each point, over a few cells so that a kink between two points does not count
    let ways: Vec<DVec3> = (0..n)
        .map(|i| {
            let (mut a, mut b) = (i, i);
            while a > 0 && length[i] - length[a] < 2.0 {
                a -= 1;
            }
            while b + 1 < n && length[b] - length[i] < 2.0 {
                b += 1;
            }
            (curve[b] - curve[a]).normalize_or_zero()
        })
        .collect();
    let mut lying = vec![axis; n];
    let mut now = axis;
    for i in 0..n {
        let run = ways[i].abs();
        let least = (0..3).min_by(|a, b| run[*a].total_cmp(&run[*b])).unwrap();
        if run[least] < run[now] - LEAN {
            now = least;
        }
        lying[i] = now;
    }
    // clear of a seam's plane: further from it than the tube reaches, climbing as steeply as it may
    let clear = 1.3 * radius + 2.0;
    for _ in 0..n {
        // runs of one axis, as their first and last points
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for i in 0..n {
            match runs.last_mut() {
                Some(run) if lying[run.0] == lying[i] => run.1 = i,
                _ => runs.push((i, i)),
            }
        }
        // A stretch lying down costs a seam at each end, where the two ways
        // of building do not meet to the cell. It is worth that only where
        // upright columns would do badly: where the curve climbs steeply.
        if let Some(run) = runs.iter().find(|run| lying[run.0] != axis && (run.0..=run.1).map(|i| ways[i][axis].abs()).fold(0.0, f64::max) < STEEP) {
            for i in run.0..=run.1 {
                lying[i] = axis;
            }
            continue;
        }
        let mut seams: Vec<Seam> = Vec::new();
        let mut given_up = None;
        for k in 1..runs.len() {
            let (before, after) = (runs[k - 1], runs[k]);
            let (a, b) = (lying[before.0], lying[after.0]);
            let across = 3 - a - b;
            let at = curve[after.0][across].round() as i32;
            let sign = ways[after.0][across].signum();
            // how far the curve is past the plane, the way it is going
            let past = |i: usize| sign * (curve[i][across] - at as f64);
            // the nearest points either side from which the rest of each run is clear of the plane
            // The nearest point either side from which the curve stays clear
            // of the plane for a tube's width more. Columns along two axes
            // do not agree to the cell on how far along the curve a part of
            // the tube is, so where a stretch's hold on its side of the
            // plane ends, the tube has to be well on that side already.
            let window = 2.0 * radius + 2.0;
            let stays = |i: usize, back: bool| (0..n).filter(|j| if back { *j <= i && length[i] - length[*j] <= window } else { *j >= i && length[*j] - length[i] <= window }).all(|j| if back { past(j) <= -clear } else { past(j) >= clear });
            let back = (before.0..=before.1).rev().find(|i| stays(*i, true));
            let on = (after.0..=after.1).find(|i| stays(*i, false));
            match (back, on) {
                (Some(back), Some(on)) if ways[after.0][across].abs() >= 0.3 => {
                    let reach = (length[after.0] - length[back]).max(length[on] - length[after.0]) + window / 2.0;
                    seams.push(Seam { across, at, length: length[after.0], reach, below: sign > 0.0 });
                }
                _ => {
                    given_up = Some(k);
                    break;
                }
            }
        }
        if let Some(k) = given_up {
            // The stretch lying down is stood up again: upright columns are
            // what the rest of the tube is built in, so they can always take
            // over. Between two stretches lying along different axes, the
            // shorter is.
            let (before, after) = (runs[k - 1], runs[k]);
            let shorter = if length[before.1] - length[before.0] < length[after.1] - length[after.0] { before } else { after };
            let up = if lying[before.0] == axis { after } else if lying[after.0] == axis { before } else { shorter };
            for i in up.0..=up.1 {
                lying[i] = axis;
            }
            continue;
        }
        if std::env::var("ARCS_DEBUG").is_ok() { for (k, run) in runs.iter().enumerate() { eprintln!("run {k}: points {}..{} axis {} length {:.1}..{:.1} steepest {:.2}", run.0, run.1, lying[run.0], length[run.0], length[run.1], (run.0..=run.1).map(|i| ways[i][axis].abs()).fold(0.0, f64::max)); } }
        return runs
            .iter()
            .enumerate()
            .map(|(k, run)| {
                let mut ends = Vec::new();
                let mut along = (f64::NEG_INFINITY, f64::INFINITY);
                if k > 0 {
                    let seam = seams[k - 1];
                    ends.push(Seam { below: !seam.below, ..seam });
                    along.0 = seam.length - seam.reach;
                }
                if k < seams.len() {
                    let seam = seams[k];
                    ends.push(seam);
                    along.1 = seam.length + seam.reach;
                }
                Stretch { axis: lying[run.0], along, ends }
            })
            .collect();
    }
    everything(axis)
}

/// Columns of a round tube along `curve`, lying along `axis`, over the cells
/// `cells` of the plane across it (see `fit_ribbon::Ground`). `wanted` says
/// whether to build the column over a cell for a pass of the curve, given
/// how far along the curve's length that pass is.
///
/// The tube has the flat pipe's eight-sided section, moved straight along
/// the axis with the curve: a level top, a chamfer at 45 degrees each side,
/// and an upright wall. It was first given a round section. A round section's
/// top falls away ever more steeply towards its sides, and a surface
/// steeper than one cell up per cell across cannot be made of bricks one
/// cell square: the sides came out as tall slivers standing proud of one
/// another. On a tube 11 across, pulled up 25, a fifteenth of the outside
/// was stepped faces that way.
///
/// `plan` is the bricks of a flat band whose ground this is, if the columns
/// stand along the axis the band lay flat across (and nothing otherwise).
/// Seen along that axis a lifted band has the outline it had flat, which
/// those bricks fit with long three-cornered bricks as whole cells cannot.
/// So the ground is then theirs: a cell for each cell of a block, and half
/// a box for each three-cornered brick (see `LONG_RISE`).
fn tube_columns(curve: &[DVec3], closed: bool, axis: usize, section: &Section, cells: &[(i32, i32)], plan: &[CellBrick], wanted: &dyn Fn((i32, i32), f64) -> bool, max_ext: i32) -> (Vec<CellBrick>, Vec<[i32; 3]>) {
    use std::collections::HashSet;
    let (ua, va) = plane_axes(axis);
    let radius = section.reach();
    // How far along the curve each of its points is, measured over the
    // ground. A curve pulled about can pass over the same ground twice, at
    // two heights: a loop seen from above. The tube is then two tubes
    // there, one above the other, and each has to be built from its own
    // stretch of the curve. Taken from whichever stretch was nearer, corner
    // by corner, a cell where the two cross got some corners from each and
    // was joined from the height of one to the height of the other: a
    // blade the whole way between them. So a stretch is named by how far
    // along the curve it is, and looked for only that near.
    let mut along_at = vec![0.0];
    let mut length = vec![0.0];
    for pair in curve.windows(2) {
        along_at.push(along_at.last().unwrap() + DVec2::new(pair[1][ua] - pair[0][ua], pair[1][va] - pair[0][va]).length());
        length.push(length.last().unwrap() + pair[0].distance(pair[1]));
    }
    let whole = *along_at.last().unwrap();
    // how far apart along the curve two places on it are, the short way round
    let apart = |a: f64, b: f64| {
        let d = (a - b).abs();
        if closed { d.min(whole - d) } else { d }
    };
    // how far along the curve's own length a place that far along it over the ground is
    let length_at = |about: f64| {
        let i = along_at.partition_point(|a| *a <= about).clamp(1, along_at.len() - 1);
        let span = along_at[i] - along_at[i - 1];
        length[i - 1] + (length[i] - length[i - 1]) * if span > 1e-9 { ((about - along_at[i - 1]) / span).clamp(0.0, 1.0) } else { 0.0 }
    };
    // further apart than this along the curve, two places over one cell are two passes
    let same_pass = 3.0 * radius + 4.0;
    // the part of the curve nearest a point of the ground, within reach of
    // `about` along it if that is given: how far off, how high, how steep,
    // and how far along; and the lowest and highest the curve is anywhere
    // over the tube's width of the point; and where that part of the curve
    // is and which way it runs
    let near_about = |u: f64, v: f64, about: Option<f64>| {
        let at = DVec2::new(u, v);
        let mut best = (f64::INFINITY, 0.0, 0.0, 0.0);
        let mut place = (DVec3::ZERO, DVec3::ZERO);
        let mut heights = (f64::INFINITY, f64::NEG_INFINITY);
        for (i, pair) in curve.windows(2).enumerate() {
            if about.is_some_and(|s| apart(s, (along_at[i] + along_at[i + 1]) / 2.0) > same_pass) {
                continue;
            }
            let (a, b) = (DVec2::new(pair[0][ua], pair[0][va]), DVec2::new(pair[1][ua], pair[1][va]));
            let along = b - a;
            let length = along.length();
            let t = if length > 1e-9 { ((at - a).dot(along) / (length * length)).clamp(0.0, 1.0) } else { 0.0 };
            let off = (a + along * t).distance(at);
            if off < radius + 1.0 {
                for end in pair {
                    heights = (heights.0.min(end[axis]), heights.1.max(end[axis]));
                }
            }
            if off < best.0 {
                let rise = pair[1][axis] - pair[0][axis];
                best = (off, pair[0][axis] + rise * t, if length > 1e-9 { rise / length } else { 0.0 }, along_at[i] + length * t);
                place = (pair[0] + (pair[1] - pair[0]) * t, (pair[1] - pair[0]).normalize_or_zero());
            }
        }
        (best.0, best.1, best.2, best.3, heights, place)
    };
    // each pass of the curve within the tube's wall of a point of the
    // ground, as how far along the curve it comes nearest
    let passes = |u: f64, v: f64| {
        let at = DVec2::new(u, v);
        let mut found: Vec<(f64, f64)> = Vec::new(); // (how far along, how far off), nearest of each pass
        for (i, pair) in curve.windows(2).enumerate() {
            let (a, b) = (DVec2::new(pair[0][ua], pair[0][va]), DVec2::new(pair[1][ua], pair[1][va]));
            let along = b - a;
            let length = along.length();
            let t = if length > 1e-9 { ((at - a).dot(along) / (length * length)).clamp(0.0, 1.0) } else { 0.0 };
            let off = (a + along * t).distance(at);
            if off >= radius {
                continue;
            }
            // A band is not as wide every way as it reaches at most: the
            // point has to be within its walls, and short of its end if
            // this is the end of an open curve.
            if let Section::Band { across, half_thick, half_deep, run_on } = *section {
                let (place, way) = (pair[0] + (pair[1] - pair[0]) * t, (pair[1] - pair[0]).normalize_or_zero());
                let (outside, middle, _, _) = band_over(across, half_thick, half_deep, axis, place, way, u, v);
                let mut from = DVec3::ZERO;
                (from[ua], from[va], from[axis]) = (u, v, middle);
                let past = (from - place).dot(way);
                let ended = !closed && ((i == 0 && t <= 0.0 && -past > run_on) || (i + 2 == curve.len() && t >= 1.0 && past > run_on));
                if outside >= 0.0 || ended {
                    continue;
                }
            }
            let here = along_at[i] + length * t;
            match found.iter_mut().find(|f| apart(f.0, here) <= same_pass) {
                Some(pass) if off < pass.1 => *pass = (here, off),
                Some(_) => {}
                None => found.push((here, off)),
            }
        }
        found.into_iter().map(|f| f.0).collect::<Vec<f64>>()
    };
    // Under side and top. Across the tube the top is level as far as the
    // chamfer, falls one for one across it, and from the wall outwards
    // stays at the wall's top, so that a cell at the wall has all four of
    // its corners at the wall's height and not one of them at nothing. The
    // tube's section stands square to its curve, so
    // where the curve climbs it is that much taller measured straight up.
    // That holds while the curve keeps climbing for the tube's whole width.
    // Where it turns straight up for a short way, the rule alone makes the
    // tube as tall as the climb is steep, a blade far above and below the
    // curve: so the tube is never taken further than its own half height
    // past the lowest and highest the curve is over that ground.
    //
    // A band has no chamfer, and its top is not level across it when it
    // lies on its side: see `band_over`. It is held to the same limit.
    let span = |u: f64, v: f64, about: f64| {
        let (off, lift, slope, _, (lowest, highest), (place, way)) = near_about(u, v, Some(about));
        let (lift, tall, half_tall) = match *section {
            Section::Pipe { radius, chamfer, half_tall } => (lift, (half_tall - (off - (radius - chamfer)).clamp(0.0, chamfer)) * (1.0 + slope * slope).sqrt(), half_tall),
            Section::Band { across, half_thick, half_deep, .. } => {
                let (_, middle, half_long, half_roof) = band_over(across, half_thick, half_deep, axis, place, way, u, v);
                (middle, half_long, half_roof)
            }
        };
        let (lowest, highest) = if lowest <= highest { (lowest.min(lift), highest.max(lift)) } else { (lift, lift) };
        ((lift - tall).max(lowest - half_tall), (lift + tall).min(highest + half_tall))
    };
    // The tube stands over the cells whose middles are within its wall,
    // once for each pass of the curve over them. Two passes at heights that
    // meet are the tube running through itself: the lower one is built and
    // the other left out there, since no cell can be in two bricks.
    let mut over: std::collections::HashMap<(i32, i32), Vec<f64>> = std::collections::HashMap::new();
    // A flat band's own bricks, where they are given. `open` is then the
    // ground left to the rule above: under the three-cornered bricks that
    // are not kept. `long` is the ones that are, by their three corners
    // (the square one first) and the pass of the curve they belong to.
    let lifted: HashSet<(i32, i32)> = if plan.is_empty() { HashSet::new() } else { cells.iter().copied().collect() };
    let mut open: HashSet<(i32, i32)> = HashSet::new();
    let mut long: Vec<([(i32, i32); 3], f64)> = Vec::new();
    for b in plan {
        let (u, v, wu, wv) = (b.min[ua], b.min[va], b.ext[ua], b.ext[va]);
        let under = (0..wv).flat_map(move |j| (0..wu).map(move |i| (u + i, v + j)));
        // the pass of the curve a point of the ground is nearest, and whether its columns are wanted here
        let pass = |c: (i32, i32)| {
            let about = near_about(c.0 as f64 + 0.5, c.1 as f64 + 0.5, None).3;
            Some(about).filter(|a| wanted(c, length_at(*a)))
        };
        if b.mask == CUBE {
            for c in under.filter(|c| lifted.contains(c)) {
                if let Some(about) = pass(c) {
                    over.insert(c, vec![about]);
                }
            }
            continue;
        }
        // the corners of its box it stands on, numbered as a brick's are
        let foot: Vec<usize> = (0..4usize).filter(|k| b.mask >> ((k & 1) << ua | (k >> 1) << va) & 1 == 1).collect();
        let upright = crate::shapes::shape_info(b.mask).is_some_and(|i| i.kind == crate::shapes::Kind::Ramp && i.axis as usize == axis) && foot.len() == 3;
        let about = pass((u + wu / 2, v + wv / 2)).filter(|_| upright && under.clone().all(|c| lifted.contains(&c) && pass(c).is_some()));
        let corner = |k: usize| (u + (k as i32 & 1) * wu, v + (k as i32 >> 1) * wv);
        let kept = about.filter(|about| {
            let tops = foot.iter().map(|k| span(corner(*k).0 as f64, corner(*k).1 as f64, *about).1);
            let (low, high) = tops.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| (lo.min(t), hi.max(t)));
            high - low <= LONG_RISE
        });
        match kept {
            Some(about) => {
                let square = 3 - (0..4).find(|k| !foot.contains(k)).unwrap();
                let ends: Vec<usize> = foot.iter().copied().filter(|k| *k != square).collect();
                long.push(([corner(square), corner(ends[0]), corner(ends[1])], about));
            }
            None => open.extend(under.filter(|c| lifted.contains(c))),
        }
    }
    for &(u, v) in cells.iter().filter(|c| plan.is_empty() || open.contains(c)) {
        let (mid_u, mid_v) = (u as f64 + 0.5, v as f64 + 0.5);
        let mut found: Vec<(f64, f64, f64)> = passes(mid_u, mid_v)
            .into_iter()
            .filter(|about| wanted((u, v), length_at(*about)))
            .map(|about| {
                let (under, top) = span(mid_u, mid_v, about);
                (under, top, about)
            })
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut kept: Vec<(f64, f64, f64)> = Vec::new();
        for pass in found {
            // (two cells of air between them: corners are rounded, and a
            // steep pass is taller at its corners than at its middle)
            if kept.last().is_none_or(|below| pass.0 - below.1 >= 2.0) {
                kept.push(pass);
            }
        }
        if !kept.is_empty() {
            over.insert((u, v), kept.into_iter().map(|k| k.2).collect());
        }
    }
    let mut cells: Vec<(i32, i32)> = over.keys().copied().collect();
    cells.sort();
    // The ground of the lifted part, its cells sharing their corners (see
    // `fit_ribbon::Ground`). Where the curve passes twice there are two
    // tubes over one corner, so a corner is known by its place and by how
    // far along the curve the pass over it is.
    let mut ground = crate::fit_ribbon::Ground { at: Vec::new(), wanted: Vec::new(), squares: Vec::new(), halves: Vec::new() };
    let mut numbered: std::collections::HashMap<(i32, i32), Vec<(f64, usize)>> = std::collections::HashMap::new();
    let mut corner = |ground: &mut crate::fit_ribbon::Ground, p: (i32, i32), about: f64| -> usize {
        let known = numbered.entry(p).or_default();
        if let Some(k) = known.iter().find(|k| apart(k.0, about) <= same_pass) {
            return k.1;
        }
        ground.at.push(p);
        ground.wanted.push(span(p.0 as f64, p.1 as f64, about));
        known.push((about, ground.at.len() - 1));
        ground.at.len() - 1
    };
    for &(u, v) in &cells {
        for &about in &over[&(u, v)] {
            let corners = [(u, v), (u + 1, v), (u, v + 1), (u + 1, v + 1)].map(|p| corner(&mut ground, p, about));
            ground.squares.push((corners, span(u as f64 + 0.5, v as f64 + 0.5, about)));
        }
    }
    // The wall. Whole cells give it the outline of a staircase wherever the
    // tube runs askew of the grid, every tread and riser of it an upright
    // face pointing the wrong way. Each notch of that staircase, a cell
    // outside the tube with the tube on two sides of it that meet, is
    // filled with half a cell: a ramp standing on end, its sloped face
    // closing the notch. A staircase of one across and one along becomes a
    // flat wall at 45 degrees, and a shallower one loses its inside
    // corners. The half cell's corners are corners of the whole cells
    // beside it, so its top and its under side are the wall's.
    let inside: HashSet<(i32, i32)> = cells.iter().copied().collect();
    let mut notches: Vec<(i32, i32)> = cells.iter().flat_map(|&(u, v)| [(u + 1, v), (u - 1, v), (u, v + 1), (u, v - 1)]).filter(|c| !inside.contains(c)).collect();
    notches.sort();
    notches.dedup();
    for (u, v) in notches.into_iter().filter(|c| plan.is_empty() || open.contains(c)) {
        for (sx, sy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
            if !(inside.contains(&(u + sx, v)) && inside.contains(&(u, v + sy)) && !inside.contains(&(u - sx, v)) && !inside.contains(&(u, v - sy))) {
                continue;
            }
            // (where the curve passes twice there are two walls, one over
            // the other, and a notch of one need not be a notch of the
            // other: those are left as they are)
            let (&[about], &[other]) = (over[&(u + sx, v)].as_slice(), over[&(u, v + sy)].as_slice()) else { continue };
            // (nor is a cell the caller keeps for other columns filled from here)
            if apart(about, other) > same_pass || !wanted((u, v), length_at(about)) {
                continue;
            }
            let (ox, oy) = ((sx > 0) as i32, (sy > 0) as i32);
            // the half cell's corners on the ground: where the tube's two
            // sides meet, then the far end of each of them
            let plan = [(u + ox, v + oy), (u + 1 - ox, v + oy), (u + ox, v + 1 - oy)];
            let corners = plan.map(|p| corner(&mut ground, p, about));
            ground.halves.push(corners);
            break;
        }
    }
    for (plan, about) in long {
        let corners = plan.map(|p| corner(&mut ground, p, about));
        ground.halves.push(corners);
    }
    ground.lay(axis, max_ext)
}

/// How far the top of a lifted band may climb from one corner of a
/// three-cornered brick of its flat outline to another, in cells, for the
/// brick to be kept as half a box with one sloped brick on top and one
/// underneath. One brick cannot follow a top that bends along it, and
/// cannot be sloped both on top and underneath when the band climbs along
/// its long side (see `fit_ribbon::cap_of`), so a brick the band climbs
/// further over is given up for whole cells with their notches filled.
const LONG_RISE: f64 = 1.5;

/// A flat band whose curve has been pulled out of the plane it lay in, at
/// `plane` along `axis`: the flat band's bricks where the curve is still in
/// the plane, and columns where it has left it (see `lifted_band`). `None`
/// if the band is not a flat outline to begin with.
fn raised(spec: &Spec, path: &[DVec3], closed: bool, axis: usize, plane: f64) -> Result<Option<(Solid, Vec<DVec3>)>, String> {
    let level: Vec<DVec3> = path
        .iter()
        .map(|p| {
            let mut q = *p;
            q[axis] = plane;
            q
        })
        .collect();
    let (shift, run_on) = band_shift(spec, &level, closed, axis);
    let (flat, _) = planar(spec, level, closed, axis)?;
    let Solid::Flat { w0, depth, .. } = &flat else { return Ok(None) };
    let (w0, depth) = (*w0, *depth);
    let laid = fit(spec, &flat, Vec::new(), 1.0).bricks;
    // the curve where the flat band was laid: half a cell aside if the band was (see `planar`)
    let (ua, va) = plane_axes(axis);
    let aside: Vec<DVec3> = path
        .iter()
        .map(|p| {
            let mut q = *p;
            (q[ua], q[va]) = (p[ua] + shift, p[va] + shift);
            q
        })
        .collect();
    let bricks = lifted_band(spec, &laid, &aside, axis, plane, run_on);
    // What it is measured against: the band along the curve through the
    // middle of the flat band's depth, which is half a cell off the curve's
    // own plane when the depth is even.
    let through: Vec<DVec3> = aside
        .iter()
        .map(|p| {
            let mut q = *p;
            q[axis] += w0 as f64 + depth as f64 / 2.0 - plane;
            q
        })
        .collect();
    let (half_thick, half_deep) = (spec.thick / 2.0, depth as f64 / 2.0);
    let reach = half_thick.hypot(half_deep);
    let field = Field::Rail { stroke: Stroke::new(&through, reach, closed, Some(run_on)), axis, half_thick, half_deep };
    let lo = through.iter().fold(DVec3::MAX, |a, p| a.min(*p)) - reach - 1.0;
    let hi = through.iter().fold(DVec3::MIN, |a, p| a.max(*p)) + reach + 1.0;
    let (solid, path) = volume(field.clone(), lo, hi, path.to_vec())?;
    let Solid::Volume { min, max, .. } = solid else { unreachable!() };
    // laid out whole and measured afterwards, as a pyramid is
    Ok(Some((Solid::Pyramid { bricks: Some(bricks).filter(|b| !b.is_empty() && spec.wedges), field, min, max }, path)))
}

/// How far a flat band along `path` is moved aside along both axes of its
/// plane to land on the lattice, and how far it then runs on past each end
/// of an open path (see `planar`, which explains both).
fn band_shift(spec: &Spec, path: &[DVec3], closed: bool, axis: usize) -> (f64, f64) {
    let (ua, _) = plane_axes(axis);
    let shifted = !spec.fill && off_lattice(path, closed, ua, (spec.thick.round() as i32).max(1));
    if shifted { (-0.5, 0.0) } else { (0.0, 0.5) }
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
    let (shift, extend) = band_shift(spec, &path, closed, axis);
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
/// A turned form laid out one of the two ways `fit_lathe` knows; `None` if
/// `solid` is not one or cannot be laid out so.
fn turned(spec: &Spec, solid: &Solid, g: f64, short_sides: bool) -> Option<Vec<CellBrick>> {
    let Solid::Lathe { profile, axis, far, low, high, .. } = solid else { return None };
    let section = |out: f64, up: f64| profile.eval(DVec2::new(out, up));
    let lathe = Lathe { axis: *axis, section: &section, far: *far, low: *low, high: *high, brick_cost: spec.simplify / (g * g), short_sides, max_ext: spec.max_ext };
    fit_lathe(&lathe).filter(|_| spec.wedges)
}

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
                shades: Vec::new(),
            }
        }
        Solid::Lathe { field, min, max, .. } => {
            // Laid out both ways `fit_lathe` knows, and the closer kept: which
            // way suits depends on the form (a straight cone one, a thin
            // curved wall the other), and measuring both is cheaper than a
            // rule that would have to guess.
            let mut seen = crate::report::Seen::new();
            let form = Probe::of(Solid::Volume { field: field.clone(), min: *min, max: *max });
            let ways = [false, true].map(|short_sides| {
                let bricks = turned(spec, solid, g, short_sides)?;
                let mut built = Built { bricks, solid: 0.0, error: 0.0, path: Vec::new(), grain: 1, unmeasured: true, shades: Vec::new() };
                // The two totals only: the record a report keeps of every
                // cell took most of the time here, and was thrown away. And
                // what the form holds of each cell is asked once for both.
                if let Ok((volume, wrong)) = crate::report::measure_with(&form, &built, &mut seen) {
                    (built.solid, built.error, built.unmeasured) = (volume, wrong, false);
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
            Some(bricks) => Built { bricks, solid: 0.0, error: 0.0, path, grain: 1, unmeasured: true, shades: Vec::new() },
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
                shades: Vec::new(),
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
            let mut shades = Vec::new();
            let opts = fit_planar::Opts { brick_cost: spec.simplify / (g * g), max_ext: spec.max_ext, wedges: spec.wedges };
            let fit = match (&spec.stones, &spec.mosaic) {
                // each stone its own outline, where a solid form is one
                (Some(stones), _) => crate::pattern::lay_stones(&|p| region.eval(p), stones, *axis, *min, *max, *w0, *depth, &opts, &mut bricks, &mut shades),
                // each colour of the mosaic its own outline
                (None, Some(mosaic)) => crate::pattern::lay_mosaic(&|p| region.eval(p), mosaic, *axis, *min, *max, *w0, *depth, &opts, &mut bricks, &mut shades),
                (None, None) => fit_layer(spec, region, *axis, *min, *max, *w0, *depth, g, &mut bricks),
            };
            let depth = *depth as f64;
            Built {
                bricks,
                solid: fit.solid * depth,
                error: fit.error * depth,
                path,
                grain: 1,
                unmeasured: false,
                shades,
            }
        }
        Solid::Pipe { top, full, chamfer, rise, axis, first, layers, min, max, .. } => {
            let opts = fit_planar::Opts { brick_cost: spec.simplify / (g * g), max_ext: spec.max_ext, wedges: true };
            let narrower = |x: f64, y: f64| top.eval(DVec2::new(x, y));
            let fit = fit_planar_towards(&|x, y| full.eval(DVec2::new(x, y)), *min, *max, &opts, Some((&narrower, *chamfer)));
            let pipe = crate::fit_stack::Pipe { wall: &fit.pieces, top: &narrower, chamfer: *chamfer, rise: *rise, axis: *axis, first: *first, layers: *layers, max_ext: spec.max_ext };
            // measured against the round tube by the caller
            Built { bricks: crate::fit_stack::pipe(&pipe), solid: 0.0, error: 0.0, path, grain: 1, unmeasured: true, shades: Vec::new() }
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
                shades: Vec::new(),
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
        // No room in two bricks. Two may share a cell and no room: the two
        // halves of a box cut along its diagonal, as along the crease of a
        // spun profile. So a cell two bricks share is sampled, at 27 points
        // clear of its faces and of the diagonals, and none may be in both.
        let holds = |b: &CellBrick, p: [f64; 3]| {
            let u = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
            crate::shapes::inside_shape(shape_info(b.mask).unwrap(), u[0], u[1], u[2])
        };
        let mut seen: std::collections::HashMap<[i32; 3], Vec<&CellBrick>> = std::collections::HashMap::new();
        for b in &built.bricks {
            assert!(b.ext.iter().all(|&e| (1..=80).contains(&e)), "{b:?}");
            assert!(solver.place(b).is_some(), "{b:?} has no orientation");
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        let cell = [b.min[0] + x, b.min[1] + y, b.min[2] + z];
                        let there = seen.entry(cell).or_default();
                        for other in there.iter() {
                            let shared = (0..27).any(|k| {
                                let p = [cell[0] as f64 + (k % 3) as f64 / 3.0 + 0.17, cell[1] as f64 + (k / 3 % 3) as f64 / 3.0 + 0.15, cell[2] as f64 + (k / 9) as f64 / 3.0 + 0.19];
                                holds(b, p) && holds(other, p)
                            });
                            assert!(!shared, "{b:?} overlaps {other:?}");
                        }
                        there.push(b);
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
    fn a_pattern_is_laid_at_single_microbricks_whatever_the_budget() {
        // (`grain: 0` is the brick size chosen by the budget, as the editor has it)
        // a floor 240 by 160 and 4 deep, which as a plain floor is a handful of bricks (a floor one deep is at single microbricks anyway: there is no coarser brick it can be built of)
        let floor = |mosaic| Spec { form: Form::Spline, points: vec![DVec3::new(-119.5, 0.5, 0.5), DVec3::new(120.5, 0.5, 0.5)], thick: 160.0, depth: 4, plane: 2, grain: 0, budget: 2500, mosaic, ..Spec::default() };
        let rug = crate::pattern::STYLES.iter().position(|style| style.name == "rug").unwrap();
        let built = build(&floor(Some(crate::pattern::Mosaic { style: rug, size: 2, bevel: 0, border: 0, seed: 8, mortar: true, stagger: -1, upright: false, uneven: [0; 3] }))).unwrap();
        // Far over the budget, at the finest size: some 25,000 bricks. (Held
        // to the budget it is built of bricks 2 or more across and is under
        // 8,000. A flat form's `grain` says 1 either way, its bricks being
        // given in microbricks whatever size they were fitted at.)
        assert!(built.bricks.len() > 20_000, "{} bricks", built.bricks.len());
        assert_eq!(built.shades.len(), built.bricks.len());
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
        let spec = Spec { form: Form::Pyramid, points: vec![at(0.0, 0.0, 0.0)], radius: 8.5, rise: Some(8.0), fill: true, ..Spec::default() };
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
        // Not filled, it is a tent: the same outside, a valley up each
        // inside ridge, and still exactly the form it is measured against.
        let tent = check(&Spec { radius: 12.5, rise: Some(12.0), thick: 4.0, fill: false, ..spec.clone() });
        let of = |k: Kind| tent.bricks.iter().filter(|b| shape_info(b.mask).unwrap().kind == k).count();
        assert!(of(Kind::Hip) >= 4 && of(Kind::Inner) >= 4, "{} hips, {} inner corners", of(Kind::Hip), of(Kind::Inner));
        assert!(tent.error < 0.01 * tent.solid, "a tent {} wrong of {}", tent.error, tent.solid);
        let whole = check(&Spec { radius: 12.5, rise: Some(12.0), fill: true, ..spec.clone() });
        assert!(tent.solid < 0.8 * whole.solid, "the tent is not hollow: {} of {}", tent.solid, whole.solid);
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
        // (0.118 while the lip's crest and the groove inside it were whole cubes)
        assert!(steps < 0.08, "the bell is {steps} steps");
        for kind in [Kind::Ramp, Kind::Tet, Kind::Cut] {
            assert!(built.bricks.iter().any(|b| shape_info(b.mask).unwrap().kind == kind), "no {kind:?}");
        }
        // the same all the way round: mirrored in x and in y, and with the two exchanged
        let cells: HashSet<[i32; 3]> = built.bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).flat_map(move |y| (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1] + y, b.min[2] + z])))).collect();
        assert!(cells.iter().all(|c| cells.contains(&[-c[0], c[1], c[2]]) && cells.contains(&[c[0], -c[1], c[2]]) && cells.contains(&[c[1], c[0], c[2]])), "the bell is not the same all round");
        // Built for a drag it is laid out, sloped rings and all, but not
        // measured; and the two ways it can be laid out are measured as one
        // way alone would be.
        let quick = build_as(&bell, false).unwrap();
        assert!(quick.unmeasured && quick.error == 0.0 && quick.bricks.iter().any(|b| b.mask != CUBE));
        assert!(!built.unmeasured && built.error > 0.0);
        let (volume, wrong) = crate::report::measure(&Probe::new(&bell, built.grain).unwrap(), &built).unwrap();
        assert!((volume - built.solid).abs() < 1e-9 && (wrong - built.error).abs() < 1e-9, "measured apart {wrong} of {volume}, together {} of {}", built.error, built.solid);
        // solid, and a plain dome's profile
        let (_, solid) = stepped(&Spec { fill: true, ..bell.clone() });
        let (_, dome) = stepped(&Spec { points: vec![at(0.0, 0.0, 14.0), at(9.0, 0.0, 11.0), at(14.0, 0.0, 0.0)], fill: true, ..bell.clone() });
        assert!(solid < 0.1 && dome < 0.06, "solid {solid}, dome {dome}");
        // cubes alone are still the staircase, and an axis between the two grids is still fitted cell by cell
        let (_, cubes) = stepped(&Spec { wedges: false, ..bell.clone() });
        assert!(cubes > 0.4, "cubes {cubes}");
        check(&Spec { points: vec![DVec3::new(0.5, 0.0, 14.5), DVec3::new(9.5, 0.0, 11.5), DVec3::new(14.5, 0.0, 0.5)], fill: true, ..bell.clone() });
        // Shells with creases: a bowl with a lip turned outwards, and a
        // vase with a neck and a flared mouth. The crest of each lip, where
        // the outer face and the hollow's wall end on one circle, was a ring
        // of whole cubes until the two shared an outline there (9.5% and
        // 6.0% stepped).
        let (_, bowl) = stepped(&Spec { points: vec![at(0.0, 0.0, 4.0), at(14.0, 0.0, 6.0), at(22.0, 0.0, 14.0), at(21.0, 0.0, 22.0), at(26.0, 0.0, 25.0)], thick: 3.0, ..bell.clone() });
        let (_, vase) = stepped(&Spec { points: vec![at(0.0, 0.0, 3.0), at(9.0, 0.0, 3.0), at(17.0, 0.0, 18.0), at(8.0, 0.0, 30.0), at(6.0, 0.0, 35.0), at(13.0, 0.0, 42.0)], thick: 3.0, ..bell });
        assert!(bowl < 0.085 && vase < 0.05, "bowl {bowl}, vase {vase}");
    }

    #[test]
    fn a_pull_moves_the_curve_near_it_and_leaves_the_rest() {
        // a ring of radius 20 in the upright plane, pulled 6 up at its top and 4 in at one side
        let ring = Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 20.0, plane: 1, thick: 2.0, ..Spec::default() };
        let (top, side) = (at(0.0, 0.0, 20.0), at(20.0, 0.0, 0.0));
        let both = Spec { pulls: vec![(top, DVec3::new(0.0, 0.0, 6.0)), (side, DVec3::new(-4.0, 0.0, 0.0))], pull_reach: 10.0, ..ring.clone() };
        let (plain, bumped) = (build(&ring).unwrap(), check(&both));
        let highest = |b: &Built| b.bricks.iter().map(|k| k.min[2] + k.ext[2]).max().unwrap();
        let widest = |b: &Built| b.bricks.iter().map(|k| k.min[0] + k.ext[0]).max().unwrap();
        let lowest = |b: &Built| b.bricks.iter().map(|k| k.min[2]).min().unwrap();
        assert_eq!(highest(&bumped), highest(&plain) + 6, "the top went up by the pull");
        // the bottom, far round the ring from both, has not moved
        assert_eq!(lowest(&bumped), lowest(&plain));
        // the side pulled in is no longer the widest part, which is now just above or below it
        assert!(widest(&bumped) <= widest(&plain));
        // still flat in its plane, so still a band of whole bricks one deep
        assert!(bumped.bricks.iter().all(|b| b.min[1] == plain.bricks[0].min[1] && b.ext[1] == plain.bricks[0].ext[1]));
        // the curve itself: the point held moves by exactly the pull, a point beyond its reach not at all
        let path: Vec<DVec3> = (0..360).map(|d| (d as f64).to_radians()).map(|a| DVec3::new(20.0 * a.cos(), 0.0, 20.0 * a.sin())).collect();
        let moved = pulled(&path, true, &[(DVec3::new(0.0, 0.0, 20.0), DVec3::new(0.0, 0.0, 6.0))], 10.0);
        let top_now = moved.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((top_now - 26.0).abs() < 1e-6, "{top_now}");
        assert!(moved.iter().all(|p| p.z >= -20.0 - 1e-9) && moved.iter().any(|p| (p.z + 20.0).abs() < 1e-6));
        // smooth: no two neighbours of the finer curve further apart than a cell
        assert!(moved.windows(2).all(|w| w[0].distance(w[1]) < 1.0));
        // Out of its plane the band keeps its outline and is raised to
        // follow the curve: sloped bricks above and below, far fewer bricks
        // and far fewer steps than the same curve fitted cell by cell as a tube.
        let lifted = Spec { pulls: vec![(top, DVec3::new(0.0, 5.0, 0.0))], thick: 3.0, ..ring.clone() };
        let leaning = build(&lifted).unwrap();
        assert!(leaning.bricks.iter().any(|b| b.min[1] >= 4), "nothing was pulled out of the plane");
        let stepped = |spec: &Spec, built: &Built| {
            let survey = crate::surface::survey(&Probe::new(spec, built.grain).unwrap(), built).unwrap();
            survey.stepped / survey.area
        };
        let tube = Spec { tube: true, ..lifted.clone() };
        let as_tube = build(&tube).unwrap();
        let (band_steps, tube_steps) = (stepped(&lifted, &leaning), stepped(&tube, &as_tube));
        // (the tube keeps its pipe where it was not lifted: see `patched`)
        assert!(band_steps < 0.12 && tube_steps < 0.12, "the band is {band_steps} steps, a tube {tube_steps}");
        // Seen along its axis it is the flat ring still: the same cells of
        // the plane, but for a few under the three-cornered bricks of the
        // outline it climbs too steeply over to keep (see `LONG_RISE`).
        let plan = |b: &Built| -> HashSet<[i32; 2]> { b.bricks.iter().flat_map(|k| (0..k.ext[0]).flat_map(move |x| (0..k.ext[2]).map(move |z| [k.min[0] + x, k.min[2] + z]))).collect() };
        let (now, flat) = (plan(&leaning), plan(&build(&Spec { thick: 3.0, ..ring }).unwrap()));
        let changed = now.symmetric_difference(&flat).count();
        assert!(changed * 50 <= flat.len(), "{changed} of the plan's {} cells are not the flat ring's", flat.len());
    }

    #[test]
    fn a_band_pulled_up_steeply_is_sloped_not_stepped() {
        // (the forms of two saved reports: small rings two deep, one point
        // pulled seven and a half cells up and another three and a half)
        let pulls = vec![(DVec3::new(-0.2841371226364836, -7.461477813377575, 0.5), DVec3::new(0.0, 0.0, 7.5)), (DVec3::new(2.0607225761290264, 8.346282243225843, 0.5), DVec3::new(0.0, 0.0, 3.5))];
        let ring = Spec { form: Form::Circle, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: 8.0, depth: 2, pulls, pull_reach: 8.0, simplify: 0.25, ..Spec::default() };
        // Built from the flat band's bricks moved up and down, 23.9% of the
        // thin one's outside was stepped faces and 16.3% of the thick one's.
        for (thick, most) in [(2.0, 0.15), (3.5, 0.12)] {
            let spec = Spec { thick, ..ring.clone() };
            let built = build(&spec).unwrap();
            let probe = Probe::new(&spec, built.grain).unwrap();
            let survey = crate::surface::survey(&probe, &built).unwrap();
            assert!(survey.stepped / survey.area < most, "thick {thick}: {} stepped", survey.stepped / survey.area);
            // it went up by the pull
            assert!(built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() >= 8);
            // No point is in two bricks, and no brick stands out of the
            // band: a long brick of the outline moved whole stood a cell
            // and a half clear of it.
            let mut held_by: std::collections::HashMap<[i32; 3], Vec<usize>> = std::collections::HashMap::new();
            for (n, b) in built.bricks.iter().enumerate() {
                for k in 0..b.ext[0] * b.ext[1] * b.ext[2] {
                    held_by.entry([b.min[0] + k % b.ext[0], b.min[1] + k / b.ext[0] % b.ext[1], b.min[2] + k / (b.ext[0] * b.ext[1])]).or_default().push(n);
                }
            }
            for (cell, bricks) in &held_by {
                for k in 0..64 {
                    let p = [cell[0] as f64 + ((k % 4) as f64 + 0.37) / 4.0, cell[1] as f64 + ((k / 4 % 4) as f64 + 0.41) / 4.0, cell[2] as f64 + ((k / 16) as f64 + 0.43) / 4.0];
                    let held = bricks
                        .iter()
                        .filter(|n| {
                            let b = &built.bricks[**n];
                            let u = [0, 1, 2].map(|i| (p[i] - b.min[i] as f64) / b.ext[i] as f64);
                            crate::shapes::inside_shape(shape_info(b.mask).unwrap(), u[0], u[1], u[2])
                        })
                        .count();
                    assert!(held <= 1, "thick {thick}: {held} bricks hold {p:?}");
                    assert!(held == 0 || probe.distance(DVec3::from_array(p)) < 1.25, "thick {thick}: a brick holds {p:?}, {} outside the band", probe.distance(DVec3::from_array(p)));
                }
            }
        }
    }

    #[test]
    fn a_thick_tube_pulled_up_steeply_keeps_its_sides_smooth() {
        // (the form of a saved report: a ring 11 thick, one side pulled up 25)
        let ring = Spec { form: Form::Circle, points: vec![at(0.5, 0.5, 0.5)], radius: 44.0, thick: 11.0, tube: true, simplify: 0.25, ..Spec::default() };
        let pulled = Spec { pulls: vec![(DVec3::new(-18.5, -8.5, 0.5), DVec3::new(3.5, -7.0, 25.0))], pull_reach: 40.0, ..ring };
        let built = build(&pulled).unwrap();
        let survey = crate::surface::survey(&Probe::new(&pulled, built.grain).unwrap(), &built).unwrap();
        // with a round section where it is lifted, 6.7% was stepped; with each cell rounding its own corners, 4.9%
        assert!(survey.stepped / survey.area < 0.04, "{} stepped", survey.stepped / survey.area);
        // (and 4071 bricks, before the ramps side by side were joined)
        assert!(built.bricks.len() < 3800, "{} bricks", built.bricks.len());
        assert!(built.error < 0.16 * built.solid, "{} wrong of {}", built.error, built.solid);
    }

    #[test]
    fn a_tube_pulled_into_a_loop_stays_within_reach_of_its_curve() {
        // (the form of a saved report: six points pulled about until the
        // ring crossed over itself and, in one place, ran straight up)
        let ring = Spec { form: Form::Circle, points: vec![at(0.5, 0.5, 0.5)], radius: 44.0, thick: 11.0, tube: true, simplify: 0.25, ..Spec::default() };
        let pulls = vec![
            (DVec3::new(-18.48, -8.48, 0.5), DVec3::new(-27.5, 0.0, 19.5)),
            (DVec3::new(6.10, 44.13, 0.5), DVec3::new(13.0, 0.0, -7.0)),
            (DVec3::new(14.41, -41.06, 0.5), DVec3::new(37.5, 0.0, 19.5)),
            (DVec3::new(-23.48, -38.50, 6.74), DVec3::new(-11.0, 0.0, -19.5)),
            (DVec3::new(42.68, 11.19, 0.5), DVec3::new(14.5, 0.0, -24.5)),
            (DVec3::new(33.78, 33.45, 4.64), DVec3::new(-35.5, 0.0, -64.0)),
        ];
        let spec = Spec { pulls, pull_reach: 40.0, ..ring };
        let built = build(&spec).unwrap();
        // no brick further above or below the curve than the tube is thick:
        // it stood 120 above it and 60 below
        let (low, high) = built.path.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.z), hi.max(p.z)));
        let (bottom, top) = (built.bricks.iter().map(|b| b.min[2]).min().unwrap() as f64, built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() as f64);
        assert!(top <= high + 8.0 && bottom >= low - 8.0, "bricks from {bottom} to {top}, the curve from {low} to {high}");
        // Where the curve climbs steeply the tube is built in columns lying
        // on their sides. In upright columns throughout, 17.9% of it was wrong.
        assert!(built.error < 0.16 * built.solid, "{} wrong of {}", built.error, built.solid);
        // The stretches built one way and the other meet without sharing
        // any room and without a gap: no point is in two bricks, and every
        // point well inside the tube is in one.
        let mut held_by: std::collections::HashMap<[i32; 3], Vec<usize>> = std::collections::HashMap::new();
        for (n, b) in built.bricks.iter().enumerate() {
            for k in 0..b.ext[0] * b.ext[1] * b.ext[2] {
                held_by.entry([b.min[0] + k % b.ext[0], b.min[1] + k / b.ext[0] % b.ext[1], b.min[2] + k / (b.ext[0] * b.ext[1])]).or_default().push(n);
            }
        }
        let probe = Probe::new(&spec, built.grain).unwrap();
        let (lo, hi) = (built.bricks.iter().fold([i32::MAX; 3], |a, b| [0, 1, 2].map(|k| a[k].min(b.min[k]))), built.bricks.iter().fold([i32::MIN; 3], |a, b| [0, 1, 2].map(|k| a[k].max(b.min[k] + b.ext[k]))));
        let (mut inside, mut empty) = (0, 0);
        let n = 56;
        for k in 0..n * n * n {
            let t = [(k % n) as f64 + 0.31, (k / n % n) as f64 + 0.47, (k / (n * n)) as f64 + 0.29].map(|v| v / n as f64);
            let p = [0, 1, 2].map(|i| lo[i] as f64 + t[i] * (hi[i] - lo[i]) as f64);
            let held = held_by.get(&p.map(|v| v.floor() as i32)).map_or(0, |bricks| {
                bricks
                    .iter()
                    .filter(|n| {
                        let b = &built.bricks[**n];
                        let u = [0, 1, 2].map(|i| (p[i] - b.min[i] as f64) / b.ext[i] as f64);
                        crate::shapes::inside_shape(shape_info(b.mask).unwrap(), u[0], u[1], u[2])
                    })
                    .count()
            });
            assert!(held <= 1, "{held} bricks hold {p:?}");
            if probe.distance(DVec3::from_array(p)) < -2.5 {
                inside += 1;
                empty += (held == 0) as i32;
            }
        }
        assert!(inside > 1500 && empty * 200 <= inside, "{empty} of {inside} points well inside the tube are in no brick");
    }

    #[test]
    fn no_brick_is_longer_than_the_longest_allowed() {
        // large forms of every kind, with the longest brick set well below
        // what each would otherwise use
        let longest = 12;
        let base = Spec { max_ext: longest, ..Spec::default() };
        let forms = [
            ("sphere", Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, fill: true, ..base.clone() }),
            ("shell", Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius: 40.0, thick: 4.0, fill: false, ..base.clone() }),
            ("pyramid", Spec { form: Form::Pyramid, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, fill: true, ..base.clone() }),
            ("tent", Spec { form: Form::Pyramid, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, thick: 4.0, fill: false, ..base.clone() }),
            ("arch", Spec { form: Form::Arch(ArchType::Round), points: vec![at(-60.0, 0.0, 0.0), at(60.0, 0.0, 0.0)], thick: 6.0, depth: 40, ..base.clone() }),
            ("disc", Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, thick: 4.0, depth: 30, fill: true, ..base.clone() }),
            ("torus", Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, thick: 10.0, tube: true, ..base.clone() }),
            ("lifted torus", Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, thick: 10.0, tube: true, pulls: vec![(at(-60.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 30.0))], pull_reach: 30.0, ..base.clone() }),
            ("bell", Spec { form: Form::Revolve, points: vec![at(0.0, 0.0, 60.0), at(30.0, 0.0, 40.0), at(50.0, 0.0, 0.0)], thick: 4.0, ..base.clone() }),
        ];
        for (name, spec) in forms {
            let built = build(&spec).unwrap_or_else(|e| panic!("{name}: {e}"));
            let worst = built.bricks.iter().flat_map(|b| b.ext).max().unwrap();
            assert!(worst <= longest, "{name}: a brick {worst} long at brick size {}, with {longest} allowed", built.grain);
        }
    }

    #[test]
    fn a_point_added_to_a_laid_out_form_changes_it_only_there() {
        let key = |b: &CellBrick| (b.mask, b.min, b.ext);
        let kept = |plain: &Built, pulled: &Built| {
            let now: HashSet<(u8, [i32; 3], [i32; 3])> = pulled.bricks.iter().map(key).collect();
            plain.bricks.iter().filter(|b| now.contains(&key(b))).count() as f64 / plain.bricks.len() as f64
        };
        let stepped = |spec: &Spec, built: &Built| {
            let survey = crate::surface::survey(&Probe::new(spec, built.grain).unwrap(), built).unwrap();
            survey.stepped / survey.area
        };
        let no_two_share = |built: &Built| {
            let mut seen = HashSet::new();
            built.bricks.iter().all(|b| (0..b.ext[0]).all(|x| (0..b.ext[1]).all(|y| (0..b.ext[2]).all(|z| seen.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z])))))
        };
        // a ball with a bump pulled out of one side: the rest of it is the ball's own bricks
        let ball = Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius: 20.0, fill: true, ..Spec::default() };
        let lump = Spec { pulls: vec![(at(20.0, 0.0, 0.0), DVec3::new(6.0, 0.0, 0.0))], pull_reach: 8.0, ..ball.clone() };
        let (round, lumpy) = (build(&ball).unwrap(), build(&lump).unwrap());
        assert!(kept(&round, &lumpy) > 0.7, "only {} of the ball's bricks are left", kept(&round, &lumpy));
        assert!(no_two_share(&lumpy));
        assert!(stepped(&lump, &lumpy) < 0.08, "{} stepped", stepped(&lump, &lumpy));
        assert!(lumpy.error < 0.05 * lumpy.solid, "{} wrong of {}", lumpy.error, lumpy.solid);
        assert!(lumpy.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap() >= round.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap() + 4);
        // a large ring as a round tube, one side of it lifted: the far side is the pipe still
        let ring = Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 30.0, thick: 8.0, tube: true, ..Spec::default() };
        let lifted = Spec { pulls: vec![(at(-30.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 6.0))], pull_reach: 10.0, ..ring.clone() };
        let (flat, raised) = (build(&ring).unwrap(), build(&lifted).unwrap());
        assert!(kept(&flat, &raised) > 0.5, "only {} of the pipe's bricks are left", kept(&flat, &raised));
        // No point is in two bricks. (Not `no_two_share`: where it is lifted
        // the tube is built of half squares, two to a box.) Looked for over
        // the lifted part, where the new bricks and the kept ones meet.
        for k in 0..30 * 30 * 30 {
            let t = [(k % 30) as f64 + 0.31, (k / 30 % 30) as f64 + 0.47, (k / 900) as f64 + 0.29].map(|v| v / 30.0);
            let p = [-46.0 + t[0] * 32.0, -22.0 + t[1] * 44.0, -6.0 + t[2] * 20.0];
            let held = raised
                .bricks
                .iter()
                .filter(|b| {
                    let u = [0, 1, 2].map(|i| (p[i] - b.min[i] as f64) / b.ext[i] as f64);
                    u.iter().all(|v| (0.0..1.0).contains(v)) && crate::shapes::inside_shape(shape_info(b.mask).unwrap(), u[0], u[1], u[2])
                })
                .count();
            assert!(held <= 1, "{held} bricks hold {p:?}");
        }
        assert!(stepped(&lifted, &raised) < 0.08, "{} stepped", stepped(&lifted, &raised));
        assert!(raised.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() >= flat.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() + 4);
        // every brick it keeps on the far side is one the plain ring has
        let plain: HashSet<(u8, [i32; 3], [i32; 3])> = flat.bricks.iter().map(key).collect();
        assert!(raised.bricks.iter().filter(|b| b.min[0] > 5).all(|b| plain.contains(&key(b))));
    }

    #[test]
    fn a_turned_form_is_the_same_bricks_facing_another_way() {
        use crate::shapes::{compose, quarter};
        let pyramid = Spec { form: Form::Pyramid, points: vec![at(3.0, -2.0, 5.0)], radius: 6.5, rise: Some(9.0), fill: true, ..Spec::default() };
        let upright = build(&pyramid).unwrap();
        // a quarter turn about x: what stood up along z lies along y
        let lying = Spec { turned: quarter(0), ..pyramid.clone() };
        let built = check(&lying);
        let span = |b: &Built, k: usize| b.bricks.iter().map(|p| p.min[k] + p.ext[k]).max().unwrap() - b.bricks.iter().map(|p| p.min[k]).min().unwrap();
        assert_eq!([span(&built, 0), span(&built, 1), span(&built, 2)], [span(&upright, 0), span(&upright, 2), span(&upright, 1)]);
        assert_eq!(built.bricks.len(), upright.bricks.len());
        // as well fitted lying down as standing: measured against the form as it now faces
        assert!((built.error - upright.error).abs() < 1e-6 && (built.solid - upright.solid).abs() < 1e-6);
        // four quarter turns, one after another, are the form as it was
        let round = (0..4).fold(crate::shapes::UPRIGHT, |o, _| compose(o, quarter(2)));
        let mut again = build(&Spec { turned: round, ..pyramid.clone() }).unwrap().bricks;
        let mut first = upright.bricks.clone();
        again.sort_by_key(|b| (b.min, b.ext, b.mask));
        first.sort_by_key(|b| (b.min, b.ext, b.mask));
        assert_eq!(again, first);
        // a point of the form goes where the turn says, and comes back
        let turn = Turn::of(&lying);
        let p = DVec3::new(1.25, -4.0, 9.5);
        assert!((turn.back(turn.point(p)) - p).length() < 1e-12);
        assert!((turn.point(p) - turn.point(DVec3::ZERO) - turn.back_way(turn.back_way(turn.back_way(p)))).length() < 1e-12, "three turns back are one turn on");
    }

    #[test]
    fn a_mirrored_form_is_its_low_side_twice() {
        // an arch with one foot two cells higher than the other
        let lopsided = Spec { form: Form::Arch(ArchType::Round), points: vec![at(-12.0, 0.0, -2.0), at(16.0, 0.0, 0.0)], thick: 3.0, ..Spec::default() };
        let cells = |b: &Built| -> HashSet<[i32; 3]> { b.bricks.iter().flat_map(|k| (0..k.ext[0]).flat_map(move |x| (0..k.ext[1]).flat_map(move |y| (0..k.ext[2]).map(move |z| [k.min[0] + x, k.min[1] + y, k.min[2] + z])))).collect() };
        // the middle of its points along x is 2.5, so cell x has its image at 4 - x
        let same_both_sides = |c: &HashSet<[i32; 3]>| c.iter().all(|p| c.contains(&[4 - p[0], p[1], p[2]]));
        let plain = cells(&build(&lopsided).unwrap());
        assert!(!same_both_sides(&plain), "the arch was symmetric to begin with");
        let fixed = check(&Spec { mirror: [true, false, false], ..lopsided.clone() });
        let both = cells(&fixed);
        assert!(same_both_sides(&both), "the mirrored arch is not the same on both sides");
        // its low side is the arch's own low side, foot and all
        let lowest = |c: &HashSet<[i32; 3]>, side: fn(i32) -> bool| c.iter().filter(|p| side(p[0])).map(|p| p[2]).min().unwrap();
        assert_eq!(lowest(&both, |x| x < -6), lowest(&plain, |x| x < -6));
        assert_eq!(lowest(&both, |x| x > 10), lowest(&plain, |x| x < -6), "the far foot is not the near foot's image");
        // and brick for brick, not only cell for cell
        let mut left: Vec<(u8, [i32; 3], [i32; 3])> = fixed.bricks.iter().map(|b| (b.mask, b.min, b.ext)).collect();
        let mut right: Vec<(u8, [i32; 3], [i32; 3])> = fixed.bricks.iter().map(|b| ((0..8u8).fold(0u8, |m, c| m | (b.mask >> c & 1) << (c ^ 1)), [5 - b.min[0] - b.ext[0], b.min[1], b.min[2]], b.ext)).collect();
        left.sort();
        right.sort();
        assert_eq!(left, right);

        // a ring pulled out on its low side comes out on the other side too
        let ring = Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 20.0, plane: 1, thick: 2.0, pulls: vec![(at(-20.0, 0.0, 0.0), DVec3::new(-5.0, 0.0, 0.0))], pull_reach: 8.0, ..Spec::default() };
        let reach = |b: &Built| (b.bricks.iter().map(|k| k.min[0]).min().unwrap(), b.bricks.iter().map(|k| k.min[0] + k.ext[0]).max().unwrap());
        let (one, two) = (reach(&build(&ring).unwrap()), reach(&check(&Spec { mirror: [true, false, false], ..ring.clone() })));
        assert_eq!(one.0, two.0);
        // (about the ring's own middle, which is on a lattice line or in the middle of a cell)
        assert!(two.0 + two.1 == 0 || two.0 + two.1 == 1, "the bump is on one side only: {two:?}");
        assert!(two.1 > one.1 + 3);

        // A ball with a point pulled out of its side is no longer round, and
        // still builds; mirrored, it bulges on both sides alike.
        let ball = Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius: 10.0, fill: true, ..Spec::default() };
        let lump = Spec { pulls: vec![(at(-10.0, 0.0, 0.0), DVec3::new(-6.0, 0.0, 0.0))], pull_reach: 8.0, ..ball.clone() };
        let (round, lumpy, both) = (reach(&build(&ball).unwrap()), reach(&build(&lump).unwrap()), reach(&build(&Spec { mirror: [true, false, false], ..lump.clone() }).unwrap()));
        assert!(lumpy.0 <= round.0 - 4 && lumpy.1 == round.1, "round {round:?}, pulled {lumpy:?}");
        assert!(both.0 == lumpy.0 && both.1 >= round.1 + 4 && both.0 + both.1 == 1, "mirrored {both:?}");
        // and a turned form's profile takes a point as a curve does: wider all round at that height
        let vase = Spec { form: Form::Revolve, points: vec![at(0.0, 0.0, 20.0), at(6.0, 0.0, 10.0), at(8.0, 0.0, 0.0)], thick: 2.0, fill: true, ..Spec::default() };
        let belly = Spec { pulls: vec![(at(6.0, 0.0, 10.0), DVec3::new(5.0, 0.0, 0.0))], pull_reach: 6.0, ..vase.clone() };
        let (slim, wide) = (reach(&build(&vase).unwrap()), reach(&check(&belly)));
        assert!(wide.1 >= slim.1 + 2 && wide.0 <= slim.0 - 2, "slim {slim:?}, pulled {wide:?}");

        // A point added on the high side of a mirror counts as much as one on
        // the low side: the ring bulges on both sides either way, and the same.
        let high = Spec { pulls: vec![(at(20.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0))], mirror: [true, false, false], ..ring.clone() };
        let low = Spec { mirror: [true, false, false], ..ring.clone() };
        let (from_high, from_low) = (build(&high).unwrap(), build(&low).unwrap());
        let key = |b: &Built| {
            let mut all: Vec<(u8, [i32; 3], [i32; 3])> = b.bricks.iter().map(|k| (k.mask, k.min, k.ext)).collect();
            all.sort();
            all
        };
        assert_eq!(key(&from_high), key(&from_low));
        assert!(reach(&from_high).1 > one.1 + 3);

        // the curve itself: an open one kept from whichever end is low, a closed one round its low side
        let open = [DVec3::new(-4.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 3.0), DVec3::new(2.0, 0.0, 5.0), DVec3::new(6.0, 0.0, 9.0)];
        let made = mirrored(&open, false, 0, 1.0);
        assert_eq!((made[0], made[made.len() - 1]), (open[0], DVec3::new(6.0, 0.0, 0.0)));
        assert!(made.iter().zip(made.iter().rev()).all(|(p, q)| (p.x + q.x - 2.0).abs() < 1e-9 && (p.z - q.z).abs() < 1e-9));
        let turned: Vec<DVec3> = open.iter().rev().copied().collect();
        assert_eq!(mirrored(&turned, false, 0, 1.0), made);
        let square = [DVec3::new(-3.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 4.0), DVec3::new(-3.0, 0.0, 2.0)];
        let made = mirrored(&square, true, 0, 0.0);
        assert!(made.iter().all(|p| made.iter().any(|q| (p.x + q.x).abs() < 1e-9 && (p.z - q.z).abs() < 1e-9)) && made.iter().any(|p| p.x == 3.0));
        // crossing the middle three times, there is no one low side: left as it is
        let wavy = [DVec3::new(-2.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 1.0), DVec3::new(-2.0, 0.0, 2.0), DVec3::new(2.0, 0.0, 3.0)];
        assert_eq!(mirrored(&wavy, false, 0, 0.0), wavy.to_vec());
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
