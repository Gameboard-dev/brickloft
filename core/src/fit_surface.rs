//! A surface given by its height over the ground, laid out whole.
//!
//! The surface's height is taken at the corners of a square grid and rounded
//! to whole cells. Where no two corners next to each other differ by more
//! than one cell, every square of the grid is exact in bricks:
//!
//! | its four corners | bricks |
//! |---|---|
//! | level | none: only what is underneath |
//! | two next to each other higher | a ramp |
//! | one higher | a hip, or a tetrahedron if the surface is hollow there |
//! | three higher | an inner corner, or a cut cube if the surface bulges there |
//! | two opposite higher (a saddle) | two half outer corners along the ridge between them, or two tetrahedra if it is a valley |
//! | three heights (a slope across the diagonal) | a half outer corner below and a tetrahedron above, each on half the square |
//!
//! The half outer corner is what makes the last two rows possible: without
//! it a saddle is a step.
//!
//! "No more than one cell between neighbours" is a slope of at most one
//! cell up per square across. So the squares are made as large as the
//! surface's steepest slope allows: a surface that climbs one in three at
//! most is laid out on squares three cells across, and its slopes are long
//! ramps, not flats with a short ramp every third cell. Where the surface is
//! steeper than one in one it is worn down until it is not; the measurement
//! afterwards shows by how much.
//!
//! Underneath, the surface is either filled down to a floor or given a
//! thickness, straight down: the same surface again, lower, and each piece
//! of the top answered there by what it leaves of its box.

use crate::fit_round::rest_of_box;
use crate::fit_stack::valley;
use crate::fit_volume::merge;
use crate::shapes::{shape_info, CellBrick, CUBE};

/// A surface to build. Everything is in cells.
pub struct Surface<'a> {
    /// how high the surface is over the point (x, y)
    pub height: &'a dyn Fn(f64, f64) -> f64,
    /// the ground it covers: low corner and high, on the lattice
    pub min: [i32; 2],
    pub max: [i32; 2],
    /// solid down to this level; or, with `skin`, ignored
    pub floor: i32,
    /// how thick it is, straight down, if it is not filled to the floor
    pub skin: Option<i32>,
    pub max_ext: i32,
}

/// The largest square the surface can be laid out on without being worn
/// down, in cells: one over its steepest slope along x or y, and a divisor
/// of both its sides.
pub fn square(surface: &Surface) -> i32 {
    let (w, h) = (surface.max[0] - surface.min[0], surface.max[1] - surface.min[1]);
    let f = |x: i32, y: i32| (surface.height)(x as f64, y as f64);
    let mut steepest: f64 = 0.0;
    for y in surface.min[1]..=surface.max[1] {
        for x in surface.min[0]..=surface.max[0] {
            if x < surface.max[0] {
                steepest = steepest.max((f(x + 1, y) - f(x, y)).abs());
            }
            if y < surface.max[1] {
                steepest = steepest.max((f(x, y + 1) - f(x, y)).abs());
            }
        }
    }
    // (a slope of exactly one in three must not come out as a little over it)
    let most = if steepest > 0.125 { (1.0 / steepest + 1e-6).floor() as i32 } else { 8 };
    (1..=most.clamp(1, 8).min(surface.max_ext.max(1))).rev().find(|g| w % g == 0 && h % g == 0).unwrap_or(1)
}

/// The bricks of `surface`, or `None` if it covers no ground.
pub fn fit_surface(surface: &Surface) -> Option<Vec<CellBrick>> {
    let (w, h) = (surface.max[0] - surface.min[0], surface.max[1] - surface.min[1]);
    if w < 1 || h < 1 {
        return None;
    }
    let limit = surface.max_ext.max(1);
    let g = square(surface);
    let (nx, ny) = ((w / g) as usize, (h / g) as usize);
    let at = |i: usize, j: usize| ((surface.min[0] + i as i32 * g) as f64, (surface.min[1] + j as i32 * g) as f64);
    // a filled surface stands at least a cell above its floor
    let least = if surface.skin.is_none() { surface.floor + 1 } else { i32::MIN };
    let mut height: Vec<i32> = (0..(nx + 1) * (ny + 1))
        .map(|k| {
            let (x, y) = at(k % (nx + 1), k / (nx + 1));
            ((surface.height)(x, y).round() as i32).max(least)
        })
        .collect();
    // Worn down until no corner is more than a cell above a neighbour. Only
    // ever lowered, and each corner to the same rule, so a surface that is
    // its own mirror image stays so.
    loop {
        let mut changed = false;
        for j in 0..=ny {
            for i in 0..=nx {
                let k = i + (nx + 1) * j;
                let mut lowest = i32::MAX;
                for (di, dj) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (i as i32 + di, j as i32 + dj);
                    if a >= 0 && b >= 0 && a <= nx as i32 && b <= ny as i32 {
                        lowest = lowest.min(height[a as usize + (nx + 1) * b as usize]);
                    }
                }
                if lowest < i32::MAX && height[k] > lowest + 1 {
                    height[k] = lowest + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    let thick = surface.skin.map(|t| t.max(2));
    let top = *height.iter().max()?;
    let bottom = match thick {
        Some(t) => *height.iter().min()? - t,
        None => surface.floor,
    };
    let layers = (top - bottom).max(1) as usize;
    // whole squares of solid, to be joined into as few cubes as may be
    let mut cubes = vec![0u8; nx * ny * layers];
    let mut bricks: Vec<CellBrick> = Vec::new();

    for j in 0..ny {
        for i in 0..nx {
            // the square's corners, numbered as a brick's are: x, then y
            let c = [height[i + (nx + 1) * j], height[i + 1 + (nx + 1) * j], height[i + (nx + 1) * (j + 1)], height[i + 1 + (nx + 1) * (j + 1)]];
            let (lo, hi) = (*c.iter().min().unwrap(), *c.iter().max().unwrap());
            let (x, y) = (surface.min[0] + i as i32 * g, surface.min[1] + j as i32 * g);
            // how far up from `lo` to `hi` the true surface is over the middle of the square
            let middle = ((surface.height)(x as f64 + g as f64 / 2.0, y as f64 + g as f64 / 2.0) - lo as f64) / (hi - lo).max(1) as f64;
            let high = (0..4).filter(|k| c[*k] == hi).fold(0u8, |m, k| m | 1 << k);
            // The square in parts: the corners each part stands on (all
            // four, or three for a triangle), the level its top piece
            // starts from, the corners that piece reaches a cell higher (0
            // for no piece), and whether the piece is a valley.
            let mut parts: Vec<(u8, i32, u8, bool)> = Vec::new();
            // a triangle: three corners, by the one it lacks
            let without = |k: usize| 15 & !(1u8 << k);
            if hi == lo {
                parts.push((15, lo, 0, false));
            } else if hi - lo == 1 && c.iter().all(|v| *v == lo || *v == hi) {
                match high.count_ones() {
                    // a ramp, or a saddle
                    2 if high == 0b0110 || high == 0b1001 => {
                        let low = 15 & !high;
                        let (first, second) = (low.trailing_zeros() as usize, 7 - low.leading_zeros() as usize);
                        if middle > 0.5 {
                            // a ridge between the two high corners: each half rises to it
                            parts.push((without(first), lo, high, false));
                            parts.push((without(second), lo, high, false));
                        } else {
                            // a valley between the two low ones: each half has one corner up
                            let (a, b) = (high.trailing_zeros() as usize, 7 - high.leading_zeros() as usize);
                            parts.push((without(b), lo, 1 << a, false));
                            parts.push((without(a), lo, 1 << b, false));
                        }
                    }
                    2 => parts.push((15, lo, high, false)),
                    1 => {
                        let k = high.trailing_zeros() as usize;
                        if middle > 0.25 {
                            parts.push((15, lo, high, false)); // a hip
                        } else {
                            // only the half at the high corner rises: the corner opposite is the one it lacks
                            parts.push((without(3 - k), lo, high, false));
                            parts.push((without(k), lo, 0, false));
                        }
                    }
                    _ => parts.push((15, lo, high, middle < 0.75)),
                }
            } else {
                // Three heights. Corners 0 and 3 are opposite, and 1 and 2;
                // one pair is level with itself, and each of the other two
                // corners makes a triangle with it, rising to it or from it.
                let pair = if c[0] == c[3] { Some((0, 3)) } else if c[1] == c[2] { Some((1, 2)) } else { None };
                match pair {
                    Some((a, b)) => {
                        let level = c[a];
                        for r in (0..4).filter(|r| *r != a && *r != b) {
                            let triangle = without(3 - r);
                            match c[r].cmp(&level) {
                                std::cmp::Ordering::Greater => parts.push((triangle, level, 1 << r, false)),
                                std::cmp::Ordering::Less => parts.push((triangle, c[r], 1 << a | 1 << b, false)),
                                std::cmp::Ordering::Equal => parts.push((triangle, level, 0, false)),
                            }
                        }
                    }
                    // cannot be, once worn down; if it were, a flat top at the lowest corner
                    None => parts.push((15, lo, 0, false)),
                }
            }

            for (foot, from, up, in_valley) in parts {
                let mut brick = |mask: u8, z0: i32, z1: i32| {
                    if shape_info(mask).is_none() || z1 <= z0 {
                        return;
                    }
                    // an upright piece may be cut to length; a sloped one is one cell tall
                    let mut z = z0;
                    while z < z1 {
                        let run = (z1 - z).min(limit);
                        bricks.push(CellBrick { mask, min: [x, y, z], ext: [g, g, run] });
                        z += run;
                    }
                };
                // solid from `z0` to `z1` over the part's own ground
                let mut column = |z0: i32, z1: i32, bricks_too: &mut dyn FnMut(u8, i32, i32)| {
                    if foot == 15 {
                        for z in z0.max(bottom)..z1 {
                            cubes[i + nx * (j + ny * (z - bottom) as usize)] = CUBE;
                        }
                    } else {
                        bricks_too(foot | foot << 4, z0, z1);
                    }
                };
                // the top piece, in the cell above `from`
                let piece = if up == 0 {
                    0
                } else if in_valley {
                    valley(foot | up << 4, 2)
                } else {
                    foot | up << 4
                };
                let peak = if up == 0 { from } else { from + 1 };
                brick(piece, from, peak);
                match thick {
                    None => column(bottom, from, &mut brick),
                    Some(t) => {
                        // the same piece's place in the under side, `t` lower: what it leaves of its box there
                        let under = if up == 0 {
                            0
                        } else if foot == 15 {
                            rest_of_box(piece)
                        } else {
                            (foot & !up) | foot << 4
                        };
                        brick(under, from - t, peak - t);
                        column(peak - t, from, &mut brick);
                    }
                }
            }
        }
    }
    for b in merge(cubes, [nx, ny, layers], (limit / g).max(1) as usize) {
        bricks.push(CellBrick { mask: CUBE, min: [surface.min[0] + b.min[0] * g, surface.min[1] + b.min[1] * g, bottom + b.min[2]], ext: [b.ext[0] * g, b.ext[1] * g, b.ext[2]] });
    }
    (!bricks.is_empty()).then_some(bricks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{inside_shape, Kind};

    fn count(bricks: &[CellBrick], kind: Kind) -> usize {
        bricks.iter().filter(|b| shape_info(b.mask).unwrap().kind == kind).count()
    }

    /// How many of the bricks hold `p`.
    fn holding(bricks: &[CellBrick], p: [f64; 3]) -> usize {
        bricks
            .iter()
            .filter(|b| {
                let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
                t.iter().all(|v| (0.0..1.0).contains(v)) && inside_shape(shape_info(b.mask).unwrap(), t[0], t[1], t[2])
            })
            .count()
    }

    /// Every point of a grid is in at most one brick, and in one exactly
    /// when it is under `top` and over `under`, to within `slack` cells.
    fn follows(bricks: &[CellBrick], s: &Surface, top: &dyn Fn(f64, f64) -> f64, under: &dyn Fn(f64, f64) -> f64, slack: f64) {
        let n = 37;
        let (z0, z1) = (bricks.iter().map(|b| b.min[2]).min().unwrap() as f64 - 1.0, bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() as f64 + 1.0);
        for k in 0..n * n * n {
            // (not the same way along x as along y, or every sample on the diagonal would lie on the face between two half squares)
            let u = [(k % n) as f64 + 0.31, (k / n % n) as f64 + 0.47, (k / (n * n)) as f64 + 0.31].map(|v| v / n as f64);
            let p = [s.min[0] as f64 + u[0] * (s.max[0] - s.min[0]) as f64, s.min[1] as f64 + u[1] * (s.max[1] - s.min[1]) as f64, z0 + u[2] * (z1 - z0)];
            let held = holding(bricks, p);
            assert!(held <= 1, "{held} bricks hold {p:?}");
            let (over, below) = (p[2] - top(p[0], p[1]), under(p[0], p[1]) - p[2]);
            if over < -slack && below < -slack {
                assert_eq!(held, 1, "a hole at {p:?}");
            }
            if over > slack || below > slack {
                assert_eq!(held, 0, "a brick at {p:?}, outside the surface");
            }
        }
    }

    #[test]
    fn a_saddle_is_half_outer_corners_and_no_steps() {
        // z = 10 + (x² - y²) / 12 over 24 by 24: a slope of one at its rim
        let saddle = |x: f64, y: f64| 10.0 + (x * x - y * y) / 12.0;
        let s = Surface { height: &saddle, min: [-6, -6], max: [6, 6], floor: 0, skin: None, max_ext: 80 };
        let bricks = fit_surface(&s).unwrap();
        assert_eq!(square(&s), 1);
        assert!(count(&bricks, Kind::HalfOuter) >= 4, "{} half outer corners", count(&bricks, Kind::HalfOuter));
        // no brick stands a whole cell above the surface or leaves a cell-deep hole in it
        follows(&bricks, &s, &saddle, &|_, _| 0.0, 1.0);
        // the same turned a quarter and upside down is its own mirror image: so are the bricks
        let cells: std::collections::HashSet<[i32; 3]> = bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).flat_map(move |y| (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1] + y, b.min[2] + z])))).collect();
        assert!(cells.iter().all(|c| cells.contains(&[-1 - c[0], c[1], c[2]]) && cells.contains(&[c[0], -1 - c[1], c[2]])), "not the same on both sides");
    }

    #[test]
    fn a_gentle_slope_is_long_ramps() {
        // one up in three across, along x: squares three cells wide, and nothing but ramps on top
        let slope = |x: f64, _: f64| 4.0 + x / 3.0;
        let s = Surface { height: &slope, min: [0, 0], max: [12, 6], floor: 0, skin: None, max_ext: 80 };
        assert_eq!(square(&s), 3);
        let bricks = fit_surface(&s).unwrap();
        assert!(bricks.iter().all(|b| matches!(shape_info(b.mask).unwrap().kind, Kind::Cube | Kind::Ramp)));
        assert!(bricks.iter().filter(|b| b.mask != CUBE).all(|b| b.ext == [3, 3, 1]));
        follows(&bricks, &s, &slope, &|_, _| 0.0, 0.01);
        // a slope both ways at once is half outer corners below and tetrahedra above, exactly
        let tilted = |x: f64, y: f64| 6.0 + x + y;
        let s = Surface { height: &tilted, min: [-3, -3], max: [3, 3], floor: -8, skin: None, max_ext: 80 };
        let bricks = fit_surface(&s).unwrap();
        assert!(count(&bricks, Kind::HalfOuter) >= 30 && count(&bricks, Kind::Tet) >= 30);
        follows(&bricks, &s, &tilted, &|_, _| -8.0, 0.01);
    }

    #[test]
    fn a_skin_is_the_surface_twice() {
        // a dome-like bump, three thick: every piece on top has its answer underneath
        let bump = |x: f64, y: f64| 12.0 - (x * x + y * y) / 16.0;
        let s = Surface { height: &bump, min: [-8, -8], max: [8, 8], floor: 0, skin: Some(3), max_ext: 80 };
        let bricks = fit_surface(&s).unwrap();
        assert!(count(&bricks, Kind::Hip) >= 1 && count(&bricks, Kind::Inner) >= 1);
        follows(&bricks, &s, &bump, &|x, y| bump(x, y) - 3.0, 1.0);
        // steeper than one in one, it is worn down, not stepped
        let steep = |x: f64, _: f64| 3.0 * x.abs();
        let s = Surface { height: &steep, min: [-4, -2], max: [4, 2], floor: 0, skin: None, max_ext: 80 };
        let bricks = fit_surface(&s).unwrap();
        assert_eq!(bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap(), 5, "one cell up for each across, from 1 in the middle");
    }
}
