//! The outside of a build, looked at the way an eye does: which way each
//! exposed face points, against which way the form's surface points there.
//!
//! Volume says how much of a form is built and nothing about how it looks. A
//! slope built as a staircase and the same slope built as one ramp can
//! disagree with the form by the same volume; the staircase is all treads and
//! risers, each pointing tens of degrees away from the surface it stands for,
//! and the ramp is not. So every exposed face of every brick is sampled, and
//! at each sample the face's direction is compared with the form's own (the
//! gradient of its distance function). The report gives how much of the
//! outside is how far off, where the worst faces are, and sections cut
//! through the bricks and the form together so that an outline can be read.
//!
//! Measured from the bricks alone, like `report::analyse`: nothing here knows
//! how the fitters reached their answer.

use crate::shapes::{shape_info, CellBrick, Kind, ShapeInfo};
use crate::spec::{Built, Probe};
use glam::DVec3;
use std::fmt::Write;

/// Samples per cell along a face, each way.
const PER_CELL: usize = 2;

/// A level or upright face this many degrees or more off the form's surface
/// is a step: the tread or riser of a staircase where the form slopes. Faces
/// of a faceted curve are within about 20 degrees of it; a riser on even a
/// gentle slope is 60 or more.
pub const STEP_DEGREES: f64 = 30.0;

/// Upper edges of the bands the outside is sorted into, in degrees.
pub const BANDS: [f64; 5] = [10.0, 20.0, 30.0, 45.0, 180.0];

/// Most cells a survey will index (4 bytes each).
const MAX_CELLS: usize = 16_000_000;

/// One face of one brick, as far as it is exposed.
#[derive(Clone, Copy, Debug)]
pub struct FaceNote {
    pub brick: usize,
    /// the way the face points
    pub normal: DVec3,
    /// exposed area, in cells squared
    pub area: f64,
    /// mean angle between the face and the form's surface, in degrees
    pub angle: f64,
    /// mean distance of the face from the form's surface: positive outside it
    pub gap: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayerNote {
    pub area: f64,
    pub stepped: f64,
    /// the sum of angle times area, for the mean
    turn: f64,
}

impl LayerNote {
    pub fn mean_angle(&self) -> f64 {
        if self.area > 0.0 {
            self.turn / self.area
        } else {
            0.0
        }
    }
}

/// The outside of a build measured against its form.
pub struct Survey {
    /// exposed area, in cells squared
    pub area: f64,
    /// that area by how far off the form's surface it points; see [`BANDS`]
    pub bands: [f64; 5],
    pub mean_angle: f64,
    /// area of level and upright faces [`STEP_DEGREES`] or more off
    pub stepped: f64,
    /// mean distance of the outside from the form's surface, ignoring sign
    pub mean_gap: f64,
    /// every exposed face, most stepped area first
    pub faces: Vec<FaceNote>,
    /// the outside layer by layer up z, from `floor`
    pub layers: Vec<LayerNote>,
    pub floor: i32,
    grid: Grid,
}

/// Which brick holds each cell.
struct Grid {
    min: [i32; 3],
    dims: [usize; 3],
    owner: Vec<u32>,
    /// the cells with more than one brick, and the others in each: half
    /// bricks either side of a diagonal share a box (see `fit_surface`)
    shared: std::collections::HashMap<usize, Vec<u32>>,
}

impl Grid {
    fn new(bricks: &[CellBrick]) -> Result<Grid, String> {
        let min = [0, 1, 2].map(|i| bricks.iter().map(|b| b.min[i]).min().unwrap() - 1);
        let max = [0, 1, 2].map(|i| bricks.iter().map(|b| b.min[i] + b.ext[i]).max().unwrap() + 1);
        let dims = [0, 1, 2].map(|i| (max[i] - min[i]) as usize);
        let count = dims[0] * dims[1] * dims[2];
        if count > MAX_CELLS {
            return Err(format!(
                "the build spans {count} cells, too many to survey (the limit is {MAX_CELLS})"
            ));
        }
        let mut owner = vec![0u32; count];
        let mut shared: std::collections::HashMap<usize, Vec<u32>> = std::collections::HashMap::new();
        for (i, b) in bricks.iter().enumerate() {
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    let row = (b.min[0] - min[0]) as usize
                        + dims[0]
                            * ((b.min[1] + y - min[1]) as usize
                                + dims[1] * (b.min[2] + z - min[2]) as usize);
                    for cell in row..row + b.ext[0] as usize {
                        if owner[cell] == 0 {
                            owner[cell] = i as u32 + 1;
                        } else {
                            shared.entry(cell).or_default().push(i as u32 + 1);
                        }
                    }
                }
            }
        }
        Ok(Grid { min, dims, owner, shared })
    }

    /// The bricks whose boxes hold `p`.
    fn holders(&self, p: DVec3) -> impl Iterator<Item = usize> + '_ {
        let mut at = Some(0usize);
        for i in (0..3).rev() {
            let c = p[i].floor() as i64 - self.min[i] as i64;
            at = at.filter(|_| c >= 0 && c < self.dims[i] as i64).map(|a| a * self.dims[i] + c as usize);
        }
        let first = at.map(|a| self.owner[a]).filter(|o| *o > 0);
        let rest = at.and_then(|a| self.shared.get(&a)).map(|v| v.as_slice()).unwrap_or(&[]);
        first.into_iter().chain(rest.iter().copied()).map(|o| o as usize - 1)
    }

    /// Whether `p` is inside a brick.
    fn solid(&self, bricks: &[CellBrick], p: DVec3) -> bool {
        self.holders(p).any(|h| {
            let b = &bricks[h];
            let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
            shape_info(b.mask).is_some_and(|info| part(info, t) >= 0.5)
        })
    }
}

/// A point of a shape's box measured from the shape's own corner: each
/// coordinate runs 0 to 1 away from it.
fn from_corner(info: ShapeInfo, t: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|k| {
        if info.corner >> k & 1 != 0 {
            1.0 - t[k]
        } else {
            t[k]
        }
    })
}

/// How much of the point `t` of a shape's box (each coordinate 0 to 1) the
/// shape holds: 1 inside, 0 outside, a half exactly on a sloped face.
///
/// The half is for samples of a level face, which line up with the edge
/// where a sloped face meets it: counted all in or all out, a triangular
/// face would measure a quarter or three quarters of its square.
fn part(info: ShapeInfo, t: [f64; 3]) -> f64 {
    if !info.kind.by_corner() {
        let held = crate::shapes::sloped_sides(info.kind, info.local(t)).map(|d| -> f64 { if d.abs() < 1e-5 { 0.5 } else if d < 0.0 { 1.0 } else { 0.0 } });
        // inside both faces; for the inner corner, which is two ramps together, inside either
        return if info.kind == Kind::Inner { held[0].max(held[1]) } else { held[0].min(held[1]) };
    }
    let c = from_corner(info, t);
    let side = |sum: f64, solid_beyond: bool| {
        if (sum - 1.0).abs() < 1e-5 {
            0.5
        } else if (sum > 1.0) == solid_beyond {
            1.0
        } else {
            0.0
        }
    };
    match info.kind {
        Kind::Cube => 1.0,
        Kind::Tet => side(c[0] + c[1] + c[2], false),
        Kind::Cut => side(c[0] + c[1] + c[2], true),
        Kind::Ramp => {
            let (p, q) = [(1, 2), (0, 2), (0, 1)][info.axis as usize];
            side(c[p] + c[q], true)
        }
        // inside both of its sloped faces, each of which is where one
        // coordinate across equals the one along its axis
        Kind::Hip => {
            let (p, q) = [(1, 2), (0, 2), (0, 1)][info.axis as usize];
            let along = c[info.axis as usize];
            let within = |x: f64| -> f64 { if (x - along).abs() < 1e-5 { 0.5 } else if x < along { 1.0 } else { 0.0 } };
            within(c[p]).min(within(c[q]))
        }
        Kind::Inner | Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv => unreachable!("answered above"),
    }
}

/// A sample of a face: where, and what share of the face's area it stands for.
type Sample = (DVec3, f64);

/// The six level and upright faces of a brick: each one's direction and samples.
fn square_faces(b: &CellBrick, info: ShapeInfo) -> Vec<(DVec3, Vec<Sample>)> {
    let mut faces = Vec::new();
    for axis in 0..3 {
        let (p, q) = [(1, 2), (0, 2), (0, 1)][axis];
        for high in [false, true] {
            let (np, nq) = (b.ext[p] as usize * PER_CELL, b.ext[q] as usize * PER_CELL);
            let each = 1.0 / (PER_CELL * PER_CELL) as f64;
            let mut samples = Vec::new();
            for j in 0..nq {
                for i in 0..np {
                    let mut t = [0.0; 3];
                    (t[p], t[q], t[axis]) = (
                        (i as f64 + 0.5) / np as f64,
                        (j as f64 + 0.5) / nq as f64,
                        if high { 1.0 } else { 0.0 },
                    );
                    // just inside the face, to ask whether the shape reaches it here
                    let mut within = t;
                    within[axis] = if high { 1.0 - 1e-7 } else { 1e-7 };
                    let held = part(info, within);
                    if held > 0.0 {
                        let at = DVec3::from_array(
                            [0, 1, 2].map(|k| b.min[k] as f64 + t[k] * b.ext[k] as f64),
                        );
                        samples.push((at, each * held));
                    }
                }
            }
            if !samples.is_empty() {
                let mut normal = DVec3::ZERO;
                normal[axis] = if high { 1.0 } else { -1.0 };
                faces.push((normal, samples));
            }
        }
    }
    faces
}

/// A brick's sloped faces: each one's direction and samples. A hip has two,
/// the other shapes one, a cube none.
fn sloped_faces(b: &CellBrick, info: ShapeInfo) -> Vec<(DVec3, Vec<Sample>)> {
    if !info.kind.by_corner() {
        // The newer kinds: each face a triangle given in the kind's own box,
        // cut into n * n smaller ones of equal area and sampled at the
        // middle of each, as a tetrahedron's is.
        let ext = b.ext.map(|e| e as f64);
        let place = |l: [f64; 3]| {
            let t = info.world(l);
            DVec3::from_array([0, 1, 2].map(|k| b.min[k] as f64 + t[k] * ext[k]))
        };
        let (triangles, within) = crate::shapes::sloped_triangles(info.kind);
        let n = (b.ext.iter().max().unwrap() * PER_CELL as i32).clamp(2, 12) as usize;
        return triangles
            .iter()
            .map(|t| {
                let [o, u, v] = t.map(|l| DVec3::from_array(l));
                let (at, side_u, side_v) = (place(t[0]), place(t[1]) - place(t[0]), place(t[2]) - place(t[0]));
                let mut normal = side_u.cross(side_v);
                let area = normal.length() / 2.0;
                if normal.dot(place(within) - at) > 0.0 {
                    normal = -normal;
                }
                let mut samples = Vec::new();
                for i in 0..n {
                    for j in 0..n - i {
                        for third in [1.0, 2.0] {
                            if third == 2.0 && i + j + 1 >= n {
                                continue;
                            }
                            let (p, q) = ((i as f64 + third / 3.0) / n as f64, (j as f64 + third / 3.0) / n as f64);
                            samples.push((place((o + (u - o) * p + (v - o) * q).to_array()), area / (n * n) as f64));
                        }
                    }
                }
                (normal.normalize(), samples)
            })
            .collect();
    }
    if info.kind != Kind::Hip {
        return sloped_face(b, info).into_iter().collect();
    }
    let ext = b.ext.map(|e| e as f64);
    let axis = info.axis as usize;
    let (p, q) = [(1, 2), (0, 2), (0, 1)][axis];
    // from the apex's corner back to the brick's box: a sign for each axis
    let sign = |k: usize| if info.corner >> k & 1 != 0 { -1.0 } else { 1.0 };
    let place = |c: [f64; 3]| {
        let t = from_corner(info, c);
        DVec3::from_array([0, 1, 2].map(|k| b.min[k] as f64 + t[k] * ext[k]))
    };
    [(p, q), (q, p)]
        .into_iter()
        .map(|(across, level)| {
            // The face where `across` equals the distance along the axis: a
            // right triangle from the apex down to one side of the whole
            // face. It looks outwards along `across` and back towards the
            // apex along the axis.
            let mut normal = DVec3::ZERO;
            normal[across] = sign(across) / ext[across];
            normal[axis] = -sign(axis) / ext[axis];
            let area = 0.5 * ext[across].hypot(ext[axis]) * ext[level];
            // sampled evenly in how far down and how far along; a row's
            // length grows with how far down it is, and so does its share
            let n = (b.ext.iter().max().unwrap() * PER_CELL as i32).clamp(2, 12) as usize;
            let mut samples = Vec::new();
            for i in 0..n {
                for j in 0..n {
                    let (down, along) = ((i as f64 + 0.5) / n as f64, (j as f64 + 0.5) / n as f64);
                    let mut c = [0.0; 3];
                    (c[axis], c[across], c[level]) = (down, down, along * down);
                    samples.push((place(c), area * 2.0 * down / (n * n) as f64));
                }
            }
            (normal.normalize(), samples)
        })
        .collect()
}

/// The sloped face of a ramp, tetrahedron or cut cube: its direction and samples.
fn sloped_face(b: &CellBrick, info: ShapeInfo) -> Option<(DVec3, Vec<Sample>)> {
    let ext = b.ext.map(|e| e as f64);
    // back from the shape's own corner to the brick's box, and out to space
    let place = |c: [f64; 3]| {
        let t = from_corner(info, c);
        DVec3::from_array([0, 1, 2].map(|k| b.min[k] as f64 + t[k] * ext[k]))
    };
    // The face is the plane where the corner coordinates sum to 1, so it
    // points along (1/x, 1/y, 1/z) of the brick's size: away from the corner
    // for a tetrahedron, whose solid is at the corner, and towards it for a
    // ramp or cut cube, whose solid is everything else.
    let outward = |axes: &[usize], away: bool| {
        let mut n = DVec3::ZERO;
        for &k in axes {
            let towards_corner = if info.corner >> k & 1 != 0 { 1.0 } else { -1.0 };
            n[k] = if away {
                -towards_corner
            } else {
                towards_corner
            } / ext[k];
        }
        n.normalize()
    };
    match info.kind {
        Kind::Cube | Kind::Hip | Kind::Inner | Kind::HalfOuter | Kind::HalfInner | Kind::HalfInnerInv => None,
        Kind::Ramp => {
            let axis = info.axis as usize;
            let (p, q) = [(1, 2), (0, 2), (0, 1)][axis];
            let (na, ns) = (
                b.ext[axis] as usize * PER_CELL,
                b.ext[p].max(b.ext[q]) as usize * PER_CELL,
            );
            let area = ext[axis] * ext[p].hypot(ext[q]);
            let mut samples = Vec::new();
            for j in 0..ns {
                for i in 0..na {
                    let mut c = [0.0; 3];
                    let s = (j as f64 + 0.5) / ns as f64;
                    (c[axis], c[p], c[q]) = ((i as f64 + 0.5) / na as f64, s, 1.0 - s);
                    samples.push((place(c), area / (na * ns) as f64));
                }
            }
            Some((outward(&[p, q], false), samples))
        }
        Kind::Tet | Kind::Cut => {
            // a triangle with a corner on each axis: split into n * n smaller
            // ones of equal area and sampled at the middle of each
            let n = (b.ext.iter().max().unwrap() * PER_CELL as i32).clamp(2, 12) as usize;
            let area = 0.5
                * ((ext[0] * ext[1]).powi(2)
                    + (ext[1] * ext[2]).powi(2)
                    + (ext[0] * ext[2]).powi(2))
                .sqrt();
            let mut samples = Vec::new();
            for i in 0..n {
                for j in 0..n - i {
                    for third in [1.0, 2.0] {
                        if third == 2.0 && i + j + 1 >= n {
                            continue;
                        }
                        let (u, v) = (
                            (i as f64 + third / 3.0) / n as f64,
                            (j as f64 + third / 3.0) / n as f64,
                        );
                        samples.push((place([u, v, 1.0 - u - v]), area / (n * n) as f64));
                    }
                }
            }
            Some((outward(&[0, 1, 2], info.kind == Kind::Tet), samples))
        }
    }
}

/// Measure the outside of `built` against the form.
pub fn survey(probe: &Probe, built: &Built) -> Result<Survey, String> {
    if built.bricks.is_empty() {
        return Err("there are no bricks to survey".into());
    }
    let grid = Grid::new(&built.bricks)?;
    let floor = grid.min[2];
    let mut layers = vec![LayerNote::default(); grid.dims[2]];
    let mut faces = Vec::new();
    let mut bands = [0.0; 5];
    let (mut area, mut turn, mut stepped, mut gaps) = (0.0, 0.0, 0.0, 0.0);
    // The form's direction is the slope of its distance function, taken
    // across a fifth of a cell: wide enough to be steady where the function
    // is pieced together from segments, narrow enough to stay on one face.
    let h = 0.1;
    let slope = |p: DVec3| {
        let d = |k: usize| {
            let mut e = DVec3::ZERO;
            e[k] = h;
            probe.distance(p + e) - probe.distance(p - e)
        };
        DVec3::new(d(0), d(1), d(2))
    };
    for (index, b) in built.bricks.iter().enumerate() {
        let info = shape_info(b.mask).ok_or("a brick has a mask that is not a shape")?;
        let square = square_faces(b, info);
        let sloped = sloped_faces(b, info);
        for (is_square, (normal, samples)) in square
            .into_iter()
            .map(|f| (true, f))
            .chain(sloped.into_iter().map(|f| (false, f)))
        {
            let mut note = FaceNote {
                brick: index,
                normal,
                area: 0.0,
                angle: 0.0,
                gap: 0.0,
            };
            let mut face_stepped = 0.0;
            for (at, share) in samples {
                // A level or upright face is hidden where another brick lies
                // against it: asked a hair beyond the face, and a sample on
                // the very edge of the brick there counts as covered, since
                // two triangular faces that match meet edge to edge. A sloped
                // face looks into its own brick's box, which no other brick
                // may enter.
                if is_square && grid.solid(&built.bricks, at + normal * 1e-7) {
                    continue;
                }
                let g = slope(at);
                let gap = probe.distance(at);
                // far from the form altogether, or in the flat middle of its
                // distance function: no direction to compare with
                let angle = if g.is_finite() && g.length() > 1e-6 {
                    normal
                        .dot(g.normalize())
                        .clamp(-1.0, 1.0)
                        .acos()
                        .to_degrees()
                } else {
                    90.0
                };
                note.area += share;
                note.angle += angle * share;
                note.gap += if gap.is_finite() { gap } else { 0.0 } * share;
                gaps += if gap.is_finite() { gap.abs() } else { 0.0 } * share;
                bands[BANDS.iter().position(|edge| angle < *edge).unwrap_or(4)] += share;
                let step = if is_square && angle >= STEP_DEGREES {
                    share
                } else {
                    0.0
                };
                face_stepped += step;
                let layer = &mut layers[((at.z - 1e-9).floor() as i32 - floor)
                    .clamp(0, grid.dims[2] as i32 - 1)
                    as usize];
                layer.area += share;
                layer.turn += angle * share;
                layer.stepped += step;
            }
            if note.area > 0.0 {
                area += note.area;
                turn += note.angle;
                stepped += face_stepped;
                note.angle /= note.area;
                note.gap /= note.area;
                faces.push((face_stepped, note));
            }
        }
    }
    faces.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(b.1.angle.total_cmp(&a.1.angle))
            .then(a.1.brick.cmp(&b.1.brick))
    });
    let faces = faces.into_iter().map(|f| f.1).collect();
    Ok(Survey {
        area,
        bands,
        mean_angle: turn / area.max(1e-9),
        stepped,
        mean_gap: gaps / area.max(1e-9),
        faces,
        layers,
        floor,
        grid,
    })
}

impl Survey {
    /// The `[surface]` part of a report.
    pub fn describe(&self, built: &Built, out: &mut String) {
        let pct = |a: f64| 100.0 * a / self.area.max(1e-9);
        writeln!(
            out,
            "[surface]  every exposed face, against the way the form's surface points there"
        )
        .unwrap();
        writeln!(out, "exposed area = {:.1} cells squared", self.area).unwrap();
        writeln!(
            out,
            "mean angle off the form = {:.1} degrees; mean distance from it = {:.2} cells",
            self.mean_angle, self.mean_gap
        )
        .unwrap();
        let mut from = 0.0;
        for (edge, area) in BANDS.iter().zip(self.bands) {
            let label = if *edge > 90.0 {
                format!("{from:.0} degrees or more")
            } else {
                format!("{from:.0} to {edge:.0} degrees")
            };
            writeln!(out, "  {label}: {:.1} ({:.1}%)", area, pct(area)).unwrap();
            from = *edge;
        }
        writeln!(out, "stepped = {:.1} ({:.1}%): level and upright faces {STEP_DEGREES:.0} degrees or more off, the treads and risers of a staircase", self.stepped, pct(self.stepped)).unwrap();

        writeln!(
            out,
            "\n[surface by layer]  z | exposed | stepped | mean angle"
        )
        .unwrap();
        for (i, layer) in self.layers.iter().enumerate().filter(|l| l.1.area > 0.0) {
            writeln!(
                out,
                "{} | {:.1} | {:.1} | {:.1}",
                self.floor + i as i32,
                layer.area,
                layer.stepped,
                layer.mean_angle()
            )
            .unwrap();
        }

        writeln!(out, "\n[steps]  the faces furthest off: brick | shape, min, size | face points | exposed | degrees off | distance out").unwrap();
        for f in self
            .faces
            .iter()
            .filter(|f| f.angle >= STEP_DEGREES)
            .take(40)
        {
            let b = &built.bricks[f.brick];
            let kind =
                shape_info(b.mask).map_or("?".into(), |s| format!("{:?}", s.kind).to_lowercase());
            writeln!(
                out,
                "{} | {kind} {:?} {:?} | {:.2} {:.2} {:.2} | {:.1} | {:.0} | {:.2}",
                f.brick, b.min, b.ext, f.normal.x, f.normal.y, f.normal.z, f.area, f.angle, f.gap
            )
            .unwrap();
        }
    }

    /// A cut through the bricks and the form across `axis` at `at`.
    ///
    /// Sampled finer than the cells, so that a sloped face shows as a slope
    /// and a step as a step: an outline can be read off it, and where it
    /// leaves the form's.
    pub fn section(&self, probe: &Probe, built: &Built, axis: usize, at: f64, out: &mut String) {
        let (ua, va) = crate::spec::plane_axes(axis);
        let names = ["x", "y", "z"];
        let (nu, nv) = (self.grid.dims[ua], self.grid.dims[va]);
        // four characters to a cell if that fits a line, else two
        let across = if nu * 4 <= 200 { 4 } else { 2 };
        if nu * across > 240 {
            writeln!(out, "[section {}={at}] too wide to draw", names[axis]).unwrap();
            return;
        }
        writeln!(out, "[section {}={at}]  {} left to right from {}, {} bottom to top from {}; {across} characters to a cell across, 2 up", names[axis], names[ua], self.grid.min[ua], names[va], self.grid.min[va]).unwrap();
        writeln!(out, "'#': brick and form. 'O': brick outside the form. '!': form with no brick. '.': neither.").unwrap();
        for row in (0..nv * 2).rev() {
            let line: String = (0..nu * across)
                .map(|col| {
                    let mut p = DVec3::ZERO;
                    (p[ua], p[va], p[axis]) = (
                        self.grid.min[ua] as f64 + (col as f64 + 0.5) / across as f64,
                        self.grid.min[va] as f64 + (row as f64 + 0.5) / 2.0,
                        at,
                    );
                    match (self.grid.solid(&built.bricks, p), probe.distance(p) < 0.0) {
                        (true, true) => '#',
                        (true, false) => 'O',
                        (false, true) => '!',
                        (false, false) => '.',
                    }
                })
                .collect();
            writeln!(out, "  {line}").unwrap();
        }
        writeln!(out).unwrap();
    }

    /// The box of cells the bricks lie in, with a cell to spare all round.
    pub fn bounds(&self) -> ([i32; 3], [usize; 3]) {
        (self.grid.min, self.grid.dims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{all_masks, CUBE};
    use crate::spec::{build, Form, Spec};

    fn sphere(radius: f64, wedges: bool) -> Spec {
        Spec {
            form: Form::Sphere,
            points: vec![DVec3::splat(0.5)],
            radius,
            fill: true,
            wedges,
            ..Spec::default()
        }
    }

    #[test]
    fn every_shape_has_the_surface_area_of_its_solid() {
        // a lone brick is all exposed: its faces' areas must add up to the
        // shape's true surface, whatever its size
        for mask in all_masks() {
            let b = CellBrick {
                mask,
                min: [3, -2, 5],
                ext: [2, 3, 4],
            };
            let info = shape_info(mask).unwrap();
            let total: f64 = square_faces(&b, info)
                .iter()
                .chain(sloped_faces(&b, info).iter())
                .flat_map(|f| f.1.iter().map(|s| s.1))
                .sum();
            let [x, y, z] = b.ext.map(|e| e as f64);
            let (p, q, a) = match info.axis {
                0 => (y, z, x),
                1 => (x, z, y),
                _ => (x, y, z),
            };
            let slope = 0.5 * ((x * y).powi(2) + (y * z).powi(2) + (x * z).powi(2)).sqrt();
            // the brick's size along each axis of its kind's own box
            let perm = crate::shapes::ORIENT[info.pose as usize].perm;
            let mut own = [0.0; 3];
            for i in 0..3 {
                own[perm[i] as usize] = b.ext[i] as f64;
            }
            let [lx, ly, lz] = own;
            let expected = match info.kind {
                // floor, two whole walls, two triangular ones, two slopes
                Kind::Inner => lx * ly + 1.5 * lz * (lx + ly) + 0.5 * (lx * ly.hypot(lz) + ly * lx.hypot(lz)),
                // floor and two walls, all triangles; the upright long side; the slope
                Kind::HalfOuter => 0.5 * (lx * ly + ly * lz + lx * lz) + lz * lx.hypot(ly) + slope,
                Kind::HalfInner => 0.5 * (lx * ly + ly * lz + lz * lx.hypot(ly) + lx * ly.hypot(lz)),
                Kind::HalfInnerInv => 0.5 * (lx * ly + lx * lz + lz * lx.hypot(ly) + ly * lx.hypot(lz)),
                Kind::Cube => 2.0 * (x * y + y * z + x * z),
                // two triangular ends, two legs and the slope
                Kind::Ramp => p * q + a * (p + q) + a * p.hypot(q),
                Kind::Tet => 0.5 * (x * y + y * z + x * z) + slope,
                Kind::Cut => 1.5 * (x * y + y * z + x * z) + slope,
                // its whole face, two upright triangles and two sloped ones
                Kind::Hip => p * q + 0.5 * a * (p + q) + 0.5 * (p.hypot(a) * q + q.hypot(a) * p),
            };
            assert!(
                (total - expected).abs() < 1e-6 * expected,
                "mask {mask:#x}: {total} vs {expected}"
            );
            // and every sample of its sloped face lies on the solid's edge
            for (normal, samples) in sloped_faces(&b, info) {
                let grid = Grid::new(&[b]).unwrap();
                for (at, _) in samples {
                    assert!(
                        grid.solid(&[b], at - normal * 0.01),
                        "mask {mask:#x}: {at} is not on the solid"
                    );
                    assert!(
                        !grid.solid(&[b], at + normal * 0.01),
                        "mask {mask:#x}: {at} is inside the solid"
                    );
                }
            }
        }
    }

    #[test]
    fn a_staircase_is_stepped_and_a_fit_with_slopes_much_less() {
        let measure = |spec: &Spec| {
            let built = build(spec).unwrap();
            let s = survey(&Probe::new(spec, built.grain).unwrap(), &built).unwrap();
            // a sphere of radius 9 has a surface of 1018
            assert!((s.area / 1018.0 - 1.0).abs() < 0.6, "area {}", s.area);
            s
        };
        let (cubes, wedges) = (measure(&sphere(9.0, false)), measure(&sphere(9.0, true)));
        assert!(
            cubes.stepped / cubes.area > 0.3,
            "cubes: {} of {}",
            cubes.stepped,
            cubes.area
        );
        assert!(
            wedges.stepped < cubes.stepped * 0.8,
            "wedges {} vs cubes {}",
            wedges.stepped,
            cubes.stepped
        );
        assert!(wedges.mean_angle < cubes.mean_angle);
        // a cube's faces are square to itself: nothing is off at all
        let cube = Spec {
            form: Form::Spline,
            points: vec![DVec3::splat(0.5), DVec3::new(5.5, 0.5, 0.5)],
            thick: 3.0,
            depth: 3,
            ..Spec::default()
        };
        let built = build(&cube).unwrap();
        assert!(built.bricks.iter().all(|b| b.mask == CUBE));
        let s = survey(&Probe::new(&cube, 1).unwrap(), &built).unwrap();
        assert_eq!(s.stepped, 0.0);
        assert!(s.mean_angle < 3.0, "{}", s.mean_angle);
    }
}
