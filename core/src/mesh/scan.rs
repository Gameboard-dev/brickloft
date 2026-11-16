//! Which cells a mesh's surface passes through, and which samples of each
//! are inside it, found from the triangles themselves and at a cost that
//! goes by the surface and not by the volume.
//!
//! `fit_volume::classify` asks a distance function at every corner of
//! every block near the surface and at 64 places in every cell the surface
//! may cross. For a mesh each of those questions is a search of the
//! hierarchy for the nearest triangle and another for the winding number,
//! and a house of 22 triangles 56 cells long took ten seconds on one
//! thread. Nearly all of that work finds out what the triangles say
//! directly:
//!
//! - A cell is crossed by the surface exactly when a triangle overlaps its
//!   box. So each triangle is laid into the cells it overlaps
//!   (`overlaps`, the separating-axis test of Akenine-Möller 2001,
//!   <https://doi.org/10.1080/10867651.2001.10487535>), and no cell is
//!   asked how far it is from anything.
//! - The winding number changes by exactly one across a triangle and
//!   nowhere else. So it is worked out through the hierarchy once for a
//!   cell, at its middle, and each of the cell's samples is the middle's
//!   number plus or minus one for every triangle of that cell that the
//!   line from the middle to the sample passes through. The answer is the
//!   winding number at the sample, not an estimate of it.
//! - A cell no triangle touches is all inside or all outside, and so is
//!   the whole run of such cells between two that are touched: one
//!   winding number settles the run.
//!
//! So the hierarchy is searched once for each cell of the surface and once
//! for each run, and everything else is a few triangles against a line.
//!
//! # A mesh with holes
//!
//! All three of those hold for a mesh that is closed, and the second and
//! third fail at a hole. Near the rim of a hole the winding number is not
//! a whole number: it runs smoothly from the value on one side of the
//! surface to the value on the other, round the rim. What is inside is
//! then wherever it is over one half, which draws a skin across the hole
//! (flat, if the rim lies in a plane), and that skin passes through cells
//! no triangle touches. A run of such cells is then part inside and part
//! outside, and one number for the run paints the whole of it one or the
//! other: a hole 50 cells wide in a ball came out closed by a ragged lid,
//! with a tenth of the ball's volume wrong (docs/algorithm.md, 7.13).
//!
//! A mesh knows whether it has any holes (`Mesh::open_edges`), and one
//! that has none is scanned as above. For one that has:
//!
//! - A cell with a triangle of a hole's rim in it, or whose winding number
//!   at its middle is not near a whole number, has the winding number
//!   worked out at each of its samples and corners: 72 searches in place
//!   of one, for the cells along the rims.
//! - A run of untouched cells is tried at its two ends and three places
//!   between. If those are all near one whole number the run is settled
//!   as before. If not, the skin crosses it: the winding number is worked
//!   out at the corners of each of its cells, a cell with every corner in
//!   is full, and a cell with some in and some out is a cell of the skin.
//!   Its samples are read off its eight corners (the number is smooth
//!   where no triangle is, so a straight-line mix between corners is
//!   close), and it is fitted with a piece like any cell of the surface.
//!
//! # Sheets thinner than a cell
//!
//! A cell takes the unit piece that agrees with most of its samples, and
//! for a sheet a third of a cell thick that piece is nothing: the sheet is
//! a third of the cell wherever it runs. Built that way a thin bowl was
//! confetti. So a cell left empty is looked at again: if the surface in
//! it has another surface behind it within a cell, facing the other way
//! (a sheet, and not the sharp edge of something thick, whose two faces
//! are at an angle), and the middle of the sheet there is in this cell,
//! the cell is filled. Each column of cells across the sheet has one such
//! cell, so the sheet comes out one cell thick with no holes. This spends
//! closeness, as volume, on being there at all: a sheet a third of a cell
//! thick built a cell thick is three times its volume. The alternative, a
//! brick thinner than a cell, the game does not have.

use super::d;
use super::trial::Stand;
use crate::fit_volume::{patterns, K};
use crate::par::Threads;
use crate::shapes::{least_unshared, CUBE};
use glam::DVec3;

/// What `scan` found: as `fit_volume::classify` gives it, and for each
/// cell of the surface the nearest point of the mesh to its middle and the
/// way the triangle there faces.
pub struct Scanned {
    pub shape: Vec<u8>,
    pub sampled: Vec<(usize, u64)>,
    pub solid: u64,
    pub error: u64,
    pub facing: Vec<(DVec3, DVec3)>,
    /// and which triangle that point is on, for its colour
    pub nearest: Vec<u32>,
    /// the cells filled to keep a thin sheet whole, which no stretched
    /// piece is to be laid over (see the top of this file)
    pub kept: Vec<usize>,
    /// how long each part took, in milliseconds
    pub stages: Vec<(&'static str, f64)>,
}

/// How far from a whole number a winding number may be and still be taken
/// for it. Through the hierarchy it is within 0.02 of exact (see
/// `mesh::FAR`); at the rim of a hole it passes through every value
/// between two whole numbers, and it is within this of one only well away
/// from the rim.
const WHOLE: f64 = 0.15;

/// How nearly opposite the two faces of a sheet must be: the cosine of the
/// angle between the ways they face. A sheet's faces are opposite (-1).
/// The two faces of a roof's eave at 1 in 2 are at -0.89, and that is the
/// edge of something thick: filled as a sheet it would be a cube where a
/// ramp belongs.
const SHEET: f64 = -0.95;

/// Whether the triangle `t` overlaps the box of half size `h` about `c`.
fn overlaps(t: [DVec3; 3], c: DVec3, h: f64) -> bool {
    let v = t.map(|p| p - c);
    let apart = |axis: DVec3| {
        let p = v.map(|p| p.dot(axis));
        let reach = h * (axis.x.abs() + axis.y.abs() + axis.z.abs());
        p[0].min(p[1]).min(p[2]) > reach || p[0].max(p[1]).max(p[2]) < -reach
    };
    let edges = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    if apart(DVec3::X) || apart(DVec3::Y) || apart(DVec3::Z) || apart(edges[0].cross(edges[1])) {
        return false;
    }
    !edges.iter().any(|e| [DVec3::X, DVec3::Y, DVec3::Z].iter().any(|a| apart(a.cross(*e))))
}

/// How the winding number changes from `from` to `to` across triangle `t`:
/// 1, -1, or 0 if the line between them misses it.
///
/// The line crosses the triangle's plane if its ends are on different
/// sides, and crosses within the triangle if it passes the same way round
/// all three edges: the sign of the volume of the tetrahedron each edge
/// makes with the line (the test of Plücker coordinates; see Ericson,
/// Real-Time Collision Detection, section 5.3.4). A line that passes
/// exactly through an edge two triangles share must cross one of them and
/// not both or neither, or the count is out by one. The volume an edge
/// makes is the same number with its sign changed when the edge is taken
/// the other way, which is how the triangle beside takes it, so a volume
/// of exactly nothing is counted as positive for the edge taken from its
/// lower end to its higher and negative the other way: one of the two
/// triangles has the line, whatever the rounding. (Möller and Trumbore's
/// test divides before it compares, and promises neither.)
fn across(t: &[DVec3; 3], from: DVec3, to: DVec3) -> i32 {
    let normal = (t[1] - t[0]).cross(t[2] - t[0]);
    let (before, after) = (normal.dot(from - t[0]) >= 0.0, normal.dot(to - t[0]) >= 0.0);
    if before == after {
        return 0;
    }
    let way = to - from;
    let lower = |a: DVec3, b: DVec3| (a.x, a.y, a.z) < (b.x, b.y, b.z);
    let round = |a: DVec3, b: DVec3| {
        let volume = way.dot((a - from).cross(b - from));
        volume > 0.0 || (volume == 0.0 && lower(a, b))
    };
    let sides = [round(t[0], t[1]), round(t[1], t[2]), round(t[2], t[0])];
    if sides[0] != sides[1] || sides[1] != sides[2] {
        return 0;
    }
    // from the side the triangle faces to the side it does not is going in
    if before { 1 } else { -1 }
}

/// One cell of the surface as it was found.
struct Found {
    cell: usize,
    bits: u64,
    mask: u8,
    wrong: u32,
    /// the nearest point of the surface to the cell's middle, and the way it faces there
    on: DVec3,
    normal: DVec3,
    tri: u32,
}

/// The best unit piece for a cell whose samples and corners are these, of
/// the pieces `offered`, and how many samples it gets wrong.
fn piece_for(bits: u64, corners: u8, offered: &[(u8, u64)]) -> (u8, u32) {
    let mut scores = [(0u32, 0u32); 30];
    for (score, (mask, pattern)) in scores.iter_mut().zip(offered) {
        *score = ((bits ^ pattern).count_ones(), (corners ^ mask).count_ones());
    }
    let (mask, pattern) = offered[least_unshared(&scores[..offered.len()], 2)];
    (mask, (bits ^ pattern).count_ones())
}

pub fn scan(stand: &Stand, n: [usize; 3], wedges: bool, threads: Threads) -> Scanned {
    let mut lap = super::trial::clock();
    let mesh = stand.mesh;
    let to_cells = |p: [f32; 3]| stand.from_mesh(d(p));
    // each triangle into the cells it overlaps
    let pairs = threads.map_ranges(mesh.tris.len(), 200, |range| {
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        for tri in range {
            let t = mesh.tris[tri].map(to_cells);
            let lo = t[0].min(t[1]).min(t[2]).floor().max(DVec3::ZERO);
            let hi = t[0].max(t[1]).max(t[2]).floor();
            for z in lo.z as usize..=(hi.z as usize).min(n[2] - 1) {
                for y in lo.y as usize..=(hi.y as usize).min(n[1] - 1) {
                    for x in lo.x as usize..=(hi.x as usize).min(n[0] - 1) {
                        // (a hair over half a cell, so that a triangle lying in the face between two cells is in both)
                        if overlaps(t, DVec3::new(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5), 0.5 + 1e-9) {
                            pairs.push(((x + n[0] * (y + n[1] * z)) as u32, tri as u32));
                        }
                    }
                }
            }
        }
        pairs
    });
    let mut pairs: Vec<(u32, u32)> = pairs.into_iter().flatten().collect();
    pairs.sort_unstable();
    // where each cell's triangles begin in `pairs`
    let mut starts: Vec<usize> = (0..pairs.len()).filter(|i| *i == 0 || pairs[*i].0 != pairs[*i - 1].0).collect();
    starts.push(pairs.len());
    let mut stages = vec![("triangles laid into cells", lap())];

    let all = patterns();
    let offered = if wedges { all } else { &all[..2] };
    let closed = mesh.open_edges == 0;
    let winding = |p: DVec3| mesh.winding(stand.to_mesh(p));
    let place = |cell: usize| DVec3::new((cell % n[0]) as f64, (cell / n[0] % n[1]) as f64, (cell / (n[0] * n[1])) as f64);
    let cells = starts.len() - 1;
    let mut found: Vec<Found> = threads.map(cells, 8, |k| {
        let cell = pairs[starts[k]].0 as usize;
        let at = place(cell);
        let mine = &pairs[starts[k]..starts[k + 1]];
        let here: Vec<[DVec3; 3]> = mine.iter().map(|p| mesh.tris[p.1 as usize].map(to_cells)).collect();
        // (a little off the middle, where no face of a model drawn on whole numbers passes)
        let middle = at + DVec3::new(0.5137, 0.4929, 0.5193);
        let base = winding(middle);
        // At the rim of a hole the number changes between the middle and a sample by more than the triangles crossed
        // account for (see the top of this file): there it is asked for at each sample.
        let rim = !closed && ((base - base.round()).abs() > WHOLE || mine.iter().any(|p| mesh.open(p.1 as usize)));
        let inside = |p: DVec3| if rim { winding(p) > 0.5 } else { base + here.iter().map(|t| across(t, middle, p)).sum::<i32>() as f64 > 0.5 };
        let bits = (0..K * K * K).fold(0u64, |bits, s| {
            let p = DVec3::new((s % K) as f64 + 0.5, (s / K % K) as f64 + 0.5, (s / (K * K)) as f64 + 0.5) / K as f64;
            bits | (inside(at + p) as u64) << s
        });
        let corners = (0..8u8).fold(0u8, |m, c| m | (inside(at + DVec3::new((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2) as f64)) as u8) << c);
        let (mask, wrong) = piece_for(bits, corners, offered);
        // the nearest of its triangles to its middle, for the way the surface faces here
        let centre = at + 0.5;
        let nearest = mine.iter().map(|p| {
            let on = stand.from_mesh(super::nearest_on(&mesh.tris[p.1 as usize], stand.to_mesh(centre)));
            (on.distance_squared(centre), on, p.1)
        }).min_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2))).unwrap_or((0.0, centre, 0));
        Found { cell, bits, mask, wrong, on: nearest.1, normal: mesh.triangle(nearest.2 as usize).1, tri: nearest.2 }
    });
    stages.push(("samples of the surface's cells", lap()));

    // The cells left empty that a sheet thinner than a cell passes through, to be filled (see the top of this file).
    let triangles_of = |cell: usize| {
        let from = pairs.partition_point(|p| (p.0 as usize) < cell);
        pairs[from..].iter().take_while(move |p| p.0 as usize == cell).map(|p| p.1 as usize)
    };
    let cell_of = |p: DVec3| (0..3).all(|k| p[k] >= 0.0 && p[k] < n[k] as f64).then(|| p.x as usize + n[0] * (p.y as usize + n[1] * p.z as usize));
    let thin = threads.map(found.len(), 40, |k| {
        let cell = &found[k];
        // (A cell with a tetrahedron in it is looked at too: a sixth of a cell in the sheet's own cell is a hole beside
        // it, and a bowl a third of a cell thick was a sieve of them. A ramp or a cut cube is left: it spans its cell,
        // and a sheet over half a cell thick is closer and smoother in those than in cubes.)
        if cell.mask != 0 && crate::shapes::shape_info(cell.mask).is_none_or(|piece| piece.kind != crate::shapes::Kind::Tet) {
            return false;
        }
        // Along the axis the surface most nearly faces, through the cell's middle, a cell's length each way: where
        // does the line go into the solid, and where does it come out again? (Along that axis and not straight in from
        // the nearest point of the surface, which is off to one side of the middle on a sloping sheet: judged there,
        // some columns of cells across a curved sheet had no cell that thought the sheet's middle its own, and a bowl
        // came out full of holes.)
        let axis = (0..3).max_by(|a, b| cell.normal[*a].abs().total_cmp(&cell.normal[*b].abs()).then(b.cmp(a))).unwrap_or(2);
        let centre = place(cell.cell) + 0.5;
        let mut along = DVec3::ZERO;
        along[axis] = 1.0;
        let (from, to) = (centre - along, centre + along);
        // (how far along the line, whether it goes in or comes out, and the way the surface faces there)
        let mut met: Vec<(f64, i32, DVec3)> = Vec::new();
        for through in [-1.0, 0.0, 1.0].into_iter().filter_map(|step| cell_of(centre + along * step)) {
            for other in triangles_of(through) {
                let t = mesh.tris[other].map(to_cells);
                let crossing = across(&t, from, to);
                if crossing != 0 {
                    let facing = (t[1] - t[0]).cross(t[2] - t[0]);
                    met.push((2.0 * facing.dot(t[0] - from) / facing.dot(to - from), crossing, facing.normalize_or_zero()));
                }
            }
        }
        // (a triangle that lies in two of the three cells is met twice, at the same place)
        met.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        met.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && a.1 == b.1);
        // A sheet is a way in and then a way out, facing opposite ways, less than a cell apart measured through the
        // sheet. The cell whose own stretch of the line (from 0.5 to 1.5 along it) holds the middle of the two is the
        // one filled: one cell of each column, whichever of them asks.
        met.windows(2).any(|pair| {
            let ((first, into, front), (second, out, back)) = (pair[0], pair[1]);
            let middle = (first + second) / 2.0;
            into == 1 && out == -1 && front.dot(back) < SHEET && (second - first) * front[axis].abs() < 1.0 && (0.5..1.5).contains(&middle)
        })
    });
    let mut kept = Vec::new();
    for (cell, _) in found.iter_mut().zip(&thin).filter(|(_, thin)| **thin) {
        (cell.mask, cell.wrong) = (CUBE, (!cell.bits).count_ones());
        kept.push(cell.cell);
    }
    stages.push(("thin sheets kept", lap()));

    let mut shape = vec![0u8; n[0] * n[1] * n[2]];
    // 1 for a cell of the surface, until its piece is written over it
    for cell in &found {
        shape[cell.cell] = 1;
    }
    // every run of untouched cells along x: inside or not, by the winding number
    let rows = threads.map(n[1] * n[2], 16, |row| {
        let line = &shape[row * n[0]..(row + 1) * n[0]];
        let low = DVec3::new(0.0, (row % n[1]) as f64, (row / n[1]) as f64);
        let mut inside: Vec<(usize, usize)> = Vec::new();
        let mut skin: Vec<Found> = Vec::new();
        let mut x = 0;
        while x < n[0] {
            if line[x] != 0 {
                x += 1;
                continue;
            }
            let end = (x..n[0]).find(|i| line[*i] != 0).unwrap_or(n[0]);
            let middle_of = |i: usize| low + DVec3::new(i as f64 + 0.5, 0.5, 0.5);
            if closed {
                // (a run that reaches the edge of the cells fitted is outside: the mesh's box is within them)
                if x > 0 && end < n[0] && winding(middle_of(x)) > 0.5 {
                    inside.push((x, end));
                }
            } else {
                // at its ends and three places between; all near one whole number, and the run is all one thing
                let mut tried: Vec<usize> = [0, 1, 2, 3, 4].map(|q| x + (end - 1 - x) * q / 4).to_vec();
                tried.dedup();
                let values: Vec<f64> = tried.iter().map(|i| winding(middle_of(*i))).collect();
                let whole = values[0].round();
                if values.iter().all(|v| (v - whole).abs() <= WHOLE) {
                    if whole >= 1.0 {
                        inside.push((x, end));
                    }
                } else {
                    // the skin over a hole crosses the run: each cell by the winding number at its corners
                    let corner = |i: usize, c: usize| winding(low + DVec3::new(i as f64, (c & 1) as f64, (c >> 1) as f64));
                    let mut ahead: [f64; 4] = std::array::from_fn(|c| corner(x, c));
                    let mut run: Option<usize> = None;
                    for i in x..end {
                        let behind = ahead;
                        ahead = std::array::from_fn(|c| corner(i + 1, c));
                        // (as a cell numbers its corners: x, then y, then z)
                        let at: [f64; 8] = std::array::from_fn(|c| if c & 1 == 0 { behind[c >> 1] } else { ahead[c >> 1] });
                        let corners = (0..8).fold(0u8, |m, c| m | ((at[c] > 0.5) as u8) << c);
                        if corners == 0xff {
                            run.get_or_insert(i);
                            continue;
                        }
                        inside.extend(run.take().map(|from| (from, i)));
                        if corners == 0 {
                            continue;
                        }
                        // the number at a place in the cell, mixed from its corners by how near each is
                        let mixed = |p: DVec3| (0..8).map(|c| at[c] * (if c & 1 == 0 { 1.0 - p.x } else { p.x }) * (if c & 2 == 0 { 1.0 - p.y } else { p.y }) * (if c & 4 == 0 { 1.0 - p.z } else { p.z })).sum::<f64>();
                        let bits = (0..K * K * K).fold(0u64, |bits, s| {
                            let p = DVec3::new((s % K) as f64 + 0.5, (s / K % K) as f64 + 0.5, (s / (K * K)) as f64 + 0.5) / K as f64;
                            bits | ((mixed(p) > 0.5) as u64) << s
                        });
                        let (mask, wrong) = piece_for(bits, corners, offered);
                        // the skin faces the way the number falls, and is where it passes one half
                        let slope = DVec3::from_array([0, 1, 2].map(|k| (0..8).map(|c| if c >> k & 1 == 1 { at[c] } else { -at[c] }).sum::<f64>() / 4.0));
                        let (centre, steep) = (low + DVec3::new(i as f64 + 0.5, 0.5, 0.5), slope.length().max(1e-9));
                        let normal = if slope.length() > 1e-9 { -slope / steep } else { DVec3::Z };
                        let out = ((mixed(DVec3::splat(0.5)) - 0.5) / steep).clamp(-0.87, 0.87);
                        // (the triangle nearest it is where its colour comes from: the skin has none of its own)
                        let tri = mesh.nearest(stand.to_mesh(centre), f64::INFINITY).map_or(0, |hit| hit.tri as u32);
                        skin.push(Found { cell: row * n[0] + i, bits, mask, wrong, on: centre + normal * out, normal, tri });
                    }
                    inside.extend(run.take().map(|from| (from, end)));
                }
            }
            x = end;
        }
        (inside, skin)
    });
    let per_cell = (K * K * K) as u64;
    let (mut solid, mut error) = (0u64, 0u64);
    let mut skin: Vec<Found> = Vec::new();
    for (row, (runs, cells)) in rows.into_iter().enumerate() {
        for (from, to) in runs {
            shape[row * n[0] + from..row * n[0] + to].fill(CUBE);
            solid += (to - from) as u64 * per_cell;
        }
        skin.extend(cells);
    }
    // the cells of the skin among the cells of the surface, in the order of their numbers
    if !skin.is_empty() {
        found.extend(skin);
        found.sort_by_key(|cell| cell.cell);
    }
    stages.push(("runs of cells inside", lap()));
    let mut sampled = Vec::with_capacity(found.len());
    let mut facing = Vec::with_capacity(found.len());
    let mut nearest = Vec::with_capacity(found.len());
    for cell in found {
        shape[cell.cell] = cell.mask;
        solid += cell.bits.count_ones() as u64;
        error += cell.wrong as u64;
        sampled.push((cell.cell, cell.bits));
        facing.push((cell.on, cell.normal));
        nearest.push(cell.tri);
    }
    Scanned { shape, sampled, solid, error, facing, nearest, kept, stages }
}
