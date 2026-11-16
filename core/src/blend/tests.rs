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

    /// The same, with the last form taken away from the others.
    fn carved(forms: &[(Spec, [f64; 3])], blend: f64) -> Scene {
        let mut forms = forms.to_vec();
        forms.last_mut().unwrap().0.carve = true;
        Scene::new(&forms, blend)
    }

    /// The joined solid's distance where the forms' own are `d`: the last
    /// form joined to the others with `blend`, or taken away from them.
    fn solid(&self, d: &[f64], blend: f64) -> f64 {
        let (last, rest) = d.split_last().unwrap();
        if self.specs.last().unwrap().carve {
            let rest = rest.iter().copied().fold(f64::INFINITY, f64::min);
            return -smooth_min(-rest, *last, blend);
        }
        d.iter().skip(1).fold(d[0], |all, d| smooth_min(all, *d, blend))
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
        let solid = scene.solid(&d, blend);
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

/// A long square bar, to put through things.
fn bar() -> Spec {
    Spec { form: Form::Cube, points: vec![DVec3::ZERO], radius: 20.0, stretch: DVec3::new(0.2, 0.2, 1.0), fill: true, ..Spec::default() }
}

/// The pairs with something taken away: a sphere out of a cube's corner, a
/// sphere out of a sphere, a bar through a sphere, a ring out of a cube.
fn holes() -> Vec<(&'static str, Vec<(Spec, [f64; 3])>)> {
    vec![
        ("a sphere out of a cube", vec![(cube(), [0.0; 3]), (ball(12.0), [12.0, 4.0, 18.0])]),
        ("a sphere out of a sphere", vec![(ball(20.0), [0.0; 3]), (ball(12.0), [14.0, 4.0, 14.0])]),
        ("a bar through a sphere", vec![(ball(20.0), [0.0; 3]), (bar(), [3.0, 2.0, 0.0])]),
        ("a ring out of a cube", vec![(cube(), [0.0; 3]), (ring(), [0.0, 0.0, 19.0])]),
    ]
}

#[test]
fn a_form_taken_away_leaves_a_hole_of_its_own_shape_and_none_of_its_bricks() {
    for (name, forms) in holes() {
        for blend in [0.0, 2.0, 6.0] {
            let scene = Scene::carved(&forms, blend);
            let pieces = scene.pieces();
            assert!(shares_room(&scene, &pieces).is_none(), "{name}, blend {blend}: {:?}", shares_room(&scene, &pieces));
            assert!(pieces.iter().all(|p| p.member == 0), "{name}, blend {blend}: a brick of the form taken away was built");
            let (holes, spikes) = holes_and_spikes(&scene, &pieces, blend, 0.9);
            assert_eq!((holes, spikes), (0, 0), "{name}, blend {blend}: places inside what is left with no brick, and outside it with one");
            // and it is a hole: joined instead, the same forms fill far more room
            let room = |pieces: &[Piece]| pieces.iter().map(|p| p.brick.volume()).sum::<f64>();
            let whole = scene.built[0].bricks.iter().map(|b| b.volume()).sum::<f64>();
            assert!(room(&pieces) < whole - 100.0, "{name}, blend {blend}: {} cells left of {whole}", room(&pieces));
        }
    }
}

#[test]
fn a_hole_is_the_same_whichever_form_is_listed_first_and_on_any_number_of_threads() {
    for (name, forms) in holes() {
        let scene = Scene::carved(&forms, 2.0);
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
fn a_window_through_a_wall_is_a_few_blocks_and_nothing_sloped() {
    let wall = Spec { form: Form::Cube, points: vec![DVec3::new(0.0, 0.0, 16.0)], radius: 16.0, stretch: DVec3::new(1.0, 0.2, 1.0), fill: true, ..Spec::default() };
    let window = Spec { form: Form::Cube, points: vec![DVec3::ZERO], radius: 5.0, fill: true, ..Spec::default() };
    let room = |pieces: &[Piece]| pieces.iter().map(|p| p.brick.volume()).sum::<f64>();
    let mut left = Vec::new();
    for blend in [0.0, 4.0] {
        let scene = Scene::carved(&[(wall.clone(), [0.0; 3]), (window.clone(), [2.0, 0.0, 18.0])], blend);
        let pieces = scene.pieces();
        assert!(shares_room(&scene, &pieces).is_none());
        if blend == 0.0 {
            let blocks = crate::pattern::joined(pieces.iter().filter(|p| p.loose).map(|p| (p.brick, p.member as u8)).collect(), 80).len() + pieces.iter().filter(|p| !p.loose).count();
            assert!(pieces.iter().all(|p| p.brick.mask == CUBE && p.member == 0) && blocks <= 8, "{blocks} blocks, or something sloped");
        }
        // the hole goes right through: nothing is left on a line through the window's middle, and the wall beside it is whole
        let through = |x: f64, z: f64| (-8..8).filter(|y| scene.held(&pieces, DVec3::new(x, *y as f64 + 0.3, z))).count();
        assert_eq!((through(2.2, 18.2), through(-10.2, 18.2)), (0, scene.built[0].bricks.iter().map(|b| b.ext[1]).max().unwrap() as usize));
        left.push(room(&pieces));
    }
    // a blend rounds the window's rim, which takes a little more of the wall with sloped bricks
    assert!(left[1] < left[0] - 10.0 && left[1] > left[0] - 400.0, "{left:?} cells left of the wall, sharp and rounded");
}

#[test]
fn a_form_taken_away_cuts_the_forms_that_are_not_joined_and_a_mark_alone_cuts_nothing() {
    // half a cell off the cube, it cannot be fitted as one with it, and cuts it by its bricks alone
    let scene = Scene::carved(&[(cube(), [0.0; 3]), (ball(12.0), [12.5, 4.0, 18.0])], 0.0);
    let pieces = scene.pieces();
    assert!(pieces.iter().all(|p| p.member == 0) && pieces.len() > 20, "a form half a cell off the cube did not cut it");
    assert!(!scene.held(&pieces, DVec3::new(7.3, 4.2, 15.2)) && scene.held(&pieces, DVec3::new(-7.3, -4.2, 3.2)));
    // marked to be taken away, but standing clear and not joined: both are built whole
    let mut apart = Scene::carved(&[(cube(), [0.0; 3]), (ball(12.0), [12.0, 4.0, 18.0])], 0.0);
    apart.specs[1].blend = None;
    assert_eq!(apart.pieces().len(), apart.built[0].bricks.len() + apart.built[1].bricks.len());
}

#[test]
fn a_joined_form_half_a_cell_off_is_settled_onto_the_other_forms_cells_and_then_blends() {
    let mut scene = Scene::new(&[(cube(), [0.0; 3]), (ball(12.0), [12.5, 4.0, 17.5])], 4.0);
    // as it stands it is cut, with a notch, and nothing is fitted afresh
    let cut = scene.pieces();
    assert!(cut.iter().all(|p| p.from.is_some()) && holes_and_spikes(&scene, &cut, 4.0, 0.9).0 > 20);
    // settled, it has moved half a cell along the two axes it was off on, the way it was leaning
    let to = settled(&scene.members(), 1, [-1, 0, 0]);
    assert_eq!(to, [24, 8, 36]);
    // (the form that is not joined stays where it is, and so does a form already lined up)
    assert_eq!(settled(&scene.members(), 0, [0; 3]), [0; 3]);
    scene.places[1] = to;
    assert_eq!(settled(&scene.members(), 1, [1; 3]), to);
    let pieces = scene.pieces();
    assert!(shares_room(&scene, &pieces).is_none() && pieces.iter().any(|p| p.from.is_none()), "nothing was fitted afresh");
    assert_eq!(holes_and_spikes(&scene, &pieces, 4.0, 0.9), (0, 0));
    // far from every other form there is nothing to line up with
    scene.places[1] = [201, 8, 35];
    assert_eq!(settled(&scene.members(), 1, [0; 3]), [201, 8, 35]);
}

#[test]
fn forms_built_at_one_brick_size_are_joined_at_that_size_and_at_single_cells_if_out_of_step() {
    let coarse = |radius: f64| Spec { grain: 2, ..ball(radius) };
    // (the single cells left of cut blocks are joined into blocks by the caller: here as the caller does it)
    let whole_bricks = |pieces: &[Piece]| {
        let loose = crate::pattern::joined(pieces.iter().filter(|p| p.loose).map(|p| (p.brick, p.member as u8)).collect(), 80);
        pieces.iter().filter(|p| !p.loose).map(|p| p.brick).chain(loose.into_iter().map(|l| l.0)).all(|b| b.min.iter().chain(&b.ext).all(|v| v % 2 == 0))
    };
    let fresh = |pieces: &[Piece]| pieces.iter().filter(|p| p.from.is_none() || p.loose).count();
    for carve in [false, true] {
        // a whole number of bricks apart: every brick of the join is whole bricks of that size
        let forms = [(coarse(20.0), [0.0; 3]), (coarse(14.0), [22.0, 6.0, 10.0])];
        let scene = if carve { Scene::carved(&forms, 2.0) } else { Scene::new(&forms, 2.0) };
        assert!(scene.built.iter().all(|b| b.grain == 2));
        let pieces = scene.pieces();
        assert!(shares_room(&scene, &pieces).is_none());
        assert!(whole_bricks(&pieces) && fresh(&pieces) > 20, "carve {carve}: a brick of the join is not whole bricks of 2, or none was fitted");
        // (twice as far from the surface as at single cells counts as clear: the bricks are twice as coarse)
        assert_eq!(holes_and_spikes(&scene, &pieces, 2.0, 1.8), (0, 0), "carve {carve}");
        // one cell out of step, there is no coarser lattice the two share, and the join is single cells: sound, and several times the bricks
        let forms = [(coarse(20.0), [0.0; 3]), (coarse(14.0), [23.0, 6.0, 10.0])];
        let fine = if carve { Scene::carved(&forms, 2.0) } else { Scene::new(&forms, 2.0) };
        let pieces_fine = fine.pieces();
        assert!(shares_room(&fine, &pieces_fine).is_none() && !whole_bricks(&pieces_fine));
        assert_eq!(holes_and_spikes(&fine, &pieces_fine, 2.0, 1.8), (0, 0), "carve {carve}, out of step");
        assert!(fresh(&pieces_fine) > 2 * fresh(&pieces), "carve {carve}: {} bricks fitted at single cells and {} at the forms' size", fresh(&pieces_fine), fresh(&pieces));
        // and settled, the form out of step is moved to the nearest place in step, or the way it leans
        // (it is a cell, half a brick, out: on to the next place if it leans neither way)
        assert_eq!(settled(&fine.members(), 1, [0; 3]), [48, 12, 20]);
        assert_eq!(settled(&fine.members(), 1, [-1, 0, 0]), [44, 12, 20]);
        assert_eq!(settled(&scene.members(), 1, [1; 3]), scene.places[1]);
    }
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

/// How rough a join is where it was fitted afresh, for comparing one way of
/// fitting it with another.
#[derive(Debug, Clone, Copy)]
struct Seam {
    /// bricks in all, and those of them that are no form's own
    bricks: usize,
    fresh: usize,
    /// bricks one cell each way that are sloped, among the fresh ones
    unit_sloped: usize,
    /// and those of them with no other sloped brick against any face: a tooth
    alone: usize,
    /// the room, in cells, where the bricks and the joined solid disagree near the seam
    wrong: f64,
    /// The area, in cell faces, of the steps the bricks make near the seam
    /// that the solid does not have: where, across the face between two
    /// cells, there is brick on one side and none on the other, and the
    /// solid is the same on both. A surface that the bricks follow exactly
    /// has none, however it slopes; a tooth has its three upright sides.
    steps: f64,
}

/// [`Seam`] for a scene of two forms that stand whole cells apart. "Near
/// the seam" is where the two forms' distances are within the blend's reach
/// of one another and the solid's surface is within two cells.
fn seam(scene: &Scene, pieces: &[Piece], blend: f64) -> Seam {
    assert!(scene.places.iter().all(|p| p.iter().all(|u| u % 2 == 0)));
    let distances = scene.distances();
    let near = |p: DVec3| {
        let d = distances(p);
        let (a, b) = (d[0], if scene.specs[1].carve { -d[1] } else { d[1] });
        ((a - b).abs() < blend + 2.0 && scene.solid(&d, blend).abs() < 2.0).then(|| scene.solid(&d, blend) < 0.0)
    };
    // the bricks in each cell of the world
    let mut within: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
    for (n, piece) in pieces.iter().enumerate() {
        let off = scene.places[piece.member].map(|u| u / 2);
        for c in cells(&piece.brick) {
            within.entry([0, 1, 2].map(|k| c[k] + off[k])).or_default().push(n);
        }
    }
    let held = |p: DVec3| within.get(&p.floor().to_array().map(|v| v as i32)).is_some_and(|some| some.iter().any(|&n| holds(&pieces[n].brick, p - DVec3::from_array(scene.places[pieces[n].member].map(|u| u as f64 / 2.0)))));
    let own: Vec<HashSet<(u8, [i32; 3], [i32; 3])>> = scene.built.iter().map(|b| b.bricks.iter().map(|b| (b.mask, b.min, b.ext)).collect()).collect();
    let fresh: Vec<&Piece> = pieces.iter().filter(|p| !own[p.member].contains(&(p.brick.mask, p.brick.min, p.brick.ext))).collect();
    let sloped: HashSet<[i32; 3]> = pieces.iter().filter(|p| p.brick.mask != CUBE).flat_map(|p| cells(&p.brick).map(|c| [0, 1, 2].map(|k| c[k] + scene.places[p.member][k] / 2))).collect();
    let unit: Vec<[i32; 3]> = fresh.iter().filter(|p| p.brick.mask != CUBE && p.brick.ext == [1; 3]).map(|p| [0, 1, 2].map(|k| p.brick.min[k] + scene.places[p.member][k] / 2)).collect();
    let alone = unit.iter().filter(|c| (0..6).all(|f| !sloped.contains(&[0, 1, 2].map(|k| c[k] + if k == f / 2 { [-1, 1][f % 2] } else { 0 })))).count();
    let (low, high) = scene.extent();
    let (low, high) = (low.floor() - 2.0, high.ceil() + 2.0);
    let (mut wrong, mut steps) = (0usize, 0usize);
    const FINE: usize = 4;
    let part = |i: usize| (i as f64 + 0.5) / FINE as f64;
    for z in low.z as i32..high.z as i32 {
        for y in low.y as i32..high.y as i32 {
            for x in low.x as i32..high.x as i32 {
                let at = DVec3::new(x as f64, y as f64, z as f64);
                if near(at + 0.5).is_none() && distances(at + 0.5).iter().all(|d| d.abs() > 4.0) {
                    continue;
                }
                for s in 0..FINE * FINE * FINE {
                    let p = at + DVec3::new(part(s % FINE), part(s / FINE % FINE), part(s / (FINE * FINE)));
                    if let Some(inside) = near(p) {
                        wrong += (inside != held(p)) as usize;
                    }
                }
                // the three faces of the cell on its low sides, a little to either side of each
                for axis in 0..3 {
                    for s in 0..FINE * FINE {
                        let mut on = [part(s % FINE), part(s / FINE), 0.0];
                        on.swap(2, axis);
                        let (before, after) = (at + DVec3::from_array(on) - DVec3::AXES[axis] * 0.01, at + DVec3::from_array(on) + DVec3::AXES[axis] * 0.01);
                        if let (Some(a), Some(b)) = (near(before), near(after)) {
                            steps += (a == b && held(before) != held(after)) as usize;
                        }
                    }
                }
            }
        }
    }
    Seam { bricks: pieces.len(), fresh: fresh.len(), unit_sloped: unit.len(), alone, wrong: wrong as f64 / (FINE * FINE * FINE) as f64, steps: steps as f64 / (FINE * FINE) as f64 }
}

/// The joins the seam's roughness is measured on: the sphere on a cube that
/// was complained of, a sphere in a sphere, and a sphere out of a cube.
fn seams() -> Vec<(&'static str, Scene, f64)> {
    let mut all = Vec::new();
    for blend in [0.0, 2.0, 6.0] {
        all.push(("a sphere on a cube", Scene::new(&[(cube(), [0.0; 3]), (ball(12.0), [4.0, 2.0, 22.0])], blend), blend));
        all.push(("a sphere in a sphere", Scene::new(&[(ball(20.0), [0.0; 3]), (ball(14.0), [22.0, 6.0, 9.0])], blend), blend));
        all.push(("a sphere out of a cube", Scene::carved(&[(cube(), [0.0; 3]), (ball(12.0), [12.0, 4.0, 18.0])], blend), blend));
    }
    all
}

#[test]
fn a_seam_is_no_rougher_than_it_was_measured_to_be() {
    // The nine joins together, as they were before the seam was evened out
    // and laid with stretched bricks, and as they are with both:
    //
    //                         wrong   steps   unit sloped   standing alone
    //   before                515.0  2172.9          1634               45
    //   evened out only       513.7  1876.1          1738                6
    //   stretched only        473.2  1968.8          1104               15
    //   both                  466.2  1818.3          1136                1
    let all: Vec<Seam> = seams().iter().map(|(_, scene, blend)| seam(scene, &scene.pieces(), *blend)).collect();
    let (wrong, steps) = (all.iter().map(|s| s.wrong).sum::<f64>(), all.iter().map(|s| s.steps).sum::<f64>());
    let (unit, alone) = (all.iter().map(|s| s.unit_sloped).sum::<usize>(), all.iter().map(|s| s.alone).sum::<usize>());
    assert!(wrong < 485.0 && steps < 1900.0 && unit < 1300 && alone <= 3, "{wrong:.1} cells wrong, {steps:.1} faces of step, {unit} unit sloped bricks and {alone} of them standing alone");
}

/// The same two measures for a sphere alone, fitted by its own fitter and
/// cell by cell: what a seam's figures are to be read against. (A sphere of
/// radius 20 has 0.18 cells wrong and 0.08 faces of step for each cell of
/// its surface from its own fitter, and 0.11 and 0.40 cell by cell: its own
/// facets are smooth by being further from the sphere, not nearer.)
#[test]
#[ignore = "prints figures to compare by eye, and asserts nothing"]
fn how_rough_a_sphere_alone_is() {
    for r in [12.0, 20.0] {
        let spec = ball(r);
        let built = build(&spec).unwrap();
        let probe = Probe::new(&spec, built.grain).unwrap();
        let fit = crate::fit_volume::fit_volume(&|p| probe.distance(p), [-24; 3], [24; 3], &crate::fit_volume::Opts::default());
        for (name, bricks) in [("round", &built.bricks), ("volume", &fit.bricks)] {
            let mut within: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
            for (n, b) in bricks.iter().enumerate() {
                for c in cells(b) {
                    within.entry(c).or_default().push(n);
                }
            }
            let held = |p: DVec3| within.get(&p.floor().to_array().map(|v| v as i32)).is_some_and(|some| some.iter().any(|&n| holds(&bricks[n], p)));
            let (mut wrong, mut steps) = (0usize, 0usize);
            let part = |i: usize| (i as f64 + 0.5) / 4.0;
            for z in -24..24 {
                for y in -24..24 {
                    for x in -24..24 {
                        let at = DVec3::new(x as f64, y as f64, z as f64);
                        if probe.distance(at + 0.5).abs() > 4.0 {
                            continue;
                        }
                        for s in 0..64 {
                            let p = at + DVec3::new(part(s % 4), part(s / 4 % 4), part(s / 16));
                            wrong += ((probe.distance(p) < 0.0) != held(p)) as usize;
                        }
                        for axis in 0..3 {
                            for s in 0..16 {
                                let mut on = [part(s % 4), part(s / 4), 0.0];
                                on.swap(2, axis);
                                let (before, after) = (at + DVec3::from_array(on) - DVec3::AXES[axis] * 0.01, at + DVec3::from_array(on) + DVec3::AXES[axis] * 0.01);
                                steps += ((probe.distance(before) < 0.0) == (probe.distance(after) < 0.0) && held(before) != held(after)) as usize;
                            }
                        }
                    }
                }
            }
            let area = 4.0 * std::f64::consts::PI * r * r;
            println!("sphere {r} {name}: {} bricks, wrong {:.3} a cell of surface, steps {:.3} a cell of surface", bricks.len(), wrong as f64 / 64.0 / area, steps as f64 / 16.0 / area);
        }
    }
}

/// Prints [`Seam`] for each of [`seams`]: run with `--ignored --nocapture`
/// before and after a change to how a join is fitted.
#[test]
#[ignore = "prints figures to compare by eye, and asserts nothing"]
fn how_rough_each_seam_is() {
    println!("{:<24} {:>5} {:>7} {:>6} {:>6} {:>6} {:>8} {:>8}", "join", "blend", "bricks", "fresh", "unit", "alone", "wrong", "steps");
    for (name, scene, blend) in seams() {
        let s = seam(&scene, &scene.pieces(), blend);
        println!("{name:<24} {blend:>5} {:>7} {:>6} {:>6} {:>6} {:>8.1} {:>8.1}", s.bricks, s.fresh, s.unit_sloped, s.alone, s.wrong, s.steps);
    }
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
            // (80 ms, where alone it takes 6 to 22: at 30 it failed in a full run on a machine busy with other builds and passed by itself)
            assert!(cfg!(debug_assertions) || one.min(all) < 80.0, "{name}, blend {blend}: {one:.1} ms on one thread, {all:.1} ms on all");
        }
    }
}



#[test]
fn a_plant_or_a_pattern_is_joined_by_cutting_alone() {
    // its bricks are leaves and stones, each with a colour of its own, and not the solid its distance describes
    let plant = Spec { form: Form::Tree, points: vec![DVec3::ZERO], radius: 16.0, rise: Some(40.0), ..Spec::default() };
    let floor = Spec { form: Form::Cube, points: vec![DVec3::new(0.0, 0.0, 1.0)], radius: 16.0, stretch: DVec3::new(1.0, 1.0, 1.0 / 16.0), fill: true, mosaic: Some(crate::pattern::Mosaic { style: 3, size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: -1, upright: false, uneven: [0; 2] }), ..Spec::default() };
    for (name, form, place) in [("a plant", plant, [0.0, 0.0, 6.0]), ("a patterned floor", floor, [4.0, 0.0, 16.0])] {
        let scene = Scene::new(&[(cube(), [0.0; 3]), (form, place)], 3.0);
        assert!(!scene.built[1].shades.is_empty(), "{name} has no colours of its own any more: it could be blended");
        let pieces = scene.pieces();
        assert!(shares_room(&scene, &pieces).is_none(), "{name}: two bricks share room");
        assert!(pieces.iter().all(|p| p.from.is_some()), "{name}: a brick was fitted afresh");
        assert_eq!(pieces.iter().filter(|p| p.member == 0).count(), 1, "{name}: the cube was cut");
        assert!(pieces.iter().filter(|p| p.member == 1).count() > 20, "{name}: next to nothing is left of it");
    }
}

#[test]
fn a_tree_stood_in_grass_is_whole_and_the_grass_gives_way_to_it() {
    use crate::tree::{Kind, Tree};
    let grass = Spec { form: Form::Tree, tree: Tree::of(Kind::Grass), points: vec![DVec3::new(-20.0, 0.0, 0.0), DVec3::ZERO, DVec3::new(20.0, 0.0, 0.0)], radius: 20.0, rise: Some(5.0), ..Spec::default() };
    let oak = Spec { form: Form::Tree, tree: Tree::of(Kind::Oak), points: vec![DVec3::ZERO], radius: 30.0, rise: Some(60.0), ..Spec::default() };
    // the oak moved into the lawn, its foot a little way into the soil; and the same with the lawn the one moved
    for (name, forms) in [("the oak joined", vec![(grass.clone(), [0.0; 3]), (oak.clone(), [0.0, 0.0, 1.0])]), ("the grass joined", vec![(oak.clone(), [0.0, 0.0, 1.0]), (grass.clone(), [0.0; 3])])] {
        let scene = Scene::new(&forms, 0.0);
        let tree = scene.specs.iter().position(|s| s.tree.kind == Kind::Oak).unwrap();
        let pieces = scene.pieces();
        assert!(shares_room(&scene, &pieces).is_none(), "{name}: two bricks share room");
        // every brick of the tree as it was built, the sloped ones of its foot among them
        let of_tree: Vec<&Piece> = pieces.iter().filter(|p| p.member == tree).collect();
        assert_eq!(of_tree.len(), scene.built[tree].bricks.len(), "{name}: the tree was cut");
        assert!(of_tree.iter().all(|p| p.from.is_some_and(|n| scene.built[tree].bricks[n] == p.brick)), "{name}: a brick of the tree is not its own");
        assert!(scene.built[tree].bricks.iter().any(|b| b.mask != crate::shapes::CUBE && b.min[2] < 6), "the tree has no sloped brick near its foot to lose");
        // and the grass less than it was, by what stood where the trunk is
        let lawn = pieces.iter().filter(|p| p.member != tree).count();
        assert!(lawn > 100 && pieces.iter().filter(|p| p.member != tree && p.from.is_some()).count() < scene.built[1 - tree].bricks.len(), "{name}: the grass gave up nothing");
    }
}
