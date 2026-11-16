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

/// A ball painted in three bands by its points: red over the top third of
/// its bands of triangles, white round the middle, blue below. The colours
/// run into each other across the one band of triangles between two.
pub fn painted_ball(n: usize) -> Mesh {
    let (points, triangles) = sphere_parts(n);
    let colours: Vec<[u8; 3]> = points.iter().map(|p| if p[2] > 0.26 { [200, 30, 30] } else if p[2] < -0.26 { [30, 60, 200] } else { [240, 240, 240] }).collect();
    Mesh::new(&points, &triangles, &colours).expect("a painted ball")
}

/// A ball with a different colour at every point, by where the point is:
/// as many colours as a painted model has.
pub fn rainbow_ball(n: usize) -> Mesh {
    let (points, triangles) = sphere_parts(n);
    let colours: Vec<[u8; 3]> = points.iter().map(|p| p.map(|v| ((v + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8)).collect();
    Mesh::new(&points, &triangles, &colours).expect("a rainbow ball")
}

/// One sphere of `n` bands, `across` wide about `middle`, added to `parts`.
fn add_ball(parts: &mut Parts, n: usize, middle: DVec3, across: f64) {
    let (points, triangles) = sphere_parts(n);
    let first = parts.0.len() as u32;
    parts.0.extend(points.iter().map(|p| point(middle + DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64) * across)));
    parts.1.extend(triangles.iter().map(|t| t.map(|v| v + first)));
}

/// A ball 50 across with three scraps floating beside it, 10, 6 and 3.4
/// across: 100 along its longest side. What a model made from one
/// photograph has round it.
pub fn scattered() -> Mesh {
    let mut parts = (Vec::new(), Vec::new());
    add_ball(&mut parts, 16, DVec3::new(-25.0, 0.0, 0.0), 50.0);
    add_ball(&mut parts, 8, DVec3::new(15.3, 0.3, 0.4), 10.0);
    add_ball(&mut parts, 8, DVec3::new(32.3, 0.3, 0.4), 6.0);
    add_ball(&mut parts, 6, DVec3::new(48.3, 0.5, 0.5), 3.4);
    Mesh::new(&parts.0, &parts.1, &[]).expect("a ball and its scraps")
}

/// The surface where `solid` (negative inside, positive outside) is
/// nothing, within the box from `lo` to `hi`, found on a grid of `n` cells
/// along the box's longest side; each point coloured by `colour`.
///
/// This is how a model made by a network comes to be a mesh: the network
/// answers "inside or outside" at the points of a grid, and the surface
/// is drawn between the points that differ. So the meshes made here have
/// what those have: one skin and no parts, many small triangles of much
/// the same size whatever the surface does, flat faces that come out
/// faintly ribbed, and corners rounded off to the grid's size.
///
/// Each cell of the grid is cut into six tetrahedra and the surface drawn
/// through each (marching tetrahedra: Doi and Koide 1991, "An efficient
/// method of triangulating equi-valued surfaces by using tetrahedral
/// cells", IEICE Transactions E74). Marching cubes, which the networks'
/// own code uses, has fifteen cases and a table of 256 rows to get them
/// from; a tetrahedron has three cases and needs no table, and what comes
/// out differs only in how the triangles are cut.
pub fn surface_of(solid: impl Fn(DVec3) -> f64, colour: impl Fn(DVec3) -> [u8; 3], lo: DVec3, hi: DVec3, n: usize) -> Mesh {
    use std::collections::HashMap;
    let step = (hi - lo).max_element() / n as f64;
    let counts = ((hi - lo) / step).ceil().to_array().map(|v| v as usize + 1);
    let at = |i: [usize; 3]| lo + DVec3::new(i[0] as f64, i[1] as f64, i[2] as f64) * step;
    let number = |i: [usize; 3]| (i[0] + (counts[0] + 1) * (i[1] + (counts[1] + 1) * i[2])) as u64;
    let mut values: HashMap<u64, f64> = HashMap::new();
    let mut points: Vec<[f32; 3]> = Vec::new();
    let mut colours: Vec<[u8; 3]> = Vec::new();
    // the point where the surface crosses the line between two points of the grid, made once for the two
    let mut on_edge: HashMap<(u64, u64), u32> = HashMap::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    // a cube's corners, and the six tetrahedra that share its long diagonal from corner 0 to corner 7
    let corner = |c: usize| [c & 1, c >> 1 & 1, c >> 2 & 1];
    const TETRAHEDRA: [[usize; 4]; 6] = [[0, 1, 3, 7], [0, 3, 2, 7], [0, 2, 6, 7], [0, 6, 4, 7], [0, 4, 5, 7], [0, 5, 1, 7]];
    for z in 0..counts[2] {
        for y in 0..counts[1] {
            for x in 0..counts[0] {
                let places: [[usize; 3]; 8] = std::array::from_fn(|c| { let o = corner(c); [x + o[0], y + o[1], z + o[2]] });
                let here: [f64; 8] = std::array::from_fn(|c| *values.entry(number(places[c])).or_insert_with(|| solid(at(places[c]))));
                if here.iter().all(|v| *v < 0.0) || here.iter().all(|v| *v >= 0.0) {
                    continue;
                }
                for tetrahedron in TETRAHEDRA {
                    let inside: Vec<usize> = tetrahedron.iter().copied().filter(|c| here[*c] < 0.0).collect();
                    let outside: Vec<usize> = tetrahedron.iter().copied().filter(|c| here[*c] >= 0.0).collect();
                    let mut crossing = |a: usize, b: usize| -> u32 {
                        let (na, nb) = (number(places[a]), number(places[b]));
                        *on_edge.entry((na.min(nb), na.max(nb))).or_insert_with(|| {
                            let (pa, pb) = (at(places[a]), at(places[b]));
                            let p = pa + (pb - pa) * (here[a] / (here[a] - here[b]));
                            points.push(point(p));
                            colours.push(colour(p));
                            points.len() as u32 - 1
                        })
                    };
                    match (inside.as_slice(), outside.as_slice()) {
                        ([i], [a, b, c]) | ([a, b, c], [i]) => triangles.push([crossing(*i, *a), crossing(*i, *b), crossing(*i, *c)]),
                        ([i, j], [a, b]) => {
                            let quad = [crossing(*i, *a), crossing(*i, *b), crossing(*j, *b), crossing(*j, *a)];
                            triangles.push([quad[0], quad[1], quad[2]]);
                            triangles.push([quad[0], quad[2], quad[3]]);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    // (which way each triangle faces is put right where the mesh is made: see `oriented`)
    Mesh::new(&points, &triangles, &colours).expect("a surface")
}

/// A little figure standing on a disc: a head, a body, two arms and two
/// legs run together into one skin, as a model made from a photograph of a
/// toy is, with a colour for each part shaded as if lit from one side.
/// 100 tall; `n` is how fine the grid it is drawn on is (see [`surface_of`]).
pub fn figure(n: usize) -> Mesh {
    // distance from the line between two points, less a radius: a limb
    let limb = |p: DVec3, a: DVec3, b: DVec3, r: f64| {
        let along = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
        (p - a - (b - a) * along).length() - r
    };
    // the smaller of two distances, rounded where they meet: https://iquilezles.org/articles/smin/
    let run = |a: f64, b: f64, k: f64| {
        let h = (k - (a - b).abs()).max(0.0) / k;
        a.min(b) - h * h * k / 4.0
    };
    let parts = move |p: DVec3| -> [f64; 5] {
        let head = (p - DVec3::new(0.0, 0.0, 82.0)).length() - 17.0;
        let body = ((p - DVec3::new(0.0, 0.0, 50.0)) / DVec3::new(17.0, 12.0, 22.0)).length() * 12.0 - 12.0;
        let arms = limb(p, DVec3::new(-16.0, 0.0, 62.0), DVec3::new(-30.0, 4.0, 40.0), 5.0).min(limb(p, DVec3::new(16.0, 0.0, 62.0), DVec3::new(32.0, -8.0, 70.0), 5.0));
        let legs = limb(p, DVec3::new(-8.0, 0.0, 32.0), DVec3::new(-10.0, 0.0, 6.0), 6.5).min(limb(p, DVec3::new(8.0, 0.0, 32.0), DVec3::new(11.0, 2.0, 6.0), 6.5));
        // (the disc: flat above and below, round at its rim)
        let disc = (p.truncate().length() - 30.0).max((p.z - 2.0).abs() - 2.0);
        [head, body, arms, legs, disc]
    };
    let solid = move |p: DVec3| {
        let [head, body, arms, legs, disc] = parts(p);
        run(run(run(run(head, body, 6.0), arms, 5.0), legs, 5.0), disc, 3.0)
    };
    let colour = move |p: DVec3| {
        let of = parts(p);
        let nearest = (0..5).min_by(|a, b| of[*a].total_cmp(&of[*b])).unwrap_or(0);
        let mut base = [[232.0, 190.0, 150.0], [190.0, 40.0, 44.0], [190.0, 40.0, 44.0], [40.0, 70.0, 150.0], [90.0, 150.0, 80.0]][nearest];
        // two eyes and a mouth on the face, which looks along -y; hands of the head's colour; hair above
        let face = p - DVec3::new(0.0, 0.0, 82.0);
        if nearest == 0 && face.y < -8.0 && ((face.x.abs() - 6.0).powi(2) + (face.z - 4.0).powi(2) < 6.0 || (face.x.abs() < 6.0 && (face.z + 6.0).abs() < 1.3)) {
            base = [30.0, 24.0, 24.0];
        } else if nearest == 0 && face.z > 7.0 + 0.4 * face.y {
            base = [70.0, 44.0, 26.0];
        } else if nearest == 2 && (p.x.abs() > 27.0) {
            base = [232.0, 190.0, 150.0];
        }
        // lit from the front, the left and above: the way the surface faces is the way the distance grows
        let e = 0.01;
        let facing = DVec3::new(solid(p + DVec3::X * e) - solid(p - DVec3::X * e), solid(p + DVec3::Y * e) - solid(p - DVec3::Y * e), solid(p + DVec3::Z * e) - solid(p - DVec3::Z * e)).normalize_or_zero();
        let light = 0.62 + 0.38 * facing.dot(DVec3::new(-0.4, -0.6, 0.7).normalize()).max(0.0);
        base.map(|v| (v * light).round().clamp(0.0, 255.0) as u8)
    };
    surface_of(solid, colour, DVec3::new(-36.0, -32.0, -1.0), DVec3::new(40.0, 32.0, 101.0), n)
}

/// A mesh of about `count` triangles, for timing: the lumpy ball cut finer.
pub fn large(count: usize) -> Mesh {
    lumpy(((count as f64 / 4.0).sqrt().ceil() as usize).max(4))
}
