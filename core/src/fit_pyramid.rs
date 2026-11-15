//! A square or oblong pyramid, laid out whole.
//!
//! A pyramid is four flat slopes meeting along four ridges, so it is built
//! exactly, with no fitting: a ramp for each slope and, on each ridge, the
//! one shape that has two sloped faces, the hip (`shapes::Kind::Hip`). A
//! pyramid whose base is centred on a lattice point is four hips and nothing
//! else. Centred in a cell, as points usually are, it has a top one cell
//! square, and so a ramp between each pair of hips and a column up the middle.
//!
//! Without the hip a ridge cannot be built. The four other shapes give a
//! tetrahedron there, which cuts the ridge off flat and turns each layer's
//! corner into a notch.
//!
//! The layout is the one `fit_round` uses, with a different rule: each axis
//! is cut at distances numbered outwards from the middle, the height is cut
//! at levels numbered up from the base, and a corner of the resulting boxes
//! is inside when the larger of its two numbers across plus its number up is
//! at most the count of levels. One level is enough for any pyramid a single
//! brick can span; a larger one is cut into as few levels as keep every brick
//! within the longest the game allows.

use crate::fit_round::{bricks_of, through};
use crate::shapes::{shape_info, CellBrick};
use glam::DVec3;

/// A pyramid to build. Everything is in cells.
#[derive(Clone, Copy, Debug)]
pub struct Pyramid {
    /// the middle of its base
    pub base: DVec3,
    /// half its base's size along x and along y
    pub half: [f64; 2],
    pub height: f64,
    pub max_ext: i32,
}

/// A pyramid as it lands on the lattice.
#[derive(Clone, Copy, Debug)]
pub struct Snapped {
    /// half the size of its flat top along x and y: 0, or half a cell where
    /// the middle of the base is in the middle of a cell
    pub top: [f64; 2],
    pub half: [f64; 2],
    /// the level of its base, and its height
    pub floor: i32,
    pub height: i32,
}

impl Pyramid {
    /// Where the lattice puts it: its base's edges and its base and apex on
    /// lattice planes. `None` if its middle is not on the half-cell grid.
    pub fn snapped(&self) -> Option<Snapped> {
        let c = [self.base.x, self.base.y];
        let top = [0, 1].map(|k| (c[k] - c[k].floor()).min(c[k].ceil() - c[k]));
        if top.iter().any(|t| *t != 0.0 && *t != 0.5) || !self.height.is_finite() || self.half.iter().any(|h| !h.is_finite()) {
            return None;
        }
        // at least a cell of slope each way
        let half = [0, 1].map(|k| top[k] + (self.half[k] - top[k]).round().max(1.0));
        Some(Snapped { top, half, floor: self.base.z.floor() as i32, height: (self.height.round() as i32).max(1) })
    }
}

/// The bricks of `pyramid`, or `None` where it cannot be laid out: its middle
/// off the half-cell grid.
pub fn fit_pyramid(pyramid: &Pyramid) -> Option<Vec<CellBrick>> {
    let at = pyramid.snapped()?;
    let limit = pyramid.max_ext.max(1);
    let run = [0, 1].map(|k| (at.half[k] - at.top[k]) as i32);
    // as few levels as keep every brick within the limit, and no more than
    // there are cells to give each level one
    let needed = |length: i32| (length + limit - 1) / limit;
    let levels = needed(run[0]).max(needed(run[1])).max(needed(at.height)).min(run[0]).min(run[1]).min(at.height).max(1);
    // level i of n along a length: evenly, to the nearest cell
    let part = |length: i32, i: i32| (length as f64 * i as f64 / levels as f64).round();
    let across = |k: usize| -> Vec<(i32, usize)> {
        let t: Vec<f64> = (0..=levels).map(|i| at.top[k] + part(run[k], i)).collect();
        through([pyramid.base.x, pyramid.base.y][k], &t)
    };
    let lines = [across(0), across(1), (0..=levels).map(|l| (at.floor + part(at.height, l) as i32, l as usize)).collect()];
    let n = [0, 1, 2].map(|k| lines[k].len() - 1);
    let mut held = vec![0u8; n[0] * n[1] * n[2]];
    for z in 0..n[2] {
        for y in 0..n[1] {
            for x in 0..n[0] {
                let mask = (0..8u8).fold(0u8, |m, corner| {
                    let number = |k: usize| lines[k][[x, y, z][k] + (corner >> k & 1) as usize].1;
                    m | ((number(0).max(number(1)) + number(2) <= levels as usize) as u8) << corner
                });
                // corners that make no brick are an edge or a face of the solid, with nothing in the box
                held[x + n[0] * (y + n[1] * z)] = if shape_info(mask).is_some() { mask } else { 0 };
            }
        }
    }
    bricks_of(held, lines.map(|line| line.into_iter().map(|cut| cut.0).collect()), limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{inside_shape, Kind};

    fn count(bricks: &[CellBrick], kind: Kind) -> usize {
        bricks.iter().filter(|b| shape_info(b.mask).unwrap().kind == kind).count()
    }

    /// Whether `p` is in the bricks.
    fn solid(bricks: &[CellBrick], p: DVec3) -> bool {
        bricks.iter().any(|b| {
            let t = [0, 1, 2].map(|k| (p[k] - b.min[k] as f64) / b.ext[k] as f64);
            t.iter().all(|v| (0.0..1.0).contains(v)) && inside_shape(shape_info(b.mask).unwrap(), t[0], t[1], t[2])
        })
    }

    #[test]
    fn a_pyramid_on_a_lattice_point_is_four_hips() {
        let bricks = fit_pyramid(&Pyramid { base: DVec3::new(4.0, -2.0, 1.0), half: [6.0, 9.0], height: 5.0, max_ext: 80 }).unwrap();
        assert_eq!((bricks.len(), count(&bricks, Kind::Hip)), (4, 4));
        assert!(bricks.iter().all(|b| b.ext == [6, 9, 5] && b.min[2] == 1));
    }

    #[test]
    fn a_pyramid_is_exactly_its_four_slopes() {
        for (base, half, height, max_ext) in [(DVec3::new(0.5, 0.5, 0.5), [8.5, 8.5], 8.0, 80), (DVec3::new(0.5, 3.0, 2.0), [6.5, 4.0], 9.0, 80), (DVec3::new(0.0, 0.0, 0.0), [30.0, 20.0], 24.0, 8)] {
            let pyramid = Pyramid { base, half, height, max_ext };
            let (bricks, at) = (fit_pyramid(&pyramid).unwrap(), pyramid.snapped().unwrap());
            assert!(bricks.iter().all(|b| b.ext.iter().all(|e| *e <= max_ext)), "a brick is longer than {max_ext}");
            assert!(count(&bricks, Kind::Hip) >= 4 && count(&bricks, Kind::Tet) + count(&bricks, Kind::Cut) == 0);
            // every point of a grid: in the bricks exactly when under all four slopes
            let reach = at.half[0].max(at.half[1]) + 1.0;
            let n = 60;
            for i in 0..n * n * n {
                let u = [i % n, i / n % n, i / (n * n)].map(|v| (v as f64 + 0.31) / n as f64);
                let p = DVec3::new(base.x + (2.0 * u[0] - 1.0) * reach, base.y + (2.0 * u[1] - 1.0) * reach, at.floor as f64 - 1.0 + u[2] * (at.height as f64 + 2.0));
                let up = (p.z - at.floor as f64) / at.height as f64;
                let under = |k: usize| ([p.x - base.x, p.y - base.y][k]).abs() < at.top[k] + (at.half[k] - at.top[k]) * (1.0 - up);
                let truly = (0.0..1.0).contains(&up) && under(0) && under(1);
                // a pyramid cut into levels has its cuts rounded to the lattice, which bends its slopes a little
                let near = max_ext < 80 && (0..2).any(|k| (([p.x - base.x, p.y - base.y][k]).abs() - at.top[k] - (at.half[k] - at.top[k]) * (1.0 - up)).abs() < 0.75);
                assert!(solid(&bricks, p) == truly || near, "{pyramid:?}: {p} is {} and should be {}", solid(&bricks, p), truly);
            }
        }
    }

    #[test]
    fn a_pyramid_off_the_grid_is_left_to_the_other_fitter() {
        assert!(fit_pyramid(&Pyramid { base: DVec3::new(0.3, 0.5, 0.0), half: [6.0, 6.0], height: 5.0, max_ext: 80 }).is_none());
    }
}
