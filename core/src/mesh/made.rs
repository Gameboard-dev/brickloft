//! Meshes made by rule, for the tests and for the pictures of what a fitter
//! makes of them. No model made by anyone else is kept in this repository,
//! so every mesh a test needs is written out here from a formula.
//!
//! Each is given in whole units where it has flat faces, so that at a size
//! in microbricks equal to its longest side its faces fall on the lattice,
//! and at any other size they do not.

use super::Mesh;
use glam::DVec3;
use std::f64::consts::PI;

type Parts = (Vec<[f32; 3]>, Vec<[u32; 3]>);

fn point(p: DVec3) -> [f32; 3] {
    p.to_array().map(|v| v as f32)
}

/// A sphere of latitude and longitude, `n` bands from pole to pole.
pub fn sphere_parts(n: usize) -> Parts {
    let (rows, around) = (n.max(2), 2 * n.max(2));
    let mut points = vec![point(DVec3::Z * 0.5)];
    for i in 1..rows {
        let polar = PI * i as f64 / rows as f64;
        for j in 0..around {
            let turn = 2.0 * PI * j as f64 / around as f64;
            points.push(point(DVec3::new(polar.sin() * turn.cos(), polar.sin() * turn.sin(), polar.cos()) * 0.5));
        }
    }
    points.push(point(-DVec3::Z * 0.5));
    let at = |i: usize, j: usize| (1 + (i - 1) * around + j % around) as u32;
    let last = points.len() as u32 - 1;
    let mut triangles = Vec::new();
    for j in 0..around {
        triangles.push([0, at(1, j), at(1, j + 1)]);
        triangles.push([last, at(rows - 1, j + 1), at(rows - 1, j)]);
        for i in 1..rows - 1 {
            triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
            triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
        }
    }
    (points, triangles)
}

pub fn sphere(n: usize) -> Mesh {
    let (points, triangles) = sphere_parts(n);
    Mesh::new(&points, &triangles, &[]).expect("a sphere")
}

/// A surface over a grid of `u` by `v` places that closes on itself both
/// ways (a torus), or along `u` only with `v` running from one pole to the
/// other (a ball pulled out of shape).
fn grid(u: usize, v: usize, at: impl Fn(f64, f64) -> DVec3) -> Parts {
    let points: Vec<[f32; 3]> = (0..u * v).map(|k| point(at((k % u) as f64 / u as f64, (k / u) as f64 / v as f64))).collect();
    let index = |i: usize, j: usize| (i % u + u * (j % v)) as u32;
    let mut triangles = Vec::new();
    for j in 0..v {
        for i in 0..u {
            triangles.push([index(i, j), index(i + 1, j), index(i + 1, j + 1)]);
            triangles.push([index(i, j), index(i + 1, j + 1), index(i, j + 1)]);
        }
    }
    (points, triangles)
}

/// A ring lying flat: 3 from its middle to the middle of its tube, which is 1 in radius.
pub fn torus(n: usize) -> Mesh {
    let (points, triangles) = grid(4 * n, 2 * n, |u, v| {
        let (turn, round) = (2.0 * PI * u, 2.0 * PI * v);
        DVec3::new((3.0 + round.cos()) * turn.cos(), (3.0 + round.cos()) * turn.sin(), round.sin())
    });
    Mesh::new(&points, &triangles, &[]).expect("a torus")
}

/// A ball with bumps and hollows all over it, as a thing modelled by hand
/// or by a network has: no flat face and no straight edge anywhere.
pub fn lumpy(n: usize) -> Mesh {
    let (mut points, triangles) = sphere_parts(n);
    for p in &mut points {
        let way = DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64).normalize();
        let bump = 1.0 + 0.18 * (5.0 * way.x + 1.0).sin() * (4.0 * way.y).cos() + 0.12 * (7.0 * way.z + 2.0).sin() + 0.1 * (3.0 * way.x * way.y * 4.0).cos();
        *p = point(way * bump);
    }
    Mesh::new(&points, &triangles, &[]).expect("a lumpy ball")
}

/// The eight corners and twelve triangles of a box from `lo` to `hi`, added to `parts`.
fn add_box(parts: &mut Parts, lo: DVec3, hi: DVec3, turn: impl Fn(DVec3) -> DVec3) {
    let first = parts.0.len() as u32;
    for c in 0..8 {
        parts.0.push(point(turn(DVec3::new(if c & 1 == 0 { lo.x } else { hi.x }, if c & 2 == 0 { lo.y } else { hi.y }, if c & 4 == 0 { lo.z } else { hi.z }))));
    }
    // each face as two triangles, counter-clockwise seen from outside
    for [a, b, c, d] in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        parts.1.push([first + a, first + b, first + c]);
        parts.1.push([first + a, first + c, first + d]);
    }
}

/// A box 40 by 24 by 16 turned `degrees` about the upright axis and then
/// `tilt` about x: square to nothing in the grid unless both are 0.
pub fn turned_box(degrees: f64, tilt: f64) -> Mesh {
    let (s, c) = degrees.to_radians().sin_cos();
    let (ts, tc) = tilt.to_radians().sin_cos();
    let mut parts = (Vec::new(), Vec::new());
    add_box(&mut parts, DVec3::new(-20.0, -12.0, -8.0), DVec3::new(20.0, 12.0, 8.0), |p| {
        let q = DVec3::new(p.x * c - p.y * s, p.x * s + p.y * c, p.z);
        DVec3::new(q.x, q.y * tc - q.z * ts, q.y * ts + q.z * tc)
    });
    Mesh::new(&parts.0, &parts.1, &[]).expect("a box")
}

/// A cut gem of eight flat faces: an octahedron 48 by 32 by 24, each face
/// the plane x/24 + y/16 + z/12 = 1 or one of its mirror images. Every
/// face slopes along all three axes at once, in whole numbers at size 48.
pub fn gem() -> Mesh {
    let points = [[24.0, 0.0, 0.0], [-24.0, 0.0, 0.0], [0.0, 16.0, 0.0], [0.0, -16.0, 0.0], [0.0, 0.0, 12.0], [0.0, 0.0, -12.0f32]];
    let triangles = [[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]];
    Mesh::new(&points, &triangles, &[]).expect("a gem")
}

/// A house: walls 48 by 32 and 16 high, a roof over them that rises 1 in 2
/// to a ridge along x and overhangs the walls by 4 all round, and a
/// chimney through one slope. Three closed shells that pass through each
/// other, as a model put together from parts is. 56 along its longest side.
pub fn house() -> Mesh {
    let mut parts = (Vec::new(), Vec::new());
    let same = |p: DVec3| p;
    add_box(&mut parts, DVec3::new(-24.0, -16.0, 0.0), DVec3::new(24.0, 16.0, 16.0), same);
    add_box(&mut parts, DVec3::new(10.0, 4.0, 14.0), DVec3::new(16.0, 10.0, 30.0), same);
    // the roof: a prism with a triangle at each end, from eave to eave over the ridge
    let first = parts.0.len() as u32;
    for x in [-28.0, 28.0] {
        for (y, z) in [(-20.0, 14.0), (20.0, 14.0), (0.0, 24.0)] {
            parts.0.push(point(DVec3::new(x, y, z)));
        }
    }
    for [a, b, c] in [[0, 2, 1], [3, 4, 5], [0, 1, 4], [0, 4, 3], [1, 2, 5], [1, 5, 4], [2, 0, 3], [2, 3, 5]] {
        parts.1.push([first + a, first + b, first + c]);
    }
    Mesh::new(&parts.0, &parts.1, &[]).expect("a house")
}

/// A mesh of about `count` triangles, for timing: the lumpy ball cut finer.
pub fn large(count: usize) -> Mesh {
    lumpy(((count as f64 / 4.0).sqrt().ceil() as usize).max(4))
}
