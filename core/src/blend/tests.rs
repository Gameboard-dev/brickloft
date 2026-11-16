use super::*;
use crate::spec::{build, Form};

fn ball(radius: f64) -> Spec {
    Spec { form: Form::Sphere, points: vec![DVec3::ZERO], radius, fill: true, ..Spec::default() }
}

/// A cube 17 a side standing on the floor, as the editor adds one.
fn cube() -> Spec {
    Spec { form: Form::Cube, points: vec![DVec3::new(0.0, 0.0, 8.5)], radius: 8.5, fill: true, ..Spec::default() }
}

fn ring() -> Spec {
    Spec { form: Form::Circle, points: vec![DVec3::ZERO], radius: 12.0, thick: 6.0, tube: true, plane: 1, ..Spec::default() }
}

/// An arch narrow enough for both feet to stand in the cube. (An arch's
/// distance is no distance just inside the flat end of a foot, where it is
/// measured to the band's sides and not to the end: a foot hanging over the
/// cube's edge is judged a cell deep where it is a sliver from its end.)
fn arch() -> Spec {
    Spec { form: Form::Arch(crate::curves::ArchType::Round), points: vec![DVec3::new(-6.0, 0.0, 0.0), DVec3::new(6.0, 0.0, 0.0)], thick: 4.0, depth: 4, ..Spec::default() }
}

/// Forms standing at places given in cells, the last of them joined with `blend`.
struct Scene {
    specs: Vec<Spec>,
    built: Vec<Built>,
    places: Vec<[i32; 3]>,
}

impl Scene {
    fn new(forms: &[(Spec, [f64; 3])], blend: f64) -> Scene {
        let mut specs: Vec<Spec> = forms.iter().map(|f| f.0.clone()).collect();
        specs.last_mut().unwrap().blend = Some(blend);
        let built = specs.iter().map(|s| build(s).unwrap()).collect();
        Scene { specs, built, places: forms.iter().map(|f| f.1.map(|v| (v * 2.0).round() as i32)).collect() }
    }

    fn members(&self) -> Vec<Member<'_>> {
        (0..self.specs.len()).map(|i| Member { spec: &self.specs[i], built: &self.built[i], place: self.places[i], grid: 0 }).collect()
    }

    fn pieces(&self) -> Vec<Piece> {
        union(&self.members())
    }

    /// Each form's own distance at a place in the world, in cells.
    fn distances(&self) -> impl Fn(DVec3) -> Vec<f64> + '_ {
        let probes: Vec<Probe> = (0..self.specs.len()).map(|i| Probe::new(&self.specs[i], self.built[i].grain).unwrap()).collect();
        move |p| (0..probes.len()).map(|i| probes[i].distance(p - DVec3::from_array(self.places[i].map(|u| u as f64 / 2.0)))).collect()
    }

    /// Whether a place in the world is inside one of `pieces`.
    fn held(&self, pieces: &[Piece], p: DVec3) -> bool {
        pieces.iter().any(|piece| holds(&piece.brick, p - DVec3::from_array(self.places[piece.member].map(|u| u as f64 / 2.0))))
    }

    /// The box of all its bricks in the world, in cells.
    fn extent(&self) -> (DVec3, DVec3) {
        let mut all = (DVec3::MAX, DVec3::MIN);
        for (built, place) in self.built.iter().zip(&self.places) {
            let at = DVec3::from_array(place.map(|u| u as f64 / 2.0));
            for b in &built.bricks {
                all = (all.0.min(DVec3::from_array(b.min.map(|v| v as f64)) + at), all.1.max(DVec3::from_array([0, 1, 2].map(|k| (b.min[k] + b.ext[k]) as f64)) + at));
            }
        }
        all
    }

    /// Places all through the scene, three to every two cells each way and off the cells' own lines.
    fn samples(&self) -> Vec<DVec3> {
        let (low, high) = self.extent();
        let count = ((high - low + 4.0) / 0.7).ceil();
        let mut all = Vec::new();
        for z in 0..count.z as i32 {
            for y in 0..count.y as i32 {
                for x in 0..count.x as i32 {
                    all.push(low - 2.0 + DVec3::new(x as f64, y as f64, z as f64) * 0.7 + 0.13);
                }
            }
        }
        all
    }
}

/// The pairs looked at: a sphere in a cube, a sphere in a sphere, a ring
/// standing in a cube, an arch on a cube.
fn pairs() -> Vec<(&'static str, Vec<(Spec, [f64; 3])>)> {
    vec![
        ("a sphere in a cube", vec![(cube(), [0.0; 3]), (ball(12.0), [12.0, 4.0, 18.0])]),
        ("a sphere in a sphere", vec![(ball(20.0), [0.0; 3]), (ball(14.0), [22.0, 6.0, 9.0])]),
        ("a ring in a cube", vec![(cube(), [0.0; 3]), (ring(), [0.0, 0.0, 19.0])]),
        ("an arch on a cube", vec![(cube(), [0.0; 3]), (arch(), [0.0, 2.0, 15.0])]),
    ]
}

fn shares_room(scene: &Scene, pieces: &[Piece]) -> Option<(Piece, Piece)> {
    let boxes: Vec<Box3> = pieces.iter().map(|p| in_units(&p.brick, scene.places[p.member])).collect();
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by_key(|i| boxes[*i].0[0]);
    for (n, &a) in order.iter().enumerate() {
        for &b in order[n + 1..].iter().take_while(|b| boxes[**b].0[0] < boxes[a].1[0]) {
            if meet(&boxes[a], &boxes[b]) {
                return Some((pieces[a], pieces[b]));
            }
        }
    }
    None
}

#[test]
fn the_smooth_minimum_is_the_plain_one_far_from_the_join_and_dips_a_quarter_of_the_blend_at_it() {
    assert_eq!(smooth_min(1.0, 9.0, 4.0), 1.0);
    assert_eq!(smooth_min(3.0, 3.0, 4.0), 2.0);
    assert_eq!(smooth_min(3.0, -2.0, 0.0), -2.0);
    // it leaves each surface where the other is the blend away, and never rises above either
    assert_eq!(smooth_min(0.0, 4.0, 4.0), 0.0);
    assert!(smooth_min(0.0, 3.9, 4.0) < 0.0);
    // two forms both "far" are far, not a number that is no number
    assert_eq!(smooth_min(f64::INFINITY, f64::INFINITY, 4.0), f64::INFINITY);
    assert_eq!(smooth_min(f64::NEG_INFINITY, f64::NEG_INFINITY, 4.0), f64::NEG_INFINITY);
}

#[test]
fn no_two_bricks_of_a_join_share_room() {
    for (name, forms) in pairs() {
        for blend in [0.0, 2.0, 6.0] {
            let scene = Scene::new(&forms, blend);
            let pieces = scene.pieces();
            assert!(shares_room(&scene, &pieces).is_none(), "{name}, blend {blend}: {:?}", shares_room(&scene, &pieces));
        }
    }
}

/// How many of a scene's sample places clearly inside the joined solid no
/// brick holds, and how many clearly outside it one does: "clearly" being
/// further from the surface than two fits of one solid differ by.
///
/// Inside is looked at only down to `UNDER` below the surface. Deeper than
/// that, where one form's surface passes through the inside of another,
/// the other is cut back to whole cells round the first one's sloped
/// bricks, and the half of a cell a slope leaves empty stays empty. It is
/// sealed in under bricks all round and cannot be seen, and it is not what
/// this looks for: a notch, which starts at the surface.
fn holes_and_spikes(scene: &Scene, pieces: &[Piece], blend: f64, clear: f64) -> (usize, usize) {
    let distances = scene.distances();
    let (mut holes, mut spikes) = (0, 0);
    for p in scene.samples() {
        let d = distances(p);
        let solid = d.iter().skip(1).fold(d[0], |all, d| smooth_min(all, *d, blend));
        if solid.abs() > clear && solid < 4.0 && solid > -UNDER {
            let held = scene.held(pieces, p);
            holes += (solid < 0.0 && !held) as usize;
            spikes += (solid > 0.0 && held) as usize;
        }
    }
    (holes, spikes)
}

const UNDER: f64 = 1.5;

#[test]
fn a_join_has_no_hole_and_nothing_standing_out_of_it() {
    for (name, forms) in pairs() {
        for blend in [0.0, 2.0, 6.0] {
            let scene = Scene::new(&forms, blend);
            let (holes, spikes) = holes_and_spikes(&scene, &scene.pieces(), blend, 0.9);
            assert_eq!((holes, spikes), (0, 0), "{name}, blend {blend}: places inside the join with no brick, and outside it with one");
        }
    }
}

#[test]
fn a_sharp_join_has_no_notch_where_cutting_alone_leaves_one() {
    // cutting alone: what a join was before, and still is for forms whose cells do not line up
    let scene = Scene::new(&pairs().remove(0).1, 0.0);
    let members = scene.members();
    let boxes: Vec<Option<Box3>> = members.iter().map(|m| m.built.bricks.iter().map(|b| in_units(b, m.place)).reduce(|a, b| ([0, 1, 2].map(|k| a.0[k].min(b.0[k])), [0, 1, 2].map(|k| a.1[k].max(b.1[k]))))).collect();
    let mut cut: Vec<Vec<Piece>> = members.iter().enumerate().map(|(member, m)| m.built.bricks.iter().enumerate().map(|(n, brick)| Piece { brick: *brick, member, from: Some(n), loose: false }).collect()).collect();
    give_up_room(&members, &order(&members, &boxes), &mut cut);
    let cut: Vec<Piece> = cut.into_iter().flatten().collect();
    let (notches, _) = holes_and_spikes(&scene, &cut, 0.0, 0.9);
    assert!(notches > 20, "cutting alone left only {notches} places unfilled: this pair no longer shows the fault");
    assert_eq!(holes_and_spikes(&scene, &scene.pieces(), 0.0, 0.9).0, 0);
}

#[test]
fn bricks_far_from_the_seam_are_each_forms_own() {
    let mut far = 0;
    for (name, forms) in pairs() {
        let scene = Scene::new(&forms, 2.0);
        let pieces = scene.pieces();
        let distances = scene.distances();
        let mut near = 0;
        // A brick is changed only where the zone reaches it: where the other
        // form is as near as a cell of the zone allows, or a sloped brick
        // swallowed whole carries the zone on by its own length.
        let reach = 2.0 / 4.0 + INSIDE + apart(2.0) + 8.0;
        for (member, built) in scene.built.iter().enumerate() {
            let other = 1 - member;
            for (n, b) in built.bricks.iter().enumerate() {
                // the other form's distance at the brick's eight corners
                let corner = |c: usize| DVec3::from_array([0, 1, 2].map(|k| (b.min[k] + if c >> k & 1 == 1 { b.ext[k] } else { 0 }) as f64 + scene.places[member][k] as f64 / 2.0));
                let apart = (0..8).map(|c| distances(corner(c))[other]).fold(f64::INFINITY, f64::min);
                let kept = pieces.contains(&Piece { brick: *b, member, from: Some(n), loose: false });
                if apart > reach {
                    far += 1;
                    assert!(kept, "{name}: brick {n} of form {member}, {apart:.1} from the other form, was changed");
                } else if !kept {
                    near += 1;
                }
            }
        }
        assert!(near > 0, "{name}: no brick was changed at all");
    }
    assert!(far > 300, "only {far} bricks far from a seam were looked at");
}

#[test]
fn a_larger_blend_adds_material_and_only_near_the_seam() {
    for (name, forms) in pairs() {
        let (sharp, round) = (Scene::new(&forms, 0.0), Scene::new(&forms, 6.0));
        let (before, after) = (sharp.pieces(), round.pieces());
        let distances = sharp.distances();
        let (mut added, mut lost) = (0, 0);
        for p in sharp.samples() {
            let (was, is) = (sharp.held(&before, p), round.held(&after, p));
            let d = distances(p);
            // (clear of the sharp join: a refit may also move a sloped face by a sliver, anywhere in the zone)
            if is && !was && d.iter().all(|d| *d > 0.5) {
                added += 1;
                // within the fillet's reach of both forms: `1.25 * k` and a cell for the fit
                assert!(d.iter().all(|d| *d < 1.25 * 6.0 + 1.0), "{name}: material added {d:?} from the forms");
            }
            lost += (was && !is && d.iter().any(|d| *d < -0.5)) as usize;
        }
        assert!(added > 100 && lost == 0, "{name}: {added} places added and {lost} lost");
    }
}

#[test]
fn a_join_is_the_same_whichever_form_is_listed_first_and_on_any_number_of_threads() {
    for (name, forms) in pairs() {
        let scene = Scene::new(&forms, 2.0);
        let as_placed = |scene: &Scene, pieces: Vec<Piece>| {
            let mut all: Vec<(CellBrick, [i32; 3], Option<usize>, bool)> = pieces.iter().map(|p| (p.brick, scene.places[p.member], p.from, p.loose)).collect();
            all.sort_by_key(|(b, place, ..)| (*place, b.min, b.ext, b.mask));
            all
        };
        let one = as_placed(&scene, union_with(&scene.members(), Threads::ONE));
        for threads in [2, 3, 8] {
            assert!(one == as_placed(&scene, union_with(&scene.members(), Threads::forced(threads))), "{name}: different bricks on {threads} threads");
        }
        let turned = Scene { specs: scene.specs.iter().rev().cloned().collect(), built: scene.built.iter().rev().cloned().collect(), places: scene.places.iter().rev().copied().collect() };
        assert!(one == as_placed(&turned, union_with(&turned.members(), Threads::ONE)), "{name}: different bricks with the forms listed the other way round");
    }
}

#[test]
fn two_cubes_joined_are_a_few_blocks_and_nothing_sloped() {
    for (place, blend) in [([8.0, 6.0, 5.0], 0.0), ([8.5, 6.5, 5.5], 0.0), ([8.5, 6.5, 5.5], 4.0)] {
        let scene = Scene::new(&[(cube(), [0.0; 3]), (cube(), place)], blend);
        let pieces = scene.pieces();
        assert!(shares_room(&scene, &pieces).is_none());
        // (loose cells are joined into blocks by the caller: here they are counted as the blocks they would make)
        let blocks = crate::pattern::joined(pieces.iter().filter(|p| p.loose).map(|p| (p.brick, p.member as u8)).collect(), 80).len() + pieces.iter().filter(|p| !p.loose).count();
        assert!(pieces.iter().all(|p| p.brick.mask == CUBE) && blocks <= 12, "at {place:?}: {blocks} blocks, or something sloped");
        let cells: i32 = pieces.iter().filter(|p| p.member == 1).map(|p| p.brick.ext.iter().product::<i32>()).sum();
        assert!(cells > 17 * 17 * 17 / 2 && cells < 17 * 17 * 17);
    }
}

#[test]
fn a_form_half_a_cell_off_another_is_joined_by_cutting_alone() {
    // no cell of either lines up with the other's, so there is no lattice to fit a join on
    let scene = Scene::new(&[(cube(), [0.0; 3]), (ball(12.0), [12.5, 4.0, 18.0])], 4.0);
    let pieces = scene.pieces();
    assert!(shares_room(&scene, &pieces).is_none());
    assert!(pieces.iter().all(|p| p.from.is_some()), "a brick was fitted afresh");
    assert_eq!(pieces.iter().filter(|p| p.member == 0).count(), scene.built[0].bricks.len(), "the form that is not joined was cut");
}

#[test]
fn three_forms_joined_in_a_chain_are_one_build() {
    // a sphere in a cube, and a smaller sphere in that sphere, clear of the cube
    let mut scene = Scene::new(&[(cube(), [0.0; 3]), (ball(12.0), [12.0, 4.0, 18.0]), (ball(8.0), [24.0, 6.0, 24.0])], 3.0);
    scene.specs[1].blend = Some(3.0);
    let pieces = scene.pieces();
    assert!(shares_room(&scene, &pieces).is_none());
    let distances = scene.distances();
    let (mut holes, mut spikes) = (0, 0);
    for p in scene.samples() {
        let d = distances(p);
        // (the cube and the small sphere are both joined to the middle one, and nowhere near each other)
        let solid = smooth_min(d[0], d[1], 3.0).min(smooth_min(d[1], d[2], 3.0));
        if solid.abs() > 0.9 && solid < 4.0 && solid > -UNDER {
            holes += (solid < 0.0 && !scene.held(&pieces, p)) as usize;
            spikes += (solid > 0.0 && scene.held(&pieces, p)) as usize;
        }
    }
    assert_eq!((holes, spikes), (0, 0));
    // and listed in another order they are the same bricks
    let again = Scene { specs: vec![scene.specs[2].clone(), scene.specs[0].clone(), scene.specs[1].clone()], built: vec![scene.built[2].clone(), scene.built[0].clone(), scene.built[1].clone()], places: vec![scene.places[2], scene.places[0], scene.places[1]] };
    let placed = |scene: &Scene, pieces: &[Piece]| {
        let mut all: Vec<(CellBrick, [i32; 3])> = pieces.iter().map(|p| (p.brick, scene.places[p.member])).collect();
        all.sort_by_key(|(b, place)| (*place, b.min, b.ext, b.mask));
        all
    };
    assert!(placed(&scene, &pieces) == placed(&again, &again.pieces()));
}

#[test]
fn an_ordinary_join_is_put_together_in_a_few_milliseconds() {
    // the least of several runs, since a test shares the machine with the others
    let time = |forms: &[(Spec, [f64; 3])], blend: f64, threads: Threads| {
        let scene = Scene::new(forms, blend);
        let members = scene.members();
        (0..7).map(|_| {
            let start = std::time::Instant::now();
            std::hint::black_box(union_with(&members, threads));
            start.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::INFINITY, f64::min)
    };
    let pairs = [("a sphere of 20 in a cube of 17", vec![(cube(), [0.0; 3]), (ball(20.0), [24.0, 6.0, 14.0])]), ("two spheres of 20", vec![(ball(20.0), [0.0; 3]), (ball(20.0), [26.0, 8.0, 10.0])])];
    for (name, forms) in pairs {
        for blend in [0.0, 2.0, 6.0] {
            let (one, all) = (time(&forms, blend, Threads::ONE), time(&forms, blend, Threads::available()));
            println!("{name}, blend {blend}: {one:.2} ms on one thread, {all:.2} ms on {}", Threads::available().count());
            // (an unoptimised build is many times slower, and is not what is being measured)
            // (the better of the two: with every test running at once there may be no core free to share with)
            assert!(cfg!(debug_assertions) || one.min(all) < 30.0, "{name}, blend {blend}: {one:.1} ms on one thread, {all:.1} ms on all");
        }
    }
}


