//! A triangle mesh made by another program, as a solid that can be asked
//! where it is: how far a point is from its surface, which triangle is
//! nearest, and whether the point is inside.
//!
//! The mesh is kept whole for as long as bricks are being chosen. Nothing
//! here turns it into filled cells first: a fitter that wants to know which
//! way the surface faces at a cell, or how far a brick's sloped face stands
//! off it, asks the triangles (see [`Mesh::nearest`]). The alternative,
//! filling a grid of cells and fitting bricks to the grid, is what most
//! mesh-to-brick programs do and cannot give back a slope the grid never
//! held (see docs/algorithm.md, "Meshes").
//!
//! Inside and outside come from the generalised winding number (Jacobson,
//! Kavan and Sorkine-Hornung 2013, <https://doi.org/10.1145/2461912.2461916>)
//! and not from counting how many times a ray crosses the surface. Meshes
//! made by a reconstruction model have holes, shells that pass through each
//! other and loose scraps; a ray through a hole counts wrong and paints a
//! streak of solid or of air the whole length of the ray, where the winding
//! number only blurs near the hole. It is evaluated through the hierarchy of
//! boxes the nearest-triangle search uses, far groups of triangles standing
//! in for themselves as one dipole (Barill, Dickson, Schmidt, Levin and
//! Jacobson 2018, <https://doi.org/10.1145/3197517.3201337>), so a query
//! costs about the logarithm of the triangle count and not the count.

use glam::DVec3;

pub mod made;
pub mod scan;
pub mod trial;

/// Most vertices and triangles a mesh may have. A reconstruction model's
/// mesh is one to a few hundred thousand triangles; at the limit the mesh
/// and its hierarchy take about 150 MB (see [`Mesh::bytes`]).
pub const MAX_VERTICES: usize = 3_000_000;
pub const MAX_TRIANGLES: usize = 2_000_000;

/// Triangles in a leaf of the hierarchy. Four keeps the tree shallow
/// without making a leaf's exact tests the larger part of a query.
const LEAF: usize = 4;

/// How many of a group's radii away a point must be for the group's
/// winding number to be taken from its dipole alone. Barill et al. use 2
/// with a third-order expansion. The first two terms are used here, at
/// four radii, where the tests hold the answer to within 0.02 of the exact
/// sum (0.009 was the worst measured deep inside a ball of 36,000
/// triangles; at three radii 0.016, and over 0.02 at the middle of a
/// coarse sphere, where every group errs the same way). The answer is only
/// ever compared with one half, which it is nowhere near except at a hole,
/// where a hundredth moves the skin drawn over the hole by a small part of
/// the hole's width.
const FAR: f64 = 4.0;

/// One box of the hierarchy, with what its triangles look like from far off.
#[derive(Clone, Copy, Debug)]
struct Node {
    lo: [f32; 3],
    hi: [f32; 3],
    /// a leaf: its first triangle; otherwise its second child (the first follows it)
    at: u32,
    /// a leaf: how many triangles; otherwise 0
    count: u32,
    /// the area-weighted middle of its triangles, the sum of their area
    /// vectors, and the furthest any of their corners is from the middle
    middle: [f32; 3],
    dipole: [f32; 3],
    radius: f32,
    /// The next term of the same sum: each triangle's area vector times
    /// how far its own middle is from the group's, as the six different
    /// numbers of a symmetric matrix (xx, yy, zz, xy, xz, yz). The part of
    /// the full matrix that is not symmetric adds nothing to the sum.
    spread: [f32; 6],
}

/// The nearest point of the surface to somewhere.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// distance squared, in the mesh's own units
    pub d2: f64,
    /// which triangle, by its place in the hierarchy's order
    pub tri: usize,
    pub point: DVec3,
}

/// A mesh ready to be asked about. Its points are kept in a box one unit
/// along its longest side, about its middle, so that 32-bit numbers hold
/// them to a ten-millionth of the mesh whatever units the file was in.
pub struct Mesh {
    /// each triangle's corners, in the hierarchy's order
    tris: Vec<[[f32; 3]; 3]>,
    /// each triangle's three vertices, in the same order
    corners: Vec<[u32; 3]>,
    /// a colour for each vertex, or none
    colours: Vec<[u8; 3]>,
    nodes: Vec<Node>,
    /// half the box's size along each axis: half of one along its longest
    pub half: DVec3,
    /// a number that differs between two meshes that differ, for telling them apart cheaply
    pub id: u64,
    /// how many triangles were turned over to face the way their neighbours do
    pub turned: usize,
}

impl std::fmt::Debug for Mesh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mesh({} triangles, {:016x})", self.tris.len(), self.id)
    }
}

fn d(p: [f32; 3]) -> DVec3 {
    DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)
}

/// The nearest point of triangle `t` to `p`.
/// Ericson, Real-Time Collision Detection (2005), section 5.1.5.
fn nearest_on(t: &[[f32; 3]; 3], p: DVec3) -> DVec3 {
    let (a, b, c) = (d(t[0]), d(t[1]), d(t[2]));
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

impl Mesh {
    /// A mesh from its points, its triangles (three vertex numbers each)
    /// and a colour for each vertex or none at all.
    ///
    /// The numbers are from a file, which may be anyone's: counts are held
    /// to the limits, every coordinate must be a number, and every vertex
    /// number must be one of the vertices. Triangles with no area are left
    /// out.
    ///
    /// Triangles that face the other way from their neighbours are turned
    /// over, and a whole shell that faces inwards is turned out (see
    /// `oriented`): the winding number adds each triangle with its sign, so
    /// a shell inside out counts as a hole in whatever it overlaps.
    pub fn new(positions: &[[f32; 3]], triangles: &[[u32; 3]], colours: &[[u8; 3]]) -> Result<Mesh, String> {
        if positions.len() > MAX_VERTICES || triangles.len() > MAX_TRIANGLES {
            return Err(format!("the mesh has {} points and {} triangles, and the most that are read are {MAX_VERTICES} and {MAX_TRIANGLES}", positions.len(), triangles.len()));
        }
        if !colours.is_empty() && colours.len() != positions.len() {
            return Err("the mesh has colours for some of its points and not for others".into());
        }
        if positions.iter().any(|p| p.iter().any(|v| !v.is_finite())) {
            return Err("one of the mesh's points is not a number".into());
        }
        if let Some(bad) = triangles.iter().flatten().find(|v| **v as usize >= positions.len()) {
            return Err(format!("a triangle of the mesh names point {bad}, and it has {}", positions.len()));
        }
        let lo = positions.iter().fold(DVec3::MAX, |a, p| a.min(d(*p)));
        let hi = positions.iter().fold(DVec3::MIN, |a, p| a.max(d(*p)));
        let longest = (hi - lo).max_element();
        if triangles.is_empty() || !(longest > 0.0) || !longest.is_finite() {
            return Err("the mesh has no triangles, or no size".into());
        }
        let centre = (lo + hi) / 2.0;
        let points: Vec<[f32; 3]> = positions.iter().map(|p| ((d(*p) - centre) / longest).to_array().map(|v| v as f32)).collect();
        let area = |t: &[u32; 3]| (d(points[t[1] as usize]) - d(points[t[0] as usize])).cross(d(points[t[2] as usize]) - d(points[t[0] as usize])).length();
        let mut corners: Vec<[u32; 3]> = triangles.iter().copied().filter(|t| area(t) > 0.0).collect();
        if corners.is_empty() {
            return Err("every triangle of the mesh is flat to nothing".into());
        }
        let turned = oriented(&points, &mut corners);
        let corner_points = |t: &[u32; 3]| t.map(|v| points[v as usize]);
        // the triangles in the hierarchy's order: sorted into it by their middles
        let middles: Vec<[f32; 3]> = corners.iter().map(|t| { let c = corner_points(t); [0, 1, 2].map(|k| (c[0][k] + c[1][k] + c[2][k]) / 3.0) }).collect();
        let mut order: Vec<u32> = (0..corners.len() as u32).collect();
        let mut nodes = Vec::with_capacity(corners.len() / 2 + 1);
        let tri_points: Vec<[[f32; 3]; 3]> = corners.iter().map(corner_points).collect();
        build(&mut nodes, &mut order, 0, &middles, &tri_points);
        let tris: Vec<[[f32; 3]; 3]> = order.iter().map(|&t| tri_points[t as usize]).collect();
        let corners: Vec<[u32; 3]> = order.iter().map(|&t| corners[t as usize]).collect();
        // FNV-1a over everything that was kept: http://www.isthe.com/chongo/tech/comp/fnv/
        let mut id = 0xcbf29ce484222325u64;
        let mut mix = |bytes: &[u8]| bytes.iter().for_each(|b| id = (id ^ *b as u64).wrapping_mul(0x100000001b3));
        tris.iter().flatten().flatten().for_each(|v| mix(&v.to_le_bytes()));
        corners.iter().flatten().for_each(|v| mix(&v.to_le_bytes()));
        colours.iter().for_each(|c| mix(c));
        Ok(Mesh { tris, corners, colours: colours.to_vec(), nodes, half: (hi - lo) / longest / 2.0, id, turned })
    }

    pub fn triangles(&self) -> usize {
        self.tris.len()
    }

    /// How much memory the mesh and its hierarchy take, in bytes.
    pub fn bytes(&self) -> usize {
        self.tris.len() * 48 + self.nodes.len() * std::mem::size_of::<Node>() + self.colours.len() * 3
    }

    /// The corners of a triangle, and the way it faces (of length one).
    pub fn triangle(&self, tri: usize) -> ([DVec3; 3], DVec3) {
        let t = self.tris[tri].map(d);
        (t, (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero())
    }

    /// The colour of the surface at `point` of triangle `tri`: its
    /// corners' colours, mixed by how near each the point is. `None` for a
    /// mesh with no colours.
    pub fn colour(&self, tri: usize, point: DVec3) -> Option<[u8; 3]> {
        if self.colours.is_empty() {
            return None;
        }
        let ([a, b, c], _) = self.triangle(tri);
        // barycentric coordinates: each corner's share is the area of the triangle the point makes with the other two
        let whole = (b - a).cross(c - a).length().max(1e-30);
        let share = [(b - point).cross(c - point).length(), (c - point).cross(a - point).length(), (a - point).cross(b - point).length()].map(|s| s / whole);
        let total = share.iter().sum::<f64>().max(1e-30);
        let of = self.corners[tri].map(|v| self.colours[v as usize]);
        Some([0, 1, 2].map(|k| ((0..3).map(|i| share[i] * of[i][k] as f64).sum::<f64>() / total).round().clamp(0.0, 255.0) as u8))
    }

    /// The nearest point of the surface to `p`, if one is within the
    /// square root of `within2`.
    ///
    /// The hierarchy is walked nearer child first, and a box further off
    /// than the best found so far is not opened: about the logarithm of
    /// the triangle count in boxes, where trying every triangle is the count.
    pub fn nearest(&self, p: DVec3, within2: f64) -> Option<Hit> {
        let mut best = Hit { d2: within2, tri: usize::MAX, point: p };
        let box_d2 = |n: &Node| {
            let mut sum = 0.0;
            for k in 0..3 {
                let v = (n.lo[k] as f64 - p[k]).max(p[k] - n.hi[k] as f64).max(0.0);
                sum += v * v;
            }
            sum
        };
        // (a tree of two million triangles in leaves of four is 19 deep, and two boxes are pushed for each opened)
        let mut stack = [(0u32, 0f64); 64];
        let mut top = 1;
        stack[0] = (0, box_d2(&self.nodes[0]));
        while top > 0 {
            top -= 1;
            let (at, far) = stack[top];
            if far >= best.d2 {
                continue;
            }
            let node = &self.nodes[at as usize];
            if node.count > 0 {
                for tri in node.at as usize..(node.at + node.count) as usize {
                    let q = nearest_on(&self.tris[tri], p);
                    let d2 = q.distance_squared(p);
                    if d2 < best.d2 {
                        best = Hit { d2, tri, point: q };
                    }
                }
                continue;
            }
            let (one, two) = (at + 1, node.at);
            let (d1, d2) = (box_d2(&self.nodes[one as usize]), box_d2(&self.nodes[two as usize]));
            // the nearer on top, to be opened first
            let (near, further) = if d1 <= d2 { ((one, d1), (two, d2)) } else { ((two, d2), (one, d1)) };
            if top + 2 <= stack.len() {
                stack[top] = further;
                stack[top + 1] = near;
                top += 2;
            }
        }
        (best.tri != usize::MAX).then_some(best)
    }

    /// The winding number of the surface about `p`: 1 inside a closed
    /// shell that faces outwards, 0 outside it, 2 where two such shells
    /// overlap, and something between near a hole.
    ///
    /// Each triangle adds the solid angle it fills as seen from `p`, over
    /// the whole sphere's (Van Oosterom and Strackee 1983,
    /// <https://doi.org/10.1109/TBME.1983.325207>). A group of triangles
    /// more than [`FAR`] of its radii away adds what one flat piece with
    /// the group's summed area vector would, at the group's middle.
    pub fn winding(&self, p: DVec3) -> f64 {
        let mut sum = 0.0;
        let mut stack = [0u32; 64];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let at = stack[top];
            let node = &self.nodes[at as usize];
            let to = d(node.middle) - p;
            let far2 = to.length_squared();
            let reach = FAR * node.radius as f64;
            if far2 > reach * reach {
                // Barill et al., equation 22 with the first two terms of the series: the dipole, and its change over the group
                let (far, m) = (far2.sqrt(), node.spread.map(|v| v as f64));
                let through = m[0] * to.x * to.x + m[1] * to.y * to.y + m[2] * to.z * to.z + 2.0 * (m[3] * to.x * to.y + m[4] * to.x * to.z + m[5] * to.y * to.z);
                sum += d(node.dipole).dot(to) / (far2 * far) + (m[0] + m[1] + m[2]) / (far2 * far) - 3.0 * through / (far2 * far2 * far);
            } else if node.count > 0 {
                for tri in &self.tris[node.at as usize..(node.at + node.count) as usize] {
                    let (a, b, c) = (d(tri[0]) - p, d(tri[1]) - p, d(tri[2]) - p);
                    let (la, lb, lc) = (a.length(), b.length(), c.length());
                    sum += 2.0 * a.dot(b.cross(c)).atan2(la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb);
                }
            } else if top + 2 <= stack.len() {
                stack[top] = node.at;
                stack[top + 1] = at + 1;
                top += 2;
            }
        }
        sum / (4.0 * std::f64::consts::PI)
    }

    /// The winding number by adding every triangle, for checking the quick one.
    pub fn winding_exact(&self, p: DVec3) -> f64 {
        let mut sum = 0.0;
        for tri in &self.tris {
            let (a, b, c) = (d(tri[0]) - p, d(tri[1]) - p, d(tri[2]) - p);
            let (la, lb, lc) = (a.length(), b.length(), c.length());
            sum += 2.0 * a.dot(b.cross(c)).atan2(la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb);
        }
        sum / (4.0 * std::f64::consts::PI)
    }

    /// Distance from the surface at `p`, negative inside, in the mesh's
    /// own units, with the nearest point of the surface.
    pub fn signed(&self, p: DVec3) -> (f64, Hit) {
        let hit = self.nearest(p, f64::INFINITY).expect("a mesh has a triangle");
        let d = hit.d2.sqrt();
        (if self.winding(p) > 0.5 { -d } else { d }, hit)
    }
}

/// Build the hierarchy over `order[..]`, the triangles of this box, and
/// return the box's place in `nodes`. Split at the median middle along the
/// box's longest side: the same tree for the same mesh on any computer,
/// and never deeper than the logarithm of the count, which a split by
/// surface area can be for a mesh of slivers.
fn build(nodes: &mut Vec<Node>, order: &mut [u32], first: u32, middles: &[[f32; 3]], tris: &[[[f32; 3]; 3]]) -> u32 {
    let at = nodes.len() as u32;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let (mut area_sum, mut middle, mut dipole) = (0.0f64, DVec3::ZERO, DVec3::ZERO);
    for &t in order.iter() {
        let c = tris[t as usize];
        for p in c {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let vector = (d(c[1]) - d(c[0])).cross(d(c[2]) - d(c[0])) / 2.0;
        let area = vector.length();
        area_sum += area;
        middle += (d(c[0]) + d(c[1]) + d(c[2])) / 3.0 * area;
        dipole += vector;
    }
    let middle = if area_sum > 0.0 { middle / area_sum } else { (d(lo) + d(hi)) / 2.0 };
    let mut spread = [0f64; 6];
    for &t in order.iter() {
        let c = tris[t as usize];
        let vector = (d(c[1]) - d(c[0])).cross(d(c[2]) - d(c[0])) / 2.0;
        let off = (d(c[0]) + d(c[1]) + d(c[2])) / 3.0 - middle;
        for (k, (a, b)) in [(0, 0), (1, 1), (2, 2), (0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
            spread[k] += (off[a] * vector[b] + off[b] * vector[a]) / 2.0;
        }
    }
    let radius = order.iter().flat_map(|&t| tris[t as usize]).map(|p| d(p).distance(middle)).fold(0.0, f64::max);
    // (rounded up, so that a point just outside the true radius is never taken for far)
    nodes.push(Node { lo, hi, at: first, count: order.len() as u32, middle: middle.to_array().map(|v| v as f32), dipole: dipole.to_array().map(|v| v as f32), radius: (radius * 1.000001) as f32, spread: spread.map(|v| v as f32) });
    if order.len() <= LEAF {
        return at;
    }
    let axis = (0..3).max_by(|a, b| (hi[*a] - lo[*a]).total_cmp(&(hi[*b] - lo[*b]))).unwrap_or(0);
    let half = order.len() / 2;
    // (ties by the triangle's number, so that the order is one order)
    order.select_nth_unstable_by(half, |a, b| middles[*a as usize][axis].total_cmp(&middles[*b as usize][axis]).then(a.cmp(b)));
    let (low, high) = order.split_at_mut(half);
    build(nodes, low, first, middles, tris);
    let second = build(nodes, high, first + half as u32, middles, tris);
    nodes[at as usize].at = second;
    nodes[at as usize].count = 0;
    at
}

/// Turn triangles over so that each faces the way its neighbours do, and
/// each shell faces outwards. Returns how many were turned.
///
/// Two triangles are neighbours if they share an edge that no third
/// triangle has. Points at the same place are one point for this, since a
/// file repeats a point wherever its colour or texture has a seam. Two
/// neighbours agree when they run along their shared edge in opposite
/// directions. Each connected piece is walked from one triangle, turning
/// over every neighbour that disagrees; then the piece's volume is taken
/// about its own middle, and a piece whose volume is negative is facing
/// inwards and is turned over whole.
///
/// A shell that is meant as a hollow inside another faces inwards on
/// purpose, and is turned out with the rest, so the hollow is filled. A
/// build from bricks has its inside filled or not by a setting of its own.
fn oriented(points: &[[f32; 3]], corners: &mut [[u32; 3]]) -> usize {
    use std::collections::HashMap;
    // each point's first twin, by its exact place
    let mut first: HashMap<[u32; 3], u32> = HashMap::with_capacity(points.len());
    let one: Vec<u32> = points.iter().enumerate().map(|(i, p)| *first.entry(p.map(|v| (v + 0.0).to_bits())).or_insert(i as u32)).collect();
    // every edge with its triangle and whether it runs from its lower point to its higher
    let mut edges: Vec<(u32, u32, u32, bool)> = Vec::with_capacity(corners.len() * 3);
    for (t, c) in corners.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (one[c[k] as usize], one[c[(k + 1) % 3] as usize]);
            if a != b {
                edges.push((a.min(b), a.max(b), t as u32, a < b));
            }
        }
    }
    edges.sort_unstable();
    // each triangle's neighbours across its (up to three) shared edges, and whether the two disagree
    let mut beside: Vec<[(u32, bool); 3]> = vec![[(u32::MAX, false); 3]; corners.len()];
    let mut put = |t: u32, other: u32, disagree: bool| {
        if let Some(slot) = beside[t as usize].iter_mut().find(|s| s.0 == u32::MAX) {
            *slot = (other, disagree);
        }
    };
    let mut i = 0;
    while i < edges.len() {
        let run = edges[i..].iter().take_while(|e| (e.0, e.1) == (edges[i].0, edges[i].1)).count();
        if run == 2 && edges[i].2 != edges[i + 1].2 {
            let disagree = edges[i].3 == edges[i + 1].3;
            put(edges[i].2, edges[i + 1].2, disagree);
            put(edges[i + 1].2, edges[i].2, disagree);
        }
        i += run;
    }
    let mut seen = vec![false; corners.len()];
    let mut flip = vec![false; corners.len()];
    let mut turned = 0;
    for start in 0..corners.len() {
        if seen[start] {
            continue;
        }
        let mut piece = vec![start as u32];
        seen[start] = true;
        let mut next = 0;
        while next < piece.len() {
            let t = piece[next] as usize;
            next += 1;
            for (other, disagree) in beside[t] {
                if other != u32::MAX && !seen[other as usize] {
                    seen[other as usize] = true;
                    flip[other as usize] = flip[t] ^ disagree;
                    piece.push(other);
                }
            }
        }
        let middle = piece.iter().flat_map(|&t| corners[t as usize]).fold(DVec3::ZERO, |sum, v| sum + d(points[v as usize])) / (piece.len() * 3) as f64;
        let volume: f64 = piece
            .iter()
            .map(|&t| {
                let [a, b, c] = corners[t as usize].map(|v| d(points[v as usize]) - middle);
                a.dot(b.cross(c)) * if flip[t as usize] { -1.0 } else { 1.0 }
            })
            .sum();
        for &t in &piece {
            if flip[t as usize] ^ (volume < 0.0) {
                corners[t as usize].swap(1, 2);
                turned += 1;
            }
        }
    }
    turned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_triangle_and_the_winding_number_agree_with_trying_every_triangle() {
        let mesh = made::lumpy(24);
        let (mut inside, mut worst) = (0, 0f64);
        for i in 0..3000 {
            let f = |k: f64| ((i as f64 * k).sin() * 43758.5453).fract();
            let p = DVec3::new(f(12.9898), f(78.233), f(37.719)) * 0.7;
            let slow = mesh.tris.iter().map(|t| nearest_on(t, p).distance_squared(p)).fold(f64::MAX, f64::min);
            let quick = mesh.nearest(p, f64::INFINITY).unwrap();
            assert!((quick.d2 - slow).abs() < 1e-12, "{p}: {} against {slow}", quick.d2);
            let (w, exact) = (mesh.winding(p), mesh.winding_exact(p));
            worst = worst.max((w - exact).abs());
            inside += (w > 0.5) as usize;
        }
        assert!(worst < 0.02, "the quick winding number is {worst} from the exact one at its worst");
        assert!(inside > 100 && inside < 2900, "{inside} of the points were inside: the test is looking at one side only");
    }

    #[test]
    fn faces_inside_out_are_turned_and_a_shell_inside_out_is_turned_whole() {
        let (points, mut triangles) = made::sphere_parts(16);
        // every third triangle turned over, and then all of them
        for (n, t) in triangles.iter_mut().enumerate() {
            if n % 3 == 0 {
                t.swap(1, 2);
            }
        }
        let some = Mesh::new(&points, &triangles, &[]).unwrap();
        assert_eq!(some.turned, triangles.len().div_ceil(3));
        triangles.iter_mut().for_each(|t| t.swap(1, 2));
        let rest = Mesh::new(&points, &triangles, &[]).unwrap();
        assert_eq!(rest.turned, triangles.len() - triangles.len().div_ceil(3));
        for mesh in [some, rest] {
            assert!((mesh.winding_exact(DVec3::ZERO) - 1.0).abs() < 1e-6 && mesh.winding_exact(DVec3::splat(0.6)).abs() < 1e-6);
            assert!((mesh.winding(DVec3::ZERO) - 1.0).abs() < 0.02 && mesh.winding(DVec3::splat(0.6)).abs() < 0.02);
        }
    }

    #[test]
    fn a_file_that_lies_is_refused() {
        let (points, triangles) = made::sphere_parts(4);
        assert!(Mesh::new(&points, &[[0, 1, points.len() as u32]], &[]).is_err(), "a triangle naming a point that is not there");
        let mut bad = points.clone();
        bad[2][1] = f32::NAN;
        assert!(Mesh::new(&bad, &triangles, &[]).is_err(), "a point that is not a number");
        assert!(Mesh::new(&points, &[], &[]).is_err() && Mesh::new(&points, &[[0, 0, 1]], &[]).is_err(), "no triangle with any area");
        assert!(Mesh::new(&points, &triangles, &[[1, 2, 3]]).is_err(), "colours for one point only");
    }
}
