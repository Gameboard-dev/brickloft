use super::walls::{Walls, FLOOR, MOST_HEDGE, MOST_WALL, PLAIN_SOLUTION, SOLUTION, WALL};
use super::*;
use crate::par::Threads;
use crate::pattern::Mosaic;
use crate::spec::{build, build_with, Form, Spec};
use glam::DVec3;
use std::collections::HashSet;

fn maze(kind: Kind, rings: i32, seed: u64) -> Maze {
    Maze { kind, rings, seed, ..Maze::default() }
}

fn laid(maze: &Maze, radius: f64, thick: f64) -> Layout {
    Layout::new(maze, DVec2::splat(0.5), radius, thick).unwrap_or_else(|e| panic!("{maze:?}: {e}"))
}

fn spec(maze: Maze, radius: f64) -> Spec {
    Spec { form: Form::Maze, points: vec![DVec3::splat(0.5)], radius, thick: 3.0, rise: Some(8.0), maze, ..Spec::default() }
}

/// Every maze the tests look at: both kinds, several sizes and seeds, and every rule for how the cells of a theta maze grow.
fn every() -> Vec<(Maze, f64)> {
    let mut all = Vec::new();
    for seed in [1, 2, 3, 77, 123_456_789] {
        for rings in [2, 3, 6, 12] {
            all.push((maze(Kind::Grid, rings, seed), rings as f64 * 14.0));
            for grow in [0, 1, 2, 3] {
                for divisions in [1, 2, 4, 7] {
                    all.push((Maze { grow, divisions, entrance: (seed % 360) as i32, ..maze(Kind::Theta, rings, seed) }, rings as f64 * 14.0));
                }
            }
        }
    }
    all
}

/// The cells that can be reached from `from` through the doors, and how many doors away each is.
fn reached(layout: &Layout, from: usize) -> Vec<Option<usize>> {
    let mut beside = vec![Vec::new(); layout.cells()];
    for (a, b) in layout.doors() {
        beside[a].push(b);
        beside[b].push(a);
    }
    let mut far = vec![None; layout.cells()];
    far[from] = Some(0);
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(at) = queue.pop_front() {
        for &next in &beside[at] {
            if far[next].is_none() {
                far[next] = Some(far[at].unwrap() + 1);
                queue.push_back(next);
            }
        }
    }
    far
}

#[test]
fn every_cell_is_reached_and_by_one_way_only() {
    for (maze, radius) in every() {
        let layout = laid(&maze, radius, 3.0);
        let far = reached(&layout, layout.solution()[0]);
        assert!(far.iter().all(|f| f.is_some()), "{maze:?}: a cell cannot be reached");
        // Connected, with one door fewer than cells: a tree, so between
        // any two cells there is one way and no more. (One door more
        // would close a loop, and then there would be two.)
        assert_eq!(layout.doors().len(), layout.cells() - 1, "{maze:?}: the doors are not a tree");
        // the way through is that one way: each step a door, no cell twice, from the entrance to the room
        let way = layout.solution();
        let doors: HashSet<(usize, usize)> = layout.doors().into_iter().flat_map(|(a, b)| [(a, b), (b, a)]).collect();
        assert!(way.windows(2).all(|pair| doors.contains(&(pair[0], pair[1]))), "{maze:?}: the way through goes through a wall");
        assert_eq!(way.iter().collect::<HashSet<_>>().len(), way.len(), "{maze:?}: the way through crosses itself");
        assert_eq!(far[layout.goal()], Some(way.len() - 1), "{maze:?}");
        // the room has one door: it is the end of the way and not on it
        assert_eq!(layout.doors().iter().filter(|(a, b)| *a == layout.goal() || *b == layout.goal()).count(), 1, "{maze:?}: the room has not one door");
    }
}

/// A spanning tree of the same cells grown the other way that was
/// weighed: from the entrance, opening each time a wall chosen at random
/// among all those between the tree and a cell not yet in it (Prim's
/// method with random weights). Returns how many of the cells are dead
/// ends, and how many cells the way from the entrance to the room is.
fn grown_from_anywhere(layout: &Layout, seed: u64) -> (usize, usize) {
    let between: Vec<(usize, usize)> = layout.between().into_iter().flatten().collect();
    let (cells, start) = (layout.cells(), layout.solution()[0]);
    let mut numbers = Numbers(seed);
    let mut inside = vec![false; cells];
    inside[start] = true;
    let mut doors = vec![0usize; cells];
    let mut from = vec![usize::MAX; cells];
    for _ in 1..cells {
        let edge: Vec<&(usize, usize)> = between.iter().filter(|(a, b)| inside[*a] != inside[*b]).collect();
        let (a, b) = *edge[(numbers.unit() * edge.len() as f64) as usize];
        let (old, new) = if inside[a] { (a, b) } else { (b, a) };
        (inside[new], from[new]) = (true, old);
        doors[a] += 1;
        doors[b] += 1;
    }
    let mut long = 1;
    let mut at = layout.goal();
    while from[at] != usize::MAX {
        (at, long) = (from[at], long + 1);
    }
    (doors.iter().filter(|d| **d == 1).count(), long)
}

#[test]
fn corridors_are_long_and_dead_ends_few() {
    // Over several mazes of each kind: the share of cells that are dead
    // ends, and how long the way through is, for the walk used and for a
    // tree grown from anywhere on its edge.
    for kind in [Kind::Theta, Kind::Grid] {
        let (mut ends, mut cells, mut way, mut other_ends, mut other_way) = (0, 0, 0, 0, 0);
        for seed in 1..=12 {
            let layout = laid(&maze(kind, 9, seed), 180.0, 3.0);
            let mut doors = vec![0usize; layout.cells()];
            for (a, b) in layout.doors() {
                doors[a] += 1;
                doors[b] += 1;
            }
            ends += doors.iter().filter(|d| **d == 1).count();
            cells += layout.cells();
            way += layout.solution().len();
            let (their_ends, their_way) = grown_from_anywhere(&layout, seed);
            other_ends += their_ends;
            other_way += their_way;
        }
        let (share, other_share) = (ends as f64 / cells as f64, other_ends as f64 / cells as f64);
        println!("{kind:?}: dead ends {:.1}% against {:.1}%, way through {} cells against {} of {} a maze", 100.0 * share, 100.0 * other_share, way / 12, other_way / 12, cells / 12);
        assert!(share < 0.16, "{kind:?}: {:.1}% of cells are dead ends", 100.0 * share);
        assert!(other_share > 2.0 * share, "{kind:?}: the walk has {:.1}% dead ends and a tree grown from anywhere {:.1}%", 100.0 * share, 100.0 * other_share);
        // the way through winds through a fifth of the maze or more, and is several times the other method's
        assert!(way * 5 >= cells, "{kind:?}: the way through is {way} of {cells} cells");
        assert!(way >= 2 * other_way, "{kind:?}: the way through is {way} cells against {other_way}");
    }
}

#[test]
fn rings_are_divided_as_the_rule_says() {
    // every two rings, as the maze this replaces had it: 4, 4, 8, 8, 16, 16, with the room first
    let old = laid(&Maze { divisions: 4, grow: 2, ..maze(Kind::Theta, 7, 1) }, 140.0, 3.0);
    assert_eq!(old.counts(), [1, 4, 8, 8, 16, 16, 32]);
    // every ring, until the cells would be too narrow to pass between their spokes: then no more
    let fast = laid(&Maze { divisions: 4, grow: 1, ..maze(Kind::Theta, 8, 1) }, 80.0, 3.0);
    assert_eq!(fast.counts(), [1, 4, 8, 16, 32, 32, 32, 64]);
    for (k, n) in fast.counts().iter().enumerate().skip(1) {
        assert!(TAU * k as f64 * fast.gap() / *n as f64 - 3.0 >= LEAST_PASSAGE, "ring {k} of {n} cells is too narrow");
    }
    // by itself: every cell at least as wide as the corridor at its inner wall, and under twice that, from the second ring on
    let even = laid(&Maze { divisions: 6, grow: 0, ..maze(Kind::Theta, 12, 1) }, 240.0, 3.0);
    assert_eq!(even.counts(), [1, 6, 12, 12, 24, 24, 24, 24, 48, 48, 48, 48]);
    for (k, n) in even.counts().iter().enumerate().skip(2) {
        let wide = TAU * k as f64 / *n as f64;
        assert!((1.0..2.0).contains(&wide), "ring {k}: its cells are {wide:.2} corridors wide");
    }
    // more cells asked for round the room than there is room for: as many as fit
    let crowded = laid(&Maze { divisions: 64, ..maze(Kind::Theta, 4, 1) }, 48.0, 3.0);
    assert_eq!(crowded.counts()[1], 12);
}

#[test]
fn hostile_settings_are_refused_or_brought_within_bounds() {
    let wild = Maze { kind: Kind::Theta, rings: i32::MAX, divisions: i32::MIN, grow: -9, seed: u64::MAX, entrance: i32::MIN, floor: i32::MAX, solution: true, hedge: false };
    let tame = wild.clamped();
    assert_eq!((tame.rings, tame.divisions, tame.grow, tame.floor), (RINGS.1, 1, 0, MOST_FLOOR));
    assert!((0..360).contains(&tame.entrance));
    // forty rings in a small circle: corridors too narrow, said so and not built
    let refused = Layout::new(&wild, DVec2::ZERO, 100.0, 2.0).unwrap_err();
    assert!(refused.contains("corridors"), "{refused}");
    for bad in [f64::NAN, f64::INFINITY, 0.0, -5.0] {
        assert!(Layout::new(&Maze::default(), DVec2::ZERO, bad, 2.0).is_err());
        assert!(Layout::new(&Maze::default(), DVec2::ZERO, 100.0, bad).is_err());
    }
    // the largest maze there may be is built or refused, and soon either way
    let start = std::time::Instant::now();
    let huge = build(&Spec { thick: 2.0, rise: Some(4.0), ..spec(Maze { rings: RINGS.1, divisions: MOST_DIVISIONS, grow: 1, ..Maze::default() }, crate::spec::MAX_SPAN) });
    assert!(huge.is_err_and(|e| e.contains("too large")), "the largest maze was built");
    // too much hedge is refused by name
    let hedge = build(&Spec { thick: 20.0, rise: Some(60.0), ..spec(Maze { hedge: true, rings: 8, ..Maze::default() }, 400.0) });
    assert!(hedge.is_err_and(|e| e.contains("hedge")));
    // a size no form may be
    assert!(build(&spec(Maze::default(), 5000.0)).is_err());
    // (a busy machine is allowed for: each of these is a fraction of a second on an idle one)
    assert!(start.elapsed().as_secs() < 60, "refusing took {:?}", start.elapsed());
    let _ = (MOST_HEDGE, MOST_WALL);
}

/// The cells whose middles a build's bricks hold: of a sloped brick, the cells of its box whose middles are in its solid half.
fn boxes(bricks: &[crate::shapes::CellBrick]) -> HashSet<[i32; 3]> {
    let mut held = HashSet::new();
    for b in bricks {
        let info = crate::shapes::shape_info(b.mask).unwrap();
        let within = |at: i32, k: usize| (at as f64 + 0.5) / b.ext[k] as f64;
        for x in 0..b.ext[0] {
            for y in 0..b.ext[1] {
                for z in 0..b.ext[2] {
                    if crate::shapes::inside_shape(info, within(x, 0), within(y, 1), within(z, 2)) {
                        held.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]);
                    }
                }
            }
        }
    }
    held
}

#[test]
fn walls_stand_where_the_layout_says_and_the_way_through_is_open() {
    for (maze, radius) in [(maze(Kind::Theta, 5, 4), 80.0), (Maze { grow: 2, ..maze(Kind::Theta, 6, 9) }, 90.0), (maze(Kind::Grid, 4, 4), 56.0)] {
        let spec = spec(maze, radius);
        let built = build(&spec).unwrap();
        let layout = laid(&maze, radius, 3.0);
        let filled = boxes(&built.bricks);
        // a cell well inside a wall is filled, and one well clear of every wall is not
        let reach = layout.reach().ceil() as i32 + 2;
        let (mut inside, mut outside) = (0, 0);
        for y in -reach..reach {
            for x in -reach..reach {
                let d = layout.wall(DVec2::new(x as f64 + 0.5, y as f64 + 0.5));
                if d < -1.0 {
                    inside += 1;
                    assert!(filled.contains(&[x, y, 3]), "{maze:?}: no brick in the wall at {x}, {y}");
                } else if d > 1.5 {
                    outside += 1;
                    assert!(!filled.contains(&[x, y, 3]), "{maze:?}: a brick clear of every wall at {x}, {y}");
                }
            }
        }
        assert!(inside > 200 && outside > 2000, "{maze:?}: {inside} cells in walls and {outside} clear");
        // the way through is clear of every wall by a cell or more, all along it, and ends in the room
        let route = layout.route();
        for pair in route.windows(2) {
            for step in 0..=8 {
                let p = pair[0].lerp(pair[1], step as f64 / 8.0);
                assert!(layout.wall(p) > 1.0, "{maze:?}: the way through is {:.2} from a wall at {p}", layout.wall(p));
            }
        }
        assert!(route.last().unwrap().distance(layout.centre()) < 1e-9, "{maze:?}");
        // few bricks and a close fit: the walls are fitted as outlines, with no cell-sized steps round the rings
        // (a ring band 3 thick on its own, radius 48, is 4.2% amiss: a thin band's whole volume is near its surface)
        assert!(built.error / built.solid < 0.06, "{maze:?}: {:.1}% amiss", 100.0 * built.error / built.solid);
        let long: f64 = layout.lines().iter().map(|(line, _)| line.windows(2).map(|pair| pair[0].distance(pair[1])).sum::<f64>()).sum();
        assert!((built.bricks.len() as f64) < long * 1.2, "{maze:?}: {} bricks for {long:.0} of wall", built.bricks.len());
        if maze.kind == Kind::Grid {
            assert!(built.bricks.iter().all(|b| b.mask == crate::shapes::CUBE) && built.error == 0.0, "a grid's walls are not whole bricks");
        }
    }
}

#[test]
fn a_maze_is_the_same_bricks_on_any_number_of_threads() {
    let mosaic = |style: usize| Mosaic { style, size: 2, bevel: false, border: 0, seed: 3, mortar: true, stagger: 50, upright: false, uneven: [30; 2] };
    let bricks = crate::pattern::STYLES.iter().position(|s| s.name == "bricks").unwrap();
    let cobbles = crate::pattern::STYLES.iter().position(|s| s.name == "cobblestone").unwrap();
    let plain = spec(Maze { floor: 2, solution: true, ..maze(Kind::Theta, 4, 5) }, 64.0);
    let cases = [
        plain.clone(),
        Spec { mosaic: Some(mosaic(bricks)), ..plain.clone() },
        Spec { mosaic: Some(mosaic(cobbles)), ..spec(maze(Kind::Grid, 3, 5), 40.0) },
        Spec { thick: 6.0, rise: Some(14.0), ..spec(Maze { hedge: true, ..maze(Kind::Theta, 3, 5) }, 54.0) },
    ];
    for spec in cases {
        let one = build_with(&spec, true, Threads::ONE).unwrap();
        for count in [2, 5, 16] {
            let many = build_with(&spec, true, Threads::forced(count)).unwrap();
            assert!(many.bricks == one.bricks && many.shades == one.shades, "{:?} on {count} threads is not the maze one thread builds", spec.maze);
            assert_eq!((many.solid, many.error), (one.solid, one.error));
        }
    }
}

#[test]
fn a_texture_colours_the_walls_and_never_moves_them() {
    let plain = spec(maze(Kind::Theta, 4, 2), 64.0);
    let bare = build(&plain).unwrap();
    assert!(bare.shades.iter().all(|s| *s == WALL));
    // what a build's bricks fill, whatever they are cut into: sampled eight times to a cell each way of the ground, at every height
    let solid = |built: &crate::spec::Built| {
        let mut held: HashSet<(i32, i32, i32)> = HashSet::new();
        for b in &built.bricks {
            let info = crate::shapes::shape_info(b.mask).unwrap();
            for j in 0..b.ext[1] * 4 {
                for i in 0..b.ext[0] * 4 {
                    if crate::shapes::inside_shape(info, (i as f64 + 0.5) / (b.ext[0] * 4) as f64, (j as f64 + 0.5) / (b.ext[1] * 4) as f64, 0.5) {
                        held.extend((0..b.ext[2]).map(|z| (b.min[0] * 4 + i, b.min[1] * 4 + j, b.min[2] + z)));
                    }
                }
            }
        }
        held
    };
    let shape = solid(&bare);
    for (n, style) in crate::pattern::STYLES.iter().enumerate() {
        let mosaic = Mosaic { style: n, size: 2, bevel: true, border: 3, seed: 1, mortar: true, stagger: 50, upright: false, uneven: [40; 2] };
        let built = build(&Spec { mosaic: Some(mosaic), ..plain.clone() }).unwrap();
        assert!(solid(&built) == shape, "{}: the walls are not where they were", style.name);
        assert_eq!((built.solid, built.error), (bare.solid, bare.error), "{}", style.name);
        let shades: HashSet<u8> = built.shades.iter().copied().collect();
        assert!(!shades.contains(&SOLUTION), "{}: a wall in the strip's colour", style.name);
        // a rug is not wrapped along walls: they stay plain
        assert_eq!(shades.len() > 1, style.name != "rug", "{}: {shades:?}", style.name);
    }
}

#[test]
fn courses_of_bricks_run_level_round_every_ring() {
    let bricks = crate::pattern::STYLES.iter().position(|s| s.name == "bricks").unwrap();
    let mosaic = Mosaic { style: bricks, size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: 50, upright: false, uneven: [0; 2] };
    let built = build(&Spec { mosaic: Some(mosaic), rise: Some(16.0), ..spec(maze(Kind::Theta, 4, 2), 64.0) }).unwrap();
    // At size 2 with mortar a course is 3 cells of brick and 1 of mortar
    // (`pattern::Masonry`). Laid along the walls, every fourth layer of
    // the whole maze is mortar from side to side and nothing else, and
    // the three between hold bricks. Pushed straight through the maze
    // from above, as a floor's pattern is, every layer would be alike.
    let mut layers: Vec<(usize, usize)> = vec![(0, 0); 16];
    for (brick, shade) in built.bricks.iter().zip(&built.shades) {
        for z in brick.min[2]..brick.min[2] + brick.ext[2] {
            let layer = &mut layers[z as usize];
            if *shade == FLOOR { layer.0 += 1 } else { layer.1 += 1 }
        }
    }
    let mortar: Vec<usize> = (0..16).filter(|z| layers[*z].1 == 0).collect();
    assert_eq!(mortar.len(), 4, "the layers that are all mortar: {mortar:?} of {layers:?}");
    assert!(mortar.windows(2).all(|pair| pair[1] - pair[0] == 4), "{mortar:?}");
    // and in a layer of bricks there are joints: mortar a cell wide through the wall, at more than one place
    assert!((0..16).filter(|z| !mortar.contains(z)).all(|z| layers[z].0 > 20), "{layers:?}");
}

#[test]
fn where_two_walls_of_a_grid_cross_no_joint_is_smeared_across_the_corner() {
    let bricks = crate::pattern::STYLES.iter().position(|s| s.name == "bricks").unwrap();
    let mosaic = Mosaic { style: bricks, size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: 50, upright: false, uneven: [0; 2] };
    let m = maze(Kind::Grid, 4, 3);
    let built = build(&Spec { mosaic: Some(mosaic), thick: 4.0, rise: Some(16.0), ..spec(m, 84.0) }).unwrap();
    let layout = laid(&m, 84.0, 4.0);
    // The squares where a line of walls each way cross: 4 cells by 4 at
    // every corner of every cell. A wall's pattern goes through its
    // thickness, so a joint that fell there showed as a slab of mortar
    // the wall's whole thickness wide on the face round the corner. In
    // a course of bricks (any layer but the first and every fourth after, which are mortar
    // all round) no cell of such a square is mortar.
    let (gap, low) = (layout.gap(), layout.centre() - DVec2::splat(3.5 * layout.gap()));
    let near = |v: f64| ((v / gap).round() * gap - v).abs() <= 2.0;
    let (mut crossing, mut elsewhere) = (0, 0);
    for (brick, shade) in built.bricks.iter().zip(&built.shades) {
        for x in brick.min[0]..brick.min[0] + brick.ext[0] {
            for y in brick.min[1]..brick.min[1] + brick.ext[1] {
                let at = DVec2::new(x as f64 + 0.5, y as f64 + 0.5) - low;
                let courses = (brick.min[2]..brick.min[2] + brick.ext[2]).filter(|z| z % 4 != 0).count();
                if near(at.x) && near(at.y) {
                    crossing += courses;
                    assert!(*shade != FLOOR || courses == 0, "mortar through a corner at {x}, {y}, from {} up {}", brick.min[2], brick.ext[2]);
                } else if *shade == FLOOR {
                    elsewhere += courses;
                }
            }
        }
    }
    // (and along the walls between the corners the joints are there)
    assert!(crossing > 2000 && elsewhere > 500, "{crossing} cells of brick in the corners, {elsewhere} of mortar in joints elsewhere");
}

#[test]
fn a_floor_and_the_way_through_are_laid_when_asked() {
    let bare = build(&spec(maze(Kind::Theta, 4, 2), 64.0)).unwrap();
    let full = build(&spec(Maze { floor: 2, solution: true, ..maze(Kind::Theta, 4, 2) }, 64.0)).unwrap();
    let low = |built: &crate::spec::Built| built.bricks.iter().map(|b| b.min[2]).min().unwrap();
    let high = |built: &crate::spec::Built| built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap();
    // the floor is under the walls, which are as tall as they were
    assert_eq!((low(&bare), high(&bare), low(&full), high(&full)), (0, 8, 0, 10));
    let of = |shade: u8| full.bricks.iter().zip(&full.shades).filter(|(_, s)| **s == shade).map(|(b, _)| *b).collect::<Vec<_>>();
    let (floor, strip) = (of(FLOOR), of(PLAIN_SOLUTION));
    assert!(floor.iter().all(|b| b.min[2] == 0 && b.ext[2] == 2), "the floor is not two cells thick");
    // the floor reaches under the whole maze: its area is the disc's out to the outside of the rim
    let area: f64 = floor.iter().map(|b| b.volume()).sum::<f64>() / 2.0;
    let disc = std::f64::consts::PI * 65.5 * 65.5;
    assert!((area - disc).abs() < 0.01 * disc, "the floor is {area:.0} cells and the disc {disc:.0}");
    // the strip lies on the floor, one cell high, and shares no cell with a wall
    assert!(!strip.is_empty() && strip.iter().all(|b| b.min[2] == 2 && b.ext[2] == 1), "the strip is not one cell high on the floor");
    let walls = boxes(&of(WALL));
    assert!(boxes(&strip).is_disjoint(&walls), "the strip runs into a wall");
    // from the rim to the room
    let far = |b: &crate::shapes::CellBrick| DVec2::new(b.min[0] as f64 - 0.5, b.min[1] as f64 - 0.5).length();
    assert!(strip.iter().map(far).fold(f64::INFINITY, f64::min) < 3.0 && strip.iter().map(far).fold(0.0, f64::max) > 60.0);
}

#[test]
fn hedge_walls_are_the_plants_hedge_along_every_wall() {
    let plain = Spec { thick: 6.0, rise: Some(16.0), ..spec(maze(Kind::Theta, 3, 2), 60.0) };
    let hedge = Spec { maze: Maze { hedge: true, ..plain.maze }, ..plain.clone() };
    let (walls, grown) = (build(&plain).unwrap(), build(&hedge).unwrap());
    // leaves in the plant's greens and stems in its brown
    let shades: HashSet<u8> = grown.shades.iter().copied().collect();
    assert!(shades.contains(&crate::tree::LEAF) && shades.contains(&crate::tree::LEAF_DARK) && shades.contains(&crate::tree::TRUNK), "{shades:?}");
    // it stands where the walls stand: nearly all of the middle of the plain walls, above the stems, is hedge, and no hedge is far from a wall
    let (built, hedged) = (boxes(&walls.bricks), boxes(&grown.bricks));
    let layout = laid(&hedge.maze, 60.0, 6.0);
    let body: Vec<&[i32; 3]> = built.iter().filter(|c| (4..12).contains(&c[2]) && layout.wall(DVec2::new(c[0] as f64 + 0.5, c[1] as f64 + 0.5)) < -2.0).collect();
    let covered = body.iter().filter(|c| hedged.contains(**c)).count();
    assert!(body.len() > 2000 && covered * 10 >= body.len() * 9, "hedge fills {covered} of the {} cells down the middle of the walls", body.len());
    let stray = hedged.iter().filter(|c| layout.wall(DVec2::new(c[0] as f64 + 0.5, c[1] as f64 + 0.5)) > 2.5).count();
    assert!(stray * 100 < hedged.len(), "{stray} of {} cells of hedge are clear of every wall", hedged.len());
    // level on top, at the height asked, and the corridors open
    assert_eq!(grown.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap(), 16);
    for pair in layout.route().windows(2) {
        let p = pair[0].lerp(pair[1], 0.5);
        assert!(!hedged.contains(&[p.x.floor() as i32, p.y.floor() as i32, 6]), "hedge in the corridor at {p}");
    }
    // a hedge thinner or lower than a hedge may be is the least there is, and the maze is still built
    let thin = build(&Spec { thick: 1.0, rise: Some(2.0), ..hedge }).unwrap();
    assert!(thin.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() >= 12);
    let _ = Walls::new;
}

#[test]
fn the_spec_text_of_a_maze_reads_back_as_it_was() {
    let spec = Spec { maze: Maze { kind: Kind::Grid, rings: 9, divisions: 7, grow: 3, seed: 18_446_744_073_709_551_615, entrance: 123, floor: 3, solution: true, hedge: true }, ..spec(Maze::default(), 77.0) };
    let text = crate::report::spec_text(&spec);
    assert_eq!(crate::report::parse_spec(&text).unwrap(), spec);
    // a few of the numbers alone, and numbers no maze may have
    let short = crate::report::parse_spec("form = maze\nmaze = theta,9").unwrap();
    assert_eq!((short.form, short.maze), (Form::Maze, Maze { rings: 9, ..Maze::default() }));
    let wild = crate::report::parse_spec("form = maze\nmaze = grid,99999999999,-4,400,5,-725,77,1,0").unwrap();
    assert_eq!(wild.maze, Maze { kind: Kind::Grid, rings: RINGS.1, divisions: 1, grow: MOST_GROW, seed: 5, entrance: 355, floor: MOST_FLOOR, solution: true, hedge: false });
    assert!(crate::report::parse_spec("maze = hexagons,3").is_err());
}

#[test]
fn a_maze_of_too_much_hedge_for_single_microbricks_is_built_at_a_larger_size() {
    // the usual maze, with hedges 14 thick: over the limit at single microbricks, as asking for that size by name says
    let thick = Spec { thick: 14.0, rise: Some(30.0), ..spec(Maze { hedge: true, ..Maze::default() }, 120.0) };
    assert!(build(&Spec { grain: 1, ..thick.clone() }).is_err_and(|e| e.contains("too much hedge")));
    let built = build(&Spec { grain: 0, ..thick.clone() }).unwrap();
    assert!(built.grain > 1 && built.bricks.len() > 1000, "grain {}, {} bricks", built.grain, built.bricks.len());
    // and thicker than the same maze with hedges 8 thick
    let cells = |built: &crate::spec::Built| built.bricks.iter().map(|b| b.ext.iter().map(|e| (*e * built.grain) as i64).product::<i64>()).sum::<i64>();
    let thin = build(&Spec { grain: 0, thick: 8.0, ..thick }).unwrap();
    assert!(cells(&built) > cells(&thin) * 5 / 4, "{} cells at 14 thick, {} at 8", cells(&built), cells(&thin));
}
