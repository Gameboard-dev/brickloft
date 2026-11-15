//! Solids as distance functions: negative inside, positive outside, and near
//! the surface the value is the distance to it in cells.
//!
//! The fitters call these millions of times, so the solids are plain enums
//! matched in place, and distance to a polyline goes through a bucket grid.

use glam::{DVec2, DVec3};

/// Smallest bucket of the polyline search grid, in cells, and the most
/// buckets a grid may have. A path spread through a large volume gets larger
/// buckets instead of more of them: the grid is an index, and one that
/// outgrew memory would be no use however fine it was.
const BUCKET: f64 = 4.0;
const MAX_BUCKETS: f64 = 2_000_000.0;

/// How far beyond a solid's surface its distance is still computed exactly.
/// Further out the answer is simply infinite ("far"), on the correct side.
///
/// Exact distances everywhere would mean searching every segment of a path
/// for every query; the fitters only need them near the surface. They do skip
/// whole blocks of cells whose centre is far, though, so this must stay above
/// `fit_volume`'s block half diagonal (it asserts that at compile time).
pub const FAR_MARGIN: f64 = 8.0;

/// Distance from a point to a polyline, for points within `reach` of it.
#[derive(Clone, Debug)]
pub struct PolyDist {
    segs: Vec<(DVec3, DVec3)>,
    origin: DVec3,
    /// bucket size, in cells
    bucket: f64,
    dims: [usize; 3],
    /// bucket b holds `items[start[b]..start[b + 1]]`
    start: Vec<u32>,
    items: Vec<u32>,
}

/// The nearest point of a polyline: its distance, segment and position along it.
#[derive(Clone, Copy)]
pub struct Hit {
    pub d: f64,
    pub seg: usize,
    pub t: f64,
}

impl PolyDist {
    /// `path` needs at least 2 points. Queries further than `reach` from the
    /// path answer "far" (an infinite distance) without searching.
    ///
    /// Segments are filed in a uniform grid of buckets, each listing the
    /// segments within `reach` of it, so a query reads one bucket instead of
    /// the whole path. The lists are one flat array indexed by bucket (CSR
    /// layout) rather than a `Vec` per bucket: one allocation, and a query's
    /// candidates are contiguous in memory.
    pub fn new(path: &[DVec3], reach: f64) -> Self {
        let segs: Vec<_> = path.windows(2).map(|w| (w[0], w[1])).collect();
        let lo = path.iter().fold(DVec3::MAX, |a, p| a.min(*p)) - reach;
        let hi = path.iter().fold(DVec3::MIN, |a, p| a.max(*p)) + reach;
        let size = hi - lo;
        let bucket = BUCKET.max((size.x * size.y * size.z / MAX_BUCKETS).cbrt());
        let dims = (size / bucket).to_array().map(|d| d.floor() as usize + 1);
        let range = |a: DVec3, b: DVec3| {
            let lo_cell = ((a.min(b) - reach - lo) / bucket).floor().max(DVec3::ZERO);
            let hi_cell = ((a.max(b) + reach - lo) / bucket).floor();
            let hi_cell = [0, 1, 2].map(|i| (hi_cell[i] as usize).min(dims[i] - 1));
            (lo_cell.to_array().map(|v| v as usize), hi_cell)
        };
        let index = |x: usize, y: usize, z: usize| x + dims[0] * (y + dims[1] * z);
        // two passes: count each bucket's segments, then fill
        let mut start = vec![0u32; dims[0] * dims[1] * dims[2] + 1];
        let each = |f: &mut dyn FnMut(usize, u32)| {
            for (s, &(a, b)) in segs.iter().enumerate() {
                let (l, h) = range(a, b);
                for z in l[2]..=h[2] {
                    for y in l[1]..=h[1] {
                        for x in l[0]..=h[0] {
                            f(index(x, y, z), s as u32);
                        }
                    }
                }
            }
        };
        each(&mut |b, _| start[b + 1] += 1);
        for b in 1..start.len() {
            start[b] += start[b - 1];
        }
        let mut items = vec![0u32; *start.last().unwrap() as usize];
        let mut next = start.clone();
        each(&mut |b, s| {
            items[next[b] as usize] = s;
            next[b] += 1;
        });
        PolyDist {
            segs,
            origin: lo,
            bucket,
            dims,
            start,
            items,
        }
    }

    pub fn segments(&self) -> usize {
        self.segs.len()
    }

    pub fn query(&self, p: DVec3) -> Hit {
        let far = Hit {
            d: f64::INFINITY,
            seg: usize::MAX,
            t: 0.0,
        };
        let c = ((p - self.origin) / self.bucket).floor();
        if c.min_element() < 0.0 {
            return far;
        }
        let c = c.to_array().map(|v| v as usize);
        if c[0] >= self.dims[0] || c[1] >= self.dims[1] || c[2] >= self.dims[2] {
            return far;
        }
        let b = c[0] + self.dims[0] * (c[1] + self.dims[1] * c[2]);
        let mut best = far;
        let mut best2 = f64::INFINITY;
        for &s in &self.items[self.start[b] as usize..self.start[b + 1] as usize] {
            let (a, e) = self.segs[s as usize];
            let ab = e - a;
            let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            let d2 = (a + ab * t).distance_squared(p);
            if d2 < best2 {
                best2 = d2;
                best = Hit {
                    d: 0.0,
                    seg: s as usize,
                    t,
                };
            }
        }
        if best.seg != usize::MAX {
            best.d = best2.sqrt();
        }
        best
    }
}

/// A polyline thickened to `half` on every side.
#[derive(Clone, Debug)]
pub struct Stroke {
    pd: PolyDist,
    half: f64,
    /// For an open path cut square at its ends: each end point and the
    /// direction pointing out of the path there.
    caps: Option<[(DVec3, DVec3); 2]>,
}

impl Stroke {
    /// `square_ends` cuts an open path flat at its two ends, as an arch's feet
    /// should be, after running each end `extend` further along its own
    /// direction; `None` rounds the ends off. A closed path has no ends.
    ///
    /// The extension exists because control points sit at cell centres: a
    /// band cut off exactly there would cover half of each end cell, and
    /// half a cell is the one amount no brick can represent.
    pub fn new(path: &[DVec3], half: f64, closed: bool, square_ends: Option<f64>) -> Self {
        let mut path = path.to_vec();
        let caps = square_ends.filter(|_| !closed).map(|extend| {
            let n = path.len();
            let out = [
                (path[0] - path[1]).normalize(),
                (path[n - 1] - path[n - 2]).normalize(),
            ];
            if extend > 0.0 {
                path.insert(0, path[0] + out[0] * extend);
                path.push(path[n] + out[1] * extend);
            }
            [(path[0], out[0]), (path[path.len() - 1], out[1])]
        });
        Stroke {
            pd: PolyDist::new(&path, half + FAR_MARGIN),
            half,
            caps,
        }
    }

    pub fn eval(&self, p: DVec3) -> f64 {
        let hit = self.pd.query(p);
        if hit.seg == usize::MAX {
            return f64::INFINITY; // far from the path: no segment, so no end to cut at
        }
        let mut g = hit.d - self.half;
        if let Some(caps) = &self.caps {
            // past an end, the nearest point of the path is that end itself
            // With a tolerance: a point level with the end has t equal to 0
            // or 1 only in exact arithmetic. Compared exactly, rounding put
            // one foot of an arch inside this test and the other outside it,
            // and the two feet were cut differently.
            const AT_END: f64 = 1e-9;
            let end = if hit.seg == 0 && hit.t <= AT_END {
                Some(caps[0])
            } else if hit.seg + 1 == self.pd.segments() && hit.t >= 1.0 - AT_END {
                Some(caps[1])
            } else {
                None
            };
            if let Some((at, out)) = end {
                g = g.max((p - at).dot(out));
            }
        }
        g
    }
}

/// The inside of a closed polygon in the plane.
#[derive(Clone, Debug)]
pub struct Fill {
    poly: Vec<DVec2>,
    pd: PolyDist,
    /// the polygon's bounding box
    lo: DVec2,
    hi: DVec2,
}

impl Fill {
    /// `poly` is closed implicitly: its last point joins its first.
    pub fn new(poly: Vec<DVec2>) -> Self {
        let mut path: Vec<DVec3> = poly.iter().map(|p| p.extend(0.0)).collect();
        path.push(path[0]);
        let lo = poly.iter().fold(DVec2::MAX, |a, p| a.min(*p));
        let hi = poly.iter().fold(DVec2::MIN, |a, p| a.max(*p));
        Fill {
            pd: PolyDist::new(&path, FAR_MARGIN),
            poly,
            lo,
            hi,
        }
    }

    pub fn eval(&self, p: DVec2) -> f64 {
        let d = self.pd.query(p.extend(0.0)).d;
        // Outside the bounding box a point is outside the polygon, with no
        // need to count crossings against every edge. For a small outline
        // in a large volume (a torus's cross-section) that is nearly every
        // query, and the counting was most of the fit's time.
        if p.x < self.lo.x || p.x > self.hi.x || p.y < self.lo.y || p.y > self.hi.y {
            return d;
        }
        // even-odd rule: count the edges a ray towards +x crosses
        let mut inside = false;
        let mut a = *self.poly.last().unwrap();
        for &b in &self.poly {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
            a = b;
        }
        if inside {
            -d
        } else {
            d
        }
    }
}

/// A solid in the plane.
#[derive(Clone, Debug)]
pub enum Region {
    Stroke(Stroke),
    Fill(Fill),
}

impl Region {
    pub fn eval(&self, p: DVec2) -> f64 {
        match self {
            Region::Stroke(s) => s.eval(p.extend(0.0)),
            Region::Fill(f) => f.eval(p),
        }
    }
}

/// A solid in space.
#[derive(Clone, Debug)]
pub enum Field {
    /// a round tube along a path
    Tube(Stroke),
    /// A flat band along a path lying in the plane through `origin` with
    /// normal `normal`: `stroke` wide in the plane, `half_depth` to each side.
    Band {
        stroke: Stroke,
        origin: DVec3,
        normal: DVec3,
        half_depth: f64,
    },
    /// An ellipsoid, optionally hollow, optionally cut to `lo <= z <= hi`.
    Ellipsoid {
        center: DVec3,
        radii: DVec3,
        shell: Option<f64>,
        lo: f64,
        hi: f64,
    },
    /// a region of the (distance from a vertical axis, height) plane, spun about the axis
    Revolve { axis: DVec2, region: Region },
    /// A ring of radius `major` about `center`, facing `axis`, with a round
    /// cross-section of radius `minor`. It could be a `Revolve` of a small
    /// disc, and was: but that tests every sample against every edge of the
    /// disc's outline, and measured four times slower than this formula.
    Torus {
        center: DVec3,
        axis: usize,
        major: f64,
        minor: f64,
    },
}

impl Field {
    pub fn eval(&self, p: DVec3) -> f64 {
        match self {
            Field::Tube(s) => s.eval(p),
            Field::Band {
                stroke,
                origin,
                normal,
                half_depth,
            } => {
                let h = (p - *origin).dot(*normal);
                stroke.eval(p - *normal * h).max(h.abs() - half_depth)
            }
            Field::Ellipsoid {
                center,
                radii,
                shell,
                lo,
                hi,
            } => {
                let q = p - *center;
                // exact for a sphere, a close estimate near any ellipsoid's surface
                let k = (q / *radii).length();
                let mut g = (k - 1.0) * radii.min_element();
                if let Some(t) = shell {
                    g = g.max(-g - t);
                }
                g.max(lo - p.z).max(p.z - hi)
            }
            Field::Revolve { axis, region } => {
                region.eval(DVec2::new(p.truncate().distance(*axis), p.z))
            }
            Field::Torus {
                center,
                axis,
                major,
                minor,
            } => {
                let q = p - *center;
                let along = q[*axis];
                let out = (q.length_squared() - along * along).max(0.0).sqrt();
                (out - major).hypot(along) - minor
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(path: &[DVec3], p: DVec3) -> f64 {
        path.windows(2)
            .map(|w| {
                let ab = w[1] - w[0];
                let t = ((p - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                (w[0] + ab * t).distance(p)
            })
            .fold(f64::MAX, f64::min)
    }

    #[test]
    fn bucket_grid_agrees_with_brute_force_within_reach() {
        let path: Vec<DVec3> = (0..200)
            .map(|i| {
                let t = i as f64 * 0.1;
                DVec3::new(t.cos() * (10.0 + t), t.sin() * (10.0 + t), t * 1.5)
            })
            .collect();
        let reach = 5.0;
        let pd = PolyDist::new(&path, reach);
        let mut checked = 0;
        for i in 0..4000 {
            // a cheap deterministic scatter over the path's surroundings
            let f = |k: f64| ((i as f64 * k).sin() * 43758.5453).fract();
            let p = DVec3::new(f(12.9898) * 40.0, f(78.233) * 40.0, f(37.719).abs() * 35.0);
            let exact = brute(&path, p);
            let got = pd.query(p).d;
            if exact <= reach {
                assert!((got - exact).abs() < 1e-9, "{p}: {got} vs {exact}");
                checked += 1;
            } else {
                assert!(got > reach);
            }
        }
        assert!(checked > 100, "only {checked} points were within reach");
    }

    #[test]
    fn square_ends_cut_the_stroke_flat() {
        let path = [DVec3::ZERO, DVec3::X * 10.0];
        let round = Stroke::new(&path, 1.0, false, None);
        let square = Stroke::new(&path, 1.0, false, Some(0.0));
        let longer = Stroke::new(&path, 1.0, false, Some(0.5));
        assert!(longer.eval(DVec3::new(-0.5, 0.9, 0.0)) <= 1e-9);
        assert!((longer.eval(DVec3::new(-0.75, 0.0, 0.0)) - 0.25).abs() < 1e-9);
        let beyond = DVec3::new(-0.5, 0.0, 0.0);
        assert!(round.eval(beyond) < 0.0);
        assert!((square.eval(beyond) - 0.5).abs() < 1e-9);
        let beside = DVec3::new(5.0, 0.5, 0.0);
        assert_eq!(round.eval(beside), square.eval(beside));
    }

    #[test]
    fn fill_is_signed() {
        let square = Fill::new(vec![
            DVec2::ZERO,
            DVec2::X * 4.0,
            DVec2::splat(4.0),
            DVec2::Y * 4.0,
        ]);
        assert!((square.eval(DVec2::new(1.0, 2.0)) + 1.0).abs() < 1e-9);
        assert!((square.eval(DVec2::new(-1.0, 2.0)) - 1.0).abs() < 1e-9);
        // beyond the box in each direction, and far beyond it
        for p in [
            DVec2::new(5.0, 2.0),
            DVec2::new(2.0, -1.0),
            DVec2::new(2.0, 5.0),
        ] {
            assert!((square.eval(p) - 1.0).abs() < 1e-9, "{p}");
        }
        assert_eq!(square.eval(DVec2::new(500.0, 2.0)), f64::INFINITY);
        // a dent: inside the box but outside the polygon
        let dented = Fill::new(vec![
            DVec2::ZERO,
            DVec2::X * 4.0,
            DVec2::new(4.0, 4.0),
            DVec2::new(2.0, 1.0),
            DVec2::Y * 4.0,
        ]);
        assert!(dented.eval(DVec2::new(2.0, 3.0)) > 0.0);
        assert!(dented.eval(DVec2::new(2.0, 0.5)) < 0.0);
    }

    #[test]
    fn far_from_a_square_ended_stroke_is_far() {
        // this once overflowed an index instead of answering
        let square = Stroke::new(&[DVec3::ZERO, DVec3::X * 10.0], 1.0, false, Some(0.5));
        assert_eq!(square.eval(DVec3::new(500.0, 500.0, 0.0)), f64::INFINITY);
    }

    #[test]
    fn torus_matches_its_definition() {
        let ring = Field::Torus {
            center: DVec3::new(1.0, 2.0, 3.0),
            axis: 2,
            major: 10.0,
            minor: 2.0,
        };
        assert!(
            (ring.eval(DVec3::new(11.0, 2.0, 3.0)) + 2.0).abs() < 1e-9,
            "on the centre line of the tube"
        );
        assert!(
            (ring.eval(DVec3::new(1.0, 14.0, 3.0)) - 0.0).abs() < 1e-9,
            "on its outer surface"
        );
        assert!(
            (ring.eval(DVec3::new(1.0, 2.0, 3.0)) - 8.0).abs() < 1e-9,
            "in the middle of the hole"
        );
        let upright = Field::Torus {
            center: DVec3::ZERO,
            axis: 0,
            major: 10.0,
            minor: 2.0,
        };
        assert!((upright.eval(DVec3::new(0.0, 0.0, 10.0)) + 2.0).abs() < 1e-9);
        assert!(upright.eval(DVec3::new(10.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn hollow_dome() {
        let dome = Field::Ellipsoid {
            center: DVec3::ZERO,
            radii: DVec3::splat(10.0),
            shell: Some(2.0),
            lo: 0.0,
            hi: f64::MAX,
        };
        assert!(dome.eval(DVec3::new(0.0, 0.0, 9.0)) < 0.0); // in the shell
        assert!(dome.eval(DVec3::new(0.0, 0.0, 5.0)) > 0.0); // in the hollow
        assert!(dome.eval(DVec3::new(0.0, 9.0, -1.0)) > 0.0); // below the cut
    }
}
