//! The rectangle: a slab or a frame with its sides on the lattice.

use super::{check_size, curve, plane_axes, Solid, Spec};
use crate::sdf::{Field, Fill, Region};
use crate::shapes::{CellBrick, CUBE};
use glam::{DVec2, DVec3};

/// A rectangle about `middle`, lying across the axis `Spec::plane` names:
/// a slab if it is filled, or else a frame `thick` wide within the same
/// four sides.
///
/// Its sides are put on lattice lines, so that it is whole bricks: the
/// build then has no discrepancy at all, and is a handful of boxes.
///
/// It is fitted as a flat outline (`Solid::Flat`, by `fit_planar`), as a
/// band or a filled curve is, and has no fitter of its own. Laying the
/// boxes out directly would have been a few lines, but stepping stones and
/// mosaics are laid on a flat outline and on nothing else, and so are
/// points added to it; and the outline fitter gives the same boxes anyway,
/// since an outline that runs along lattice lines is all cubes to it, which
/// it joins into the largest boxes it can.
///
/// The outline is not the one a curve would have, though. A curve's band is
/// everything within half its thickness of the path, which rounds the
/// outside of every corner: a frame drawn that way came out with its
/// corners cut off. So the frame is the rectangle less a smaller one inside
/// it (`Region::Frame`), and the slab is the rectangle itself, from its
/// four corners. Only a rectangle with points added to it, which is no
/// longer a rectangle, is handed on as a curve: its outline if it is
/// filled, and the middle of its frame if it is not.
///
/// It is never a round tube. A pipe bent round four square corners is not
/// something bricks do well, and the setting is not offered for it.
pub(super) fn rectangle(spec: &Spec, middle: DVec3) -> Result<(Solid, Vec<DVec3>), String> {
    let axis = spec.plane.min(2);
    let (ua, va) = plane_axes(axis);
    // Each pair of sides, on lattice lines: a whole number of cells apart,
    // and about the middle. A width that cannot be centred there (an even
    // one about a cell's centre, an odd one about a lattice line) lies half
    // a cell to the low side, the way a band is moved (see `planar`).
    let sides = |k: usize| {
        let wide = (2.0 * spec.radius * spec.stretch[k].max(0.05)).round().max(1.0);
        let low = (middle[k] - wide / 2.0 - 0.5).ceil();
        (low, low + wide)
    };
    let (u, v) = (sides(ua), sides(va));
    let wall = spec.thick.round().max(1.0);
    // a frame whose walls would meet in the middle is a slab
    let hollow = !spec.fill && 2.0 * wall < (u.1 - u.0).min(v.1 - v.0);
    let corners = |inset: f64| [(u.0 + inset, v.0 + inset), (u.1 - inset, v.0 + inset), (u.1 - inset, v.1 - inset), (u.0 + inset, v.1 - inset)];
    // The curve drawn for it, and the one an added point takes hold of:
    // round its four sides with a point at every cell, since a point is
    // added at the nearest point of this curve to where it was clicked,
    // and four corners would be the only places to add one.
    let outline = |inset: f64| {
        let at = corners(inset).map(|(x, y)| {
            let mut p = middle;
            (p[ua], p[va]) = (x, y);
            p
        });
        let mut path = Vec::new();
        for k in 0..4 {
            let (from, to) = (at[k], at[(k + 1) % 4]);
            let steps = from.distance(to).round().max(1.0) as usize;
            path.extend((0..steps).map(|i| from + (to - from) * (i as f64 / steps as f64)));
        }
        path.push(at[0]); // a closed path repeats its first point
        path
    };
    if !spec.pulls.is_empty() {
        let mut normal = DVec3::ZERO;
        normal[axis] = 1.0;
        // Mirrored about where the rectangle's middle really is, which is
        // half a cell off its point when it could not be centred there.
        let mut about = middle;
        (about[ua], about[va]) = ((u.0 + u.1) / 2.0, (v.0 + v.1) / 2.0);
        let as_curve = Spec { tube: false, fill: !hollow, thick: wall, points: vec![about], ..spec.clone() };
        return curve(&as_curve, outline(if hollow { wall / 2.0 } else { 0.0 }), true, Some(normal));
    }
    let fill = |inset: f64| Fill::new(corners(inset).map(|(x, y)| DVec2::new(x, y)).to_vec());
    let region = if hollow { Region::Frame(fill(0.0), fill(wall)) } else { Region::Fill(fill(0.0)) };
    let (min, max) = ([u.0 as i32 - 1, v.0 as i32 - 1], [u.1 as i32 + 1, v.1 as i32 + 1]);
    check_size(&[(max[0] - min[0]) as i64, (max[1] - min[1]) as i64])?;
    // pushed through the grid, centred on the cell its middle is in (as in `planar`)
    let w0 = middle[axis].floor() as i32 - (spec.depth - 1) / 2;
    // One cell by one cell is laid out here, as a column. The outline
    // fitter, asked for sloped pieces, loses a square cell that stands
    // alone: it takes the cell's four sides for two slopes facing each
    // other, which want the same cell, and places neither. Changing that
    // would change how every small outline is fitted; a rectangle dragged
    // down to one cell is the only form that asks for it on purpose.
    if (u.1 - u.0, v.1 - v.0) == (1.0, 1.0) && spec.wedges {
        let mut bricks = Vec::new();
        let mut done = 0;
        while done < spec.depth {
            let run = (spec.depth - done).min(spec.max_ext.max(1));
            let (mut low, mut ext) = ([0; 3], [1; 3]);
            (low[ua], low[va], low[axis]) = (u.0 as i32, v.0 as i32, w0 + done);
            ext[axis] = run;
            bricks.push(CellBrick { mask: CUBE, min: low, ext });
            done += run;
        }
        let field = Field::Plate { fill: fill(0.0), axis, lo: w0 as f64, hi: (w0 + spec.depth) as f64 };
        let (mut lo, mut hi) = ([0; 3], [0; 3]);
        (lo[ua], lo[va], lo[axis]) = (min[0], min[1], w0 - 1);
        (hi[ua], hi[va], hi[axis]) = (max[0], max[1], w0 + spec.depth + 1);
        // (the kind of solid that comes with its bricks, as a pyramid's do)
        return Ok((Solid::Pyramid { bricks: Some(bricks), field, min: lo, max: hi }, outline(0.0)));
    }
    Ok((Solid::Flat { region, axis, min, max, w0, depth: spec.depth }, outline(0.0)))
}

#[cfg(test)]
mod tests {
    use crate::shapes::CUBE;
    use crate::spec::{build, Built, Form, Spec};
    use glam::DVec3;
    use std::collections::HashSet;

    fn rectangle(middle: DVec3, radius: f64) -> Spec {
        Spec { form: Form::Rectangle, points: vec![middle], radius, ..Spec::default() }
    }

    /// The cells a build fills, each of them by one brick only, and every
    /// brick a plain box.
    fn cells(built: &Built) -> HashSet<[i32; 3]> {
        let mut all = HashSet::new();
        for b in &built.bricks {
            assert_eq!(b.mask, CUBE, "{b:?} is not a plain box");
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        assert!(all.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]), "two bricks share a cell");
                    }
                }
            }
        }
        all
    }

    /// The cells a rectangle should fill, worked out apart from the build:
    /// `low` and `high` are its sides along the two axes of its plane, and
    /// `wall` the width of its frame (0 for a slab).
    fn wanted(plane: usize, low: [i32; 2], high: [i32; 2], across: (i32, i32), wall: i32) -> HashSet<[i32; 3]> {
        let (ua, va) = crate::spec::plane_axes(plane);
        let mut all = HashSet::new();
        for u in low[0]..high[0] {
            for v in low[1]..high[1] {
                let inside = u >= low[0] + wall && u < high[0] - wall && v >= low[1] + wall && v < high[1] - wall;
                if wall > 0 && inside {
                    continue;
                }
                for w in across.0..across.1 {
                    let mut c = [0; 3];
                    (c[ua], c[va], c[plane]) = (u, v, w);
                    all.insert(c);
                }
            }
        }
        all
    }

    #[test]
    fn a_slab_and_a_frame_are_whole_bricks_in_every_plane() {
        for plane in 0..3 {
            let (ua, va) = crate::spec::plane_axes(plane);
            // 17 by 17 about a cell's centre, then stretched to 34 by 9
            for stretch in [1.0, 2.0] {
                let mut by = DVec3::ONE;
                (by[ua], by[va]) = (stretch, if stretch == 1.0 { 1.0 } else { 0.5 });
                let (wide, deep) = if stretch == 1.0 { (17, 17) } else { (34, 9) };
                // (34 about a cell's centre cannot be centred: it lies half a cell to the low side)
                let low = [if wide == 34 { 3 - 17 } else { 3 - 8 }, -2 - deep / 2];
                let high = [low[0] + wide, low[1] + deep];
                let mut middle = DVec3::ZERO;
                (middle[ua], middle[va], middle[plane]) = (3.5, -1.5, 5.5);
                for (fill, thick, wall) in [(true, 2.0, 0), (false, 1.0, 1), (false, 3.0, 3)] {
                    let spec = Spec { stretch: by, plane, fill, thick, depth: 3, ..rectangle(middle, 8.5) };
                    let built = build(&spec).unwrap();
                    let what = format!("plane {plane}, stretch {stretch}, fill {fill}, thick {thick}");
                    assert_eq!(cells(&built), wanted(plane, low, high, (4, 7), wall), "{what}");
                    assert_eq!(built.error, 0.0, "{what}");
                    assert!(built.solid > 0.0 && !built.unmeasured, "{what}");
                    // one box for a slab, and one for each side of a frame
                    assert_eq!(built.bricks.len(), if fill { 1 } else { 4 }, "{what}: {:?}", built.bricks);
                    // the automatic brick size finds nothing to save, and builds the same
                    assert_eq!(cells(&build(&Spec { grain: 0, ..spec.clone() }).unwrap()), cells(&built), "{what}");
                    // it is never a round tube
                    assert_eq!(build(&Spec { tube: true, ..spec }).unwrap().bricks, built.bricks, "{what}");
                }
            }
        }
    }

    #[test]
    fn a_frame_too_thick_to_be_hollow_is_a_slab() {
        let frame = Spec { thick: 4.0, depth: 1, ..rectangle(DVec3::splat(0.5), 3.5) };
        let slab = Spec { fill: true, ..frame.clone() };
        assert_eq!(build(&frame).unwrap().bricks, build(&slab).unwrap().bricks);
        assert_eq!(build(&frame).unwrap().bricks.len(), 1);
    }

    #[test]
    fn sizes_off_the_lattice_are_brought_onto_it() {
        // a radius and a stretch that give sides between lattice lines
        let spec = Spec { stretch: DVec3::new(1.37, 0.81, 1.0), fill: true, depth: 1, ..rectangle(DVec3::new(0.2, 0.9, 0.5), 7.3) };
        let built = build(&spec).unwrap();
        assert_eq!(built.error, 0.0);
        assert_eq!(built.bricks.len(), 1);
        // and the smallest there is: one cell, however small it is asked for
        let speck = build(&Spec { stretch: DVec3::splat(0.05), ..spec.clone() }).unwrap();
        assert_eq!(speck.bricks.iter().map(|b| (b.min, b.ext)).collect::<Vec<_>>(), vec![([-1, 0, 0], [1, 1, 1])]);
        assert_eq!((speck.error, speck.solid), (0.0, 1.0));
        // (squares alone go another way through the fitter: the same cell)
        assert_eq!(build(&Spec { wedges: false, stretch: DVec3::splat(0.05), ..spec }).unwrap().bricks, speck.bricks);
    }

    #[test]
    fn a_pattern_is_laid_on_a_rectangle() {
        let floor = Spec { fill: true, depth: 1, stretch: DVec3::new(1.5, 1.0, 1.0), ..rectangle(DVec3::splat(0.5), 20.5) };
        let whole = cells(&build(&floor).unwrap());
        for style in 0..crate::pattern::STYLES.len() {
            let mosaic = crate::pattern::Mosaic { style, size: 2, bevel: 1, border: 1, seed: 3, mortar: true, stagger: -1, upright: false, uneven: 0 };
            let built = build(&Spec { mosaic: Some(mosaic), ..floor.clone() }).unwrap();
            let name = crate::pattern::STYLES[style].name;
            assert_eq!(built.shades.len(), built.bricks.len(), "{name}");
            assert!(built.shades.iter().collect::<HashSet<_>>().len() > 1, "{name} is one colour");
            // a mosaic covers the floor it is laid on, and nothing else
            let laid: HashSet<[i32; 3]> = built.bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).map(move |y| [b.min[0] + x, b.min[1] + y, b.min[2]]))).collect();
            assert!(laid.is_subset(&whole), "{name} is laid outside the rectangle");
            assert!(laid.len() * 10 >= whole.len() * 9, "{name} leaves the rectangle bare: {} of {}", laid.len(), whole.len());
        }
        let stones = crate::pattern::Stones { size: 7.0, gap: 1.5, uneven: 0.5, seed: 2 };
        let stepping = build(&Spec { stones: Some(stones), ..floor.clone() }).unwrap();
        assert!(stepping.bricks.len() > 4, "no stones were laid");
        // and on a frame, which is a flat outline as much as a slab is
        let frame = build(&Spec { fill: false, thick: 8.0, stones: Some(stones), ..floor }).unwrap();
        assert!(frame.bricks.len() > 4, "no stones were laid on the frame");
    }

    #[test]
    fn a_point_added_to_a_rectangle_pulls_its_side_out_there() {
        for fill in [true, false] {
            let plain = Spec { fill, thick: 2.0, depth: 1, ..rectangle(DVec3::splat(0.5), 12.5) };
            // taken hold of half way along its high x side, and pulled out by 5
            let pulled = Spec { pulls: vec![(DVec3::new(13.0, 0.5, 0.5), DVec3::new(5.0, 0.0, 0.0))], pull_reach: 6.0, ..plain.clone() };
            let (before, after) = (build(&plain).unwrap(), build(&pulled).unwrap());
            let far = |b: &Built| b.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap();
            assert_eq!(far(&before), 13);
            assert!((17..=18).contains(&far(&after)), "fill {fill}: it reaches {}", far(&after));
            // the other three sides are where they were
            let low = |b: &Built, k: usize| b.bricks.iter().map(|b| b.min[k]).min().unwrap();
            assert_eq!([low(&after, 0), low(&after, 1)], [low(&before, 0), low(&before, 1)], "fill {fill}");
            assert!(after.error / after.solid < 0.1, "fill {fill}: {} of {}", after.error, after.solid);
        }
    }
}
