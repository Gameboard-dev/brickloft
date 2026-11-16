//! Hedge grown over a solid of any shape: an arch of clipped hedge.
//!
//! The Plant form's hedge is grown straight and laid along a line on the
//! ground (see [`super::Grown::lay_along`]). An arch has no such line: its
//! band climbs and comes down again, and a hedge laid along it would be a
//! wall bent over, its "top" facing sideways at the crown. So the hedge is
//! grown here from the solid's own distance function. Tufts are set all
//! over its surface, each looking the way the surface looks there, the
//! inside is filled, and stems stand wherever the solid meets the ground.
//!
//! Nothing here knows that the solid is an arch. Any form that has a
//! distance function could be given the same hedge (a ring, a disc, a
//! sphere), which is why it takes a function and not an arch.
//!
//! The tufts are the hedge's own ([`super::tuft`]): the same balls of
//! leaves with clusters standing out of them, set back from the face by
//! the same rule, with the same berries. A second copy of them here would
//! have drifted from the first.

use super::{tuft, Grown, Tree, LEAF_DARK, TRUNK};
use glam::DVec3;
use std::sync::Arc;

/// A solid for hedge to be grown over: negative inside, positive outside,
/// and never more than the distance to its surface.
#[derive(Clone)]
pub struct Over {
    pub solid: Arc<dyn Fn(DVec3) -> f64 + Send + Sync>,
    /// a box that holds it
    pub lo: DVec3,
    pub hi: DVec3,
    /// half of how thick it is where it is thinnest: what a hedge's spread is to a hedge
    pub half: f64,
}

/// How far outside the solid its hedge may stand, in cells. A tuft's
/// clusters of leaves stand out of it, and on a wall in a garden that is
/// the hedge's look; in the opening of an arch it is the passage closing.
/// The hedge is cut off this far out (see [`super::Model::nearest`]), so
/// an opening is narrower by a cell a side at the most.
pub const PROUD: f64 = 1.0;

/// The most stems a hedge over a solid stands on. They are for the look of
/// its foot, and a ring a thousand cells round needs no more of them.
const MOST_STEMS: usize = 240;

/// Grow hedge over `over`, which is given from the plant's own foot: the
/// floor is z = 0.
pub(super) fn hedge(g: &mut Grown, tree: &Tree, over: &Over) {
    let (dense, loose) = (tree.fullness as f64 / 100.0, tree.bend as f64 / 100.0);
    // Grown over the solid made `PROUD` larger all round, which is where
    // it is cut off: a hedge's tufts are held inside its size, and held
    // inside the solid itself they left it a cell thin on every face,
    // with two fifths of an arch's own cells empty. So the hollows
    // between the tufts are at the solid's own surface and their tops a
    // cell proud of it, cut level there as shears cut them.
    let (inner, half) = (over.solid.clone(), over.half + PROUD);
    let solid = &move |p: DVec3| inner(p) - PROUD;
    let height = (over.hi.z - over.lo.z.max(0.0)).max(1.0);
    // (as a hedge's tufts are sized: by its half thickness)
    let mass = (half * (0.26 + 0.1 * dense)).clamp(2.6, (height * 0.3).max(2.6));
    // how much of its foot is bare stems, where it stands on the ground
    let foot = (height * 0.1).clamp(2.0, 6.0);
    let grounded = |p: DVec3| solid(DVec3::new(p.x, p.y, 0.5)) < 0.0;
    // which way the surface looks at a point: the slope of the distance, by differences
    let outward = |p: DVec3| {
        let e = 0.5;
        DVec3::new(solid(p + DVec3::X * e) - solid(p - DVec3::X * e), solid(p + DVec3::Y * e) - solid(p - DVec3::Y * e), solid(p + DVec3::Z * e) - solid(p - DVec3::Z * e)).normalize_or_zero()
    };
    let trunk = g.limb_record(vec![DVec3::ZERO, DVec3::Z], vec![1.2, 1.2], None, 0);
    // The inside: balls on a lattice, each as large as fits where it is.
    // A ball no larger than its depth under the surface is inside the
    // solid, so the filling never shows.
    let apart = ((half - 0.6) * 0.9).max(1.5);
    let cells = |low: f64, high: f64, step: f64| (0..=((high - low) / step).ceil().max(0.0) as usize).map(move |n| low + n as f64 * step);
    for z in cells(over.lo.z.max(0.0), over.hi.z, apart) {
        for y in cells(over.lo.y, over.hi.y, apart) {
            for x in cells(over.lo.x, over.hi.x, apart) {
                let p = DVec3::new(x, y, z);
                let r = (-solid(p)).min(half) - 0.4;
                if r >= 1.2 && !(grounded(p) && z < foot + r * 0.5) {
                    g.blob(p, DVec3::splat(r), LEAF_DARK, true);
                }
            }
        }
    }
    // The stems: wherever the solid stands on the ground, a few to each
    // patch of it, as each plant of a hedge shows a few.
    let between = (half * 0.9).clamp(2.5, 5.0);
    let mut stems = 0;
    for y in cells(over.lo.y, over.hi.y, between) {
        for x in cells(over.lo.x, over.hi.x, between) {
            let from = DVec3::new(x + g.between(-0.3, 0.3) * between, y + g.between(-0.3, 0.3) * between, 0.0);
            if stems >= MOST_STEMS || solid(from + DVec3::Z * 0.5) > -1.2 {
                continue;
            }
            let to = from + DVec3::new(g.between(-0.3, 0.3) * half, g.between(-0.3, 0.3) * half, foot + mass);
            let thick = g.between(1.0, 1.5);
            let bent = from.lerp(to, 0.5) + g.any_way() * 0.8;
            g.limb(vec![from, bent, to], vec![thick, thick * 0.9, thick * 0.8], Some(trunk), 1, TRUNK);
            stems += 1;
        }
    }
    // The tufts: one for each point of a lattice that lies within half a
    // step of the surface, moved onto the surface. Every face then has
    // one row of tufts a step apart whichever way it looks, and a lattice
    // each of whose layers is half a step along from the last does not
    // line them up in columns.
    let step = mass * 1.15;
    for (k, z) in cells(over.lo.z.max(0.0), over.hi.z + step, step).enumerate() {
        for (j, y) in cells(over.lo.y - step, over.hi.y + step, step).enumerate() {
            for x in cells(over.lo.x - step, over.hi.x + step, step) {
                let p = DVec3::new(x + 0.5 * step * ((j + k) % 2) as f64, y + 0.5 * step * (k % 2) as f64, z);
                let far = solid(p);
                if far.abs() > step * 0.5 {
                    continue;
                }
                let way = outward(p);
                if way == DVec3::ZERO {
                    continue;
                }
                let on = p - way * far;
                // Kept inside the solid, its clusters of leaves and
                // all, as a hedge's tufts are kept inside its box: as
                // deep as its own size and a cell more, or at the middle
                // where the solid is thinner than that. (And clear of
                // the bare foot, where it stands on the ground.)
                let lowest = if grounded(on) { foot } else { f64::NEG_INFINITY };
                tuft(g, mass, loose, tree.extras, on, way, 0.0, &|at, r| {
                    let deeper = ((r + 1.0).min(half) + solid(at)).max(0.0);
                    let at = at - way * deeper;
                    DVec3::new(at.x, at.y, at.z.max(lowest + r * 0.4))
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Kind, Model, LEAF, LEAF_LIGHT};

    /// A ball of hedge, for one sunk a little into the ground and one in the air.
    fn ball(centre: DVec3, radius: f64) -> Over {
        Over { solid: Arc::new(move |p: DVec3| p.distance(centre) - radius), lo: centre - radius, hi: centre + radius, half: radius }
    }

    #[test]
    fn hedge_covers_a_solid_all_over_fills_it_and_stands_no_further_out_than_it_may() {
        let over = ball(DVec3::new(0.0, 0.0, 10.0), 14.0);
        let model = Model::over(&Tree::of(Kind::Hedge), DVec3::new(0.5, 0.5, 0.5), &over);
        let mut numbers = crate::pattern::Numbers(7);
        let (mut bare, mut asked) = (0, 0);
        for _ in 0..4000 {
            let way = DVec3::new(numbers.unit() - 0.5, numbers.unit() - 0.5, numbers.unit() - 0.5).normalize();
            let at = |far: f64| DVec3::new(0.5, 0.5, 10.0) + way * far;
            // nothing of it further out than it may be, whichever way one looks
            assert!(model.eval(at(14.0 + PROUD + 0.01)) > 0.0, "hedge stands out past {way}");
            // filled: its middle is hedge all through (above its bare foot)
            if at(8.0).z > 8.0 {
                assert!(model.eval(at(8.0)) < 0.0, "a hole inside it towards {way}");
            }
            // and leaves on every face of it: two cells under the surface is hedge nearly everywhere
            if at(12.0).z > 8.0 {
                asked += 1;
                bare += (model.eval(at(12.0)) > 0.0) as usize;
            }
        }
        assert!(bare * 12 < asked, "{bare} of {asked} places on its surface are bare");
        // leaves light on top and dark underneath, and stems under it
        assert_eq!(model.shade(DVec3::new(0.5, 0.5, 23.5)), LEAF_LIGHT);
        assert!(matches!(model.shade(DVec3::new(0.5, 0.5, 14.0)), LEAF | LEAF_DARK));
        assert!(model.limbs.len() > 3, "{} stems", model.limbs.len() - 1);
        // one in the air has no stems and no bare foot
        let aloft = Model::over(&Tree::of(Kind::Hedge), DVec3::new(0.5, 0.5, 0.5), &ball(DVec3::new(0.0, 0.0, 40.0), 10.0));
        assert_eq!(aloft.limbs.len(), 1);
        assert!(aloft.eval(DVec3::new(0.5, 0.5, 32.5)) < 0.0);
    }

    use crate::curves::ArchType;
    use crate::spec::{build, Built, Form, Spec};
    use crate::tree::{EXTRA, LEAF_DARK as DARK};
    use std::collections::HashSet;

    /// An arch 60 across between its feet, 10 thick and 12 deep, plain or as hedge.
    fn arch(kind: ArchType, hedge: bool) -> Spec {
        Spec { form: Form::Arch(kind), points: vec![DVec3::new(-29.5, 0.5, 0.5), DVec3::new(30.5, 0.5, 0.5)], thick: 10.0, depth: 12, hedge, grain: 0, tree: Tree::of(Kind::Hedge), ..Spec::default() }
    }

    /// Every cell any brick of a build has a part in.
    fn cells(built: &Built) -> HashSet<[i32; 3]> {
        built.bricks.iter().flat_map(|b| (0..b.ext[0]).flat_map(move |x| (0..b.ext[1]).flat_map(move |y| (0..b.ext[2]).map(move |z| [b.min[0] + x, b.min[1] + y, b.min[2] + z])))).collect()
    }

    /// How wide the opening of an arch is `up` above the floor and how
    /// high it is at its middle, in whole cells, with nothing of any brick
    /// in them at any depth.
    fn opening(built: &Built, up: i32) -> (i32, i32) {
        let filled = cells(built);
        let (front, back) = (built.bricks.iter().map(|b| b.min[1]).min().unwrap(), built.bricks.iter().map(|b| b.min[1] + b.ext[1]).max().unwrap());
        let clear = |x: i32, z: i32| (front..back).all(|y| !filled.contains(&[x, y, z]));
        let wide = (0..200).take_while(|x| clear(*x, up)).count() + (1..200).take_while(|x| clear(-*x, up)).count();
        (wide as i32, (0..200).take_while(|z| clear(0, *z)).count() as i32)
    }

    #[test]
    fn an_arch_of_hedge_keeps_the_plain_archs_opening_and_is_hedge_on_every_face() {
        for kind in [ArchType::Round, ArchType::Pointed, ArchType::Parabolic] {
            let (plain, hedge) = (build(&arch(kind, false)).unwrap(), build(&arch(kind, true)).unwrap());
            // the opening is the plain arch's to within a cell each way: measured above the hedge's bare foot
            let (was, is) = (opening(&plain, 8), opening(&hedge, 8));
            println!("{kind:?}: the plain arch's opening is {was:?} and the hedge's {is:?}; {} and {} bricks", plain.bricks.len(), hedge.bricks.len());
            assert!((was.0 - is.0).abs() <= 2 && (was.1 - is.1).abs() <= 1 && is.0 >= was.0 - 2 && is.1 >= was.1 - 1, "{kind:?}: an opening {was:?} became {is:?}");
            // no two bricks share a cell, and each has its colour
            let mut seen = HashSet::new();
            for b in &hedge.bricks {
                for z in 0..b.ext[2] {
                    for y in 0..b.ext[1] {
                        for x in 0..b.ext[0] {
                            assert!(seen.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]), "{kind:?}: two bricks in one cell");
                        }
                    }
                }
            }
            assert_eq!(hedge.shades.len(), hedge.bricks.len());
            // Filled: of the plain arch's own cells above the bare
            // foot, seven tenths and more are hedge. The rest are the hollows
            // between tufts, and one layer where the plain arch is laid
            // half a cell aside of its curve and the hedge is not.
            let (band, grown) = (cells(&plain), cells(&hedge));
            let above: Vec<&[i32; 3]> = band.iter().filter(|c| c[2] >= 8).collect();
            let covered = above.iter().filter(|c| grown.contains(**c)).count();
            assert!(covered * 100 >= above.len() * 70, "{kind:?}: {covered} of the band's {} cells are hedge", above.len());
            // on both fronts, over the top and under the crown: the cell
            // two in from each face of the plain arch, at its crown, is hedge
            let top = plain.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap();
            let (front, back) = (plain.bricks.iter().map(|b| b.min[1]).min().unwrap(), plain.bricks.iter().map(|b| b.min[1] + b.ext[1]).max().unwrap());
            let under = was.1;
            for (face, cell) in [("top", [0, 0, top - 3]), ("soffit", [0, 0, under + 2]), ("front", [0, front + 2, (top + under) / 2]), ("back", [0, back - 3, (top + under) / 2])] {
                assert!(grown.contains(&cell), "{kind:?}: no hedge at its {face}, {cell:?}");
            }
            // and nothing of it far outside the plain arch: a cell out each way at the most, by the box they fill
            for k in 0..3 {
                let reach = |built: &Built| (built.bricks.iter().map(|b| b.min[k]).min().unwrap(), built.bricks.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap());
                let (was, is) = (reach(&plain), reach(&hedge));
                assert!(is.0 >= was.0 - 2 && is.1 <= was.1 + 2, "{kind:?}: along axis {k} the plain arch fills {was:?} and the hedge {is:?}");
            }
            // light leaves on top, dark underneath, and stems at its two feet
            let count = |wanted: u8, low: i32, high: i32| hedge.bricks.iter().zip(&hedge.shades).filter(|(b, s)| **s == wanted && b.min[2] >= low && b.min[2] < high).count();
            assert!(count(LEAF_LIGHT, top - 6, top + 2) > 20 && count(DARK, under - 1, under + 3) > 10, "{kind:?}: {} light on top, {} dark underneath", count(LEAF_LIGHT, top - 6, top + 2), count(DARK, under - 1, under + 3));
            let stems: Vec<i32> = hedge.bricks.iter().zip(&hedge.shades).filter(|(b, s)| **s == TRUNK && b.min[2] == 0).map(|(b, _)| b.min[0]).collect();
            assert!(stems.iter().any(|x| *x < -15) && stems.iter().any(|x| *x > 15) && stems.iter().all(|x| x.abs() > 15), "{kind:?}: stems at {stems:?}");
        }
    }

    #[test]
    fn an_arch_of_hedge_is_one_arch_for_a_seed_has_berries_when_asked_and_is_thickened_or_coarsened_not_refused() {
        let spec = arch(ArchType::Round, true);
        let (one, again) = (build(&spec).unwrap(), build(&spec).unwrap());
        assert_eq!((&one.bricks, &one.shades), (&again.bricks, &again.shades));
        // at single microbricks, whatever the budget of bricks: coarser, its tufts are blocks
        assert_eq!((one.grain, build(&Spec { budget: 300, ..spec.clone() }).unwrap().grain), (1, 1));
        assert_ne!(build(&Spec { tree: Tree { seed: 2, ..spec.tree }, ..spec.clone() }).unwrap().bricks, one.bricks);
        // on any number of threads
        let on = |threads: crate::par::Threads| crate::spec::build_with(&spec, true, threads).unwrap();
        let (single, shared) = (on(crate::par::Threads::ONE), on(crate::par::Threads::available()));
        assert_eq!((single.bricks, single.shades), (shared.bricks, shared.shades));
        // berries only when asked for
        let berried = build(&Spec { tree: Tree { extras: true, ..spec.tree }, ..spec.clone() }).unwrap();
        let berries = |built: &Built| built.shades.iter().filter(|s| **s == EXTRA).count();
        assert!(berries(&berried) > 10 && berries(&one) == 0, "{} and {}", berries(&berried), berries(&one));
        // too thin and too shallow for a hedge, it is the hedge of the least size and not a refusal
        let thin = build(&Spec { thick: 2.0, depth: 1, ..spec.clone() }).unwrap();
        let least = build(&Spec { thick: crate::maze::LEAST_HEDGE, depth: crate::maze::LEAST_HEDGE as i32, ..spec.clone() }).unwrap();
        assert_eq!(thin.bricks, least.bricks);
        let deep = |built: &Built| built.bricks.iter().map(|b| b.min[1] + b.ext[1]).max().unwrap() - built.bricks.iter().map(|b| b.min[1]).min().unwrap();
        assert!(deep(&thin) >= 5, "{} deep", deep(&thin));
        // too much hedge for single microbricks, it is built at a larger brick size and not refused
        let vast = Spec { points: vec![DVec3::new(-450.5, 0.5, 0.5), DVec3::new(450.5, 0.5, 0.5)], thick: 40.0, depth: 40, ..spec.clone() };
        let built = build(&vast).unwrap();
        assert!(built.grain > 1 && !built.bricks.is_empty(), "grain {}", built.grain);
        // (asked for at single microbricks by name, it is refused with what to change)
        assert!(build(&Spec { grain: 1, ..vast }).is_err_and(|e| e.contains("too much hedge")));
        // a plain arch takes no notice of the hedge's settings, and a form that is no arch none of `hedge`
        assert_eq!(build(&arch(ArchType::Round, false)).unwrap().bricks, build(&Spec { tree: Tree { seed: 9, ..spec.tree }, ..arch(ArchType::Round, false) }).unwrap().bricks);
        let ball = Spec { form: Form::Sphere, points: vec![DVec3::splat(0.5)], radius: 8.0, ..Spec::default() };
        assert_eq!(build(&Spec { hedge: true, ..ball.clone() }).unwrap().bricks, build(&ball).unwrap().bricks);
    }
}
