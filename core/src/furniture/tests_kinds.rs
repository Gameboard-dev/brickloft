//! Tests of every kind of furniture but the bookcase, which has its own.

use super::*;
use crate::spec::{build, Form, Spec};
use std::collections::HashSet;

const STYLED: [Style; 5] = [Style::Modern, Style::Farmhouse, Style::Classical, Style::Rustic, Style::Industrial];

fn made(f: &Furniture, [w, d, h]: [i32; 3]) -> Model {
    Model::new(f, [(0.0, w as f64), (0.0, d as f64), (0.0, h as f64)], 80, true).unwrap_or_else(|e| panic!("{f:?} at {w} by {d} by {h}: {e}"))
}

fn high(b: &CellBrick) -> [i32; 3] {
    [0, 1, 2].map(|k| b.min[k] + b.ext[k])
}

/// Every kind but the bookcase.
fn kinds() -> impl Iterator<Item = Kind> {
    KINDS.into_iter().map(|k| k.0).filter(|k| *k != Kind::Bookcase)
}

/// Whether a kind has a plant fitted into it (see `pots`): the three that are not exact all through, and are slow.
fn fitted(kind: Kind) -> bool {
    matches!(kind, Kind::Potted | Kind::Basket | Kind::WindowBox)
}

/// A kind at its usual size, at two thirds of it, at twice it, and at a size out of proportion.
fn sizes(kind: Kind) -> [[i32; 3]; 4] {
    let usual = Furniture::usual_size(kind).map(|v| v as i32);
    [usual, usual.map(|v| (v * 2 / 3).max(3)), usual.map(|v| v * 2), [usual[0] + 7, (usual[1] / 2).max(4), usual[2] + 5]]
}

/// Every piece worth looking at: each kind, in each style, at four sizes,
/// from three seeds, with its settings as they start and with each turned on.
fn pieces() -> Vec<(Furniture, [i32; 3])> {
    let mut all = Vec::new();
    for kind in kinds() {
        for style in STYLED {
            // (a fitted plant at two sizes and from one seed: each is half a second's work)
            for (n, size) in sizes(kind).into_iter().enumerate().take(if fitted(kind) { 2 } else { 4 }) {
                for seed in 1..if fitted(kind) { 2 } else { 4 } {
                    let f = Furniture { style, seed: seed + n as u64 * 10, ..Furniture::of(kind) };
                    all.push((f, size));
                    all.push((Furniture { extra: !f.extra, count: seed as i32, choice: (seed as i32 + n as i32) % 6, second: (seed as i32) % 4, ..f }, size));
                }
            }
        }
    }
    all
}

/// Whether two bricks' boxes meet face to face: touching along one axis, and overlapping along both of the others.
fn touch(a: &CellBrick, b: &CellBrick) -> bool {
    (0..3).any(|k| (high(a)[k] == b.min[k] || high(b)[k] == a.min[k]) && (0..3).filter(|j| *j != k).all(|j| a.min[j] < high(b)[j] && b.min[j] < high(a)[j]))
}

#[test]
fn no_two_bricks_of_any_piece_share_room_and_none_leaves_its_box() {
    for (f, size) in pieces() {
        let model = made(&f, size);
        assert!(!model.bricks.is_empty() && model.bricks.len() == model.shades.len(), "{f:?} at {size:?}");
        let mut order: Vec<usize> = (0..model.bricks.len()).collect();
        order.sort_by_key(|i| model.bricks[*i].min[0]);
        for (n, i) in order.iter().enumerate() {
            let a = &model.bricks[*i];
            assert!((0..3).all(|k| a.ext[k] >= 1 && a.min[k] >= 0 && high(a)[k] <= size[k]), "{f:?} at {size:?}: {a:?} is outside the box");
            assert!(shape_info(a.mask).is_some() && model.shades[*i] < 32, "{f:?} at {size:?}: {a:?} is no brick, or has no colour");
            for j in order[n + 1..].iter().take_while(|j| model.bricks[**j].min[0] < high(a)[0]) {
                let b = &model.bricks[*j];
                assert!(!(0..3).all(|k| a.min[k] < high(b)[k] && b.min[k] < high(a)[k]), "{f:?} at {size:?}: {a:?} and {b:?} share room");
            }
        }
    }
}

#[test]
fn nothing_floats_every_part_is_joined_to_what_the_piece_stands_on_or_hangs_from() {
    for (f, size) in pieces() {
        let model = made(&f, size);
        // what holds it: the floor, or for a pendant and a chandelier the ceiling, and for a sconce the wall behind it
        let held = |b: &CellBrick| match f.kind {
            Kind::Pendant | Kind::Chandelier | Kind::Basket => high(b)[2] == size[2],
            Kind::Sconce => high(b)[1] == size[1],
            _ => b.min[2] == 0,
        };
        // from the bricks that are held, to every brick that touches one reached already
        // (Of a fitted plant, its container alone: the pot, its rim and hanger, and the soil. The plant is a shape fitted as any plant is, and its leaves are not boards.)
        let kept: Vec<CellBrick> = model.bricks.iter().zip(&model.shades).filter(|(_, shade)| !fitted(f.kind) || **shade <= super::pots::SOIL).map(|(b, _)| *b).collect();
        let model = Model { bricks: kept, ..model };
        let mut reached: Vec<bool> = model.bricks.iter().map(held).collect();
        let mut front: Vec<usize> = (0..reached.len()).filter(|i| reached[*i]).collect();
        assert!(!front.is_empty(), "{f:?} at {size:?}: nothing of it is on the floor");
        while let Some(i) = front.pop() {
            for j in 0..reached.len() {
                if !reached[j] && touch(&model.bricks[i], &model.bricks[j]) {
                    reached[j] = true;
                    front.push(j);
                }
            }
        }
        let loose: Vec<&CellBrick> = model.bricks.iter().zip(&reached).filter(|(_, r)| !**r).map(|(b, _)| b).take(3).collect();
        assert!(loose.is_empty(), "{f:?} at {size:?}: {} bricks hang in the air, such as {loose:?}", reached.iter().filter(|r| !**r).count());
    }
}

#[test]
fn the_same_style_and_seed_are_the_same_piece_and_other_seeds_other_pieces() {
    for kind in kinds() {
        let size = Furniture::usual_size(kind).map(|v| v as i32);
        let mut seen = HashSet::new();
        for style in STYLED {
            for seed in 0..if fitted(kind) { 2 } else { 12 } {
                let f = Furniture { style, seed, ..Furniture::of(kind) };
                let (a, b) = (made(&f, size), made(&f, size));
                assert_eq!((&a.bricks, &a.shades, &a.lamps), (&b.bricks, &b.shades, &b.lamps), "{f:?}");
                seen.insert(a.bricks.iter().map(|b| (b.mask, b.min, b.ext)).collect::<Vec<_>>());
            }
        }
        // (five styles and twelve seeds: at least three different pieces of any kind, and of most many more)
        assert!(seen.len() >= if fitted(kind) { 2 } else { 3 }, "{kind:?}: only {} different pieces", seen.len());
    }
    // a random style is one of the five, the same one for the same seed, and over a few seeds all five
    let picked: HashSet<&str> = (0..40).map(|seed| STYLES.iter().find(|s| s.0 == Style::Random.resolved(seed)).unwrap().1).collect();
    assert_eq!(picked.len(), 5, "{picked:?}");
    assert!(!picked.contains("random") && Style::Random.resolved(7) == Style::Random.resolved(7));
    let f = Furniture { style: Style::Random, seed: 7, ..Furniture::of(Kind::Dining) };
    assert_eq!(made(&f, [44, 24, 19]).bricks, made(&Furniture { style: Style::Random.resolved(7), ..f }, [44, 24, 19]).bricks);
}

#[test]
fn pieces_of_one_style_and_seed_are_a_set() {
    // what a seed picks, it picks without knowing the kind: the legs, the weight, the frame's stuff and the handles are the same on every piece
    for style in STYLED {
        for seed in 0..30 {
            let set = Set::of(style, seed);
            for kind in kinds() {
                let piece = Set::for_piece(&Furniture { style, seed, ..Furniture::of(kind) });
                assert_eq!((piece.leg, piece.heavy, piece.frame, piece.handle, piece.planks, piece.moulding), (set.leg, set.heavy, set.frame, set.handle, set.planks, set.moulding), "{kind:?} in {style:?}, seed {seed}");
            }
        }
    }
    // and it shows: a table with turned legs has chairs with turned legs. (A turned leg is the one with a block two cells narrower than the leg, which no other has.)
    let turned = |model: &Model, s: i32| model.bricks.iter().filter(|b| b.ext[0] == s - 2 && b.ext[1] == s - 2 && b.min[2] == 0).count();
    let seed = (0..50).find(|seed| Set::of(Style::Farmhouse, *seed).leg == style::Leg::Turned).unwrap();
    let f = Furniture { style: Style::Farmhouse, seed, ..Furniture::of(Kind::Dining) };
    assert_eq!((turned(&made(&f, [60, 36, 24]), 4), turned(&made(&Furniture { kind: Kind::Chair, ..f }, [24, 24, 46]), 3)), (4, 4));
    // a setting of the piece's own overrules the style for that piece alone
    assert_eq!(turned(&made(&Furniture { choice: 1, ..f }, [60, 36, 24]), 4), 0);
}

#[test]
fn every_setting_of_every_kind_changes_the_piece() {
    for kind in kinds() {
        // (large, so that every part has room to be there; but a fitted plant at its usual size and in two styles, for the time it takes)
        let size = Furniture::usual_size(kind).map(|v| v as i32 * if fitted(kind) { 1 } else { 2 });
        for setting in settings(kind).iter().filter(|s| !matches!(s.field, Field::Shine | Field::Cover) && !(Furniture::of(kind).planted() && s.field == Field::Extra)) {
            let mut seen = HashSet::new();
            for style in STYLED.into_iter().take(if fitted(kind) { 2 } else { 5 }) {
                for value in 0..=setting.most {
                    let mut f = Furniture { style, seed: 3, extra: false, ..Furniture::of(kind) };
                    // (a bench's back is what its other settings are settings of)
                    f.extra = kind == Kind::Bench && setting.field != Field::Extra;
                    match setting.field {
                        Field::Count => f.count = value,
                        Field::Choice => f.choice = value,
                        Field::Second => f.second = value,
                        _ => f.extra = value == 1,
                    }
                    seen.insert((style as usize, made(&f, size).bricks.iter().map(|b| (b.mask, b.min, b.ext)).collect::<Vec<_>>()));
                }
            }
            // in some style at least, every value but "as the style has it" gives a piece of its own
            let most = (0..5).map(|style| seen.iter().filter(|s| s.0 == style).count()).max().unwrap();
            assert!(most as i32 >= setting.most.max(2).min(setting.most + 1) - (setting.field == Field::Count) as i32, "{kind:?}, {}: {most} different pieces from {} values", setting.label, setting.most + 1);
        }
    }
}

#[test]
fn a_lamp_has_its_light_and_a_chandelier_one_to_a_candle() {
    for (f, size) in pieces() {
        let model = made(&f, size);
        // (a plant in a pot has other numbers: its fifth is a green)
        if f.planted() || f.pictured() {
            assert!(model.lamps.is_empty());
            continue;
        }
        let lit = matches!(f.kind, Kind::TableLamp | Kind::FloorLamp | Kind::Pendant | Kind::Chandelier | Kind::Sconce | Kind::Candlestick);
        assert_eq!(model.shades.contains(&LIGHT), lit, "{f:?} at {size:?}");
        assert_eq!(!model.lamps.is_empty(), lit, "{f:?} at {size:?}");
        assert!(model.lamps.len() <= MOST_LIGHTS && model.lamps.iter().all(|n| model.shades[*n] == LIGHT), "{f:?} at {size:?}: {:?}", model.lamps);
        // no two of its lights are the same brick, and but for a chandelier and a candlestick of three it has one
        let own: HashSet<&usize> = model.lamps.iter().collect();
        assert_eq!(own.len(), model.lamps.len());
        if matches!(f.kind, Kind::TableLamp | Kind::FloorLamp | Kind::Pendant | Kind::Sconce) {
            assert_eq!(model.lamps.len(), 1, "{f:?} at {size:?}");
        }
    }
    let chandelier = |second: i32| made(&Furniture { second, ..Furniture::of(Kind::Chandelier) }, [21, 21, 17]).lamps.len();
    assert_eq!([chandelier(1), chandelier(2), chandelier(3)], [2, 4, 8]);
    let candles = |second: i32| made(&Furniture { second, ..Furniture::of(Kind::Candlestick) }, [11, 5, 12]).lamps.len();
    assert_eq!([candles(1), candles(2)], [1, 3]);
}

#[test]
fn a_build_of_each_kind_is_exact_turned_and_flipped_too() {
    for (n, (f, [w, d, h])) in pieces().into_iter().enumerate().filter(|(n, _)| n % 7 == 0) {
        let middle = DVec3::new(3.5, -7.0, 20.0 + (n % 2) as f64 * 0.5);
        let spec = Spec { form: Form::Furniture, points: vec![middle], radius: d as f64 / 2.0, stretch: DVec3::new(w as f64 / d as f64, 1.0, h as f64 / d as f64), furniture: f, ..Spec::default() };
        let built = build(&spec).unwrap_or_else(|e| panic!("{f:?}: {e}"));
        let what = format!("{f:?} at {w} by {d} by {h}");
        if fitted(f.kind) {
            // a plant in a pot is exact in its pot and fitted in its plant: it says how far off it is, as any plant does, and it is the same turned
            assert!(built.error > 0.0 && built.error < 0.5 * built.solid && !built.unmeasured && built.shades.len() == built.bricks.len(), "{what}: {} of {}", built.error, built.solid);
            let turned = build(&Spec { turned: 3, flip: [true, false, true], ..spec.clone() }).unwrap();
            assert_eq!((turned.bricks.len(), turned.shades), (built.bricks.len(), built.shades), "{what}");
            continue;
        }
        assert_eq!((built.error, built.unmeasured, built.grain, built.shades.len()), (0.0, false, 1, built.bricks.len()), "{what}");
        assert_eq!(built.solid, built.bricks.iter().map(CellBrick::volume).sum::<f64>(), "{what}");
        for k in 0..3 {
            let (low, top) = (built.bricks.iter().map(|b| b.min[k]).min().unwrap(), built.bricks.iter().map(|b| high(b)[k]).max().unwrap());
            assert!(top - low <= [w, d, h][k], "{what}, axis {k}: {} across", top - low);
        }
        // measured sample by sample against the form, knowing nothing of how it was laid: no cell differs
        if w * d * h < 30_000 {
            let probe = crate::spec::Probe::new(&spec, 1).unwrap();
            let (volume, wrong) = crate::report::measure(&probe, &built, crate::par::Threads::available()).unwrap();
            assert!(wrong == 0.0 && (volume - built.solid).abs() < 2.0, "{what}: measured {volume} with {wrong} wrong, built {}", built.solid);
        }
        let turned = build(&Spec { turned: 3, flip: [true, false, true], grain: 4, ..spec.clone() }).unwrap();
        assert_eq!((turned.bricks.len(), turned.error, turned.shades), (built.bricks.len(), 0.0, built.shades), "{what}");
        assert!(turned.bricks.iter().all(|b| shape_info(b.mask).is_some()), "{what}");
    }
}

#[test]
fn hostile_sizes_and_settings_of_every_kind_are_built_or_refused_at_once() {
    let started = std::time::Instant::now();
    for kind in kinds() {
        for extreme in if fitted(kind) { vec![-1, 5, i32::MAX] } else { vec![i32::MIN, -1, 0, 1, 5, i32::MAX] } {
            let f = Furniture { seed: u64::MAX, count: extreme, choice: extreme, second: extreme, board: extreme, extra: extreme > 0, style: STYLED[(extreme.unsigned_abs() % 5) as usize], ..Furniture::of(kind) };
            // every small size, and sizes far out of proportion: something is built, inside the box
            for size in (1..9).flat_map(|w| (1..7).flat_map(move |d| [1, 2, 3, 5, 8, 13].map(move |h| [w, d, h]))).chain([[3, 300, 4], [400, 3, 9], [5, 5, 600], [900, 700, 30]]) {
                let model = made(&f, size);
                assert!(!model.bricks.is_empty() && model.bricks.iter().all(|b| (0..3).all(|k| b.min[k] >= 0 && high(b)[k] <= size[k])), "{f:?} at {size:?}");
                assert_eq!(model.bricks, made(&f.clamped(), size).bricks);
            }
        }
        // as large as a form may be, in bricks one cell long: refused, and at once, where it would be millions of bricks
        let huge = Model::new(&Furniture::of(kind), [(0.0, 2000.0), (0.0, 2000.0), (0.0, 2000.0)], 1, true);
        assert!(huge.err().is_some_and(|e| e.contains("too large")), "{kind:?}: built two thousand each way in single cells");
        // in bricks no longer than one cell at a sensible size it is built, of cells
        let cells = Model::new(&Furniture::of(kind), [(0.0, 12.0), (0.0, 9.0), (0.0, 14.0)], 1, true).unwrap();
        assert!(cells.bricks.iter().all(|b| b.ext == [1, 1, 1]), "{kind:?}");
        // and without sloped pieces, of boxes only
        let boxes = Model::new(&Furniture { style: Style::Classical, ..Furniture::of(kind) }, Furniture::usual_size(kind).map(|v| (0.0, v)), 80, false).unwrap();
        assert!(boxes.bricks.iter().all(|b| b.mask == CUBE), "{kind:?}");
    }
    assert!(started.elapsed().as_secs() < 60, "it took {:?}", started.elapsed());
}

#[test]
fn a_plant_stands_in_its_pot_and_keeps_out_of_it_and_a_trellis_shows_through_its_climber() {
    use super::pots::{PLANTS, SOIL, STEM};
    for (n, (plant, name)) in PLANTS.into_iter().enumerate() {
        let f = Furniture { second: n as i32 + 1, choice: n as i32 % 4 + 1, style: Style::Farmhouse, ..Furniture::of(Kind::Potted) };
        let model = made(&f, [26, 26, 46]);
        let (pot, grown): (Vec<_>, Vec<_>) = model.bricks.iter().zip(&model.shades).partition(|(_, shade)| **shade <= SOIL);
        // a pot of a few bricks, every one a box or a sloped piece laid by rule, with soil on top; and a plant of hundreds, small enough to stay a pot plant
        assert!((4..40).contains(&pot.len()) && pot.iter().any(|(_, s)| **s == SOIL), "{name}: a pot of {} bricks", pot.len());
        assert!((150..4000).contains(&grown.len()), "{name}: a plant of {} bricks", grown.len());
        // the plant is above the floor of the pot's soil, and reaches well up the box; it has green in it, and is not all one colour
        let soil = pot.iter().filter(|(_, s)| **s == SOIL).map(|(b, _)| b.min[2]).min().unwrap();
        let (low, top) = (grown.iter().map(|(b, _)| b.min[2]).min().unwrap(), grown.iter().map(|(b, _)| high(b)[2]).max().unwrap());
        assert!(top >= soil + 8 && low <= soil + 2, "{name} ({plant:?}): from {low} to {top}, soil at {soil}");
        let colours: HashSet<u8> = grown.iter().map(|(_, s)| **s).collect();
        assert!(colours.len() >= 2 && colours.iter().all(|s| *s >= STEM) && (colours.contains(&LEAF) || colours.contains(&LEAF_DARK)), "{name}: {colours:?}");
        // the same plant from the same seed, and another from another
        assert_eq!(made(&f, [26, 26, 46]).bricks, model.bricks, "{name}");
        assert!(made(&Furniture { seed: 9, ..f }, [26, 26, 46]).bricks != model.bricks, "{name}");
    }
    // a trellis: exact, its lattice the same whatever grows on it, and more of it covered the more coverage is asked for, but never all of it
    let trellis = |books: i32, choice: i32, second: i32| made(&Furniture { books, choice, second, seed: 4, ..Furniture::of(Kind::Trellis) }, [36, 10, 56]);
    for shape in 1..=3 {
        let bare = trellis(0, shape, 2);
        assert!(bare.shades.iter().all(|s| *s <= SOIL) && bare.error == 0.0, "lattice {shape}");
        let covered: Vec<usize> = [20, 60, 100].iter().map(|books| {
            let grown = trellis(*books, shape, 2);
            assert_eq!(grown.bricks[..bare.bricks.len()], bare.bricks[..], "lattice {shape}: the climber moved a slat");
            // (how many cells of the lattice's face have a leaf, a stem or a flower in front of them)
            grown.bricks.iter().zip(&grown.shades).filter(|(_, s)| **s >= STEM).flat_map(|(b, _)| (b.min[0]..high(b)[0]).flat_map(move |x| (b.min[2]..high(b)[2]).map(move |z| (x, z)))).collect::<HashSet<_>>().len()
        }).collect();
        // (the face is 34 across and some 45 up: 1,500 cells)
        assert!(covered[0] > 20 && covered[0] * 2 < covered[1] && covered[1] < covered[2] && covered[2] < 900, "lattice {shape}: {covered:?} cells covered");
    }
    // ivy has no flowers, a clematis one colour of them, a rose two; and a bare trellis is asked for by name
    let flowers = |second: i32| trellis(90, 1, second).shades.iter().filter(|s| **s >= FLOWER).collect::<HashSet<_>>().len();
    assert_eq!([flowers(1), flowers(2), flowers(3), flowers(5)], [0, 1, 2, 0]);
    assert_eq!(trellis(90, 1, 5).bricks, trellis(0, 1, 1).bricks);
}

/// The picture of a framed piece as its pixels: the colour number of each cell of its canvas, row by row from the foot, and where the canvas begins.
fn pixels(model: &Model, [w, d, h]: [i32; 3]) -> Vec<Vec<u8>> {
    let mut grid = vec![vec![255u8; w as usize]; h as usize];
    for (b, shade) in model.bricks.iter().zip(&model.shades).filter(|(b, s)| **s >= pictures::MOUNT && b.min[1] == d - 1 && b.ext[1] == 1) {
        for x in b.min[0]..high(b)[0] {
            for z in b.min[2]..high(b)[2] {
                assert_eq!(grid[z as usize][x as usize], 255, "two bricks of the picture at {x}, {z}");
                grid[z as usize][x as usize] = *shade;
            }
        }
    }
    // (cut down to the canvas: the rows and columns that have any of it)
    grid.into_iter().filter(|row| row.iter().any(|s| *s != 255)).map(|row| row.into_iter().filter(|s| *s != 255).collect()).collect()
}

#[test]
fn a_painting_has_its_subject_and_a_portrait_a_face() {
    let size = [48, 3, 36];
    let painting = |seed: u64, choice: i32, extra: bool, style: Style| made(&Furniture { seed, choice, extra, style, ..Furniture::of(Kind::Painting) }, size);
    for seed in 0..40 {
        // a sunset: sky at its top and water at its foot in every column, a sun, and a horizon that is one level line across the picture
        let sunset = pixels(&painting(seed, 1, false, Style::Modern), size);
        let (wide, tall) = (sunset[0].len(), sunset.len());
        assert!(wide == 46 && tall == 34 && sunset.iter().all(|row| row.len() == wide), "seed {seed}: {wide} by {tall}");
        assert!(sunset[tall - 1].iter().all(|s| (3..=10).contains(s)) && sunset[0].iter().all(|s| (12..=14).contains(s)), "seed {seed}");
        let horizon = (0..tall).find(|j| sunset[*j].iter().any(|s| !(12..=14).contains(s))).unwrap();
        assert!((8..20).contains(&horizon) && sunset[horizon..].iter().all(|row| row.iter().all(|s| *s != 13 && *s != 14)), "seed {seed}: the horizon at {horizon}");
        assert!(sunset.iter().flatten().filter(|s| **s == 11).count() >= 12 && sunset[horizon].contains(&11), "seed {seed}: no sun on the horizon");
        // a landscape: sky at its top, land or water at its foot, hills of all four ranges, and firs standing on the land
        let land = pixels(&painting(seed, 2, false, Style::Modern), size);
        assert!(land[tall - 1].iter().all(|s| (3..=7).contains(s)) && land[0].iter().all(|s| (8..=14).contains(s)), "seed {seed}");
        assert!((8..=11).all(|range| land.iter().flatten().any(|s| *s == range)) && land.iter().flatten().any(|s| *s == 12) && land.iter().flatten().any(|s| *s == 13), "seed {seed}");
        // an abstract: its hues and its ground and nothing else, and no one colour all over it
        let hues = pixels(&painting(seed, 3, false, Style::Modern), size);
        let used: HashSet<u8> = hues.iter().flatten().copied().collect();
        assert!(used.len() >= 3 && used.iter().all(|s| (3..=10).contains(s)), "seed {seed}: {used:?}");
        // the same seed is the same picture, the next another, and a subject left to the seed is one of the three
        assert_eq!(pixels(&painting(seed, 1, false, Style::Modern), size), sunset);
        assert!(pixels(&painting(seed + 1, 1, false, Style::Modern), size) != sunset && land != sunset && hues != land, "seed {seed}");
        let left = pixels(&painting(seed, 0, false, Style::Modern), size);
        assert!(left == [&sunset, &land, &hues][subject(&Furniture { seed, ..Furniture::of(Kind::Painting) })][..], "seed {seed}");
    }
    assert_eq!((0..40).map(|seed| subject(&Furniture { seed, ..Furniture::of(Kind::Painting) })).collect::<HashSet<_>>().len(), 3);
    // a mount is a border of its own colour all round the picture, which is smaller by it; and a classical frame has a sloped liner, in the liner's colour
    let mounted = pixels(&painting(3, 1, true, Style::Modern), size);
    assert!(mounted[0].iter().all(|s| *s == pictures::MOUNT) && mounted.iter().all(|row| row[0] == pictures::MOUNT && row[45] == pictures::MOUNT) && mounted[17][23] != pictures::MOUNT);
    let grand = painting(3, 1, true, Style::Classical);
    assert!(grand.bricks.iter().zip(&grand.shades).filter(|(b, s)| b.mask != CUBE && **s == pictures::LINER).count() == 8 && grand.shades.iter().filter(|s| **s == pictures::FRAME).count() == 4);
    // few bricks: a sunset 46 by 34 is 1,564 pixels
    assert!(painting(3, 1, false, Style::Modern).bricks.len() < 260, "{} bricks", painting(3, 1, false, Style::Modern).bricks.len());

    // A portrait: two pupils on one row, as far to each side of the middle as each other; a mouth under them, about the middle; skin round all three; and the whole face its own mirror image. At its usual size and at half of it.
    let mut sitters = HashSet::new();
    for size in [[34, 3, 44], [20, 3, 26]] {
        for seed in 0..60 {
            for wears in [0, 2, 4] {
                let face = pixels(&made(&Furniture { seed, choice: wears, extra: false, style: Style::Modern, ..Furniture::of(Kind::Portrait) }, size), size);
                let wide = face[0].len();
                let what = format!("seed {seed}, wearing {wears}, {wide} wide");
                assert!(face.iter().all(|row| row.iter().rev().eq(row.iter()) || row.iter().any(|s| (13..=17).contains(s))), "{what}: not its own mirror image where it has no parting");
                let pupils: Vec<(usize, usize)> = (0..face.len()).flat_map(|j| (0..wide).map(move |i| (i, j))).filter(|(i, j)| face[*j][*i] == 19).collect();
                assert!(pupils.len() == 2 && pupils[0].1 == pupils[1].1 && pupils[0].0 + pupils[1].0 == wide - 1 && pupils[1].0 - pupils[0].0 >= 2, "{what}: pupils at {pupils:?}");
                let lips: Vec<(usize, usize)> = (0..face.len()).flat_map(|j| (0..wide).map(move |i| (i, j))).filter(|(i, j)| face[*j][*i] == 20).collect();
                assert!(lips.len() >= 2 && lips.iter().all(|(i, j)| *j < pupils[0].1 && *i > pupils[0].0.saturating_sub(2) && *i < pupils[1].0 + 2), "{what}: a mouth at {lips:?}");
                // (the ends of the mouth are its highest cells: it smiles)
                let ends = (lips.iter().map(|l| l.0).min().unwrap(), lips.iter().map(|l| l.1).max().unwrap());
                assert!(lips.contains(&ends), "{what}: the mouth does not turn up: {lips:?}");
                let skin = |s: u8| (5..=12).contains(&s);
                assert!(skin(face[pupils[0].1][wide / 2]) || face[pupils[0].1][wide / 2] == 26, "{what}: no face between the eyes");
                assert!(face[0].iter().any(|s| (21..=25).contains(s)) && face[face.len() - 1].iter().all(|s| *s == 3 || (13..=17).contains(s) || *s == 28), "{what}: no shoulders at the foot, or no ground at the top");
                sitters.insert(face);
            }
        }
    }
    // (sixty seeds, three ways of dressing, two sizes: nearly every one a different sitter)
    assert!(sitters.len() > 300, "only {} different sitters", sitters.len());
}

/// Prints how many bricks each kind is at its usual size, style by
/// style, and how long it takes to build. It tests nothing: run it by
/// name with `--release -- --ignored --nocapture` for the figures in the docs.
#[test]
#[ignore]
fn how_many_bricks_each_kind_is() {
    for kind in kinds() {
        let size = Furniture::usual_size(kind);
        let counts: Vec<usize> = STYLED.iter().map(|style| Model::new(&Furniture { style: *style, ..Furniture::of(kind) }, size.map(|v| (0.0, v)), 80, true).unwrap().bricks.len()).collect();
        let spec = Spec { form: Form::Furniture, points: vec![DVec3::new(0.5, 0.5, size[2] / 2.0)], radius: size[1] / 2.0, stretch: DVec3::new(size[0] / size[1], 1.0, size[2] / size[1]), furniture: Furniture::of(kind), ..Spec::default() };
        let started = std::time::Instant::now();
        for _ in 0..200 {
            build(&spec).unwrap();
        }
        println!("{:?} {}x{}x{}: {counts:?} bricks in the five styles, {:.3} ms", kind, size[0], size[1], size[2], started.elapsed().as_secs_f64() * 5.0);
    }
}
