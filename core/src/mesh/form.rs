//! A mesh as a form the editor builds: what a save keeps of it
//! ([`Import`]), the mesh stood in the lattice at a size ([`Stood`]), and
//! its bricks.
//!
//! The bricks come from the scanned route of [`super::trial`], which the
//! measurements in docs/algorithm.md (section 7.13) chose: the cells the
//! surface crosses and their samples from the triangles ([`super::scan`]),
//! stretched pieces laid where the triangles face their way, and the rest
//! joined into boxes. This file adds what a form needs round that: the
//! inside filled or left as a shell, loose pieces dropped, a colour for
//! each brick, and the settings every form has.

use super::colour::nearest_in;
use super::scan::scan;
use super::trial::{guided, Stand, PAD};
use super::{Count, Mesh, MeshError};
use crate::fit_volume::{drop_spikes, merge, patterns, K};
use crate::par::Threads;
use crate::shapes::{shape_info, CellBrick, Kind, CUBE};
use glam::DVec3;
use std::sync::Arc;

/// Which way is up in the file. Brickadia's up is z. glTF says y is up
/// (<https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units>),
/// and most OBJ files follow it; a PLY file says nothing, and programs
/// that write z-up meshes (Blender's own space, most scanners) are as
/// common as those that do not.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Up {
    #[default]
    Y,
    Z,
    X,
}

/// Each way up, with the word a save has for it.
pub const UPS: [(Up, &str); 3] = [(Up::Y, "y"), (Up::Z, "z"), (Up::X, "x")];

impl Up {
    /// Which way is up in a file of this name, as far as its kind says: y,
    /// but for a PLY file, which says nothing and is more often a scan or
    /// an export with z up.
    pub fn usual_for(name: &str) -> Up {
        if name.to_ascii_lowercase().ends_with(".ply") { Up::Z } else { Up::Y }
    }

    /// A point of the file as it stands with z up. Each is a turn and
    /// never a mirror image, so the mesh keeps its handedness and its
    /// triangles keep facing outwards.
    pub fn stood(self, [x, y, z]: [f32; 3]) -> [f32; 3] {
        match self {
            // a quarter turn about x: the file's front (its +z) comes to face -y
            Up::Y => [x, -z, y],
            Up::Z => [x, y, z],
            // each axis moved on one place
            Up::X => [y, z, x],
        }
    }
}

/// How a mesh's bricks are chosen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Fitter {
    /// stretched ramps and corner pieces lying in the mesh's faces
    #[default]
    Smoothed,
    /// cubes alone, joined into boxes
    Blocks,
}

pub const FITTERS: [(Fitter, &str); 2] = [(Fitter::Smoothed, "smoothed"), (Fitter::Blocks, "blocks")];

/// The largest piece that may be dropped as loose, in microbricks along
/// its longest side, and the most colours a mesh's bricks may be given
/// (the editor's `paint::MOST_COLOURS`).
pub const MOST_LOOSE: i32 = 200;
pub const MOST_COLOURS: i32 = 32;
/// How many colours a mesh starts with. Chosen by looking at the figure
/// of `made::figure` built at 4, 8, 12, 16 and 32 (the pictures are in
/// docs/images/mesh). At 4 its skin and the grass under it are one colour
/// and its hair is the colour of its shirt. At 8 every part has a colour
/// of its own. At 12 and at 16 each part has two or three shades, light to
/// dark, and the two look alike. At 32 its eyes, which are a few bricks,
/// get a colour that was the hair's until then. Bricks rise with colours,
/// since a brick is one colour: 3,264, 3,444, 3,572, 3,730 and 3,940. So
/// 12: the fewest at which every part is shaded, for a tenth more bricks
/// than 4.
pub const USUAL_COLOURS: i32 = 12;

/// The mesh a `Form::Imported` is built from, and how.
///
/// The mesh itself is not part of what a save keeps, nor of whether two
/// forms are the same: `name` is, and a name is made from the file's
/// content (see the editor's `import::adopt`), so two forms of one name
/// are forms of one mesh. `mesh` is filled in by whoever holds the files
/// (the editor's `import::attach`), and a form whose mesh could not be
/// found is refused when it is built, with [`MeshError::Missing`].
#[derive(Clone, Debug, Default)]
pub struct Import {
    /// The mesh's file in the imports folder, or empty for none yet.
    /// Always of the shape [`Import::named`] allows.
    pub name: String,
    pub mesh: Option<Arc<Mesh>>,
    pub up: Up,
    pub fitter: Fitter,
    /// Pieces of the build smaller than this along their longest side, in
    /// microbricks, are left out. 0 keeps everything.
    pub loose: i32,
    /// How many colours the mesh's own are reduced to. 0 for none: the
    /// form is then coloured as any other is.
    pub colours: i32,
}

impl PartialEq for Import {
    fn eq(&self, other: &Import) -> bool {
        (&self.name, self.up, self.fitter, self.loose, self.colours) == (&other.name, other.up, other.fitter, other.loose, other.colours)
    }
}

impl Import {
    /// Whether `name` may be the name of an imported mesh: sixteen
    /// hexadecimal digits, a full stop, and one of the three endings.
    ///
    /// A name reaches a form from a save or from a program driving the
    /// editor, and becomes part of a file's path. Held to this shape it
    /// cannot name anything outside the imports folder: it has no
    /// separator, no `..`, no drive and no device name in it. Checking
    /// for those one by one instead would be a list to keep complete.
    pub fn named(name: &str) -> Result<(), MeshError> {
        let fits = name.split_once('.').is_some_and(|(hash, kind)| hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) && matches!(kind, "glb" | "obj" | "ply"));
        if fits { Ok(()) } else { Err(MeshError::NotAnImport { name: name.chars().take(80).collect() }) }
    }

    /// The same, with every number brought within what it may be.
    pub fn clamped(self) -> Import {
        Import { loose: self.loose.clamp(0, MOST_LOOSE), colours: self.colours.clamp(0, MOST_COLOURS), ..self }
    }

    /// As a save has it: the name (or `none`), which way is up, the
    /// fitter, the size under which loose pieces are dropped, and how many
    /// colours.
    pub fn text(&self) -> String {
        let word = |words: &[(u8, &'static str)], which: u8| words.iter().find(|w| w.0 == which).map_or("", |w| w.1);
        let (ups, fitters) = (UPS.map(|(up, name)| (up as u8, name)), FITTERS.map(|(fitter, name)| (fitter as u8, name)));
        format!("{},{},{},{},{}", if self.name.is_empty() { "none" } else { &self.name }, word(&ups, self.up as u8), word(&fitters, self.fitter as u8), self.loose, self.colours)
    }

    /// The reverse of [`Import::text`]: the name, and then as many of the
    /// rest as are given. The mesh is not found here (see [`Import`]).
    pub fn read(text: &str) -> Result<Import, MeshError> {
        let bad = |why: &str| MeshError::Malformed { kind: "import setting", why: why.to_string() };
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        if parts.len() > 5 {
            return Err(bad("expected `name,up,fitter,loose,colours`, or the first few of them"));
        }
        let mut import = Import { colours: USUAL_COLOURS, ..Import::default() };
        if parts[0] != "none" {
            Import::named(parts[0])?;
            import.name = parts[0].to_string();
            // (which is what it is opened with in the editor, where nothing else is said)
            import.up = Up::usual_for(parts[0]);
        }
        if let Some(up) = parts.get(1) {
            import.up = UPS.iter().find(|u| u.1 == *up).ok_or_else(|| bad("up is x, y or z"))?.0;
        }
        if let Some(fitter) = parts.get(2) {
            import.fitter = FITTERS.iter().find(|f| f.1 == *fitter).ok_or_else(|| bad("the fitter is smoothed or blocks"))?.0;
        }
        let whole = |text: &str| text.parse::<i64>().map(|v| v.clamp(0, 100_000) as i32).map_err(|_| bad("expected a whole number"));
        if let Some(loose) = parts.get(3) {
            import.loose = whole(loose)?;
        }
        if let Some(colours) = parts.get(4) {
            import.colours = whole(colours)?;
        }
        Ok(import.clamped())
    }
}

/// A mesh stood in the lattice, ready to be built or asked where it is.
pub struct Stood {
    mesh: Arc<Mesh>,
    /// how long its longest side is, in cells
    size: f64,
    /// the cell the cells fitted begin at: [`PAD`] below the low corner of the mesh's box
    low: [i32; 3],
    fitter: Fitter,
    loose: i32,
    colours: usize,
    /// `None` filled; or how thick its shell is, in cells
    shell: Option<f64>,
}

/// What [`Stood::built`] gives: the bricks, the number of each one's
/// colour (empty if the mesh has none), and the volume of the form and of
/// where the bricks disagree with it, in cells.
pub struct Laid {
    pub bricks: Vec<CellBrick>,
    pub shades: Vec<u8>,
    pub solid: f64,
    pub error: f64,
    /// how long each part took, in milliseconds
    pub stages: Vec<(&'static str, f64)>,
}

impl Stood {
    /// The mesh of `import` standing on `foot`, which is the middle of the
    /// underside of its box, `radius` from the middle of its box to each
    /// end of its longest side; filled or (with `shell` set) hollow with
    /// walls so thick.
    ///
    /// It stands on its foot and not about its middle so that making it
    /// larger leaves it standing where it stood: about its middle, as it
    /// was first written, a figure made half as tall again sank a quarter
    /// of its height into the floor.
    ///
    /// The low corner of the mesh's box is put on a lattice plane along
    /// each axis, the nearest to where `foot` asks (and the one at or
    /// below it, upwards): a mesh drawn in whole units, at a size equal to
    /// its longest side, then has every one of its flat faces on the
    /// lattice, wherever it is dragged.
    pub fn new(import: &Import, foot: DVec3, radius: f64, shell: Option<f64>) -> Result<Stood, MeshError> {
        let Some(mesh) = import.mesh.clone() else {
            return Err(if import.name.is_empty() { MeshError::NoFile } else { MeshError::Missing { name: import.name.clone() } });
        };
        let size = (2.0 * radius).max(1.0);
        let low = [0, 1, 2].map(|k| if k == 2 { foot.z.floor() as i32 } else { (foot[k] - mesh.half[k] * size).round() as i32 } - PAD as i32);
        let stood = Stood { mesh, size, low, fitter: import.fitter, loose: import.loose, colours: import.colours.max(0) as usize, shell };
        let cells: u64 = stood.stand().cells().iter().map(|v| *v as u64).product();
        if cells > crate::spec::MAX_CELLS as u64 {
            return Err(MeshError::TooMany { what: Count::Cells, found: cells, most: crate::spec::MAX_CELLS as u64 });
        }
        Ok(stood)
    }

    fn stand(&self) -> Stand<'_> {
        Stand { mesh: &self.mesh, size: self.size }
    }

    /// The cells it is fitted in: the low corner and the high.
    pub fn bounds(&self) -> ([i32; 3], [i32; 3]) {
        let n = self.stand().cells();
        (self.low, [0, 1, 2].map(|k| self.low[k] + n[k]))
    }

    /// Distance from the form's surface at `p`, in cells, negative inside.
    pub fn distance(&self, p: DVec3) -> f64 {
        let d = self.stand().distance(p - DVec3::from_array(self.low.map(|v| v as f64)));
        // a shell is what is inside the mesh and not deeper in than its wall
        self.shell.map_or(d, |thick| d.max(-d - thick))
    }

    /// What a build of it measures as against the form itself, from its
    /// bricks alone (see [`super::trial::measure`]).
    pub fn measured(&self, laid: &Laid, threads: Threads) -> super::trial::Measured {
        let low = DVec3::from_array(self.low.map(|v| v as f64));
        let local: Vec<CellBrick> = laid.bricks.iter().map(|b| CellBrick { min: [0, 1, 2].map(|k| b.min[k] - self.low[k]), ..*b }).collect();
        super::trial::measure(&|p| self.distance(p + low), self.stand().cells().map(|v| v as usize), &local, threads)
    }

    /// The share of the mesh's surface that a build of it leaves bare:
    /// with no brick in the cell a point of the surface is in, nor in any
    /// of the 26 round that cell. Four points to a triangle, each standing
    /// for a quarter of its area.
    ///
    /// The volume where bricks and mesh disagree says little about a thin
    /// part: a sheet half a cell thick is a small volume whether it is
    /// built or missing. This says whether it is there.
    pub fn bare(&self, laid: &Laid) -> f64 {
        let stand = self.stand();
        let n = stand.cells().map(|v| v as usize);
        let mut held = vec![0u64; (n[0] * n[1] * n[2]).div_ceil(64)];
        for b in &laid.bricks {
            for i in 0..(b.ext[0] * b.ext[1] * b.ext[2]) as usize {
                let (ex, ey) = (b.ext[0] as usize, b.ext[1] as usize);
                let at = [(b.min[0] - self.low[0]) as usize + i % ex, (b.min[1] - self.low[1]) as usize + i / ex % ey, (b.min[2] - self.low[2]) as usize + i / (ex * ey)];
                let cell = at[0] + n[0] * (at[1] + n[1] * at[2]);
                held[cell / 64] |= 1 << (cell % 64);
            }
        }
        let near = |p: DVec3| {
            let at = p.floor().to_array().map(|v| v as i64);
            (0..27).any(|round| {
                let to = [at[0] + round % 3 - 1, at[1] + round / 3 % 3 - 1, at[2] + round / 9 - 1];
                (0..3).all(|k| to[k] >= 0 && to[k] < n[k] as i64) && {
                    let cell = to[0] as usize + n[0] * (to[1] as usize + n[1] * to[2] as usize);
                    held[cell / 64] >> (cell % 64) & 1 == 1
                }
            })
        };
        let (mut all, mut bare) = (0.0, 0.0);
        for tri in 0..self.mesh.triangles() {
            let ([a, b, c], _) = self.mesh.triangle(tri);
            let area = (b - a).cross(c - a).length();
            for share in [[1.0, 1.0, 1.0], [4.0, 1.0, 1.0], [1.0, 4.0, 1.0], [1.0, 1.0, 4.0]] {
                let p = (a * share[0] + b * share[1] + c * share[2]) / (share[0] + share[1] + share[2]);
                all += area;
                if !near(stand.from_mesh(p)) {
                    bare += area;
                }
            }
        }
        bare / all.max(1e-300)
    }

    /// The colours its bricks are numbered by (see [`Laid::shades`]).
    pub fn palette(&self) -> Vec<[u8; 3]> {
        self.mesh.palette(self.colours)
    }

    /// Its bricks. `wedges` off builds it of cubes whatever its fitter;
    /// `price` is what a brick is worth in cells of disagreement (see
    /// [`guided`]).
    pub fn built(&self, wedges: bool, max_ext: i32, price: f64, threads: Threads) -> Laid {
        let stand = self.stand();
        let n = stand.cells().map(|v| v as usize);
        let max_ext = max_ext.max(1) as usize;
        let smooth = wedges && self.fitter == Fitter::Smoothed;
        let found = scan(&stand, n, smooth, threads);
        let (mut shape, mut sampled, mut facing, mut nearest, kept, mut stages) = (found.shape, found.sampled, found.facing, found.nearest, found.kept, found.stages);
        let (mut solid, mut error) = (found.solid as i64, found.error as i64);
        let mut lap = super::trial::clock();
        if smooth {
            error += drop_spikes(&mut shape, &sampled, n);
        }
        let mut unit = [0u64; 256];
        for &(mask, pattern) in patterns() {
            unit[mask as usize] = pattern;
        }
        let per_cell = (K * K * K) as i64;
        // which cells the surface crosses, a bit each
        let mut crossed = vec![0u64; shape.len().div_ceil(64)];
        for &(cell, _) in &sampled {
            crossed[cell / 64] |= 1 << (cell % 64);
        }
        if self.loose > 0 {
            let part = |i: usize| shape[i] != 0 || crossed[i / 64] >> (i % 64) & 1 == 1;
            let mut gone = loose(&part, n, self.loose as usize, None);
            // And the pieces that hang on the rest by a neck (see `necked`): what is left once the loose pieces are out,
            // less its outermost layer of cells, falls into the body and whatever the necks held.
            let mut left = vec![0u64; shape.len().div_ceil(64)];
            for i in (0..shape.len()).filter(|i| part(*i)) {
                left[i / 64] |= 1 << (i % 64);
            }
            for &(first, last) in &gone {
                (first..last).for_each(|i| left[i / 64] &= !(1 << (i % 64)));
            }
            gone.extend(necked(&left, n, self.loose as usize));
            gone.sort_unstable();
            if !gone.is_empty() {
                // what was dropped is no part of the form any more: neither its volume nor what was wrong with it
                let mut keep = vec![true; sampled.len()];
                for &(first, last) in &gone {
                    solid -= per_cell * (first..last).filter(|i| shape[*i] == CUBE && crossed[i / 64] >> (i % 64) & 1 == 0).count() as i64;
                    // (the cells the surface crosses are in the order of their numbers: see `scan`)
                    for k in sampled.partition_point(|s| s.0 < first)..sampled.partition_point(|s| s.0 < last) {
                        let (cell, bits) = sampled[k];
                        solid -= bits.count_ones() as i64;
                        error -= (bits ^ unit[shape[cell] as usize]).count_ones() as i64;
                        crossed[cell / 64] &= !(1 << (cell % 64));
                        keep[k] = false;
                    }
                    shape[first..last].fill(0);
                }
                let mut kept = keep.iter();
                sampled.retain(|_| *kept.next().unwrap_or(&true));
                let mut kept = keep.iter();
                facing.retain(|_| *kept.next().unwrap_or(&true));
                let mut kept = keep.iter();
                nearest.retain(|_| *kept.next().unwrap_or(&true));
            }
            stages.push(("loose pieces dropped", lap()));
        }
        if let Some(thick) = self.shell {
            solid -= per_cell * hollowed(&mut shape, &crossed, n, thick) as i64;
            stages.push(("hollowed", lap()));
        }
        let mut bricks = Vec::new();
        if smooth {
            let surface = |k: usize, _: DVec3| facing[k];
            let (stretched, saved, sought, laid) = guided(&surface, &mut shape, &sampled, n, max_ext, (price.max(0.0) * per_cell as f64).round().min(1e6) as u32, &kept, threads);
            error -= saved;
            bricks = stretched;
            stages.push(("stretched pieces found", sought));
            stages.push(("stretched pieces laid", laid));
            lap();
        }
        let palette = self.palette();
        let mut shades = Vec::new();
        if palette.is_empty() {
            bricks.extend(merge(shape, n, max_ext));
        } else {
            // the colour number of the surface nearest a place, in cells
            let shade_at = |p: DVec3| -> u8 {
                let q = stand.to_mesh(p);
                self.mesh.nearest(q, f64::INFINITY).and_then(|hit| self.mesh.colour(hit.tri, hit.point)).map_or(0, |colour| nearest_in(&palette, colour))
            };
            // (a stretched piece: about 4 microseconds each, a search of the hierarchy)
            shades = threads.map(bricks.len(), 200, |b| shade_at(DVec3::from_array([0, 1, 2].map(|k| bricks[b].min[k] as f64 + bricks[b].ext[k] as f64 / 2.0))));
            let tones = tones(&shape, &sampled, &facing, &nearest, n, &stand, &palette, &shade_at, threads);
            let (boxes, of) = merge_coloured(shape, tones, n, max_ext, &shade_at);
            bricks.extend(boxes);
            shades.extend(of);
            stages.push(("coloured", lap()));
        }
        stages.push(("boxes joined", lap()));
        for b in &mut bricks {
            b.min = [0, 1, 2].map(|k| b.min[k] + self.low[k]);
        }
        Laid { bricks, shades, solid: solid as f64 / per_cell as f64, error: error as f64 / per_cell as f64, stages }
    }
}

/// What a cell that no colour need be chosen for is marked with: it may
/// be joined to a box of any colour.
const ANY: u8 = 255;

/// The runs of cells, as first and one past last along x, that belong to
/// pieces of the build smaller than `under` cells along their longest
/// side. In order.
///
/// A piece is cells that hold brick or that the surface crosses, joined
/// through their faces. Pieces are found a run of cells at a time and not
/// a cell at a time: a run is a stretch of such cells along x, runs in
/// neighbouring rows that overlap are one piece, and which piece each run
/// belongs to is kept in a union-find
/// (<https://en.wikipedia.org/wiki/Disjoint-set_data_structure>). A flood
/// fill from cell to cell would hold a place in its queue for every cell
/// of the largest piece, which is the whole inside of the model: tens of
/// millions for a large solid one, where its runs are a few hundred
/// thousand.
///
/// Pieces are judged as built and not as modelled. A model's loose scraps
/// are often several shells each, and a scrap that touches the body by one
/// triangle is one mesh with it; what the eye sees as a floating piece is
/// bricks that touch no others.
///
/// `part` says which cells are occupied. With `cores` given, the pieces are
/// what is left of pieces with a layer taken off them (see [`necked`]):
/// the piece of the most cells is then never among those returned, nor is
/// one of fewer cells than the first number, and each is judged as longer
/// than it is by the second, which is the layer put back.
fn loose(part: &dyn Fn(usize) -> bool, n: [usize; 3], under: usize, cores: Option<(u64, u32)>) -> Vec<(usize, usize)> {
    // every run: its row, and where it begins and ends along x
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    let mut first_of_row = Vec::with_capacity(n[1] * n[2] + 1);
    for row in 0..n[1] * n[2] {
        first_of_row.push(runs.len() as u32);
        let mut x = 0;
        while x < n[0] {
            if !part(row * n[0] + x) {
                x += 1;
                continue;
            }
            let end = (x..n[0]).find(|i| !part(row * n[0] + i)).unwrap_or(n[0]);
            runs.push((row as u32, x as u32, end as u32));
            x = end;
        }
    }
    first_of_row.push(runs.len() as u32);
    let mut parent: Vec<u32> = (0..runs.len() as u32).collect();
    fn root(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            // (each looked at is pointed at its grandparent, which keeps the trees shallow)
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }
    for i in 0..runs.len() {
        let (row, from, to) = runs[i];
        let (y, z) = (row as usize % n[1], row as usize / n[1]);
        // the rows before it along y and along z: the ones after are joined from their own side
        for other in [(y > 0).then(|| row as usize - 1), (z > 0).then(|| row as usize - n[1])].into_iter().flatten() {
            for j in first_of_row[other]..first_of_row[other + 1] {
                let (_, a, b) = runs[j as usize];
                if a < to && from < b {
                    let (one, two) = (root(&mut parent, i as u32), root(&mut parent, j));
                    // (the lower number is the root, so that the pieces are the same whatever order they were met in)
                    parent[one.max(two) as usize] = one.min(two);
                }
            }
        }
    }
    // each piece's box, kept at its root
    let mut boxes: Vec<([u32; 3], [u32; 3])> = vec![([u32::MAX; 3], [0; 3]); runs.len()];
    for i in 0..runs.len() {
        let (row, from, to) = runs[i];
        let r = root(&mut parent, i as u32) as usize;
        let (lo, hi) = ([from, row % n[1] as u32, row / n[1] as u32], [to, row % n[1] as u32 + 1, row / n[1] as u32 + 1]);
        boxes[r] = ([0, 1, 2].map(|k| boxes[r].0[k].min(lo[k])), [0, 1, 2].map(|k| boxes[r].1[k].max(hi[k])));
    }
    // (how many cells each piece is, for the one that is spared)
    let mut cells = vec![0u64; runs.len()];
    for i in 0..runs.len() {
        cells[parent[i] as usize] += (runs[i].2 - runs[i].1) as u64;
    }
    let largest = (0..runs.len()).max_by_key(|i| (cells[*i], std::cmp::Reverse(*i))).filter(|_| cores.is_some());
    let (least, more) = cores.unwrap_or((0, 0));
    (0..runs.len())
        .filter(|i| {
            let piece = parent[*i] as usize;
            let (lo, hi) = boxes[piece];
            let sides = [0, 1, 2].map(|k| hi[k] - lo[k]);
            // (a piece with its layer off is a lump only if it is still two cells through every way: see `necked`)
            Some(piece) != largest && cells[piece] >= least && (cores.is_none() || sides.iter().all(|side| *side >= 2)) && sides.iter().max().unwrap_or(&0) + more < under as u32
        })
        .map(|i| (runs[i].0 as usize * n[0] + runs[i].1 as usize, runs[i].0 as usize * n[0] + runs[i].2 as usize))
        .collect()
}

/// The cells, as runs along x, of the pieces smaller than `under` that are
/// joined to the rest of the build only by a neck two cells through or
/// less. `left` has a bit for each occupied cell.
///
/// A model made from one picture has scraps of ground and stray lumps
/// that are not loose: each hangs on the model by a thread. With the
/// outermost layer of cells taken off everything, a neck of two cells or
/// fewer is gone, and what it joined falls apart into the body, which is
/// the piece of the most cells, and the rest. Each of the rest that is
/// smaller than `under` is dropped, with the layer that was taken off it
/// (the cells within two steps of it that are in no piece themselves).
/// A part that is thin all along, a chair's leg or a sheet, is no piece
/// by this rule: with its outer layer off there is nothing left of it to
/// be one, and it stays with whatever it is joined to.
///
/// This is opening, in the words of mathematical morphology (Serra, Image
/// Analysis and Mathematical Morphology, 1982), done one layer deep. A
/// deeper cut would part thicker necks, and a snowman's head from its
/// body.
fn necked(left: &[u64], n: [usize; 3], under: usize) -> Vec<(usize, usize)> {
    let strides = [1, n[0], n[0] * n[1]];
    let has = |i: usize| left[i / 64] >> (i % 64) & 1 == 1;
    let at = |i: usize| [i % n[0], i / n[0] % n[1], i / (n[0] * n[1])];
    let beside = move |i: usize| {
        let p = at(i);
        (0..6).filter_map(move |face| {
            let k = face / 2;
            if face % 2 == 0 { (p[k] > 0).then(|| i - strides[k]) } else { (p[k] + 1 < n[k]).then(|| i + strides[k]) }
        })
    };
    let core = |i: usize| has(i) && beside(i).filter(|b| has(*b)).count() == 6;
    // (A piece is at least 27 cells with its layer off and two through every way: a lump, and not the cells left of a
    // sheet where it happens to lie three thick, which dropped with two steps round each made a sieve of a bowl. And it is as long as it was
    // with its layer on, so that a scrap that is loose and too large to drop is not dropped here as a small one.)
    let pieces = loose(&core, n, under, Some((27, 2)));
    // the layer taken off each: two steps out through cells that are in no piece's core
    let mut gone: std::collections::BTreeSet<usize> = pieces.iter().flat_map(|(first, last)| *first..*last).collect();
    let mut edge: Vec<usize> = gone.iter().copied().collect();
    for _ in 0..2 {
        let mut next = Vec::new();
        for cell in edge {
            for other in beside(cell).filter(|b| has(*b) && !core(*b)) {
                if gone.insert(other) {
                    next.push(other);
                }
            }
        }
        edge = next;
    }
    gone.into_iter().map(|cell| (cell, cell + 1)).collect()
}

/// Empty the cells further inside than a wall `thick` cells through, and
/// return how many were emptied.
///
/// The wall is the cells the surface crosses and, behind them, the full
/// cells within `thick` of those, counted in steps from a cell to any of
/// the 26 round it: found by spreading inwards from the surface a layer at
/// a time, which looks only at the cells of the wall. (Not `thick` less
/// one: a cell the surface crosses is on average half full, and where it
/// is less than half full it holds no brick at all, so a wall of those
/// cells alone has holes in it.)
///
/// Counted that way the wall is never thinner than `thick` and is up to
/// 1.7 times as thick along a diagonal. Counted through faces only, as it
/// was first written, it was as thin as 0.58 of `thick` along a diagonal:
/// a ball asked for a wall of 6 had less brick in it than a true wall of 6
/// has. The inner face is the faces of cells, stepped, and is not seen
/// from outside. An exact distance for every cell inside (Felzenszwalb and
/// Huttenlocher 2012, <https://doi.org/10.4086/toc.2012.v008a019>) would
/// make the wall the same thickness every way, for four more bytes a cell
/// over the whole box, to shape a face nobody looks at.
fn hollowed(shape: &mut [u8], crossed: &[u64], n: [usize; 3], thick: f64) -> usize {
    let is_crossed = |i: usize| crossed[i / 64] >> (i % 64) & 1 == 1;
    let deep = |shape: &[u8], i: usize| shape[i] == CUBE && !is_crossed(i);
    let mut kept = vec![0u64; shape.len().div_ceil(64)];
    let mut edge: Vec<usize> = (0..shape.len()).filter(|i| is_crossed(*i)).collect();
    let layers = (thick.round() as i64).max(1);
    for _ in 0..layers {
        let mut next = Vec::new();
        for &cell in &edge {
            let at = [cell % n[0], cell / n[0] % n[1], cell / (n[0] * n[1])].map(|v| v as i64);
            for round in (0..27).filter(|r| *r != 13) {
                let to = [at[0] + round % 3 - 1, at[1] + round / 3 % 3 - 1, at[2] + round / 9 - 1];
                if (0..3).any(|k| to[k] < 0 || to[k] >= n[k] as i64) {
                    continue;
                }
                let beside = to[0] as usize + n[0] * (to[1] as usize + n[1] * to[2] as usize);
                if deep(shape, beside) && kept[beside / 64] >> (beside % 64) & 1 == 0 {
                    kept[beside / 64] |= 1 << (beside % 64);
                    next.push(beside);
                }
            }
        }
        edge = next;
    }
    let mut emptied = 0;
    for i in 0..shape.len() {
        if deep(shape, i) && kept[i / 64] >> (i % 64) & 1 == 0 {
            shape[i] = 0;
            emptied += 1;
        }
    }
    emptied
}

/// A colour number for every cell that holds brick and can be seen, and
/// [`ANY`] for the rest.
///
/// A cell can be seen if the surface crosses it, or if any of the six
/// cells beside it is not a full cube. Each takes the colour of the
/// surface nearest its middle: for a cell the surface crosses that is the
/// point the scan already found, and for the others one search of the
/// hierarchy. The cells deeper in are never seen, and are left free to
/// join whatever box reaches them, so that a model's inside is a few
/// large boxes and not the colours of its skin carried through it.
#[allow(clippy::too_many_arguments)]
fn tones(shape: &[u8], sampled: &[(usize, u64)], facing: &[(DVec3, DVec3)], nearest: &[u32], n: [usize; 3], stand: &Stand, palette: &[[u8; 3]], shade_at: &(dyn Fn(DVec3) -> u8 + Sync), threads: Threads) -> Vec<u8> {
    let strides = [1, n[0], n[0] * n[1]];
    let mut tones = vec![ANY; shape.len()];
    // (about a microsecond each: no search)
    let crossed = threads.map(sampled.len(), 500, |k| {
        let cell = sampled[k].0;
        if shape[cell] == 0 {
            return ANY;
        }
        stand.mesh.colour(nearest[k] as usize, stand.to_mesh(facing[k].0)).map_or(0, |colour| nearest_in(palette, colour))
    });
    for (k, tone) in crossed.into_iter().enumerate() {
        tones[sampled[k].0] = tone;
    }
    // the full cubes with a face to the air or to a sloped piece, a layer of cells at a time
    let seen = threads.map_ranges(n[2], 1, |layers| {
        let mut found: Vec<(usize, u8)> = Vec::new();
        for z in layers {
            for y in 0..n[1] {
                for x in 0..n[0] {
                    let cell = x + n[0] * (y + n[1] * z);
                    if shape[cell] != CUBE || tones[cell] != ANY {
                        continue;
                    }
                    let at = [x, y, z];
                    let open = (0..3).any(|k| at[k] == 0 || at[k] + 1 == n[k] || shape[cell - strides[k]] != CUBE || shape[cell + strides[k]] != CUBE);
                    if open {
                        found.push((cell, shade_at(DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5))));
                    }
                }
            }
        }
        found
    });
    for (cell, tone) in seen.into_iter().flatten() {
        tones[cell] = tone;
    }
    tones
}

/// [`merge`] for cells with colours: cubes of one colour joined into
/// boxes, a cell of [`ANY`] colour joining whichever box reaches it, and
/// ramps of one pose and colour into longer ramps. Returns the bricks and
/// the colour number of each. A box of cells that are all of any colour is
/// deep inside, and is given the colour `deep` finds for its middle.
fn merge_coloured(mut shape: Vec<u8>, tones: Vec<u8>, n: [usize; 3], max_ext: usize, deep: &dyn Fn(DVec3) -> u8) -> (Vec<CellBrick>, Vec<u8>) {
    let index = |x: usize, y: usize, z: usize| x + n[0] * (y + n[1] * z);
    let (mut bricks, mut shades) = (Vec::new(), Vec::new());
    // the one colour of a row of full cubes that may join a box of colour `with`, if the row is such a row
    let row = |shape: &[u8], from: usize, to: usize, with: u8| -> Option<u8> {
        let mut colour = with;
        for i in from..to {
            if shape[i] != CUBE || (tones[i] != ANY && colour != ANY && tones[i] != colour) {
                return None;
            }
            if tones[i] != ANY {
                colour = tones[i];
            }
        }
        Some(colour)
    };
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                let start = index(x, y, z);
                let mask = shape[start];
                if mask == 0 {
                    continue;
                }
                let at = [x, y, z];
                let mut ext = [1usize; 3];
                let mut colour = tones[start];
                match shape_info(mask).map(|info| (info.kind, info.axis as usize)) {
                    Some((Kind::Cube, _)) => {
                        while x + ext[0] < n[0] && ext[0] < max_ext {
                            let Some(joined) = row(&shape, start + ext[0], start + ext[0] + 1, colour) else { break };
                            colour = joined;
                            ext[0] += 1;
                        }
                        while y + ext[1] < n[1] && ext[1] < max_ext {
                            let from = index(x, y + ext[1], z);
                            let Some(joined) = row(&shape, from, from + ext[0], colour) else { break };
                            colour = joined;
                            ext[1] += 1;
                        }
                        'slabs: while z + ext[2] < n[2] && ext[2] < max_ext {
                            let mut joined = colour;
                            for yy in y..y + ext[1] {
                                let from = index(x, yy, z + ext[2]);
                                let Some(with) = row(&shape, from, from + ext[0], joined) else { break 'slabs };
                                joined = with;
                            }
                            colour = joined;
                            ext[2] += 1;
                        }
                    }
                    Some((Kind::Ramp, a)) => {
                        let step = |k: usize| start + k * [1, n[0], n[0] * n[1]][a];
                        while at[a] + ext[a] < n[a] && ext[a] < max_ext && shape[step(ext[a])] == mask && tones[step(ext[a])] == colour {
                            ext[a] += 1;
                        }
                    }
                    _ => {}
                }
                for zz in z..z + ext[2] {
                    for yy in y..y + ext[1] {
                        shape[index(x, yy, zz)..index(x + ext[0], yy, zz)].fill(0);
                    }
                }
                if colour == ANY {
                    colour = deep(DVec3::from_array([0, 1, 2].map(|k| at[k] as f64 + ext[k] as f64 / 2.0)));
                }
                bricks.push(CellBrick { mask, min: at.map(|v| v as i32), ext: ext.map(|v| v as i32) });
                shades.push(colour);
            }
        }
    }
    (bricks, shades)
}

#[cfg(test)]
mod tests {
    use super::super::made;
    use super::*;

    fn stood(mesh: Mesh, radius: f64, import: Import, shell: Option<f64>) -> Stood {
        // (on the floor: its box from 0 upwards, and about cell 0 across)
        let _ = radius;
        Stood::new(&Import { mesh: Some(Arc::new(mesh)), ..import }, DVec3::new(0.5, 0.5, 0.5), radius, shell).unwrap()
    }

    fn no_two_share_a_cell(bricks: &[CellBrick]) {
        let mut seen = std::collections::HashSet::new();
        for b in bricks {
            for i in 0..b.ext[0] * b.ext[1] * b.ext[2] {
                assert!(seen.insert([b.min[0] + i % b.ext[0], b.min[1] + i / b.ext[0] % b.ext[1], b.min[2] + i / (b.ext[0] * b.ext[1])]), "two bricks in one cell at {:?}", b.min);
            }
        }
    }

    #[test]
    fn a_name_is_a_hash_and_an_ending_and_nothing_that_could_be_a_path() {
        assert!(Import::named("0123456789abcdef.glb").is_ok() && Import::named("ffffffffffffffff.obj").is_ok() && Import::named("0000000000000000.ply").is_ok());
        for bad in ["", "none", "../0123456789abcdef.glb", "0123456789abcdef.glb/..", "0123456789abcdef.exe", "0123456789ABCDEF.glb", "0123456789abcde.glb", "0123456789abcdef0.glb", "C:\\0123456789abcdef.glb", "0123456789abcdef.glb.glb", "0123456789abcdef", "con.glb", "0123456789abcde/.glb", "0123456789abcdef.glb\0"] {
            assert_eq!(Import::named(bad), Err(MeshError::NotAnImport { name: bad.to_string() }), "{bad:?}");
        }
        // and what a save says of a mesh is read back as it was written, with its numbers brought within bounds
        let import = Import { name: "0123456789abcdef.ply".into(), mesh: None, up: Up::X, fitter: Fitter::Blocks, loose: 7, colours: 5 };
        assert_eq!(Import::read(&import.text()).unwrap(), import);
        assert_eq!(Import::read("none").unwrap(), Import { colours: USUAL_COLOURS, ..Import::default() });
        // a name alone is the mesh as the editor opens it: a PLY file with z up, the others with y
        assert_eq!((Import::read("0123456789abcdef.ply").unwrap().up, Import::read("0123456789abcdef.obj").unwrap().up, Import::read("0123456789abcdef.ply,y").unwrap().up), (Up::Z, Up::Y, Up::Y));
        assert_eq!(Import::read("none,z,smoothed,99999,99999").unwrap(), Import { up: Up::Z, loose: MOST_LOOSE, colours: MOST_COLOURS, ..Import::default() });
        assert!(matches!(Import::read("../../secret.glb"), Err(MeshError::NotAnImport { .. })));
        for bad in ["none,w", "none,y,round", "none,y,blocks,many", "none,y,blocks,1,2,3"] {
            assert!(matches!(Import::read(bad), Err(MeshError::Malformed { .. })), "{bad}");
        }
    }

    #[test]
    fn each_way_up_is_a_turn_and_never_a_mirror_image() {
        for (up, _) in UPS {
            let [x, y, z] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(|p| DVec3::from_array(up.stood(p).map(f64::from)));
            assert!((x.cross(y).dot(z) - 1.0).abs() < 1e-9, "{up:?}");
        }
        assert_eq!((Up::Y.stood([0.0, 1.0, 0.0]), Up::X.stood([1.0, 0.0, 0.0]), Up::Z.stood([0.0, 0.0, 1.0])), ([0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]));
    }

    #[test]
    fn a_form_with_no_mesh_or_one_too_large_is_refused_by_name() {
        assert_eq!(Stood::new(&Import::default(), DVec3::ZERO, 20.0, None).err(), Some(MeshError::NoFile));
        // made larger, it stands where it stood
        let ball = |radius| Stood::new(&Import { mesh: Some(Arc::new(made::sphere(8))), ..Import::default() }, DVec3::new(3.5, 3.5, 7.5), radius, None).unwrap().bounds();
        assert_eq!((ball(10.0).0[2], ball(30.0).0[2], ball(30.0).1[2] - ball(10.0).1[2]), (7 - PAD as i32, 7 - PAD as i32, 40));
        let named = Import { name: "0123456789abcdef.glb".into(), ..Import::default() };
        assert_eq!(Stood::new(&named, DVec3::ZERO, 20.0, None).err(), Some(MeshError::Missing { name: named.name.clone() }));
        let ball = Import { mesh: Some(Arc::new(made::sphere(8))), ..Import::default() };
        assert!(matches!(Stood::new(&ball, DVec3::ZERO, 400.0, None), Err(MeshError::TooMany { what: Count::Cells, .. })));
    }

    #[test]
    fn a_house_is_built_where_it_is_put_the_same_on_any_number_of_threads() {
        // at the size its faces are on the lattice, wherever its foot is asked to be
        for centre in [DVec3::new(0.5, 0.5, 0.5), DVec3::new(10.3, -7.8, 40.6)] {
            let import = Import { mesh: Some(Arc::new(made::house())), ..Import::default() };
            let stood = Stood::new(&import, centre, 28.0, None).unwrap();
            let one = stood.built(true, 80, 0.0, Threads::ONE);
            let many = stood.built(true, 80, 0.0, Threads::forced(5));
            assert_eq!((&one.bricks, &one.shades), (&many.bricks, &many.shades));
            no_two_share_a_cell(&one.bricks);
            assert!(one.error.abs() < 1e-9 && one.bricks.len() <= 12 && one.shades.is_empty(), "{} bricks, {} wrong", one.bricks.len(), one.error);
            // within the cells it says it is in, and about where it was asked to be
            let (lo, hi) = stood.bounds();
            assert!(one.bricks.iter().all(|b| (0..3).all(|k| b.min[k] >= lo[k] && b.min[k] + b.ext[k] <= hi[k])));
            let middle = [0, 1].map(|k| (one.bricks.iter().map(|b| b.min[k]).min().unwrap() + one.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap()) as f64 / 2.0);
            assert!((middle[0] - centre.x).abs() <= 0.5 && (middle[1] - centre.y).abs() <= 0.5, "{middle:?} for {centre}");
            // and the distance it answers with is of the same solid: inside its bricks, outside beyond them
            let volume: f64 = one.bricks.iter().map(CellBrick::volume).sum();
            assert!((volume - one.solid).abs() < 1e-6);
            assert!(stood.distance(DVec3::new(centre.x.round(), centre.y.round(), lo[2] as f64 + 10.0)) < -5.0 && stood.distance(DVec3::new(centre.x, centre.y, hi[2] as f64 + 3.0)) > 2.0);
            // standing on its foot: its lowest brick is in the cell the foot is in
            assert_eq!(one.bricks.iter().map(|b| b.min[2]).min(), Some(centre.z.floor() as i32));
        }
    }

    #[test]
    fn an_imported_form_is_built_by_the_way_every_form_is() {
        use crate::spec::{build, build_with, Form, Probe, Spec};
        let spec = |mesh: Option<Mesh>, radius: f64| Spec { form: Form::Imported, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius, fill: true, import: Import { mesh: mesh.map(Arc::new), name: "0123456789abcdef.glb".into(), ..Import::default() }, ..Spec::default() };
        let house = spec(Some(made::house()), 28.0);
        let built = build_with(&house, true, Threads::forced(3)).unwrap();
        assert_eq!(built.bricks, build_with(&house, true, Threads::ONE).unwrap().bricks);
        assert!(built.error.abs() < 1e-9 && built.solid > 20_000.0 && built.grain == 1 && !built.unmeasured && built.bricks.len() <= 12, "{} bricks, {} of {} wrong", built.bricks.len(), built.error, built.solid);
        // the form answers for itself where its bricks are
        let probe = Probe::new(&house, 1).unwrap();
        assert!(probe.distance(DVec3::new(0.5, 0.5, 8.0)) < -5.0 && probe.distance(DVec3::new(0.5, 0.5, 60.0)) > 10.0);
        // a quarter turn turns its bricks and its answer with them
        let turned = Spec { turned: crate::shapes::quarter(0), ..house.clone() };
        assert_eq!(build(&turned).unwrap().bricks.len(), built.bricks.len());
        assert!(build(&turned).unwrap().bricks != built.bricks);
        // each failure by its own name: no mesh in hand, nothing left, too large
        assert_eq!(build(&spec(None, 28.0)).unwrap_err(), MeshError::Missing { name: "0123456789abcdef.glb".into() }.to_string());
        assert_eq!(build(&Spec { import: Import::default(), ..spec(None, 28.0) }).unwrap_err(), MeshError::NoFile.to_string());
        let all_dropped = Spec { import: Import { loose: 200, ..house.import.clone() }, ..house.clone() };
        assert_eq!(build(&all_dropped).unwrap_err(), MeshError::NothingBuilt.to_string());
        assert!(build(&spec(Some(made::sphere(8)), 400.0)).unwrap_err().contains("too large to build"));
        // a brick size asked for is the size of every brick, and left to the budget the finest within it is found
        let ball = spec(Some(made::lumpy(16)), 30.0);
        let coarse = build(&Spec { grain: 3, ..ball.clone() }).unwrap();
        assert!(coarse.grain == 3 && coarse.bricks.iter().all(|b| b.min.iter().chain(&b.ext).all(|v| v % 3 == 0)));
        let fine = build(&Spec { grain: 1, ..ball.clone() }).unwrap();
        let within = build(&Spec { grain: 0, budget: fine.bricks.len() / 3, ..ball.clone() }).unwrap();
        assert!(within.grain > 1 && within.bricks.len() <= fine.bricks.len() / 3, "{} bricks at size {} for a budget of {}", within.bricks.len(), within.grain, fine.bricks.len() / 3);
        // two forms of one name are the same form, whether or not the mesh is in hand
        assert_eq!(spec(None, 28.0), house);
    }

    #[test]
    fn blocks_are_cubes_alone_and_so_is_any_fitter_with_wedges_off() {
        let ball = |fitter| stood(made::sphere(16), 12.0, Import { fitter, ..Import::default() }, None);
        let (blocks, smooth) = (ball(Fitter::Blocks).built(true, 80, 0.0, Threads::ONE), ball(Fitter::Smoothed).built(true, 80, 0.0, Threads::ONE));
        assert!(blocks.bricks.iter().all(|b| b.mask == CUBE) && smooth.bricks.iter().any(|b| b.mask != CUBE));
        assert!(smooth.error < 0.6 * blocks.error, "{} smoothed and {} in blocks", smooth.error, blocks.error);
        assert_eq!(ball(Fitter::Smoothed).built(false, 80, 0.0, Threads::ONE).bricks, blocks.bricks);
    }

    #[test]
    fn a_shell_is_as_thick_as_asked_and_a_filled_form_has_no_hollow() {
        let ball = |shell| stood(made::sphere(24), 20.0, Import::default(), shell);
        let filled = ball(None).built(true, 80, 0.0, Threads::forced(3));
        let exact = 4.0 / 3.0 * std::f64::consts::PI * 20f64.powi(3);
        assert!((filled.solid / exact - 1.0).abs() < 0.02, "{} against {exact}", filled.solid);
        for thick in [1.0, 3.0, 6.0] {
            let shell = ball(Some(thick)).built(true, 80, 0.0, Threads::forced(3));
            no_two_share_a_cell(&shell.bricks);
            let volume: f64 = shell.bricks.iter().map(CellBrick::volume).sum();
            // at least a shell that thick measured straight in, and no more than one measured from cell to cell with diagonal steps allowed (thicker along a diagonal)
            let straight = exact - 4.0 / 3.0 * std::f64::consts::PI * (20.0 - thick).powi(3);
            assert!(volume > straight && volume < 2.2 * straight + 300.0, "a shell {thick} thick is {volume}, and straight in {straight}");
            // the middle is empty, the outside is the filled ball's, and the form says so of itself
            assert!(!shell.bricks.iter().any(|b| (0..3).all(|k| b.min[k] <= [0, 0, 20][k] && b.min[k] + b.ext[k] > [0, 0, 20][k])));
            assert!((shell.solid - (filled.solid - (filled.bricks.iter().map(CellBrick::volume).sum::<f64>() - volume))).abs() < 0.02 * filled.solid);
            let form = ball(Some(thick));
            assert!(form.distance(DVec3::new(0.5, 0.5, 20.0)) > 0.0 && form.distance(DVec3::new(0.5, 0.5, thick / 2.0)) < 0.0);
        }
    }

    #[test]
    fn loose_pieces_under_the_size_asked_are_dropped_and_no_others() {
        // a ball 0.5 across with three scraps beside it, the largest 0.1 across: at 100 long, 50 and 10, 6 and 3
        let scattered = |loose| stood(made::scattered(), 50.0, Import { loose, ..Import::default() }, None).built(true, 80, 0.0, Threads::forced(4));
        // how many pieces the bricks are in: cells that share a face are of one piece
        let pieces = |laid: &Laid| {
            let mut cells: std::collections::HashSet<[i32; 3]> = laid.bricks.iter().flat_map(|b| (0..b.ext[0] * b.ext[1] * b.ext[2]).map(move |i| [b.min[0] + i % b.ext[0], b.min[1] + i / b.ext[0] % b.ext[1], b.min[2] + i / (b.ext[0] * b.ext[1])])).collect();
            let mut count = 0;
            while let Some(&start) = cells.iter().next() {
                count += 1;
                let mut edge = vec![start];
                cells.remove(&start);
                while let Some(at) = edge.pop() {
                    for face in 0..6 {
                        let mut beside = at;
                        beside[face / 2] += [-1, 1][face % 2];
                        if cells.remove(&beside) {
                            edge.push(beside);
                        }
                    }
                }
            }
            count
        };
        let all = scattered(0);
        assert_eq!(pieces(&all), 4);
        // nothing is 2 across or less; the smallest goes under 6, the next under 9, the largest scrap under 14
        assert_eq!(scattered(2).bricks, all.bricks);
        let (six, nine, fourteen) = (scattered(6), scattered(9), scattered(14));
        assert_eq!((pieces(&six), pieces(&nine), pieces(&fourteen)), (3, 2, 1));
        no_two_share_a_cell(&fourteen.bricks);
        // what is left is the ball, which ends half way along, and the volume is the ball's alone
        let most = |laid: &Laid| laid.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap();
        assert!(most(&all) - most(&fourteen) >= 49, "the bricks end at {} with the scraps and {} without", most(&all), most(&fourteen));
        let volume = |laid: &Laid| laid.bricks.iter().map(CellBrick::volume).sum::<f64>();
        assert!((volume(&fourteen) - fourteen.solid).abs() < 0.03 * fourteen.solid && fourteen.solid < all.solid - 400.0, "{} of {} built, {} with the scraps", volume(&fourteen), fourteen.solid, all.solid);
        assert!(fourteen.error <= all.error && fourteen.error >= 0.0);
        // and with everything dropped there is nothing
        assert!(scattered(200).bricks.is_empty());
    }

    #[test]
    fn a_hole_is_closed_by_a_skin_as_close_as_the_rest_of_the_surface() {
        // a ball with a cap and a patch gone, the holes 28 and 12 cells wide at this size
        let ball = stood(made::holed_ball(24), 24.0, Import::default(), None);
        let (one, many) = (ball.built(true, 80, 0.0, Threads::ONE), ball.built(true, 80, 0.0, Threads::forced(5)));
        assert_eq!(one.bricks, many.bricks);
        no_two_share_a_cell(&many.bricks);
        // against the mesh asked at every sample, which is the slow way and has no shortcuts to go wrong at a hole
        let measured = ball.measured(&many, Threads::forced(4));
        assert!(measured.wrong < 0.03 * measured.solid, "{} of {} wrong", measured.wrong, measured.solid);
        // and what it says of its own fit is what it measures as, skin and all
        // (to within the skin's own cells, whose samples are read off their corners and not each asked for)
        assert!((many.error - measured.wrong).abs() < 0.015 * measured.solid && (many.solid - measured.solid).abs() < 0.015 * measured.solid, "said {} of {}, measured {} of {}", many.error, many.solid, measured.wrong, measured.solid);
        // the skin is laid in sloped pieces like any surface: over the top hole there is more than cubes
        let top = many.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap();
        assert!(many.bricks.iter().any(|b| b.mask != CUBE && b.min[2] + b.ext[2] >= top - 1));
    }

    #[test]
    fn a_sheet_thinner_than_a_cell_is_built_a_cell_thick_and_whole() {
        // a bowl and a leaning plate, each a third of a cell through at this size
        for fitter in [Fitter::Smoothed, Fitter::Blocks] {
            let sheets = stood(made::thin(0.3), 58.5, Import { fitter, ..Import::default() }, None);
            let laid = sheets.built(true, 80, 0.0, Threads::forced(4));
            assert_eq!(laid.bricks, sheets.built(true, 80, 0.0, Threads::ONE).bricks);
            no_two_share_a_cell(&laid.bricks);
            assert!(sheets.bare(&laid) < 0.01, "{:.1}% of the sheets has no brick near it", 100.0 * sheets.bare(&laid));
        }
        // the sharp edge of something thick is no sheet: a roof's eaves stay ramps, and the house is still exact
        let house = stood(made::house(), 28.0, Import::default(), None).built(true, 80, 0.0, Threads::ONE);
        assert!(house.error.abs() < 1e-9 && house.bricks.len() <= 12);
    }

    #[test]
    fn a_lump_hanging_by_a_thread_is_dropped_with_the_loose_pieces_and_one_on_a_thick_neck_is_not() {
        let built = |neck: f64, loose| stood(made::tethered(neck), 30.0, Import { loose, ..Import::default() }, None).built(true, 80, 0.0, Threads::forced(3));
        let reach = |laid: &Laid| laid.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap() - laid.bricks.iter().map(|b| b.min[0]).min().unwrap();
        // kept whole when nothing is asked to be dropped; the lump goes when pieces under 20 are, and the ball (40) stays
        let (whole, cut) = (built(1.6, 0), built(1.6, 20));
        assert!(reach(&whole) >= 58 && reach(&cut) <= 50 && reach(&cut) >= 40, "{} long whole and {} cut", reach(&whole), reach(&cut));
        no_two_share_a_cell(&cut.bricks);
        assert_eq!(cut.bricks, built(1.6, 20).bricks);
        // a neck five cells thick is no thread: the lump is part of the thing
        assert!(reach(&built(5.0, 20)) >= 58);
        // and a thing thin all along is not cut off from itself: the bowl and the plate, each one cell thick as built, both stay
        let sheets = |loose| stood(made::thin(0.3), 58.5, Import { loose, ..Import::default() }, None).built(true, 80, 0.0, Threads::forced(3));
        // (but for crumbs at their edges, which are loose and small)
        let (all, kept) = (sheets(0), sheets(20));
        assert!(reach(&kept) == reach(&all) && kept.bricks.len() * 20 > all.bricks.len() * 19, "{} bricks {} long, and {} bricks {} long with pieces dropped", all.bricks.len(), reach(&all), kept.bricks.len(), reach(&kept));
    }

    #[test]
    fn a_brick_takes_the_colour_of_the_surface_nearest_it() {
        // red above, white round the middle, blue below
        // (three colours asked for: the band of triangles between two colours shades from one to the other, and more would be spent on those shades)
        let ball = stood(made::painted_ball(24), 20.0, Import { colours: 3, ..Import::default() }, None);
        let palette = ball.palette();
        let laid = ball.built(true, 80, 0.0, Threads::forced(4));
        assert_eq!(laid.bricks, ball.built(true, 80, 0.0, Threads::ONE).bricks);
        assert_eq!(laid.shades, ball.built(true, 80, 0.0, Threads::ONE).shades);
        assert_eq!((palette.len(), laid.shades.len()), (3, laid.bricks.len()));
        no_two_share_a_cell(&laid.bricks);
        let named = |shade: u8| { let [r, _, b] = palette[shade as usize]; if r > 150 && b > 150 { "white" } else if r > 150 { "red" } else { "blue" } };
        let mut seen = std::collections::HashSet::new();
        for (brick, shade) in laid.bricks.iter().zip(&laid.shades) {
            // a brick at the very top is red, one at the very bottom blue, and one at the ball's widest white (the bands part a quarter of the height from each end)
            let wanted = if brick.min[2] + brick.ext[2] >= 40 { "red" } else if brick.min[2] <= 0 { "blue" } else if brick.min[0] <= -19 || brick.min[0] + brick.ext[0] >= 20 { "white" } else { continue };
            assert_eq!(named(*shade), wanted, "{brick:?}");
            seen.insert(wanted);
        }
        assert_eq!(seen.len(), 3);
        // Every cell of a box that can be seen is the colour of the surface at it: a box is never joined across two
        // colours. (Where the colour changes within a cell of the place, either colour will do.)
        let stand = ball.stand();
        let colour_at = |cell: [i32; 3]| {
            let local = DVec3::from_array([0, 1, 2].map(|k| (cell[k] - ball.low[k]) as f64 + 0.5));
            let hit = ball.mesh.nearest(stand.to_mesh(local), f64::INFINITY).unwrap();
            nearest_in(&palette, ball.mesh.colour(hit.tri, hit.point).unwrap())
        };
        let cells_of = |b: &CellBrick| { let b = *b; (0..b.ext[0] * b.ext[1] * b.ext[2]).map(move |i| [b.min[0] + i % b.ext[0], b.min[1] + i / b.ext[0] % b.ext[1], b.min[2] + i / (b.ext[0] * b.ext[1])]) };
        let full: std::collections::HashSet<[i32; 3]> = laid.bricks.iter().filter(|b| b.mask == CUBE).flat_map(cells_of).collect();
        let mut checked = 0;
        for (brick, shade) in laid.bricks.iter().zip(&laid.shades).filter(|(b, _)| b.mask == CUBE) {
            for cell in cells_of(brick) {
                let beside: Vec<[i32; 3]> = (0..6).map(|face| { let mut c = cell; c[face / 2] += [-1, 1][face % 2]; c }).collect();
                if beside.iter().all(|c| full.contains(c)) {
                    continue;
                }
                let here = colour_at(cell);
                if beside.iter().all(|c| colour_at(*c) == here) {
                    assert_eq!(*shade, here, "the cell {cell:?} of {brick:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 300, "only {checked} cells were looked at");
        // the same bricks fill the same cells as with no colours, in more pieces: a box is one colour
        let plain = stood(made::painted_ball(24), 20.0, Import { colours: 0, ..Import::default() }, None).built(true, 80, 0.0, Threads::ONE);
        let volume = |laid: &Laid| laid.bricks.iter().map(CellBrick::volume).sum::<f64>();
        assert!(plain.shades.is_empty() && (volume(&plain) - volume(&laid)).abs() < 1e-9 && laid.bricks.len() >= plain.bricks.len());
        // and not many more: the inside is not cut up by the colours of the skin
        assert!(laid.bricks.len() < plain.bricks.len() * 2, "{} coloured and {} plain", laid.bricks.len(), plain.bricks.len());
    }

    #[test]
    fn a_brick_priced_above_nothing_buys_fewer_bricks_for_a_looser_fit() {
        let ball = stood(made::lumpy(24), 24.0, Import::default(), None);
        let (close, loose) = (ball.built(true, 80, 0.0, Threads::forced(4)), ball.built(true, 80, 0.5, Threads::forced(4)));
        no_two_share_a_cell(&loose.bricks);
        assert!(loose.bricks.len() < close.bricks.len() && loose.error > close.error, "{} bricks {} wrong, priced: {} and {}", close.bricks.len(), close.error, loose.bricks.len(), loose.error);
        // and what it says is wrong is what is wrong
        let stand = ball.stand();
        let local: Vec<CellBrick> = loose.bricks.iter().map(|b| CellBrick { min: [0, 1, 2].map(|k| b.min[k] - ball.low[k]), ..*b }).collect();
        let measured = super::super::trial::measure(&|p| stand.distance(p), stand.cells().map(|v| v as usize), &local, Threads::forced(4));
        assert!((measured.wrong - loose.error).abs() < 1e-6, "said {} and measured {}", loose.error, measured.wrong);
    }
}
