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

use super::trial::Stand;
use super::d;
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
    /// how long each part took, in milliseconds
    pub stages: Vec<(&'static str, f64)>,
}

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
    let cells = starts.len() - 1;
    let found = threads.map(cells, 8, |k| {
        let cell = pairs[starts[k]].0 as usize;
        let at = DVec3::new((cell % n[0]) as f64, (cell / n[0] % n[1]) as f64, (cell / (n[0] * n[1])) as f64);
        let here: Vec<[DVec3; 3]> = pairs[starts[k]..starts[k + 1]].iter().map(|p| mesh.tris[p.1 as usize].map(to_cells)).collect();
        // (a little off the middle, where no face of a model drawn on whole numbers passes)
        let middle = at + DVec3::new(0.5137, 0.4929, 0.5193);
        let base = mesh.winding(stand.to_mesh(middle));
        let inside = |p: DVec3| base + here.iter().map(|t| across(t, middle, p)).sum::<i32>() as f64 > 0.5;
        let bits = (0..K * K * K).fold(0u64, |bits, s| {
            let p = DVec3::new((s % K) as f64 + 0.5, (s / K % K) as f64 + 0.5, (s / (K * K)) as f64 + 0.5) / K as f64;
            bits | (inside(at + p) as u64) << s
        });
        let corners = (0..8u8).fold(0u8, |m, c| m | (inside(at + DVec3::new((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2) as f64)) as u8) << c);
        let mut scores = [(0u32, 0u32); 30];
        for (score, (mask, pattern)) in scores.iter_mut().zip(offered) {
            *score = ((bits ^ pattern).count_ones(), (corners ^ mask).count_ones());
        }
        let (mask, pattern) = offered[least_unshared(&scores[..offered.len()], 2)];
        // the nearest of its triangles to its middle, for the way the surface faces here
        let centre = at + 0.5;
        let nearest = pairs[starts[k]..starts[k + 1]].iter().map(|p| {
            let on = stand.from_mesh(super::nearest_on(&mesh.tris[p.1 as usize], stand.to_mesh(centre)));
            (on.distance_squared(centre), on, p.1)
        }).min_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2))).expect("a cell of the surface has a triangle");
        (cell, bits, mask, (bits ^ pattern).count_ones(), (nearest.1, mesh.triangle(nearest.2 as usize).1), nearest.2)
    });
    stages.push(("samples of the surface's cells", lap()));

    let mut shape = vec![0u8; n[0] * n[1] * n[2]];
    // 1 for a cell of the surface, until its piece is written over it
    for &(cell, ..) in &found {
        shape[cell] = 1;
    }
    // every run of untouched cells along x: inside or not, by the winding number at its first cell
    let rows = threads.map(n[1] * n[2], 16, |row| {
        let line = &shape[row * n[0]..(row + 1) * n[0]];
        let mut inside: Vec<(usize, usize)> = Vec::new();
        let mut x = 0;
        while x < n[0] {
            if line[x] != 0 {
                x += 1;
                continue;
            }
            let end = (x..n[0]).find(|i| line[*i] != 0).unwrap_or(n[0]);
            // (a run that reaches the edge of the cells fitted is outside: the mesh's box is within them)
            if x > 0 && end < n[0] && mesh.winding(stand.to_mesh(DVec3::new(x as f64 + 0.5, (row % n[1]) as f64 + 0.5, (row / n[1]) as f64 + 0.5))) > 0.5 {
                inside.push((x, end));
            }
            x = end;
        }
        inside
    });
    let per_cell = (K * K * K) as u64;
    let (mut solid, mut error) = (0u64, 0u64);
    for (row, runs) in rows.into_iter().enumerate() {
        for (from, to) in runs {
            shape[row * n[0] + from..row * n[0] + to].fill(CUBE);
            solid += (to - from) as u64 * per_cell;
        }
    }
    stages.push(("runs of cells inside", lap()));
    let mut sampled = Vec::with_capacity(found.len());
    let mut facing = Vec::with_capacity(found.len());
    let mut nearest = Vec::with_capacity(found.len());
    for (cell, bits, mask, wrong, faces, tri) in found {
        shape[cell] = mask;
        solid += bits.count_ones() as u64;
        error += wrong as u64;
        sampled.push((cell, bits));
        facing.push(faces);
        nearest.push(tri);
    }
    Scanned { shape, sampled, solid, error, facing, nearest, stages }
}
