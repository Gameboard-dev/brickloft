use super::*;
use crate::spec::{build, build_with, Form, Spec};
use std::sync::Arc;
use crate::tree::{ground, Kind, Sculpt};

/// Grass through `points`, reaching `spread` to each side, as a spec.
fn along(points: &[(f64, f64)], spread: f64, closed: bool) -> Spec {
    let (height, _) = Tree::usual_size(Kind::Grass);
    Spec { form: Form::Tree, points: points.iter().map(|(x, y)| DVec3::new(x + 0.5, y + 0.5, 0.5)).collect(), radius: spread, rise: Some(height), closed, stretch: DVec3::ONE, tree: Tree { seed: 1, ..Tree::of(Kind::Grass) }, ..Spec::default() }
}

/// Where the bricks of the parts `wanted` are: the middle of each.
fn bricks_of(built: &crate::spec::Built, wanted: &[u8]) -> Vec<DVec3> {
    built.bricks.iter().zip(&built.shades).filter(|(_, s)| wanted.contains(s)).map(|(b, _)| DVec3::new(b.min[0] as f64 + b.ext[0] as f64 / 2.0, b.min[1] as f64 + b.ext[1] as f64 / 2.0, b.min[2] as f64 + b.ext[2] as f64 / 2.0)).collect()
}

fn extent(built: &crate::spec::Built, wanted: &[u8]) -> (DVec3, DVec3) {
    bricks_of(built, wanted).iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(*p), hi.max(*p)))
}

/// How tall a build stands over a place on the ground.
fn top_at(built: &crate::spec::Built, x: f64, y: f64) -> Option<i32> {
    built.bricks.iter().filter(|b| (b.min[0] as f64) <= x && x < (b.min[0] + b.ext[0]) as f64 && (b.min[1] as f64) <= y && y < (b.min[1] + b.ext[1]) as f64).map(|b| b.min[2] + b.ext[2]).max()
}

/// The lawn the timings are of: 180 cells by 120, with 24 touches of the
/// brush on it, hills and hollows (the lawn of `timing_sculpting_a_lawn`
/// in the editor).
fn sculpted() -> Spec {
    let mut seed = 7u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) % 1000) as f64 / 1000.0
    };
    let pulls = (0..24).map(|_| (DVec3::new(next() * 100.0 - 50.0, next() * 80.0 - 40.0, 2.0), DVec3::new(10.0 + next() * 15.0, 0.0, next() * 12.0 - 4.0))).collect();
    Spec { pulls, ..along(&[(-30.0, 0.0), (0.0, 0.0), (30.0, 0.0)], 60.0, false) }
}

/// A lawn 180 by 120 with 24 touches, a lawn with raised ponds and a stream, and one with a fall over a cliff, each built from nothing and timed: run by name with `--release -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn how_long_a_lawn_takes() {
    for (name, spec) in [("24 touches", sculpted()), ("raised ponds and a stream", Spec { tube: true, ..two_hollows() }), ("a fall over a cliff", fall())] {
        let spec = spec.snapped();
        for threads in [Threads::ONE, Threads::available()] {
            // (from nothing: built as any form is, a lawn built a second time is the lawn kept from the first)
            let least = (0..7).map(|_| { let at = std::time::Instant::now(); let built = fresh_of(&spec, threads); (at.elapsed(), built.bricks.len()) }).min().unwrap();
            println!("{name}, on {} threads: {:?}, {} bricks", threads.count(), least.0, least.1);
        }
    }
}

#[test]
fn grass_is_a_skin_of_soil_and_blades_that_added_points_raise() {
    let (height, spread) = Tree::usual_size(Kind::Grass);
    let strip = along(&[(-30.0, 0.0), (0.0, 0.0), (30.0, 0.0)], spread, false);
    let flat = build(&strip).unwrap();
    // thin: nowhere taller than its blades and the soil under them
    let (lo, hi) = extent(&flat, &[TRUNK, LEAF, LEAF_DARK, LEAF_LIGHT]);
    assert!(hi.z <= height + 3.0 && lo.z >= 0.0, "{} to {} high", lo.z, hi.z);
    // soil under all of it, with no gap: over the whole patch the ground is covered, and it is the soil's colour at the bottom
    let (mut covered, mut all) = (0, 0);
    for x in (-28..28).step_by(3) {
        for y in (-18..18).step_by(3) {
            all += 1;
            covered += top_at(&flat, x as f64 + 0.5, y as f64 + 0.5).is_some() as usize;
        }
    }
    assert_eq!(covered, all, "bare ground showing through the grass");
    // (two cells thick, in few large bricks: level ground is one box to a tile)
    let soil: Vec<&CellBrick> = flat.bricks.iter().zip(&flat.shades).filter(|(_, s)| **s == TRUNK).map(|(b, _)| b).collect();
    let cells: i32 = soil.iter().map(|b| b.ext[0] * b.ext[1] * b.ext[2]).sum();
    assert!(cells > 60 * 36 * 2 && soil.len() < 200 && soil.iter().all(|b| b.min[2] >= 0 && b.min[2] + b.ext[2] <= 2), "{cells} cells of soil in {} bricks", soil.len());
    // three greens, and blades of different heights
    for shade in [LEAF, LEAF_DARK, LEAF_LIGHT] {
        assert!(bricks_of(&flat, &[shade]).len() > 200, "few bricks of green {shade}");
    }
    let tops: std::collections::HashSet<i32> = (-25..25).filter_map(|x| top_at(&flat, x as f64 + 0.5, 0.5)).collect();
    assert!(tops.len() >= 4, "the blades are all one height: {tops:?}");
    // thicker growth is more blades
    let blades = |full: i32| build(&Spec { tree: Tree { fullness: full, ..strip.tree }, ..strip.clone() }).unwrap().bricks.len();
    assert!(blades(100) > blades(0) * 3 / 2);

    // a point added and lifted raises the ground round it, by about as much, and leaves the far end alone
    let mound = build(&Spec { pulls: vec![(DVec3::new(10.5, 2.5, 0.5), DVec3::new(16.0, 0.0, 12.0))], ..strip.clone() }).unwrap();
    let (at_it, far) = (top_at(&mound, 10.5, 2.5).unwrap(), top_at(&mound, -25.5, 2.5).unwrap());
    assert!(at_it as f64 > 12.0 && at_it as f64 <= 12.0 + height + 3.0 && far as f64 <= height + 3.0, "{at_it} high at the mound, {far} far from it");
    // a surface: under the mound there is nothing
    assert!(!mound.bricks.iter().any(|b| b.min[0] <= 10 && 10 < b.min[0] + b.ext[0] && b.min[1] <= 2 && 2 < b.min[1] + b.ext[1] && b.min[2] <= 4 && 4 < b.min[2] + b.ext[2]), "the mound is solid");
    // the soil is its skin still: soil bricks up at the mound's height
    assert!(bricks_of(&mound, &[TRUNK]).iter().any(|b| b.z > 9.0));
    // pushed down, the ground stays on the floor it stands on
    let dip = build(&Spec { pulls: vec![(DVec3::new(10.5, 2.5, 0.5), DVec3::new(16.0, 0.0, -12.0))], ..strip.clone() }).unwrap();
    // (pushed down in flat ground it digs below the floor, and the soil goes down with it)
    let lowest = dip.bricks.iter().map(|b| b.min[2]).min().unwrap();
    assert!(lowest <= -10 && lowest >= -15, "a hollow dug 12 deep reaches {lowest}");
    // (water stands in it, 3 below the lawn)
    assert!(top_at(&dip, 10.5, 2.5).unwrap() == -3 && top_at(&dip, -25.5, 2.5).unwrap() >= 1);

    // The ground as a brush shapes it. A wide push is a hill, not a
    // spike: half way out it is half as high, and at its edge it is gone.
    let hill = Sculpt { at: DVec3::ZERO, range: 30.0, amount: 10.0, flatten: false, sharp: false };
    let at = |touches: &[Sculpt], x: f64| ground(touches, DVec3::new(x, 0.0, 0.0));
    assert!((at(&[hill], 0.0) - 10.0).abs() < 1e-9 && (at(&[hill], 15.0) - 5.0).abs() < 1e-9 && at(&[hill], 30.0) == 0.0 && at(&[hill], 29.0) < 0.1);
    // level at its top and at its edge: no crease where it meets the flat
    assert!(at(&[hill], 0.0) - at(&[hill], 1.0) < 0.05 && at(&[hill], 29.0) - at(&[hill], 30.0) < 0.05);
    // two pushes add, and a push down takes a hollow out of raised ground and no more
    let beside = Sculpt { at: DVec3::new(20.0, 0.0, 0.0), ..hill };
    assert!(at(&[hill, beside], 10.0) > at(&[hill], 10.0) + 4.0);
    let hollow = Sculpt { at: DVec3::ZERO, range: 10.0, amount: -4.0, flatten: false, sharp: false };
    assert!((at(&[hill, hollow], 0.0) - 6.0).abs() < 1e-9 && at(&[hollow], 0.0) == -4.0);
    // flattening brings the ground to its level within half its range, and leaves it alone beyond it
    let terrace = Sculpt { at: DVec3::new(12.0, 0.0, 0.0), range: 12.0, amount: 3.0, flatten: true, sharp: false };
    for x in [7.0, 12.0, 17.0] {
        assert!((at(&[hill, terrace], x) - 3.0).abs() < 1e-9, "{} at {x}", at(&[hill, terrace], x));
    }
    assert_eq!(at(&[hill, terrace], -5.0), at(&[hill], -5.0));
    // strokes are in order: a hill pushed up after the flattening stands on the terrace
    assert!(at(&[terrace, hill], 12.0) > 3.0);
    // and built: a flattened top is level, where the hill it was cut from was not
    let top = |pulls: Vec<(DVec3, DVec3)>| { let built = build(&Spec { pulls, ..strip.clone() }).unwrap(); bricks_of(&built, &[TRUNK]).iter().filter(|b| (b.x - 0.5).abs() < 6.0 && b.y.abs() < 6.0).map(|b| b.z).fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(z), hi.max(z))) };
    let (raised, levelled) = (top(vec![(DVec3::new(8.5, 0.5, 0.5), DVec3::new(24.0, 0.0, 12.0))]), top(vec![(DVec3::new(8.5, 0.5, 0.5), DVec3::new(24.0, 0.0, 12.0)), (DVec3::new(0.5, 0.5, 0.5), DVec3::new(20.0, 16.0, 6.0))]));
    assert!(raised.1 - raised.0 > 2.5 && levelled.1 - levelled.0 <= 2.0 && (levelled.0 - 6.0).abs() < 2.5, "raised {raised:?}, flattened {levelled:?}");

    // a closed line is an outline, filled; open, it is a strip along the line
    let corners = [(-40.0, -40.0), (40.0, -40.0), (40.0, 40.0), (-40.0, 40.0)];
    let (lawn, strips) = (build(&along(&corners, 6.0, true)).unwrap(), build(&along(&corners, 6.0, false)).unwrap());
    assert!(top_at(&lawn, 0.5, 0.5).is_some() && top_at(&strips, 0.5, 0.5).is_none());
}

#[test]
fn a_hollow_dug_below_the_waters_level_is_a_pond() {
    let lawn = Spec { closed: true, depth: 6, ..along(&[(-40.0, -30.0), (40.0, -30.0), (40.0, 30.0), (-40.0, 30.0)], 4.0, true) };
    let level = -(lawn.tree.count as f64);
    // a hollow 10 deep, 22 wide: far deeper than the water stands (3 below the lawn)
    let dig = |amount: f64| Spec { pulls: vec![(DVec3::new(0.5, 0.5, 0.5), DVec3::new(22.0, 0.0, amount))], ..lawn.clone() };
    let pond = build(&dig(-10.0)).unwrap();
    let water = bricks_of(&pond, &[WATER]);
    assert!(water.len() > 5, "no water in a hollow 10 deep");
    // flat on top, at the level, and nowhere above it; under it, down to the bed
    let top = pond.bricks.iter().zip(&pond.shades).filter(|(_, s)| **s == WATER).map(|(b, _)| b.min[2] + b.ext[2]).max().unwrap();
    assert_eq!(top as f64, level, "the water's top");
    for x in [-6.0, 0.0, 5.0] {
        let over = pond.bricks.iter().zip(&pond.shades).filter(|(b, _)| b.min[0] as f64 <= x && x < (b.min[0] + b.ext[0]) as f64 && b.min[1] <= 0 && 0 < b.min[1] + b.ext[1]).max_by_key(|(b, _)| b.min[2] + b.ext[2]).unwrap();
        assert_eq!((*over.1, over.0.min[2] + over.0.ext[2]), (WATER, top), "over the middle of the pond at {x}");
    }
    // in few large bricks
    let cells: i32 = pond.bricks.iter().zip(&pond.shades).filter(|(_, s)| **s == WATER).map(|(b, _)| b.ext[0] * b.ext[1] * b.ext[2]).sum();
    assert!(cells / water.len() as i32 >= 10, "{cells} cells of water in {} bricks", water.len());
    // the bank in order going down: grass, then sand, then the bed; and stones only on the bed
    let mean = |shades: &[u8]| { let these = bricks_of(&pond, shades); assert!(these.len() > 3, "no bricks of {shades:?}"); these.iter().map(|p| p.truncate().length()).sum::<f64>() / these.len() as f64 };
    let (bed, sand) = (mean(&[BED]), mean(&[SAND]));
    assert!(bed < sand && sand < 22.0, "the bed is {bed:.1} from the middle and the sand {sand:.1}");
    let blades = bricks_of(&build(&Spec { depth: 1, ..dig(-10.0) }).unwrap(), &[LEAF, LEAF_DARK, LEAF_LIGHT]);
    let near = blades.iter().filter(|p| p.truncate().length() < sand - 3.0).count();
    assert!(blades.len() > 500);
    assert_eq!(near, 0, "grass growing in the water");
    // (a large one may stand out of shallow water)
    assert!(bricks_of(&pond, &[STONE]).iter().all(|p| p.z < level + 3.0));
    // reeds round the margin, more when asked for more, and none when asked for none
    let reeds = |depth: i32| build(&Spec { depth, ..dig(-10.0) }).unwrap().bricks.iter().filter(|b| b.ext[2] >= 3 && b.min[2] + b.ext[2] > 12).count();
    assert!(reeds(1) == 0 && reeds(4) > 0 && reeds(11) > reeds(4) * 2, "{} {} {}", reeds(1), reeds(4), reeds(11));
    // no water in a hollow that stops above the level, on a hill, or when water is switched off
    assert!(bricks_of(&build(&dig(-2.0)).unwrap(), &[WATER, SAND, BED]).is_empty() || bricks_of(&build(&dig(-2.0)).unwrap(), &[WATER]).is_empty());
    assert!(bricks_of(&build(&dig(8.0)).unwrap(), &[WATER]).is_empty());
    assert!(bricks_of(&build(&Spec { tree: Tree { count: 0, ..lawn.tree }, ..dig(-10.0) }).unwrap(), &[WATER]).is_empty());
    // Digging the pond leaves the lawn far from it blade for blade as it was. (From its own level up: a patch with a pond in it is cut off at the pond's depth and not at its own level, so the underside of its soil, most of a cell, is built all over it where a flat lawn's is cut away.)
    let flat = build(&lawn).unwrap();
    // (as the cells it fills: the bricks are joined up a little differently, the fit having begun from another corner)
    let far = |built: &crate::spec::Built| { let mut cells = std::collections::BTreeSet::new(); for b in built.bricks.iter().filter(|b| b.min[0] > 28 && b.min[2] >= 2 && b.mask == CUBE) { for x in 0..b.ext[0] { for y in 0..b.ext[1] { for z in 0..b.ext[2] { cells.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]); } } } } cells };
    let (was, now) = (far(&flat), far(&pond));
    assert!(was.len() > 300 && was.symmetric_difference(&now).count() * 50 < was.len(), "{} of {} cells differ", was.symmetric_difference(&now).count(), was.len());
}

/// A lawn with a hill that has a hollow on it, and a hollow in the flat beside the hill.
fn two_hollows() -> Spec {
    let lawn = Spec { depth: 4, ..along(&[(-60.0, -40.0), (60.0, -40.0), (60.0, 40.0), (-60.0, 40.0)], 4.0, true) };
    let touch = |x: f64, range: f64, amount: f64| (DVec3::new(x + 0.5, 0.5, 0.5), DVec3::new(range, 0.0, amount));
    Spec { pulls: vec![touch(-25.0, 34.0, 16.0), touch(-25.0, 16.0, -14.0), touch(30.0, 20.0, -10.0)], ..lawn }
}

/// The heights water's surface stands at over the cell `x`, `y`.
fn water_tops(_: &Spec, built: &crate::spec::Built, x: i32, y: i32) -> std::collections::BTreeSet<i32> {
    built.bricks.iter().zip(&built.shades).filter(|(b, s)| **s == WATER && b.min[0] <= x && x < b.min[0] + b.ext[0] && b.min[1] <= y && y < b.min[1] + b.ext[1]).map(|(b, _)| b.min[2] + b.ext[2]).collect()
}

/// The heights water's surface stands at over the places from `from` to `to` along x.
fn water_tops_along(built: &crate::spec::Built, from: f64, to: f64) -> std::collections::BTreeSet<i32> {
    let mut tops = std::collections::BTreeMap::new();
    for (b, _) in built.bricks.iter().zip(&built.shades).filter(|(b, s)| **s == WATER && (b.min[0] as f64) < to && (b.min[0] + b.ext[0]) as f64 > from) {
        for x in (b.min[0]..b.min[0] + b.ext[0]).filter(|x| (*x as f64 + 1.0) > from && (*x as f64) < to) {
            for y in b.min[1]..b.min[1] + b.ext[1] {
                let top = tops.entry((x, y)).or_insert(i32::MIN);
                *top = (*top).max(b.min[2] + b.ext[2]);
            }
        }
    }
    tops.into_values().collect()
}

#[test]
fn two_hollows_at_two_heights_hold_water_at_two_levels_and_a_stream_runs_down_between_them() {
    let (one, two) = (two_hollows(), Spec { tube: true, ..two_hollows() });
    let (flat, stepped) = (build(&one).unwrap(), build(&two).unwrap());
    // one level: water only at the lawn's own, in the lower hollow
    // (a cell of water at a pond's very edge may be a cell lower, under the lip of its bank)
    let level = |tops: &std::collections::BTreeSet<i32>| { let (low, high) = (*tops.iter().next().unwrap(), *tops.iter().next_back().unwrap()); assert!(high - low <= 1, "a pond's water at {tops:?}"); high };
    assert_eq!(level(&water_tops_along(&flat, -100.0, 100.0)), -3);
    assert!(water_tops_along(&flat, -100.0, 5.0).is_empty());
    // two: the hollow on the hill holds water too, higher, and each pond is level
    let (upper, lower) = (water_tops_along(&stepped, -30.0, -20.0), water_tops_along(&stepped, 26.0, 34.0));
    assert_eq!(level(&lower), -3);
    let high = level(&upper);
    assert!(high >= 0, "the upper pond stands at {high}");
    // the stream: water all the way from one to the other, never rising on the way down, never over its banks
    let along: Vec<i32> = (-18..=22).map(|x| water_tops_along(&stepped, x as f64 + 0.25, x as f64 + 0.75).into_iter().max().unwrap_or_else(|| panic!("the stream is dry at {x}"))).collect();
    // (taken two cells apart: where it bends, the end of one reach lies beside the start of the next, and which is the higher at one x is by a cell either way)
    assert!(along.windows(3).all(|w| w[2] <= w[0]) && along[0] == high && *along.last().unwrap() == -3, "the stream's water, going down: {along:?}");
    assert!(along.iter().collect::<std::collections::BTreeSet<_>>().len() >= 4, "it falls in {along:?}: no steps");
    let mut worst: f64 = f64::MIN;
    for (b, _) in stepped.bricks.iter().zip(&stepped.shades).filter(|(b, s)| **s == WATER && b.min[0] > -12 && b.min[0] + b.ext[0] < 10) {
        for x in 0..b.ext[0] {
            for y in 0..b.ext[1] {
                let at = DVec3::new((b.min[0] + x) as f64 + 0.5, (b.min[1] + y) as f64 + 0.5, 0.5);
                worst = worst.max((b.min[2] + b.ext[2]) as f64 - crate::spec::grass_ground(&two, at));
            }
        }
    }
    assert!(worst <= 1.0, "the stream stands {worst} over the ground it was cut into");
    // narrow: a few cells of water across, where it runs along x
    let across = |x: i32| stepped.bricks.iter().zip(&stepped.shades).filter(|(b, s)| **s == WATER && b.min[0] <= x && x < b.min[0] + b.ext[0]).map(|(b, _)| (b.min[1], b.min[1] + b.ext[1])).fold((i32::MAX, i32::MIN), |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
    assert!((2..=9).contains(&(across(0).1 - across(0).0)), "the stream is {:?} across", across(0));
    // its bed is sand, and the same on any number of threads
    assert!(bricks_of(&stepped, &[SAND]).iter().filter(|p| p.x > -10.0 && p.x < 12.0).count() > 20);
    let many = crate::spec::build_with(&two, true, crate::par::Threads::forced(5)).unwrap();
    assert_eq!((&stepped.bricks, &stepped.shades), (&many.bricks, &many.shades));
    // one hollow in a flat lawn: two levels are the same bricks as one
    let single = Spec { pulls: two_hollows().pulls[2..].to_vec(), ..two_hollows() };
    let (a, b) = (build(&single).unwrap(), build(&Spec { tube: true, ..single.clone() }).unwrap());
    assert_eq!((&a.bricks, &a.shades), (&b.bricks, &b.shades));
    // a hollow on a hill with no lower water to run to: a pond up there, and no stream
    let alone = build(&Spec { tube: true, pulls: two_hollows().pulls[..2].to_vec(), ..two_hollows() }).unwrap();
    assert!(!water_tops_along(&alone, -30.0, -20.0).is_empty() && water_tops_along(&alone, -8.0, 100.0).is_empty(), "{:?}", water_tops_along(&alone, -8.0, 100.0));
}

#[test]
fn a_pond_is_a_basin_as_deep_as_it_was_dug_with_water_down_to_its_bed() {
    let lawn = Spec { closed: true, depth: 1, ..along(&[(-40.0, -30.0), (40.0, -30.0), (40.0, 30.0), (-40.0, 30.0)], 4.0, true) };
    for deep in [6.0, 10.0, 16.0] {
        let pond = build(&Spec { pulls: vec![(DVec3::new(0.5, 0.5, 0.5), DVec3::new(22.0, 0.0, -deep))], ..lawn.clone() }).unwrap();
        // the cells over the pond's middle, from the top down: which part each is
        let mut column: Vec<(i32, u8)> = pond.bricks.iter().zip(&pond.shades).filter(|(b, _)| b.min[0] <= 0 && 0 < b.min[0] + b.ext[0] && b.min[1] <= 0 && 0 < b.min[1] + b.ext[1]).flat_map(|(b, s)| (b.min[2]..b.min[2] + b.ext[2]).map(|z| (z, *s))).collect();
        column.sort();
        column.reverse();
        let water: Vec<i32> = column.iter().filter(|c| c.1 == WATER).map(|c| c.0).collect();
        // water from the level (3 below the lawn) down to within a cell or two of the dug depth, with nothing else in it
        assert_eq!(column[0], (-4, WATER), "dug {deep}: the top of {column:?}");
        let bottom = *water.last().unwrap();
        assert!(water.len() as i32 == -3 - bottom && (bottom as f64 + deep).abs() <= 2.0, "dug {deep}: water in {water:?}");
        // and under it the bed, at the dug depth: not soil under the surface, nor a slab of water on flat ground
        let under: Vec<(i32, u8)> = column.iter().filter(|c| c.0 < bottom).copied().collect();
        assert!(!under.is_empty() && under.iter().all(|c| matches!(c.1, BED | STONE)) && under.len() <= 4, "dug {deep}: under the water {under:?}");
    }
}


/// Lawns of every sort: sculpted and dry; with ponds at two levels, a stream and as many reeds as there can be; a strip of tall rough grass along a bent line, leaning hard.
fn lawns() -> Vec<(&'static str, Spec)> {
    let rough = Spec { rise: Some(14.0), tree: Tree { bend: 100, fullness: 100, ..Tree::of(Kind::Grass) }, pulls: vec![(DVec3::new(4.5, 2.5, 0.5), DVec3::new(18.0, 0.0, 14.0)), (DVec3::new(-20.5, 2.5, 0.5), DVec3::new(14.0, 0.0, -9.0))], depth: 11, ..along(&[(-40.0, 0.0), (0.0, 12.0), (40.0, -8.0)], 14.0, false) };
    vec![("sculpted", sculpted()), ("two levels", Spec { tube: true, depth: 11, ..two_hollows() }), ("rough", rough)]
}

/// The cells a build's bricks have between them, each with the number of the brick that has it; a sloped brick has every cell of its box.
fn cells_of(built: &crate::spec::Built) -> std::collections::HashMap<[i32; 3], usize> {
    let mut cells = std::collections::HashMap::new();
    for (n, b) in built.bricks.iter().enumerate() {
        for z in 0..b.ext[2] {
            for y in 0..b.ext[1] {
                for x in 0..b.ext[0] {
                    if let Some(other) = cells.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z], n) {
                        panic!("bricks {:?} and {:?} are both in the cell {:?}", built.bricks[other], b, [b.min[0] + x, b.min[1] + y, b.min[2] + z]);
                    }
                }
            }
        }
    }
    cells
}

#[test]
fn no_two_bricks_of_a_lawn_share_a_cell_and_it_is_the_same_on_any_number_of_threads() {
    for (name, spec) in lawns() {
        let one = build_with(&spec, true, Threads::ONE).unwrap();
        // every brick a shape there is, no longer than a brick may be, and a part of the lawn
        assert!(one.bricks.len() == one.shades.len() && one.bricks.iter().all(|b| crate::shapes::shape_info(b.mask).is_some() && b.ext.iter().all(|e| *e >= 1 && *e <= spec.max_ext)), "{name}");
        assert!(one.shades.iter().all(|s| [TRUNK, SAND, BED, STONE, WATER, LEAF, LEAF_DARK, LEAF_LIGHT, CLIFF, ROCK, ROCK_DARK, ROCK_LIGHT, LEDGE, FOAM].contains(s)), "{name}");
        assert!(cells_of(&one).len() > 1000, "{name}");
        // exactly what it is: nothing to measure, and nothing amiss
        assert!(!one.unmeasured && one.error == 0.0 && (one.solid - one.bricks.iter().map(|b| b.volume()).sum::<f64>()).abs() < 1e-6, "{name}");
        for count in [2, 5, 16] {
            let many = build_with(&spec, true, Threads::forced(count)).unwrap();
            assert!(one.bricks == many.bricks && one.shades == many.shades, "{name}: other bricks on {count} threads");
        }
    }
}

/// A build's bricks tile by tile, less its water: a tile's, in the order they were laid.
fn by_tile(built: &crate::spec::Built) -> std::collections::BTreeMap<[i32; 2], Vec<(CellBrick, u8)>> {
    let mut tiles: std::collections::BTreeMap<[i32; 2], Vec<(CellBrick, u8)>> = std::collections::BTreeMap::new();
    for (b, s) in built.bricks.iter().zip(&built.shades).filter(|(_, s)| **s != WATER) {
        tiles.entry([b.min[0].div_euclid(TILE), b.min[1].div_euclid(TILE)]).or_default().push((*b, *s));
    }
    tiles
}

#[test]
fn a_touch_changes_the_bricks_of_the_tiles_it_reaches_and_of_no_other() {
    // (a dry lawn: a pond's level is found from the whole of the ground, and reeds stand where the water is)
    let lawn = Spec { tree: Tree { count: 0, ..Tree::of(Kind::Grass) }, ..sculpted() };
    let (at, range) = (DVec3::new(-38.5, 20.5, 0.5), 14.0);
    let mut touched = lawn.clone();
    touched.pulls.push((at, DVec3::new(range, 0.0, 9.0)));
    let (before, after) = (by_tile(&build(&lawn).unwrap()), by_tile(&build(&touched).unwrap()));
    assert!(before.len() >= 20 && before.keys().eq(after.keys()), "{} tiles before and {} after", before.len(), after.len());
    // a tile is reached if the touch comes within a blade's reach of it (blades of the usual height lean under 7 cells)
    let reached = |tile: &[i32; 2]| (0..2).all(|k| at[k] + range + 7.0 > (tile[k] * TILE) as f64 && at[k] - range - 7.0 < ((tile[k] + 1) * TILE) as f64);
    let (mut same, mut changed) = (0, 0);
    for (tile, bricks) in &before {
        if reached(tile) {
            changed += (after[tile] != *bricks) as usize;
        } else {
            assert!(after[tile] == *bricks, "the tile {tile:?} changed, which the touch at {at} does not reach");
            same += 1;
        }
    }
    assert!(changed >= 1 && same >= before.len() * 2 / 3, "{changed} tiles changed and {same} did not, of {}", before.len());
}

#[test]
fn the_ground_can_be_asked_its_height_its_slope_and_its_water() {
    let spec = Spec { tube: true, ..two_hollows() }.snapped();
    let lawn = crate::spec::lawn(&spec).unwrap();
    let g = &lawn.ground;
    let at = |x: i32, y: i32| g.at(x, y).unwrap();
    // its height is the sculpted ground's, wherever no stream is cut
    for (x, y) in [(-50, -30), (-25, 0), (30, 0), (50, 30), (-45, 5)] {
        let here = crate::spec::grass_ground(&spec, DVec3::new(x as f64 + 0.5, y as f64 + 0.5, 0.5));
        assert!((at(x, y).height - here).abs() < 1e-9 && at(x, y).top == (here + SOIL_OVER).round() as i32, "at {x}, {y}: {:?}, sculpted {here}", at(x, y));
    }
    // flat lawn far from everything; uphill towards the hill's middle on its flank
    assert_eq!(g.slope(52, 32).unwrap(), DVec2::ZERO);
    let flank = g.slope(-45, 0).unwrap();
    assert!(flank.x > 0.3 && flank.y.abs() < 0.1, "the hill's west flank slopes {flank}");
    // water: the lower pond at the lawn's level, the upper higher, both ponds and not streams; none on the open lawn
    let (lower, upper) = (at(30, 0), at(-25, 0));
    assert!(lower.water == Some(-3) && !lower.stream && lower.skin == BED && lower.top < -3, "{lower:?}");
    assert!(upper.water.is_some_and(|level| level >= 0) && !upper.stream, "{upper:?}");
    assert!(at(50, 30).water.is_none() && at(50, 30).skin == TRUNK && at(50, 30).covered);
    // a stream between them: on the way down there is water that is a stream's, stepping down from the upper level to the lower
    let stream: Vec<Place> = (-12..=18).filter_map(|x| (-8..=8).map(|y| at(x, y)).find(|p| p.stream && p.water.is_some())).collect();
    assert!(stream.len() >= 25, "stream water at {} of 31 places across", stream.len());
    let levels: Vec<i32> = stream.iter().map(|p| p.water.unwrap()).collect();
    assert!(levels[0] > *levels.last().unwrap() && levels.iter().all(|l| *l >= -3 && *l <= upper.water.unwrap()), "{levels:?}");
    // beyond the table there is nothing to say; a cell past the lawn's edge is there, and not covered
    let (lo, hi) = g.cells();
    assert!(g.at(lo[0] - 1, 0).is_none() && g.at(hi[0], 0).is_none() && !at(lo[0], lo[1]).covered);
    // and the ground alone, made without the bricks, is the same ground
    let alone = crate::spec::lawn_ground(&spec).unwrap();
    assert!(alone.cells() == g.cells() && (lo[0]..hi[0]).all(|x| alone.at(x, 3) == g.at(x, 3)));
}

#[test]
fn a_lawn_knows_which_part_is_where_and_how_far_off_it_is() {
    for (name, spec) in lawns() {
        let spec = spec.snapped();
        let (lawn, built) = (crate::spec::lawn(&spec).unwrap(), build(&spec).unwrap());
        assert!(lawn.bricks == built.bricks && lawn.shades == built.shades, "{name}");
        // a place in each brick's box is of that brick's part, and if the brick is a box, inside the lawn
        for (b, shade) in built.bricks.iter().zip(&built.shades).step_by(7) {
            let within = DVec3::new(b.min[0] as f64 + 0.5, b.min[1] as f64 + 0.5, b.min[2] as f64 + 0.25);
            assert_eq!(lawn.part(within), Some(*shade), "{name}: the brick {b:?}");
            assert!(b.mask != CUBE || lawn.distance(within) < 0.0, "{name}: {within} is in {b:?} and {} from the lawn", lawn.distance(within));
        }
        // well over it there is nothing, and it is that far off or less
        let (lo, hi) = lawn.bounds();
        let above = DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, hi.z + 5.0);
        assert!(lawn.part(above).is_none() && lawn.distance(above) > 0.0 && lawn.distance(above) <= 5.0 + 1e-9, "{name}");
    }
}

/// A lawn `long` by `wide` with `touches` touches of the brush spread over it, each 10 to 25 in range.
fn sized(long: f64, wide: f64, touches: usize) -> Spec {
    let mut seed = 7u64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) % 1000) as f64 / 1000.0
    };
    let spread = wide / 2.0;
    let half = long / 2.0 - spread;
    let pulls = (0..touches).map(|_| (DVec3::new((next() - 0.5) * (long - 20.0), (next() - 0.5) * (wide - 20.0), 2.0), DVec3::new(10.0 + next() * 15.0, 0.0, next() * 12.0 - 4.0))).collect();
    Spec { pulls, ..along(&[(-half, 0.0), (0.0, 0.0), (half, 0.0)], spread, false) }
}

/// A spec's lawn laid with `lawn` holding what is kept of it, and from nothing.
fn laid_from(lawn: &mut kept::Lawn, spec: &Spec, threads: Threads) -> Arc<Model> {
    let (lay, _) = crate::spec::tree_lay(spec, spec.points[0]).unwrap();
    Model::laid(lawn, &spec.tree, spec.points[0], spec.rise.unwrap(), spec.radius, &lay, spec.max_ext, threads)
}

fn fresh_of(spec: &Spec, threads: Threads) -> Arc<Model> {
    let (lay, _) = crate::spec::tree_lay(spec, spec.points[0]).unwrap();
    Model::fresh(&spec.tree, spec.points[0], spec.rise.unwrap(), spec.radius, &lay, spec.max_ext, threads)
}

/// What a touch costs on lawns of three sizes, built from nothing and from what was kept of the build before: run by name with `--release -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn where_a_touch_on_a_lawn_goes() {
    use std::time::Instant;
    for (name, long, wide, touches) in [("180 by 120", 180.0, 120.0, 24), ("300 by 240", 300.0, 240.0, 24), ("600 by 480", 600.0, 480.0, 24), ("600 by 480, 120 touches", 600.0, 480.0, 120)] {
        for levels in [1, 2] {
            let spec = Spec { tube: levels == 2, ..sized(long, wide, touches) }.snapped();
            // one more touch, 20 in range, and the same lawn without it, turn about: each build is one touch's worth of change
            let mut touched = spec.clone();
            touched.pulls.push((DVec3::new(long / 4.0 + 0.5, wide / 5.0 + 0.5, 0.5), DVec3::new(20.0, 0.0, 7.0)));
            for threads in [Threads::available(), Threads::ONE] {
                let whole = (0..5).map(|_| { let at = Instant::now(); std::hint::black_box(fresh_of(&touched, threads)); at.elapsed() }).min().unwrap();
                let ground = (0..5).map(|_| { let (lay, _) = crate::spec::tree_lay(&touched, touched.points[0]).unwrap(); let at = Instant::now(); std::hint::black_box(Ground::new(&touched.tree, touched.points[0], touched.radius, &lay, threads)); at.elapsed() }).min().unwrap();
                let mut lawn = kept::Lawn::default();
                laid_from(&mut lawn, &spec, threads);
                let all = lawn.laid;
                let mut laid = 0;
                let kept = (0..10).map(|n| { let at = Instant::now(); let built = laid_from(&mut lawn, if n % 2 == 0 { &touched } else { &spec }, threads); let took = at.elapsed(); laid = lawn.laid; std::hint::black_box(built); took }).min().unwrap();
                // and through the whole of a build, as the editor asks for it
                build_with(&spec, true, threads).unwrap();
                let built = (0..10).map(|n| { let at = Instant::now(); std::hint::black_box(build_with(if n % 2 == 0 { &touched } else { &spec }, true, threads).unwrap().bricks.len()); at.elapsed() }).min().unwrap();
                println!("{name}, {levels} level(s), {} thread(s): from nothing {whole:?} (its ground {ground:?}); a touch with the last build kept {kept:?}, laying {laid} of {all} tiles; as a whole build {built:?}", threads.count());
            }
        }
    }
}

/// A number that stands for a build's bricks and their parts, in order.
fn print_of(built: &crate::spec::Built) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut take = |v: i64| { h = (h ^ v as u64).wrapping_mul(0x100000001b3); };
    for (b, s) in built.bricks.iter().zip(&built.shades) {
        take(b.mask as i64);
        b.min.iter().chain(&b.ext).for_each(|v| take(*v as i64));
        take(*s as i64);
    }
    take(built.solid.to_bits() as i64);
    h
}

/// Every sort of lawn the tests have, for comparing one way of building with another.
fn every_lawn() -> Vec<(String, Spec)> {
    let mut all: Vec<(String, Spec)> = lawns().into_iter().map(|(name, spec)| (name.to_string(), spec)).collect();
    all.push(("two hollows".into(), two_hollows()));
    all.push(("tall".into(), Spec { rise: Some(40.0), depth: 6, ..sized(120.0, 90.0, 12) }));
    all.push(("large, two levels".into(), Spec { tube: true, depth: 8, tree: Tree { seed: 5, count: 2, ..Tree::of(Kind::Grass) }, ..sized(300.0, 240.0, 40) }));
    all.push(("closed".into(), Spec { depth: 5, pulls: vec![(DVec3::new(0.5, 0.5, 0.5), DVec3::new(22.0, 0.0, -10.0))], ..along(&[(-40.0, -30.0), (40.0, -30.0), (40.0, 30.0), (-40.0, 30.0)], 4.0, true) }));
    // (and with cliffs: a fall, and hills steep enough for soil and for rock with ponds on and among them)
    all.push(("a fall".into(), fall()));
    all.push(("steep hills".into(), Spec { tube: true, depth: 6, tree: Tree { seed: 9, baubles: 3, ..Tree::of(Kind::Grass) }, pulls: vec![touch(-28.0, 0.0, 26.0, 30.0, 0.0), touch(-28.0, 0.0, 9.0, -9.0, 0.0), touch(25.0, 2.0, 24.0, 16.0, 0.0), touch(25.0, -20.0, 14.0, -12.0, SHARP), touch(-2.0, 22.0, 12.0, 6.0, SHARP + FLATTENS)], ..along(&[(-60.0, -40.0), (60.0, -40.0), (60.0, 40.0), (-60.0, 40.0)], 4.0, true) }));
    all
}

use crate::spec::{FLATTENS, SHARP};

/// A touch of the brush at `x`, `y`, as a spec keeps it: `how` is 0 for a push, [`FLATTENS`] to flatten, and [`SHARP`] more for a sharp edge.
fn touch(x: f64, y: f64, range: f64, amount: f64, how: f64) -> (DVec3, DVec3) {
    (DVec3::new(x + 0.5, y + 0.5, 0.5), DVec3::new(range, how, amount))
}

/// A lawn 120 by 80 with a table 16 high in its west half, cut with a sharp edge so that its sides are cliffs; a hollow on the table near its east edge; and a hollow in the low ground to the east. Raised ponds are on and cliffs are at their middle setting, so the upper hollow's water goes over the cliff to the lower.
fn fall() -> Spec {
    Spec { tube: true, depth: 4, tree: Tree { seed: 3, baubles: 2, ..Tree::of(Kind::Grass) }, pulls: vec![touch(-31.0, 0.0, 30.0, 16.0, SHARP), touch(-18.0, 0.0, 9.0, -8.0, 0.0), touch(22.0, 2.0, 17.0, -9.0, 0.0)], ..along(&[(-60.0, -40.0), (60.0, -40.0), (60.0, 40.0), (-60.0, 40.0)], 4.0, true) }
}

/// The cells of a build that hold water, white water among them, and the stones that lie in it: a stone in the shallows where a stream comes into a pond may lie right across it, as one in a real stream does, and the water is no less one water for that.
fn wet_cells(built: &crate::spec::Built) -> std::collections::HashSet<[i32; 3]> {
    let mut cells = std::collections::HashSet::new();
    for (b, _) in built.bricks.iter().zip(&built.shades).filter(|(_, s)| matches!(**s, WATER | FOAM | STONE)) {
        for z in 0..b.ext[2] {
            for y in 0..b.ext[1] {
                for x in 0..b.ext[0] {
                    cells.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]);
                }
            }
        }
    }
    cells
}

/// The cells of water that can be reached from `from` through water, going from a cell to one that shares a face with it.
fn reached(wet: &std::collections::HashSet<[i32; 3]>, from: [i32; 3]) -> std::collections::HashSet<[i32; 3]> {
    let (mut seen, mut stack) = (std::collections::HashSet::from([from]), vec![from]);
    while let Some(c) = stack.pop() {
        for (axis, step) in [(0, 1), (0, -1), (1, 1), (1, -1), (2, 1), (2, -1)] {
            let mut n = c;
            n[axis] += step;
            if wet.contains(&n) && seen.insert(n) {
                stack.push(n);
            }
        }
    }
    seen
}

#[test]
fn no_grass_grows_on_a_cliff_and_past_the_second_threshold_it_is_rock() {
    for (name, spec) in every_lawn().into_iter().filter(|(name, _)| name == "a fall" || name == "steep hills") {
        // (with no reeds, which are green too and may hang a leaf over a rock by the water)
        let spec = Spec { depth: 1, ..spec }.snapped();
        let (lawn, built) = (crate::spec::lawn(&spec).unwrap(), build(&spec).unwrap());
        let g = &lawn.ground;
        let (soil, rock) = STEEP[spec.tree.baubles as usize - 1];
        let (lo, hi) = g.cells();
        // how steep the sculpted ground is at a cell, by the cells to each side, as the lawn has it before its edge is made to wander (by up to 15 parts in 100 either way)
        let steep = |x: i32, y: i32| {
            let at = |x: i32, y: i32| crate::spec::grass_ground(&spec, DVec3::new(x.clamp(lo[0], hi[0] - 1) as f64 + 0.5, y.clamp(lo[1], hi[1] - 1) as f64 + 0.5, 0.5));
            DVec2::new(at(x + 1, y) - at(x - 1, y), at(x, y + 1) - at(x, y - 1)).length() / 2.0
        };
        // the blades standing on each cell: the bricks of the greens whose foot is on the top of its soil (and not those that lean over it from a cell beside it)
        let mut blades: HashMap<[i32; 2], usize> = HashMap::new();
        for (b, _) in built.bricks.iter().zip(&built.shades).filter(|(b, s)| matches!(**s, LEAF | LEAF_DARK | LEAF_LIGHT) && g.at(b.min[0], b.min[1]).is_some_and(|place| place.top == b.min[2])) {
            *blades.entry([b.min[0], b.min[1]]).or_default() += 1;
        }
        // (and any green at all over a cell, for rock, which has none)
        let over: std::collections::HashSet<[i32; 2]> = built.bricks.iter().zip(&built.shades).filter(|(_, s)| matches!(**s, LEAF | LEAF_DARK | LEAF_LIGHT)).map(|(b, _)| [b.min[0], b.min[1]]).collect();
        // what the column of soil under each cell is made of, and what is on top of it
        let mut columns: HashMap<[i32; 2], Vec<(i32, u8)>> = HashMap::new();
        for (b, s) in built.bricks.iter().zip(&built.shades).filter(|(b, s)| b.mask == CUBE && matches!(**s, TRUNK | CLIFF | ROCK | ROCK_DARK | ROCK_LIGHT | LEDGE | SAND | BED)) {
            for y in b.min[1]..b.min[1] + b.ext[1] {
                for x in b.min[0]..b.min[0] + b.ext[0] {
                    columns.entry([x, y]).or_default().push((b.min[2] + b.ext[2], *s));
                }
            }
        }
        let (mut grassy, mut bare, mut rocky, mut on_grass, mut on_bare) = (0, 0, 0, 0, 0);
        for y in lo[1]..hi[1] {
            for x in lo[0]..hi[0] {
                let place = g.at(x, y).unwrap();
                // (dry ground the lawn covers that is soil and not sand, and has no fall beside it: there the cliff is the ground's steepness and nothing else)
                if !place.covered || place.skin != TRUNK || place.water.is_some() || place.stream {
                    continue;
                }
                let s = steep(x, y);
                // well past each threshold it is that, and well short of the first it is neither
                if s * 0.85 >= rock {
                    assert_eq!(place.cliff, 2, "{name}: the ground at {x}, {y} rises {s:.2} for each cell along, and is not rock");
                } else if s * 0.85 >= soil {
                    assert!(place.cliff >= 1, "{name}: the ground at {x}, {y} rises {s:.2} for each cell along, and is no cliff");
                } else if s * 1.15 < soil {
                    assert_eq!(place.cliff, 0, "{name}: the ground at {x}, {y} rises {s:.2} for each cell along, and is a cliff");
                }
                let here = blades.get(&[x, y]).copied().unwrap_or(0);
                // (up to the top of the soil: a reed's head over the cell is no part of its ground)
                let column: Vec<(i32, u8)> = columns.get(&[x, y]).unwrap_or_else(|| panic!("{name}: no soil at {x}, {y}")).iter().copied().filter(|c| c.0 <= place.top).collect();
                let top = column.iter().max().unwrap();
                match place.cliff {
                    2 => {
                        rocky += 1;
                        // rock all through, a ledge on top, and no grass standing on it
                        assert!(top.1 == LEDGE && column.iter().all(|c| matches!(c.1, ROCK | ROCK_DARK | ROCK_LIGHT | LEDGE)), "{name}: the rock at {x}, {y} is {column:?}");
                        assert!(!over.contains(&[x, y]), "{name}: grass on the rock at {x}, {y}, or over it");
                    }
                    1 => {
                        bare += 1;
                        on_bare += here;
                        assert!(column.iter().all(|c| matches!(c.1, TRUNK | CLIFF)), "{name}: the cliff of soil at {x}, {y} is {column:?}");
                    }
                    _ => {
                        // (ground with all its grass: short of where it begins to thin)
                        if s * 1.15 < 0.75 * soil {
                            grassy += 1;
                            on_grass += here;
                        }
                        assert!(column.iter().all(|c| c.1 == TRUNK), "{name}: the soil at {x}, {y} is {column:?}");
                    }
                }
            }
        }
        // both kinds of cliff are there, in both tones and all the greys; and a cliff of soil has the odd tuft: a quarter of the blades level ground has, or fewer (one tuft in fourteen is kept, and the cells are a hundred or two, so the share is rough)
        assert!(rocky > 60 && bare > 60 && grassy > 2000, "{name}: {rocky} cells of rock, {bare} of bare soil, {grassy} of grass");
        assert!(on_bare * grassy * 4 <= on_grass * bare, "{name}: {on_bare} blades on {bare} cells of cliff, {on_grass} on {grassy} of grass");
        let count = |shades: &[u8]| built.shades.iter().filter(|s| shades.contains(s)).count();
        for shade in [CLIFF, ROCK, LEDGE] {
            assert!(count(&[shade]) > 3, "{name}: no bricks of part {shade}");
        }
        // (the rock in more greys than one: which beds are darker and which paler goes by the seed, and a low face may have none of one)
        assert!(count(&[ROCK_DARK, ROCK_LIGHT]) > 3, "{name}: the rock is all one grey");
        // rock is in large bricks: beds joined along the face, some cells in each
        let rock_bricks: Vec<&CellBrick> = built.bricks.iter().zip(&built.shades).filter(|(_, s)| matches!(**s, ROCK | ROCK_DARK | ROCK_LIGHT)).map(|(b, _)| b).collect();
        let cells: i32 = rock_bricks.iter().map(|b| b.ext[0] * b.ext[1] * b.ext[2]).sum();
        assert!(cells >= rock_bricks.len() as i32 * 3, "{name}: {cells} cells of rock in {} bricks", rock_bricks.len());
        // and with cliffs at none there is none of it, on the same ground
        let plain = build(&Spec { tree: Tree { baubles: 0, ..spec.tree }, ..spec.clone() }).unwrap();
        assert!(!plain.shades.iter().any(|s| matches!(*s, CLIFF | ROCK | ROCK_DARK | ROCK_LIGHT | LEDGE)) || name == "a fall", "{name}: cliffs at none, and there is rock");
    }
}

#[test]
fn a_brush_with_a_sharp_edge_makes_ground_steep_enough_for_rock_and_the_easing_brush_does_not_at_the_same_height() {
    let at = |touches: &[Sculpt], x: f64| ground(touches, DVec3::new(x, 0.0, 0.0));
    let steepest = |touch: Sculpt| (0..400).map(|n| (at(&[touch], n as f64 * 0.1) - at(&[touch], n as f64 * 0.1 + 0.1)).abs() / 0.1).fold(0.0, f64::max);
    let (soil, rock) = STEEP[1];
    let push = Sculpt { at: DVec3::ZERO, range: 30.0, amount: 12.0, flatten: false, sharp: false };
    // a hill 12 high from a brush 30 in range is 32 degrees at its steepest: grass all over
    assert!(steepest(push) < 0.75 * soil, "{}", steepest(push));
    // the same push with a sharp edge: level on top out to four fifths of its range, and past the angle for rock in the band beyond
    let table = Sculpt { sharp: true, ..push };
    assert!((at(&[table], 0.0) - 12.0).abs() < 1e-9 && (at(&[table], 23.9) - 12.0).abs() < 1e-9 && at(&[table], 30.0) == 0.0 && steepest(table) > rock, "{}", steepest(table));
    // a pit, and a terrace cut into a hill, likewise
    assert!(steepest(Sculpt { amount: -12.0, ..table }) > rock && (at(&[Sculpt { amount: -12.0, ..table }], 5.0) + 12.0).abs() < 1e-9);
    let hill = Sculpt { range: 60.0, amount: 30.0, ..push };
    let terrace = Sculpt { at: DVec3::new(20.0, 0.0, 0.0), range: 15.0, amount: 4.0, flatten: true, sharp: true };
    assert!((at(&[hill, terrace], 20.0) - 4.0).abs() < 1e-9 && (at(&[hill, terrace], 31.0) - 4.0).abs() < 1e-9 && at(&[hill, terrace], 36.0) == at(&[hill], 36.0));
    // and as a form keeps a touch: 0 and 16 are what they were, 32 and 48 the same with a sharp edge
    let spec = Spec { pulls: vec![touch(0.0, 0.0, 30.0, 12.0, 0.0), touch(0.0, 0.0, 30.0, 3.0, FLATTENS), touch(0.0, 0.0, 30.0, 12.0, SHARP), touch(0.0, 0.0, 30.0, 3.0, SHARP + FLATTENS)], ..along(&[(-40.0, 0.0), (40.0, 0.0)], 40.0, false) };
    let (lay, _) = crate::spec::tree_lay(&spec, spec.points[0]).unwrap();
    assert_eq!(lay.mounds.iter().map(|t| (t.flatten, t.sharp)).collect::<Vec<_>>(), [(false, false), (true, false), (false, true), (true, true)]);
}

#[test]
fn a_pond_above_a_cliff_above_a_pond_is_joined_by_falling_water_and_on_a_gentle_slope_by_a_stream() {
    let spec = fall().snapped();
    let (lawn, built) = (crate::spec::lawn(&spec).unwrap(), build(&spec).unwrap());
    let g = &lawn.ground;
    let wet = wet_cells(&built);
    // the upper pond on the table and the lower in the low ground, each level, the upper well above the lower
    let (upper, lower) = (g.at(-18, 0).unwrap().water.expect("no water in the hollow on the table"), g.at(22, 2).unwrap().water.expect("no water in the low hollow"));
    assert!(upper >= lower + 10 && upper - g.at(-18, 0).unwrap().top >= 3, "the upper pond stands at {upper} and the lower at {lower}");
    // one piece of water from the one to the other, cell to cell through faces: the stream out through the lip, the fall down the face, the stream to the pool
    let from_upper = reached(&wet, [-18, 0, upper - 1]);
    // (most of the lower pond's top layer of water: a stone or a reed may stand in a cell of it)
    let pool = |cells: &std::collections::HashSet<[i32; 3]>| cells.iter().filter(|c| c[2] == lower - 1 && c[0] > 12).count();
    assert!(pool(&wet) > 100 && pool(&from_upper) == pool(&wet), "the water of the upper pond does not reach the lower: {} of its {} cells, and down to {:?}", pool(&from_upper), pool(&wet), from_upper.iter().map(|c| c[2]).min());
    // the fall: somewhere on the way, water standing in one cell's column over most of the drop, where the ground says water comes down
    let falls: Vec<(i32, i32, Place)> = (-8..=20).flat_map(|x| (-8..=8).map(move |y| (x, y))).filter_map(|(x, y)| g.at(x, y).filter(|p| p.falls_from.is_some()).map(|p| (x, y, p))).collect();
    let tallest = falls.iter().map(|(_, _, p)| p.falls_from.unwrap() - p.water.unwrap()).max().unwrap_or(0);
    assert!(tallest >= 3, "the water comes down {tallest} cells at the most in one place");
    let (x, y, place) = falls.iter().max_by_key(|(_, _, p)| p.falls_from.unwrap() - p.water.unwrap()).unwrap();
    assert!((place.top..place.falls_from.unwrap()).all(|z| wet.contains(&[*x, *y, z])), "the fall at {x}, {y} is not water from {} up to {}", place.top, place.falls_from.unwrap());
    // a notch in the lip: the stream's water where the table's edge was, well below the table's top
    assert!((-9..=-5).all(|x| (-4..=4).any(|y| g.at(x, y).is_some_and(|p| p.stream && p.water == Some(upper) && p.top < 16))), "no channel through the lip of the table");
    // rock behind and beside it, and white water where it lands, lying in the water's top cell
    assert!(falls.iter().filter(|(_, _, p)| p.falls_from.unwrap() - p.water.unwrap() >= 2).all(|(_, _, p)| p.cliff == 2), "a fall with no rock under it");
    assert!(falls.iter().all(|(x, y, _)| [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dy)| g.at(x + dx, y + dy).is_some_and(|beside| beside.water.is_some() || beside.cliff == 2 || beside.skin == SAND))), "grass or soil beside the fall");
    let white: Vec<DVec3> = bricks_of(&built, &[FOAM]);
    assert!(white.len() >= 2 && white.iter().all(|p| p.z < (lower + 4) as f64 && p.x > -6.0), "white water at {white:?}");
    // The same two ponds on a gentle hill (27 degrees at its steepest): a stream, with no fall in it and no white water. It is one piece of water too.
    let gentle = Spec { pulls: vec![touch(-25.0, 0.0, 44.0, 14.0, 0.0), touch(-25.0, 0.0, 14.0, -10.0, 0.0), touch(32.0, 2.0, 17.0, -9.0, 0.0)], ..spec.clone() }.snapped();
    let (lawn, built) = (crate::spec::lawn(&gentle).unwrap(), build(&gentle).unwrap());
    let g = &lawn.ground;
    let (lo, hi) = g.cells();
    let (upper, lower) = (g.at(-25, 0).unwrap().water.expect("no water in the hollow on the hill"), g.at(32, 2).unwrap().water.expect("no water in the low hollow"));
    assert!(upper > lower + 3);
    let (wet, pool) = (wet_cells(&built), |cells: &std::collections::HashSet<[i32; 3]>| cells.iter().filter(|c| c[2] == lower - 1 && c[0] > 22).count());
    assert!(pool(&wet) > 100 && pool(&reached(&wet, [-25, 0, upper - 1])) == pool(&wet), "the stream is broken");
    let drops: Vec<i32> = (lo[0]..hi[0]).flat_map(|x| (lo[1]..hi[1]).map(move |y| (x, y))).filter_map(|(x, y)| g.at(x, y)).filter_map(|p| Some(p.falls_from? - p.water?)).collect();
    assert!(!drops.is_empty() && drops.iter().all(|d| *d < 2), "on a gentle slope the water comes down {:?} cells at a step", drops.iter().max());
    assert!(bricks_of(&built, &[FOAM]).is_empty() && (lo[0]..hi[0]).all(|x| (lo[1]..hi[1]).all(|y| g.at(x, y).unwrap().cliff == 0)));
    // and with raised ponds off, on the ground with the cliff: no water up on the table, and none coming down
    let off = build(&Spec { tube: false, ..spec.clone() }).unwrap();
    assert!(wet_cells(&off).iter().all(|c| c[2] < 0) && bricks_of(&off, &[FOAM]).is_empty());
}

#[test]
fn two_raised_ponds_at_different_heights_hold_water_at_different_heights() {
    // two hills, one twice as high as the other, each with a hollow in its top, and a hollow in the low ground between them
    let lawn = Spec { tube: true, depth: 1, tree: Tree { seed: 4, ..Tree::of(Kind::Grass) }, pulls: vec![touch(-38.0, 0.0, 30.0, 26.0, 0.0), touch(-38.0, 0.0, 12.0, -18.0, 0.0), touch(38.0, 0.0, 30.0, 13.0, 0.0), touch(38.0, 0.0, 12.0, -15.0, 0.0), touch(0.0, 22.0, 14.0, -8.0, 0.0)], ..along(&[(-75.0, -40.0), (75.0, -40.0), (75.0, 40.0), (-75.0, 40.0)], 4.0, true) };
    let built = build(&lawn).unwrap();
    let level = |tops: &std::collections::BTreeSet<i32>| *tops.iter().next_back().unwrap_or_else(|| panic!("a hollow is dry"));
    // (the water over the middle of each hollow)
    let top_at = |x: i32, y: i32| level(&water_tops(&Spec::snapped(&lawn), &built, x, y));
    let (high, low, ground) = (top_at(-38, 0), top_at(38, 0), top_at(0, 22));
    // each as far below its own rim as the lowest is below the lawn: the high one about 13 above the low one, as its hill is
    assert!(ground == -3 && low >= 4 && high >= low + 8, "the ponds stand at {high}, {low} and {ground}");
    // and each is deep: water for several cells down, where at the lowest raised pond's level the higher hollow held none
    let g = crate::spec::lawn_ground(&lawn).unwrap();
    for (x, at) in [(-38, high), (38, low)] {
        let place = g.at(x, 0).unwrap();
        assert!(place.water == Some(at) && at - place.top >= 3, "the pond at {x} is {} deep", at - place.top);
    }
}


#[test]
#[ignore = "prints, checks nothing"]
fn prints_of_every_lawn() {
    for (name, spec) in every_lawn() {
        println!("PRINT {name}: {:#x}", print_of(&build(&spec).unwrap()));
    }
}

/// Whether two grounds are the same in every number of every cell.
fn same_ground(a: &Ground, b: &Ground) -> bool {
    let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<u64>>();
    (a.lo, a.n, a.watered) == (b.lo, b.n, b.watered) && bits(&a.high) == bits(&b.high) && a.top == b.top && a.level == b.level && a.skin == b.skin && a.flags == b.flags && a.stone.iter().map(|x| x.to_bits()).eq(b.stone.iter().map(|x| x.to_bits()))
}

/// A lawn's touches as they might come and go: one at a time (or, of many, a few at a time after the first six), then one taken out of the middle, one changed where it stands, one moved, two made in the other order, a new one put in among the others, all gone, and all back.
fn comings_and_goings(all: &[(DVec3, DVec3)]) -> Vec<Vec<(DVec3, DVec3)>> {
    let mut steps: Vec<Vec<(DVec3, DVec3)>> = vec![Vec::new()];
    let mut n = 0;
    while n < all.len() {
        n = (n + if n < 6 { 1 } else { 7 }).min(all.len());
        steps.push(all[..n].to_vec());
    }
    if all.len() >= 2 {
        let middle = all.len() / 2;
        let mut less = all.to_vec();
        less.remove(middle);
        steps.push(less.clone());
        let mut changed = all.to_vec();
        changed[middle].1.z -= 5.0;
        steps.push(changed.clone());
        changed[middle].1.x += 6.0;
        steps.push(changed.clone());
        changed[0].0 += DVec3::new(9.0, -4.0, 0.0);
        steps.push(changed.clone());
        changed.swap(0, middle);
        steps.push(changed.clone());
        // (flattened, among the others)
        changed.insert(1, (all[0].0 + DVec3::new(3.0, 2.0, 0.0), DVec3::new(14.0, 1.0, 2.0)));
        steps.push(changed);
        steps.push(less);
    }
    steps.push(Vec::new());
    steps.push(all.to_vec());
    steps
}

#[test]
fn a_lawn_built_touch_by_touch_from_what_was_kept_is_the_lawn_built_whole() {
    for (name, spec) in every_lawn() {
        let spec = spec.snapped();
        let mut lawn = kept::Lawn::default();
        let mut reused = 0;
        let step = |lawn: &mut kept::Lawn, now: &Spec, what: &str, k: usize| {
            let threads = [Threads::ONE, Threads::forced(3), Threads::available()][k % 3];
            let (from_kept, whole) = (laid_from(lawn, now, threads), fresh_of(now, Threads::ONE));
            assert!(from_kept.bricks == whole.bricks, "{name}, {what} {k}: other bricks from what was kept ({} and {})", from_kept.bricks.len(), whole.bricks.len());
            assert!(from_kept.shades == whole.shades && from_kept.volume.to_bits() == whole.volume.to_bits() && from_kept.bounds() == whole.bounds(), "{name}, {what} {k}: other parts, volume or box");
            assert!(same_ground(&from_kept.ground, &whole.ground), "{name}, {what} {k}: another ground");
            lawn.laid
        };
        let count = (laid_from(&mut kept::Lawn::default(), &spec, Threads::ONE).ground.n[0] / TILE as usize + 2) * (fresh_of(&spec, Threads::ONE).ground.n[1] / TILE as usize + 2);
        let steps = comings_and_goings(&spec.pulls).len();
        for (k, pulls) in comings_and_goings(&spec.pulls).into_iter().enumerate() {
            reused += (step(&mut lawn, &Spec { pulls, ..spec.clone() }, "touches", k) * 2 < count) as usize;
        }
        // its settings: thicker, leaning, taller, more reeds and none, shorter bricks, and back to what it was; a touch after each
        let settings = [
            Spec { tree: Tree { fullness: 90, ..spec.tree }, ..spec.clone() },
            Spec { tree: Tree { bend: 80, ..spec.tree }, ..spec.clone() },
            Spec { rise: Some(spec.rise.unwrap() + 3.0), ..spec.clone() },
            Spec { depth: 11, ..spec.clone() },
            Spec { depth: 1, ..spec.clone() },
            Spec { max_ext: 6, ..spec.clone() },
            spec.clone(),
        ];
        for (k, now) in settings.iter().enumerate() {
            step(&mut lawn, now, "settings", k);
            let mut touched = now.clone();
            touched.pulls.push((spec.points[0] + DVec3::new(7.0, 3.0, 0.0), DVec3::new(9.0, 0.0, -6.0)));
            step(&mut lawn, &touched, "a touch after settings", k);
        }
        // (and what was kept was used: most steps laid fewer than half the tiles)
        // (but for the tall grass, 40 high on a lawn 120 by 90: a blade there reaches 35 cells, so a tile is laid from most of the lawn and a touch anywhere reaches most tiles)
        assert!(reused * 3 >= steps || name == "tall", "{name}: {reused} steps of {steps} laid fewer than half of {count} tiles");
    }
}

#[test]
fn a_touch_lays_the_tiles_it_reaches_and_works_out_the_ground_it_reaches() {
    for levels in [1, 2] {
        let spec = Spec { tube: levels == 2, depth: 6, ..sized(300.0, 240.0, 24) }.snapped();
        let mut lawn = kept::Lawn::default();
        let first = laid_from(&mut lawn, &spec, Threads::available());
        let all = lawn.laid;
        assert!(all >= 80 && all == lawn.tiles.len(), "{all} tiles laid of {}", lawn.tiles.len());
        // the same again: the lawn that was built, and nothing laid
        let again = laid_from(&mut lawn, &spec, Threads::available());
        assert!(Arc::ptr_eq(&first, &again) && lawn.laid == 0);
        drop((first, again));
        // a touch 14 in range, on dry ground: the tiles within its range and a blade's reach of it, nine at the most
        let mut touched = spec.clone();
        touched.pulls.push((DVec3::new(-100.5, 60.5, 0.5), DVec3::new(14.0, 0.0, 6.0)));
        laid_from(&mut lawn, &touched, Threads::available());
        assert!((1..=9).contains(&lawn.laid), "{levels} level(s): a touch laid {} of {all} tiles", lawn.laid);
        // taken away again: the same few
        laid_from(&mut lawn, &spec, Threads::available());
        assert!((1..=9).contains(&lawn.laid), "{levels} level(s): undoing a touch laid {} of {all} tiles", lawn.laid);
        // thicker grass: every tile that has grass, from the ground as it was
        laid_from(&mut lawn, &Spec { tree: Tree { fullness: 80, ..spec.tree }, ..spec.clone() }, Threads::available());
        assert_eq!(lawn.laid, all);
    }
}

/// A tile is kept under the reeds over it as well as its ground: a reed
/// may come to fill other heights of the same cells, by a change to a
/// reed that it gives way to, far from the tile. That is rare enough that
/// no lawn here does it, so the reeds kept with a tile are changed by
/// hand, as if they had been other reeds, and the tile must be laid again.
#[test]
fn a_tile_kept_is_laid_again_when_the_reeds_over_it_are_others() {
    let spec = Spec { depth: 11, ..two_hollows() }.snapped();
    let mut lawn = kept::Lawn::default();
    laid_from(&mut lawn, &spec, Threads::ONE);
    let (at, were) = lawn.tiles.iter().find(|(_, tile)| !tile.stems.is_empty()).map(|(at, tile)| (*at, tile.stems.clone())).expect("no tile has a reed over it");
    lawn.tiles.get_mut(&at).unwrap().stems[0].1[0].1 += 1;
    // (a touch far from that tile, so that the lawn is built again and that tile's ground is as it was)
    let far = DVec3::new(if at[0] < 0 { 50.5 } else { -50.5 }, if at[1] < 0 { 30.5 } else { -30.5 }, 0.5);
    let mut touched = spec.clone();
    touched.pulls.push((far, DVec3::new(5.0, 0.0, 1.0)));
    let built = laid_from(&mut lawn, &touched, Threads::ONE);
    assert!(lawn.tiles[&at].stems == were, "the tile at {at:?} was kept, though it was laid from other reeds");
    assert!(built.bricks == fresh_of(&touched, Threads::ONE).bricks);
}

/// Every lawn the tests have with cliffs at none and its water at one
/// level is the bricks it was before there were cliffs, raised ponds at
/// heights of their own or anything kept between builds: the numbers are
/// those of the lawns as `main` built them at 63b2fd6. Two of them have
/// ground steep enough for a cliff ("rough" rises 1.2 for each cell
/// along, and "tall" 1.1), and one a hollow on a hill that would be a
/// raised pond ("two hollows"). The lawns with raised ponds are not
/// here: their streams are narrower than they were and each pond has its
/// own level.
#[test]
fn the_lawns_are_the_bricks_they_were_with_cliffs_at_none_and_no_raised_ponds() {
    let were = [("sculpted", 0xd5a905604c89faedu64), ("rough", 0xd7e6a86f7bd9ad03), ("two hollows", 0x3832e3a0a7b80296), ("tall", 0x50ed6b8d59974b81), ("closed", 0x4e4f45365858131e)];
    let lawns = every_lawn();
    for (name, was) in were {
        let spec = &lawns.iter().find(|lawn| lawn.0 == name).unwrap().1;
        assert!(spec.tree.baubles == 0 && !spec.tube, "{name}");
        assert_eq!(print_of(&build(spec).unwrap()), was, "{name}");
    }
}

#[test]
fn what_is_kept_of_lawns_is_bounded_and_safe_to_build_from_on_several_threads_at_once() {
    // more lawns than are kept, each built, touched and built again, on six threads at once, two of them the same lawn: each build is the lawn built whole
    let lawns: Vec<Spec> = (0..6).map(|n| Spec { tube: n % 2 == 0, depth: 4, tree: Tree { seed: 40 + (n as u64).min(4), ..Tree::of(Kind::Grass) }, ..sized(150.0, 100.0, 10) }.snapped()).collect();
    std::thread::scope(|scope| {
        for spec in &lawns {
            scope.spawn(move || {
                for (k, pulls) in comings_and_goings(&spec.pulls).into_iter().enumerate() {
                    let now = Spec { pulls, ..spec.clone() };
                    let (built, whole) = (build(&now).unwrap(), fresh_of(&now, Threads::ONE));
                    assert!(built.bricks == whole.bricks && built.shades == whole.shades && built.solid.to_bits() == whole.volume.to_bits(), "step {k} of the lawn with seed {}", spec.tree.seed);
                    let (lawns, bytes) = kept::held();
                    assert!(lawns <= kept::MOST_LAWNS && bytes <= kept::MOST_BYTES, "{lawns} lawns kept, {bytes} bytes");
                }
            });
        }
    });
    // (something is kept, and a lawn this size is a few million bytes of it)
    let (count, bytes) = kept::held();
    assert!(count >= 1 && bytes > 100_000, "{count} lawns kept, {bytes} bytes");
}

/// Allowing for a machine busy with other things: a touch from what was kept is some tenth of a build from nothing, and is asked to be under half.
#[test]
fn a_touch_on_a_large_lawn_is_quicker_from_what_was_kept() {
    use std::time::Instant;
    let spec = sized(300.0, 240.0, 24).snapped();
    let mut touched = spec.clone();
    touched.pulls.push((DVec3::new(60.5, -40.5, 0.5), DVec3::new(18.0, 0.0, 7.0)));
    let whole = (0..4).map(|_| { let at = Instant::now(); std::hint::black_box(fresh_of(&touched, Threads::ONE)); at.elapsed() }).min().unwrap();
    let mut lawn = kept::Lawn::default();
    laid_from(&mut lawn, &spec, Threads::ONE);
    let kept = (0..8).map(|n| { let at = Instant::now(); std::hint::black_box(laid_from(&mut lawn, if n % 2 == 0 { &touched } else { &spec }, Threads::ONE)); at.elapsed() }).min().unwrap();
    assert!(kept * 2 < whole, "a touch took {kept:?} from what was kept and {whole:?} from nothing");
}
