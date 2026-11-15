//! Curves. Every builder returns a polyline flattened to within [`TOL`] cells
//! of the true curve, so nothing downstream cares what kind of curve it was.

use glam::DVec3;
use std::f64::consts::PI;

/// Flattening tolerance in cells: far below what a brick can show.
pub const TOL: f64 = 0.02;

/// Adaptively sample `f` over `[t0, t1]`.
pub fn flatten(f: impl Fn(f64) -> DVec3, t0: f64, t1: f64) -> Vec<DVec3> {
    fn rec(
        f: &impl Fn(f64) -> DVec3,
        a: f64,
        pa: DVec3,
        b: f64,
        pb: DVec3,
        depth: u32,
        out: &mut Vec<DVec3>,
    ) {
        let m = (a + b) / 2.0;
        let pm = f(m);
        // always split a few times, so a symmetric curve's midpoint can't hide it
        if depth < 4 || (depth < 18 && pm.distance((pa + pb) / 2.0) > TOL) {
            rec(f, a, pa, m, pm, depth + 1, out);
            rec(f, m, pm, b, pb, depth + 1, out);
        } else {
            out.push(pb);
        }
    }
    let p0 = f(t0);
    let mut out = vec![p0];
    rec(&f, t0, p0, t1, f(t1), 0, &mut out);
    out
}

/// Drop consecutive duplicate points.
pub fn dedupe(mut path: Vec<DVec3>) -> Vec<DVec3> {
    path.dedup_by(|a, b| a.distance(*b) < 1e-9);
    path
}

#[derive(Clone, Copy, Debug)]
pub struct Circle {
    pub center: DVec3,
    pub radius: f64,
    /// unit vectors spanning the circle's plane; `u` points at the first point
    pub u: DVec3,
    pub v: DVec3,
    pub normal: DVec3,
    /// angles of the second and third points, `0 < a1 < a2 < 2π`
    pub a1: f64,
    pub a2: f64,
}

impl Circle {
    pub fn at(&self, t: f64) -> DVec3 {
        self.center + (self.u * t.cos() + self.v * t.sin()) * self.radius
    }

    /// The part of the circle from angle `from` to angle `to`.
    pub fn path(&self, from: f64, to: f64) -> Vec<DVec3> {
        flatten(|t| self.at(t), from, to)
    }

    /// A circle from its centre, radius and the axis (0, 1 or 2) it faces.
    pub fn axis_aligned(center: DVec3, radius: f64, axis: usize) -> Circle {
        let e = [DVec3::X, DVec3::Y, DVec3::Z];
        let (u, v) = (e[(axis + 1) % 3], e[(axis + 2) % 3]);
        Circle {
            center,
            radius,
            u,
            v,
            normal: e[axis],
            a1: PI,
            a2: 2.0 * PI,
        }
    }
}

/// The circle through three points, or `None` if they are collinear.
pub fn circle3(p0: DVec3, p1: DVec3, p2: DVec3) -> Option<Circle> {
    let (a, b) = (p1 - p0, p2 - p0);
    let n = a.cross(b);
    let n2 = n.length_squared();
    if n2 <= 1e-12 * a.length_squared() * b.length_squared() {
        return None;
    }
    let center =
        p0 + (b.cross(n) * a.length_squared() + n.cross(a) * b.length_squared()) / (2.0 * n2);
    let normal = n.normalize();
    let u = (p0 - center).normalize();
    let v = normal.cross(u);
    // the points run anticlockwise about `normal`, so both angles are positive
    let angle = |p: DVec3| {
        let d = p - center;
        d.dot(v).atan2(d.dot(u)).rem_euclid(2.0 * PI)
    };
    Some(Circle {
        center,
        radius: p0.distance(center),
        u,
        v,
        normal,
        a1: angle(p1),
        a2: angle(p2),
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArchType {
    /// a circular arc through both feet and the crown
    Round,
    /// two arcs centred on the springing line, meeting at a point
    Pointed,
    /// two straight sides
    Triangular,
    Elliptical,
    Parabolic,
    /// the shape a hanging chain takes, upside down
    Catenary,
}

pub const ARCH_TYPES: [ArchType; 6] = [
    ArchType::Round,
    ArchType::Pointed,
    ArchType::Triangular,
    ArchType::Elliptical,
    ArchType::Parabolic,
    ArchType::Catenary,
];

impl ArchType {
    /// The crown height that suits a half-span of `h`: a semicircle's, or an
    /// equilateral arch's for the pointed type.
    pub fn default_rise(self, h: f64) -> f64 {
        match self {
            ArchType::Pointed => 3f64.sqrt() * h,
            _ => h,
        }
    }
}

/// An arch springing from `a` and `b` and rising `rise` above the chord along
/// `up`. Returns the path and the normal of the plane it lies in.
pub fn arch(
    a: DVec3,
    b: DVec3,
    kind: ArchType,
    rise: f64,
    up: DVec3,
) -> Result<(Vec<DVec3>, DVec3), String> {
    let h = a.distance(b) / 2.0;
    if h < 1e-9 {
        return Err("the arch's two feet are the same point".into());
    }
    let x = (b - a) / (2.0 * h);
    let y = up - x * up.dot(x);
    if y.length() < 1e-9 {
        return Err("the arch's two feet are directly above each other".into());
    }
    let y = y.normalize();
    if !(rise > 0.0) {
        return Err("the arch's rise must be positive".into());
    }
    let mid = (a + b) / 2.0;
    let at = |px: f64, py: f64| mid + x * px + y * py;
    let s = rise;
    let path = match kind {
        ArchType::Round => {
            let r = (h * h + s * s) / (2.0 * s);
            let half = h.atan2(r - s);
            flatten(|t| at(r * t.sin(), s - r + r * t.cos()), -half, half)
        }
        ArchType::Pointed => {
            // below a semicircle's height the two arcs would not meet at a point
            let s = s.max(h * 1.001);
            let cx = (s * s - h * h) / (2.0 * h);
            let r = cx + h;
            let top = s.atan2(-cx);
            let left = flatten(|t| at(cx + r * t.cos(), r * t.sin()), PI, top);
            // the right side is the left one mirrored across the centre line
            let right = left
                .iter()
                .rev()
                .skip(1)
                .map(|&p| p - x * (2.0 * (p - mid).dot(x)));
            left.iter().copied().chain(right).collect()
        }
        ArchType::Triangular => vec![a, at(0.0, s), b],
        ArchType::Elliptical => flatten(|t| at(-h * t.cos(), s * t.sin()), 0.0, PI),
        ArchType::Parabolic => flatten(|t| at(t, s * (1.0 - (t / h) * (t / h))), -h, h),
        ArchType::Catenary => {
            // solve k (cosh(h / k) - 1) = s for k; the left side falls as k grows
            let (mut lo, mut hi) = (h * 1e-3, h * 1e6);
            for _ in 0..200 {
                let k = (lo * hi).sqrt();
                if k * ((h / k).cosh() - 1.0) > s {
                    lo = k;
                } else {
                    hi = k;
                }
            }
            let k = (lo * hi).sqrt();
            flatten(|t| at(t, s - k * ((t / k).cosh() - 1.0)), -h, h)
        }
    };
    Ok((dedupe(path), x.cross(y)))
}

/// Centripetal Catmull-Rom spline through every point. Centripetal spacing
/// (knots a square root of the chord apart) is the variant that cannot form
/// cusps or loops within a span.
pub fn catmull_rom(points: &[DVec3], closed: bool) -> Result<Vec<DVec3>, String> {
    let pts = dedupe(points.to_vec());
    let n = pts.len();
    if n < 2 {
        return Err("a spline needs at least 2 distinct points".into());
    }
    if n == 2 {
        return Ok(pts);
    }
    let get = |i: isize| -> DVec3 {
        let n = n as isize;
        if closed {
            pts[i.rem_euclid(n) as usize]
        } else if i < 0 {
            pts[0] * 2.0 - pts[1] // mirror the first span to make an end tangent
        } else if i >= n {
            pts[n as usize - 1] * 2.0 - pts[n as usize - 2]
        } else {
            pts[i as usize]
        }
    };
    let mut out = Vec::new();
    let spans = if closed { n } else { n - 1 };
    for i in 0..spans as isize {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        let t1 = p0.distance(p1).sqrt().max(1e-6);
        let t2 = t1 + p1.distance(p2).sqrt();
        let t3 = t2 + p2.distance(p3).sqrt().max(1e-6);
        let mix = |a: DVec3, b: DVec3, ta: f64, tb: f64, t: f64| a.lerp(b, (t - ta) / (tb - ta));
        // the Barry-Goldman pyramid
        let piece = flatten(
            |t| {
                let (a1, a2, a3) = (
                    mix(p0, p1, 0.0, t1, t),
                    mix(p1, p2, t1, t2, t),
                    mix(p2, p3, t2, t3, t),
                );
                let (b1, b2) = (mix(a1, a2, 0.0, t2, t), mix(a2, a3, t1, t3, t));
                mix(b1, b2, t1, t2, t)
            },
            t1,
            t2,
        );
        out.extend(piece.into_iter().skip((i > 0) as usize));
    }
    Ok(dedupe(out))
}

/// Rational Bézier curve: the points are handles, and a weight above 1 pulls
/// the curve towards its handle. With three handles, a middle weight of
/// `cos(half the arc's angle)` gives an exact circular arc, which no plain
/// Bézier can.
pub fn rational_bezier(points: &[DVec3], weights: &[f64]) -> Result<Vec<DVec3>, String> {
    if points.len() < 2 {
        return Err("a Bézier curve needs at least 2 points".into());
    }
    let w = |i: usize| weights.get(i).copied().filter(|w| *w > 0.0).unwrap_or(1.0);
    let start: Vec<(DVec3, f64)> = points
        .iter()
        .enumerate()
        .map(|(i, p)| (*p * w(i), w(i)))
        .collect();
    Ok(dedupe(flatten(
        |t| {
            // de Casteljau in homogeneous coordinates
            let mut h = start.clone();
            for level in (1..h.len()).rev() {
                for i in 0..level {
                    h[i] = (
                        h[i].0.lerp(h[i + 1].0, t),
                        h[i].1 + (h[i + 1].1 - h[i].1) * t,
                    );
                }
            }
            h[0].0 / h[0].1
        },
        0.0,
        1.0,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_through_three_points() {
        let (p0, p1, p2) = (
            DVec3::new(3.0, 1.0, 2.0),
            DVec3::new(-2.0, 5.0, 4.0),
            DVec3::new(0.5, -3.0, 7.0),
        );
        let c = circle3(p0, p1, p2).unwrap();
        for p in [p0, p1, p2] {
            assert!((p.distance(c.center) - c.radius).abs() < 1e-9);
            assert!((p - c.center).dot(c.normal).abs() < 1e-9);
        }
        assert!(c.at(0.0).distance(p0) < 1e-9);
        assert!(c.at(c.a1).distance(p1) < 1e-9);
        assert!(c.at(c.a2).distance(p2) < 1e-9);
        assert!(0.0 < c.a1 && c.a1 < c.a2 && c.a2 < 2.0 * PI);
        assert!(circle3(DVec3::ZERO, DVec3::X, DVec3::X * 2.0).is_none());
    }

    #[test]
    fn rational_bezier_is_an_exact_quarter_circle() {
        let w = (PI / 4.0).cos();
        let path = rational_bezier(
            &[DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y],
            &[1.0, w, 1.0],
        )
        .unwrap();
        assert!(path.len() > 8);
        for p in path {
            assert!((p.length() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn every_arch_springs_from_its_feet_and_reaches_its_rise() {
        let (a, b) = (DVec3::new(-6.0, 2.0, 1.0), DVec3::new(6.0, 2.0, 1.0));
        for kind in ARCH_TYPES {
            let rise = kind.default_rise(6.0) * 1.2;
            let (path, normal) = arch(a, b, kind, rise, DVec3::Z).unwrap();
            assert!(path[0].distance(a) < 1e-6, "{kind:?}");
            assert!(path.last().unwrap().distance(b) < 1e-6, "{kind:?}");
            let top = path.iter().map(|p| p.z).fold(f64::MIN, f64::max);
            assert!((top - 1.0 - rise).abs() < 0.03, "{kind:?}: top {top}");
            assert!(path.iter().all(|p| (p.y - 2.0).abs() < 1e-9));
            assert!(normal.distance(-DVec3::Y) < 1e-9);
        }
        assert!(arch(a, a, ArchType::Round, 1.0, DVec3::Z).is_err());
        assert!(arch(a, a + DVec3::Z, ArchType::Round, 1.0, DVec3::Z).is_err());
    }

    #[test]
    fn spline_passes_through_its_points() {
        let pts = [
            DVec3::ZERO,
            DVec3::new(4.0, 3.0, 0.0),
            DVec3::new(9.0, -2.0, 5.0),
            DVec3::new(12.0, 6.0, 1.0),
        ];
        for closed in [false, true] {
            let path = catmull_rom(&pts, closed).unwrap();
            for p in pts {
                assert!(
                    path.iter().any(|q| q.distance(p) < 1e-6),
                    "closed {closed}: misses {p}"
                );
            }
        }
    }
}
