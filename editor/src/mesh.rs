//! Triangle meshes for the brick shapes.
//!
//! A shape is the convex hull of some corners of a unit box, so its mesh is
//! found by brute force over those at most 8 points: every plane through
//! three of them that has all the others on one side is a face. This runs
//! once per shape at start-up; a general convex-hull routine would be more
//! code for the same meshes.
//!
//! One shape is not a convex hull: the inner corner, a valley (see
//! `arcs_core::shapes`). Its seven faces are written out instead.

use arcs_core::DVec3;

/// A shape's triangles, three unshared vertices each.
pub struct ShapeMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Per vertex, 0 on the rim of its face and 1 at the face's middle: the
    /// material draws a line where it nears zero.
    ///
    /// Each face is cut into triangles from its middle, so every triangle
    /// has exactly one edge on the rim, opposite the middle vertex, and one
    /// number per vertex is enough to say how near a pixel is to it. Cutting
    /// a face from one of its corners instead needs three numbers a vertex
    /// (some triangles then have two rim edges and some one), and the only
    /// spare three-number attribute is the vertex colour, which is wanted
    /// for the brick's own colour.
    pub rim: Vec<f32>,
}

/// Triangles for the shape with corner mask `mask`, in a unit box from
/// (0,0,0) to (1,1,1).
pub fn shape_mesh(mask: u8) -> ShapeMesh {
    use arcs_core::shapes::{shape_info, sloped_triangles, Kind};
    let (mut positions, mut normals, mut rim) = (Vec::new(), Vec::new(), Vec::new());
    // a triangle from the face's middle to each of its sides, its corners
    // taken in order around its normal
    let mut emit = |mut face: Vec<DVec3>, normal: DVec3| {
        let centre = face.iter().sum::<DVec3>() / face.len() as f64;
        let u = (face[0] - centre).normalize();
        let v = normal.cross(u);
        face.sort_by(|a, b| {
            let angle = |p: &DVec3| (*p - centre).dot(v).atan2((*p - centre).dot(u));
            angle(a).total_cmp(&angle(b))
        });
        for t in 0..face.len() {
            for (p, at_rim) in [(centre, 1.0), (face[t], 0.0), (face[(t + 1) % face.len()], 0.0)] {
                positions.push([p.x as f32, p.y as f32, p.z as f32]);
                normals.push([normal.x as f32, normal.y as f32, normal.z as f32]);
                rim.push(at_rim);
            }
        }
    };
    if let Some(info) = shape_info(mask).filter(|i| i.kind == Kind::Inner) {
        // In its own box: the floor, the two whole walls, the two walls cut
        // to triangles, and the two slopes that meet in the valley.
        let (slopes, within) = sloped_triangles(Kind::Inner);
        let c = |x: i32, y: i32, z: i32| [x as f64, y as f64, z as f64];
        let mut faces = vec![
            vec![c(0, 0, 0), c(1, 0, 0), c(1, 1, 0), c(0, 1, 0)],
            vec![c(0, 0, 0), c(0, 1, 0), c(0, 1, 1), c(0, 0, 1)],
            vec![c(0, 0, 0), c(1, 0, 0), c(1, 0, 1), c(0, 0, 1)],
            vec![c(1, 0, 0), c(1, 1, 0), c(1, 0, 1)],
            vec![c(0, 1, 0), c(1, 1, 0), c(0, 1, 1)],
        ];
        faces.extend(slopes.iter().map(|t| t.to_vec()));
        let inside = DVec3::from_array(info.world(within));
        for face in faces {
            let face: Vec<DVec3> = face.into_iter().map(|l| DVec3::from_array(info.world(l))).collect();
            let mut normal = (face[1] - face[0]).cross(face[2] - face[0]).normalize();
            if normal.dot(inside - face[0]) > 0.0 {
                normal = -normal;
            }
            emit(face, normal);
        }
        return ShapeMesh { positions, normals, rim };
    }
    let points: Vec<DVec3> = (0..8u8)
        .filter(|c| mask >> c & 1 == 1)
        .map(|c| DVec3::new((c & 1) as f64, (c >> 1 & 1) as f64, (c >> 2 & 1) as f64))
        .collect();
    let mut seen: Vec<(DVec3, f64)> = Vec::new();
    let n = points.len();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let mut normal = (points[j] - points[i]).cross(points[k] - points[i]);
                if normal.length_squared() < 1e-9 {
                    continue;
                }
                normal = normal.normalize();
                let side: Vec<f64> = points.iter().map(|p| (*p - points[i]).dot(normal)).collect();
                if side.iter().all(|s| *s > -1e-9) {
                    normal = -normal; // everything is in front: the face looks the other way
                } else if !side.iter().all(|s| *s < 1e-9) {
                    continue; // points on both sides: a cut through the shape, not a face
                }
                let offset = points[i].dot(normal);
                if seen.iter().any(|(n, o)| n.distance(normal) < 1e-6 && (o - offset).abs() < 1e-6) {
                    continue;
                }
                seen.push((normal, offset));

                emit(points.iter().copied().filter(|p| (p.dot(normal) - offset).abs() < 1e-9).collect(), normal);
            }
        }
    }
    ShapeMesh { positions, normals, rim }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcs_core::shapes::{all_masks, shape_info};

    #[test]
    fn every_shape_is_closed_and_has_its_volume() {
        for mask in all_masks() {
            let ShapeMesh { positions, normals, rim } = shape_mesh(mask);
            assert_eq!(positions.len() % 3, 0);
            // divergence theorem: a closed, outward-facing mesh's volume is
            // the sum over triangles of (a . (b x c)) / 6
            let mut volume = 0.0;
            for (t, n) in positions.chunks_exact(3).zip(normals.chunks_exact(3)) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                volume += a.dot(b.cross(c)) / 6.0;
                // wound anticlockwise seen from outside, matching its normal
                let facing = (b - a).cross(c - a).normalize();
                assert!(facing.distance(DVec3::new(n[0][0] as f64, n[0][1] as f64, n[0][2] as f64)) < 1e-6, "mask {mask:#x}");
            }
            let expected = shape_info(mask).unwrap().kind.volume();
            // to single precision: the faces' middles are not exact in it
            assert!((volume - expected).abs() < 1e-6, "mask {mask:#x}: volume {volume}, expected {expected}");

            // Lines fall exactly on the shape's real edges: each triangle's
            // rim edge (its two vertices marked 0) lies on the border of its
            // face, which is to say it is shared with a triangle of another
            // face, one with a different normal.
            let key = |p: [f32; 3]| p.map(|v| (v * 2.0).round() as i32);
            let mut rims = std::collections::HashMap::new();
            for (t, tri) in positions.chunks_exact(3).enumerate() {
                assert_eq!(&rim[t * 3..t * 3 + 3], &[1.0, 0.0, 0.0], "the middle vertex first, then the rim");
                let (mut p, mut q) = (key(tri[1]), key(tri[2]));
                if p > q {
                    std::mem::swap(&mut p, &mut q);
                }
                rims.entry((p, q)).or_insert_with(Vec::new).push(normals[t * 3]);
            }
            for (edge, faces) in rims {
                assert_eq!(faces.len(), 2, "mask {mask:#x}: edge {edge:?} is the rim of {} faces", faces.len());
                assert_ne!(faces[0], faces[1], "mask {mask:#x}: edge {edge:?} is not a crease");
            }
        }
    }
}
