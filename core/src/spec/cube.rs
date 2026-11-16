//! The cube: a box with its six sides on the lattice, solid or hollow.

use super::{check_size, curve, plane_axes, Solid, Spec};
use crate::sdf::{Field, Fill, Region};
use crate::shapes::{CellBrick, CUBE};
use glam::{DVec2, DVec3};

/// Where the six sides of a box about `middle` are: along each axis, its
/// low side and its high one. `spec` is taken as it stands, so one not yet
/// on the grid is to be snapped first (`Spec::snapped`), as a build snaps it.
///
/// Each pair of sides is on lattice lines, a whole number of cells apart
/// and about the middle. A length that cannot be centred there (an even one
/// about a cell's centre, an odd one about a lattice line) lies half a cell
/// to the low side, the way a band is moved (see `planar`).
///
/// Asked for by the editor too, which puts a dot on each side: worked out
/// there a second time, the dots would sooner or later stand off the bricks.
pub fn sides(spec: &Spec, middle: DVec3) -> [(f64, f64); 3] {
    [0, 1, 2].map(|k| {
        let long = (2.0 * spec.radius * spec.stretch[k].max(0.05)).round().max(1.0);
        let low = (middle[k] - long / 2.0 - 0.5).ceil();
        (low, low + long)
    })
}

/// The axis a box is built through: the one it is shortest along, and of
/// two as short as each other the later (z before y before x), so that a
/// cube is built through its height.
///
/// This is the axis a pattern is pushed through, so the pattern shows on
/// the box's largest face: the top of a floor, the face of a wall. The
/// player is not asked. The pass that lays a pattern on the flat faces of
/// other forms (`pattern_flat_faces`) chooses the same way, by the most
/// flat columns, and a setting for it would be one more thing to put right
/// each time a floor was stood up into a wall.
pub fn through(sides: &[(f64, f64); 3]) -> usize {
    (0..3).rev().min_by(|a, b| (sides[*a].1 - sides[*a].0).total_cmp(&(sides[*b].1 - sides[*b].0))).unwrap_or(2)
}

/// A box about `middle`: solid if it is filled, or else hollow, with walls
/// `thick` through on every side.
///
/// Its sides are put on lattice lines, so that it is whole bricks: the
/// build then has no discrepancy at all, and is a handful of boxes.
///
/// It is fitted as a flat outline (`Solid::Flat`, by `fit_planar`), as a
/// band or a filled curve is: a rectangle pushed through the box's shortest
/// length (see [`through`]). It has no fitter of its own. Laying the boxes
/// out directly would have been a few lines, but stepping stones and
/// mosaics are laid on a flat outline, and so are points added to it; and
/// the outline fitter gives the same boxes anyway, since an outline that
/// runs along lattice lines is all cubes to it, which it joins into the
/// largest boxes it can.
///
/// The outline is not the one a curve would have, though. A curve's band is
/// everything within half its thickness of the path, which rounds the
/// outside of every corner: a frame drawn that way came out with its
/// corners cut off. So a frame is the rectangle less a smaller one inside
/// it (`Region::Frame`), and a slab is the rectangle itself, from its four
/// corners.
///
/// A hollow box is one of two things. Where it is too short to have a
/// hollow between a floor and a lid, the hollow goes right through its
/// shortest length and it is a frame: the border of a rug, the surround of
/// a window. Otherwise it is closed all round, as a hollow sphere is, and
/// built as three outlines stacked (`Solid::Stack`): a slab, a frame and a
/// slab. Walls on four sides only, as a pyramid is a tent open underneath,
/// would have left a room without a floor or a tower without a shaft,
/// depending on which length happened to be shortest.
///
/// A box with points added to it is no longer a box, and is handed on as a
/// curve: its outline if it is solid, and the middle of its frame if it is
/// hollow, which is then open through however tall it is.
///
/// It is never a round tube. A pipe bent round four square corners is not
/// something bricks do well, and the setting is not offered for it.
pub(super) fn cube(spec: &Spec, middle: DVec3) -> Result<(Solid, Vec<DVec3>), String> {
    let all = sides(spec, middle);
    let axis = through(&all);
    let (ua, va) = plane_axes(axis);
    let (u, v) = (all[ua], all[va]);
    let (w0, depth) = (all[axis].0 as i32, (all[axis].1 - all[axis].0) as i32);
    let wall = spec.thick.round().max(1.0);
    // a box whose walls would meet in the middle is solid
    let hollow = !spec.fill && 2.0 * wall < (u.1 - u.0).min(v.1 - v.0);
    let closed = hollow && 2 * (wall as i32) < depth;
    let corners = |inset: f64| [(u.0 + inset, v.0 + inset), (u.1 - inset, v.0 + inset), (u.1 - inset, v.1 - inset), (u.0 + inset, v.1 - inset)];
    // The curve drawn for it, and the one an added point takes hold of:
    // round its four sides with a point at every cell, since a point is
    // added at the nearest point of this curve to where it was clicked,
    // and four corners would be the only places to add one. It lies in
    // the cell a flat curve of this depth would be in (see `planar`).
    let outline = |inset: f64| {
        let at = corners(inset).map(|(x, y)| {
            let mut p = middle;
            (p[ua], p[va], p[axis]) = (x, y, (w0 + (depth - 1) / 2) as f64 + 0.5);
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
        // Mirrored about where the box's middle really is, which is half a
        // cell off its point when it could not be centred there.
        let mut about = middle;
        (about[ua], about[va]) = ((u.0 + u.1) / 2.0, (v.0 + v.1) / 2.0);
        let as_curve = Spec { tube: false, fill: !hollow, thick: wall, depth, points: vec![about], ..spec.clone() };
        return curve(&as_curve, outline(if hollow { wall / 2.0 } else { 0.0 }), true, Some(normal));
    }
    let fill = |inset: f64| Fill::new(corners(inset).map(|(x, y)| DVec2::new(x, y)).to_vec());
    let (min, max) = ([u.0 as i32 - 1, v.0 as i32 - 1], [u.1 as i32 + 1, v.1 as i32 + 1]);
    check_size(&[(max[0] - min[0]) as i64, (max[1] - min[1]) as i64, depth as i64])?;
    // One cell is laid out here. The outline fitter, asked for sloped
    // pieces, loses a square cell that stands alone: it takes the cell's
    // four sides for two slopes facing each other, which want the same
    // cell, and places neither. Changing that would change how every small
    // outline is fitted; a box dragged down to one cell is the only form
    // that asks for it on purpose. (One cell across its two longer lengths
    // is one cell along its shortest too.)
    if (u.1 - u.0, v.1 - v.0) == (1.0, 1.0) && spec.wedges {
        let (mut low, mut lo, mut hi) = ([0; 3], [0; 3], [0; 3]);
        (low[ua], low[va], low[axis]) = (u.0 as i32, v.0 as i32, w0);
        (lo[ua], lo[va], lo[axis]) = (min[0], min[1], w0 - 1);
        (hi[ua], hi[va], hi[axis]) = (max[0], max[1], w0 + depth + 1);
        let field = Field::Plate { fill: fill(0.0), axis, lo: w0 as f64, hi: (w0 + depth) as f64 };
        // (the kind of solid that comes with its bricks, as a pyramid's do)
        return Ok((Solid::Pyramid { bricks: Some(vec![CellBrick { mask: CUBE, min: low, ext: [1; 3] }]), field, min: lo, max: hi }, outline(0.0)));
    }
    if closed {
        let wall = wall as i32;
        let frame = Region::Frame(fill(0.0), fill(wall as f64));
        let layers = vec![(Region::Fill(fill(0.0)), w0, wall), (frame, w0 + wall, depth - 2 * wall), (Region::Fill(fill(0.0)), w0 + depth - wall, wall)];
        let outside = Field::Plate { fill: fill(0.0), axis, lo: w0 as f64, hi: (w0 + depth) as f64 };
        let inside = Field::Plate { fill: fill(wall as f64), axis, lo: (w0 + wall) as f64, hi: (w0 + depth - wall) as f64 };
        return Ok((Solid::Stack { layers, axis, min, max, field: Field::Without(Box::new(outside), Box::new(inside)) }, outline(0.0)));
    }
    let region = if hollow { Region::Frame(fill(0.0), fill(wall)) } else { Region::Fill(fill(0.0)) };
    Ok((Solid::Flat { region, axis, min, max, w0, depth }, outline(0.0)))
}

#[cfg(test)]
mod tests {
    use crate::shapes::CUBE;
    use crate::spec::{build, Built, Form, Spec};
    use glam::DVec3;
    use std::collections::HashSet;

    fn cube(middle: DVec3, radius: f64) -> Spec {
        Spec { form: Form::Cube, points: vec![middle], radius, fill: true, ..Spec::default() }
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

    /// The cells a box should fill, worked out apart from the build: from
    /// `low` to `high`, less, if `hollow`, what is `wall` in from each side
    /// along the axes in `walled` (the box is open through the others).
    fn wanted(low: [i32; 3], high: [i32; 3], wall: i32, walled: [bool; 3], hollow: bool) -> HashSet<[i32; 3]> {
        let mut all = HashSet::new();
        for x in low[0]..high[0] {
            for y in low[1]..high[1] {
                for z in low[2]..high[2] {
                    let c = [x, y, z];
                    let inside = (0..3).all(|k| !walled[k] || (c[k] >= low[k] + wall && c[k] < high[k] - wall));
                    if !(hollow && inside) {
                        all.insert(c);
                    }
                }
            }
        }
        all
    }

    #[test]
    fn a_box_is_whole_bricks_whichever_length_is_shortest() {
        // a cube 17 each way about a cell's centre, and boxes 34 by 9 by 5
        // with the 5 along each axis in turn
        let middle = DVec3::new(3.5, -1.5, 5.5);
        let mut cases = vec![(DVec3::ONE, [17, 17, 17], 2)];
        for thin in 0..3 {
            let mut long = [0; 3];
            (long[thin], long[(thin + 1) % 3], long[(thin + 2) % 3]) = (5, 34, 9);
            cases.push((DVec3::new(long[0] as f64, long[1] as f64, long[2] as f64) / 17.0, long, thin));
        }
        for (stretch, long, thin) in cases {
            // (an even length about a cell's centre cannot be centred: it lies half a cell to the low side)
            let low = [0, 1, 2].map(|k| (middle[k] - long[k] as f64 / 2.0 - 0.5).ceil() as i32);
            let high = [0, 1, 2].map(|k| low[k] + long[k]);
            // solid; hollow with a wall of 1, closed all round; and with a wall of 3, too thick for a box 5 through to be closed
            for (fill, thick) in [(true, 2.0), (false, 1.0), (false, 3.0)] {
                let spec = Spec { stretch, fill, thick, ..cube(middle, 8.5) };
                let built = build(&spec).unwrap();
                let what = format!("{long:?}, fill {fill}, thick {thick}");
                let closed = 2 * (thick as i32) < long[thin];
                let mut walled = [true; 3];
                walled[thin] = closed;
                assert_eq!(cells(&built), wanted(low, high, thick as i32, walled, !fill), "{what}");
                assert_eq!(built.error, 0.0, "{what}");
                assert!(built.solid > 0.0 && !built.unmeasured, "{what}");
                // one box if solid, four round a frame, and a floor and a lid besides if closed
                assert_eq!(built.bricks.len(), if fill { 1 } else if closed { 6 } else { 4 }, "{what}: {:?}", built.bricks);
                // the automatic brick size finds nothing to save, and builds the same
                assert_eq!(cells(&build(&Spec { grain: 0, ..spec.clone() }).unwrap()), cells(&built), "{what}");
                // it is never a round tube, and takes no notice of the settings of a flat curve
                assert_eq!(build(&Spec { tube: true, depth: 9, plane: 0, ..spec }).unwrap().bricks, built.bricks, "{what}");
            }
        }
    }

    #[test]
    fn a_box_longer_than_a_brick_is_cut_into_bricks() {
        let built = build(&Spec { stretch: DVec3::new(1.0, 1.0, 10.0), max_ext: 80, ..cube(DVec3::splat(0.5), 10.0) }).unwrap();
        assert_eq!((built.bricks.len(), built.error), (3, 0.0));
        assert_eq!(cells(&built).len(), 20 * 20 * 200);
    }

    #[test]
    fn a_hollow_box_too_small_for_a_hollow_is_solid() {
        let hollow = Spec { fill: false, thick: 4.0, ..cube(DVec3::splat(0.5), 3.5) };
        let solid = Spec { fill: true, ..hollow.clone() };
        assert_eq!(build(&hollow).unwrap().bricks, build(&solid).unwrap().bricks);
        assert_eq!(build(&hollow).unwrap().bricks.len(), 1);
    }

    #[test]
    fn sizes_off_the_lattice_are_brought_onto_it() {
        // a radius and a stretch that give sides between lattice lines
        let spec = Spec { stretch: DVec3::new(1.37, 0.81, 0.33), ..cube(DVec3::new(0.2, 0.9, 0.5), 7.3) };
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
    fn a_pattern_is_laid_through_a_boxes_shortest_length() {
        // a floor 61 by 41 and 2 deep, and the same slab stood up as a wall
        for thin in [2, 0] {
            let mut stretch = DVec3::new(61.0, 41.0, 41.0) / 41.0;
            stretch[thin] = 2.0 / 41.0;
            let slab = Spec { stretch, ..cube(DVec3::splat(0.5), 20.5) };
            let whole = cells(&build(&slab).unwrap());
            let low = whole.iter().map(|c| c[thin]).min().unwrap();
            for style in 0..crate::pattern::STYLES.len() {
                let mosaic = crate::pattern::Mosaic { style, size: 2, bevel: 0, border: 1, seed: 3, mortar: true, stagger: -1, upright: false, uneven: [0; 3] };
                let built = build(&Spec { mosaic: Some(mosaic), ..slab.clone() }).unwrap();
                let name = crate::pattern::STYLES[style].name;
                assert_eq!(built.shades.len(), built.bricks.len(), "{name}");
                assert!(built.shades.iter().collect::<HashSet<_>>().len() > 1, "{name} is one colour");
                // every tile goes right through the slab, so the pattern shows on its broad face
                assert!(built.bricks.iter().all(|b| b.min[thin] == low && b.ext[thin] == 2), "{name}: a tile does not go through the slab's thin length");
                // and the tiles cover the slab, and nothing else
                let laid: HashSet<[i32; 3]> = built.bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).flat_map(move |y| (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1] + y, b.min[2] + z])))).collect();
                assert!(laid.is_subset(&whole), "{name} is laid outside the box");
                assert!(laid.len() * 10 >= whole.len() * 9, "{name} leaves the box bare: {} of {}", laid.len(), whole.len());
            }
            let stones = crate::pattern::Stones { size: 7.0, gap: 1.5, uneven: 0.5, seed: 2 };
            let stepping = build(&Spec { stones: Some(stones), ..slab.clone() }).unwrap();
            assert!(stepping.bricks.len() > 4, "no stones were laid");
            // and on a frame, which is a flat outline as much as a slab is
            let frame = build(&Spec { fill: false, thick: 8.0, stones: Some(stones), ..slab }).unwrap();
            assert!(frame.bricks.len() > 4, "no stones were laid on the frame");
        }
    }

    #[test]
    fn a_point_added_to_a_box_pulls_its_side_out_there() {
        for fill in [true, false] {
            let plain = Spec { fill, thick: 2.0, stretch: DVec3::new(1.0, 1.0, 0.2), ..cube(DVec3::splat(0.5), 12.5) };
            // taken hold of half way along its high x side, and pulled out by 5
            let pulled = Spec { pulls: vec![(DVec3::new(13.0, 0.5, 0.5), DVec3::new(5.0, 0.0, 0.0))], pull_reach: 6.0, ..plain.clone() };
            let (before, after) = (build(&plain).unwrap(), build(&pulled).unwrap());
            let far = |b: &Built| b.bricks.iter().map(|b| b.min[0] + b.ext[0]).max().unwrap();
            assert_eq!(far(&before), 13);
            assert!((17..=18).contains(&far(&after)), "fill {fill}: it reaches {}", far(&after));
            // the other sides are where they were
            let low = |b: &Built, k: usize| b.bricks.iter().map(|b| b.min[k]).min().unwrap();
            let high = |b: &Built, k: usize| b.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap();
            assert_eq!([low(&after, 0), low(&after, 1), low(&after, 2), high(&after, 2)], [low(&before, 0), low(&before, 1), low(&before, 2), high(&before, 2)], "fill {fill}");
            assert!(after.error / after.solid < 0.1, "fill {fill}: {} of {}", after.error, after.solid);
        }
    }
}
