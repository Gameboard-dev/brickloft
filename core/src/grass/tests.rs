use super::*;
use crate::spec::{build, build_with, Form, Spec};
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

/// A lawn 180 by 120 with 24 touches, timed: run by name with `--release -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn how_long_a_lawn_takes() {
    for (name, spec) in [("24 touches", sculpted()), ("two levels and a stream", Spec { tube: true, ..two_hollows() })] {
        for threads in [Threads::ONE, Threads::available()] {
            let least = (0..5).map(|_| { let at = std::time::Instant::now(); let built = build_with(&spec, true, threads).unwrap(); (at.elapsed(), built.bricks.len()) }).min().unwrap();
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
    let hill = Sculpt { at: DVec3::ZERO, range: 30.0, amount: 10.0, flatten: false };
    let at = |touches: &[Sculpt], x: f64| ground(touches, DVec3::new(x, 0.0, 0.0));
    assert!((at(&[hill], 0.0) - 10.0).abs() < 1e-9 && (at(&[hill], 15.0) - 5.0).abs() < 1e-9 && at(&[hill], 30.0) == 0.0 && at(&[hill], 29.0) < 0.1);
    // level at its top and at its edge: no crease where it meets the flat
    assert!(at(&[hill], 0.0) - at(&[hill], 1.0) < 0.05 && at(&[hill], 29.0) - at(&[hill], 30.0) < 0.05);
    // two pushes add, and a push down takes a hollow out of raised ground and no more
    let beside = Sculpt { at: DVec3::new(20.0, 0.0, 0.0), ..hill };
    assert!(at(&[hill, beside], 10.0) > at(&[hill], 10.0) + 4.0);
    let hollow = Sculpt { at: DVec3::ZERO, range: 10.0, amount: -4.0, flatten: false };
    assert!((at(&[hill, hollow], 0.0) - 6.0).abs() < 1e-9 && at(&[hollow], 0.0) == -4.0);
    // flattening brings the ground to its level within half its range, and leaves it alone beyond it
    let terrace = Sculpt { at: DVec3::new(12.0, 0.0, 0.0), range: 12.0, amount: 3.0, flatten: true };
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

/// The heights water's surface stands at over the places from `from` to `to` along x.
fn water_tops(built: &crate::spec::Built, from: f64, to: f64) -> std::collections::BTreeSet<i32> {
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
    assert_eq!(level(&water_tops(&flat, -100.0, 100.0)), -3);
    assert!(water_tops(&flat, -100.0, 5.0).is_empty());
    // two: the hollow on the hill holds water too, higher, and each pond is level
    let (upper, lower) = (water_tops(&stepped, -30.0, -20.0), water_tops(&stepped, 26.0, 34.0));
    assert_eq!(level(&lower), -3);
    let high = level(&upper);
    assert!(high >= 0, "the upper pond stands at {high}");
    // the stream: water all the way from one to the other, never rising on the way down, never over its banks
    let along: Vec<i32> = (-18..=22).map(|x| water_tops(&stepped, x as f64 + 0.25, x as f64 + 0.75).into_iter().max().unwrap_or_else(|| panic!("the stream is dry at {x}"))).collect();
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
    assert!(!water_tops(&alone, -30.0, -20.0).is_empty() && water_tops(&alone, -8.0, 100.0).is_empty(), "{:?}", water_tops(&alone, -8.0, 100.0));
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
        assert!(one.shades.iter().all(|s| [TRUNK, SAND, BED, STONE, WATER, LEAF, LEAF_DARK, LEAF_LIGHT].contains(s)), "{name}");
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
