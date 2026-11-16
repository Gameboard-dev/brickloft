use super::*;
use crate::spec::{build, Form, Spec};
use std::collections::HashSet;

/// A bookcase `w` wide, `d` deep and `h` tall with its low corner at the
/// origin, in bricks up to 80 long and with sloped pieces.
fn made(f: &Furniture, w: i32, d: i32, h: i32) -> Model {
    Model::new(f, [(0.0, w as f64), (0.0, d as f64), (0.0, h as f64)], 80, true).unwrap_or_else(|e| panic!("{w} by {d} by {h}: {e}"))
}

fn plain() -> Furniture {
    Furniture { moulding: false, ..Furniture::of(Kind::Bookcase) }
}

fn high(b: &CellBrick) -> [i32; 3] {
    [0, 1, 2].map(|k| b.min[k] + b.ext[k])
}

/// Whether the boxes of two bricks have any room in common.
fn share_room(a: &CellBrick, b: &CellBrick) -> bool {
    (0..3).all(|k| a.min[k] < b.min[k] + b.ext[k] && b.min[k] < a.min[k] + a.ext[k])
}

/// Sizes and settings to try everything on: small, usual, wide, tall,
/// shallow, with and without each part.
fn cases() -> Vec<(Furniture, [i32; 3])> {
    let mut all = Vec::new();
    for (n, size) in [[36, 10, 60], [12, 5, 16], [20, 7, 20], [150, 12, 80], [9, 3, 90], [64, 2, 40], [33, 9, 47], [300, 14, 120]].into_iter().enumerate() {
        for (m, (board, spacing, bay, plinth)) in [(2, 11, 34, 3), (1, 6, 14, 0), (3, 16, 0, 6), (1, 3, 4, 1)].into_iter().enumerate() {
            for books in [100, 85, 40] {
                let seed = (n * 31 + m * 7 + books as usize) as u64;
                all.push((Furniture { kind: Kind::Bookcase, seed, board, shelves: 0, spacing, bay, plinth, back: (n + m) % 3 != 0, moulding: (n + m) % 2 == 0, books, ..crate::furniture::Furniture::of(crate::furniture::Kind::Bookcase) }, size));
            }
        }
    }
    all
}

#[test]
fn no_two_bricks_share_room_and_none_leaves_the_box() {
    for (f, [w, d, h]) in cases() {
        let model = made(&f, w, d, h);
        let what = format!("{f:?} at {w} by {d} by {h}");
        assert_eq!(model.bricks.len(), model.shades.len(), "{what}");
        // (sorted along x, so that each brick is compared only with those that begin before it ends)
        let mut order: Vec<usize> = (0..model.bricks.len()).collect();
        order.sort_by_key(|i| model.bricks[*i].min[0]);
        for (n, i) in order.iter().enumerate() {
            let a = &model.bricks[*i];
            assert!((0..3).all(|k| a.ext[k] >= 1 && a.min[k] >= 0 && high(a)[k] <= [w, d, h][k]), "{what}: {a:?} is outside the box");
            assert!(shape_info(a.mask).is_some(), "{what}: {a:?} is no brick");
            for j in order[n + 1..].iter().take_while(|j| model.bricks[**j].min[0] < high(a)[0]) {
                assert!(!share_room(a, &model.bricks[*j]), "{what}: {a:?} and {:?} share room", model.bricks[*j]);
            }
        }
    }
}

#[test]
fn every_book_stands_on_a_shelf_or_a_book_inside_its_opening() {
    let mut seen = (0, 0, 0);
    for (f, [w, d, h]) in cases() {
        let model = made(&f, w, d, h);
        let what = format!("{f:?} at {w} by {d} by {h}");
        // the top of everything a book may stand on: boards, and other books
        let tops: HashSet<[i32; 3]> = model.bricks.iter().zip(&model.shades).filter(|(b, shade)| **shade != BACK && b.mask == CUBE).flat_map(|(b, _)| (b.min[0]..high(b)[0]).flat_map(move |x| (b.min[1]..high(b)[1]).map(move |y| [x, y, high(b)[2]]))).collect();
        for (b, shade) in model.bricks.iter().zip(&model.shades).filter(|(_, shade)| **shade >= FIRST_BOOK) {
            assert!(*shade < FIRST_BOOK + BOOK_COLOURS, "{what}: {shade} is no book's colour");
            // inside one opening: between its sides, in front of its back, on or above its shelf and under the next
            let within = |o: &Opening| b.min[0] >= o.x.0 && high(b)[0] <= o.x.1 && b.min[1] >= o.y.0 && high(b)[1] <= o.y.1 && b.min[2] >= o.z && high(b)[2] <= o.z + o.clear;
            let opening = model.count.openings.iter().find(|o| within(o)).unwrap_or_else(|| panic!("{what}: the book {b:?} is in no opening"));
            // every cell of its foot is on something: nothing floats, and nothing overhangs what it lies on
            for x in b.min[0]..high(b)[0] {
                for y in b.min[1]..high(b)[1] {
                    assert!(tops.contains(&[x, y, b.min[2]]), "{what}: the book {b:?} has nothing under it at {x}, {y}");
                }
            }
            seen.0 += 1;
            seen.1 += (b.mask != CUBE) as usize;
            seen.2 += (b.min[2] > opening.z) as usize;
        }
        // (a brick may be a part of a book, but there are never more books than bricks of them)
        assert!(model.count.books <= model.bricks.iter().zip(&model.shades).filter(|(_, s)| **s >= FIRST_BOOK).count(), "{what}");
    }
    // and the cases had all of it to look at: books, leaning ones, and ones lying on others
    assert!(seen.0 > 5000 && seen.1 > 50 && seen.2 > 200, "{seen:?}");
}

#[test]
fn a_leaning_book_rests_against_a_book_at_least_as_tall() {
    let mut leaning = 0;
    for (f, [w, d, h]) in cases() {
        let model = made(&f, w, d, h);
        let books: Vec<&CellBrick> = model.bricks.iter().zip(&model.shades).filter(|(_, s)| **s >= FIRST_BOOK).map(|(b, _)| b).collect();
        // the ramp under a leaning book's overhang lacks one edge at its foot: on the low x side if it leans left, the high if right
        for b in books.iter().filter(|b| b.mask == mask_of(|x, _, z| !(x == 0 && z == 0)) || b.mask == mask_of(|x, _, z| !(x == 1 && z == 0))) {
            let left = b.mask == mask_of(|x, _, z| !(x == 0 && z == 0));
            let face = if left { b.min[0] } else { high(b)[0] };
            // an upright book whose side is the face it leans on, standing on the same shelf and reaching its top
            let against = books.iter().any(|o| o.mask == CUBE && o.min[2] == b.min[2] && high(o)[2] >= high(b)[2] && if left { high(o)[0] == face } else { o.min[0] == face });
            assert!(against, "{f:?} at {w} by {d} by {h}: the leaning book {b:?} leans on nothing");
            leaning += 1;
        }
    }
    assert!(leaning > 20, "only {leaning} leaning books were looked at");
}

#[test]
fn the_same_seed_is_the_same_bookcase_and_another_seed_another() {
    let f = Furniture::of(Kind::Bookcase);
    let (a, b) = (made(&f, 80, 10, 60), made(&f, 80, 10, 60));
    assert_eq!((&a.bricks, &a.shades, a.volume, &a.count), (&b.bricks, &b.shades, b.volume, &b.count));
    let other = made(&Furniture { seed: f.seed + 1, ..f }, 80, 10, 60);
    assert!(other.bricks != a.bricks, "another seed gave the same books");
    // the same carcass all the same: what the seed changes is the books
    let wood = |m: &Model| m.bricks.iter().zip(&m.shades).filter(|(_, s)| **s < FIRST_BOOK).map(|(b, _)| *b).collect::<Vec<_>>();
    assert_eq!(wood(&a), wood(&other));
    assert_eq!((a.count.shelves, a.count.bays), (other.count.shelves, other.count.bays));
    // a bookcase's books are in every colour books have
    let colours: HashSet<u8> = a.shades.iter().copied().filter(|s| *s >= FIRST_BOOK).collect();
    assert_eq!(colours.len(), BOOK_COLOURS as usize);
}

#[test]
fn no_two_books_side_by_side_are_one_colour() {
    let mut pairs = 0;
    for (f, [w, d, h]) in cases() {
        let model = made(&f, w, d, h);
        // whole books and ornaments (a leaning book's own pieces are one colour, and it has a ramp between its box and anything else)
        let mut books: Vec<(&CellBrick, u8)> = model.bricks.iter().zip(model.shades.iter().copied()).filter(|(b, s)| *s >= FIRST_BOOK && b.mask == CUBE).collect();
        books.sort_by_key(|(b, _)| b.min[0]);
        for (n, (a, shade)) in books.iter().enumerate() {
            // those that begin where this one ends, and face it: their heights and depths overlap
            for (b, other) in books[n + 1..].iter().take_while(|(b, _)| b.min[0] <= high(a)[0]).filter(|(b, _)| b.min[0] == high(a)[0] && (1..3).all(|k| a.min[k] < high(b)[k] && b.min[k] < high(a)[k])) {
                assert!(shade != other, "{f:?} at {w} by {d} by {h}: {a:?} and {b:?} are side by side and both colour {shade}");
                pairs += 1;
            }
        }
    }
    assert!(pairs > 5000, "only {pairs} pairs of neighbours were looked at");
}

#[test]
fn a_taller_bookcase_has_more_shelves_and_a_wider_one_more_bays_and_books() {
    let f = plain();
    let tall: Vec<Count> = [20, 40, 60, 90, 150].iter().map(|h| made(&f, 36, 10, *h).count).collect();
    assert_eq!(tall.iter().map(|c| c.shelves).collect::<Vec<_>>(), vec![0, 2, 3, 6, 10]);
    assert!(tall.iter().all(|c| c.bays == 1) && tall.windows(2).all(|p| p[0].books <= p[1].books) && tall[4].books > 5 * tall[0].books, "{:?}", tall.iter().map(|c| c.books).collect::<Vec<_>>());
    let wide: Vec<Count> = [24, 38, 39, 80, 200].iter().map(|w| made(&f, *w, 10, 60).count).collect();
    // (34 is the widest a bay may be: 38 wide is 34 inside, and a cell more is two bays)
    assert_eq!(wide.iter().map(|c| c.bays).collect::<Vec<_>>(), vec![1, 1, 2, 3, 6]);
    // (more books the wider it is, but for the cell at which a bay is divided in two and an upright takes the room of two books)
    assert!(wide.iter().all(|c| c.shelves == 3) && wide[0].books < wide[1].books && wide[2].books < wide[3].books && wide[3].books < wide[4].books && wide[4].books > 5 * wide[0].books, "{:?}", wide.iter().map(|c| (c.shelves, c.books)).collect::<Vec<_>>());
    // every opening is within a cell of the spacing asked for, at any height with room for two; and the bays are equal to a cell
    for h in 30..200 {
        let count = made(&f, 100, 10, h).count;
        let clears: Vec<i32> = count.openings.iter().map(|o| o.clear).collect();
        assert!(clears.iter().all(|c| (c - f.spacing).abs() <= 3), "{h} tall: openings {clears:?}");
        assert!(clears.iter().max().unwrap() - clears.iter().min().unwrap() <= 1, "{h} tall: openings {clears:?}");
        let widths: Vec<i32> = count.openings.iter().map(|o| o.x.1 - o.x.0).collect();
        assert!(widths.iter().max().unwrap() - widths.iter().min().unwrap() <= 1 && *widths.iter().max().unwrap() <= f.bay, "{h} tall: bays {widths:?}");
    }
    // a number of shelves asked for is the number there are, where there is room; and no uprights are none
    assert_eq!(made(&Furniture { shelves: 7, bay: 0, ..f }, 200, 10, 90).count, Count { shelves: 7, bays: 1, ..made(&Furniture { shelves: 7, bay: 0, ..f }, 200, 10, 90).count });
    assert_eq!(made(&Furniture { shelves: 60, ..f }, 36, 10, 30).count.shelves, 5, "more shelves than there is room for");
    // none at all with no books, and at full every cell of every shelf's width has a book standing on it
    assert_eq!(made(&Furniture { books: 0, ..f }, 80, 10, 60).count.books, 0);
    // how much of the shelves' width has something standing on it, over twenty bookcases
    let stood = |books: i32| {
        let (mut on, mut all) = (0, 0);
        for seed in 0..20 {
            let model = made(&Furniture { books, seed, ..f }, 80, 10, 60);
            for o in &model.count.openings {
                let cells: HashSet<i32> = model.bricks.iter().zip(&model.shades).filter(|(b, s)| **s >= FIRST_BOOK && b.min[2] == o.z && b.min[0] >= o.x.0 && high(b)[0] <= o.x.1).flat_map(|(b, _)| b.min[0]..high(b)[0]).collect();
                assert!(books < 100 || cells.len() as i32 == o.x.1 - o.x.0, "a full shelf has room left on it: {o:?}");
                (on, all) = (on + cells.len() as i32, all + o.x.1 - o.x.0);
            }
        }
        on as f64 / all as f64
    };
    assert_eq!(stood(100), 1.0);
    // and in between, about the share asked for
    for books in [20, 50, 80] {
        assert!((stood(books) - books as f64 / 100.0).abs() < 0.07, "asked to be {books}% full, the shelves are {:.0}% full", 100.0 * stood(books));
    }
}

#[test]
fn a_shelf_keeps_its_books_while_the_bookcase_grows_round_it() {
    // A cell taller, from a height at which the openings are all equal:
    // the cell goes to the lowest opening, and every shelf above it is
    // the same shelf a cell higher. (61 tall in boards of 2 on a plinth of
    // 3 is 54 inside: three shelves, and four openings of 12.)
    let f = plain();
    let (a, b) = (made(&f, 36, 10, 61), made(&f, 36, 10, 62));
    assert_eq!(a.count.shelves, b.count.shelves);
    let books_of = |m: &Model, level: usize| {
        let o = m.count.openings[level];
        let mut all: Vec<([i32; 3], [i32; 3], u8, u8)> = m.bricks.iter().zip(&m.shades).filter(|(b, s)| **s >= FIRST_BOOK && b.min[2] >= o.z && high(b)[2] <= o.z + o.clear).map(|(b, s)| ([b.min[0], b.min[1], b.min[2] - o.z], b.ext, b.mask, *s)).collect();
        all.sort();
        all
    };
    assert_eq!(a.count.openings[0].clear + 1, b.count.openings[0].clear);
    for level in 1..=a.count.shelves {
        assert!(!books_of(&a, level).is_empty());
        assert_eq!(books_of(&a, level), books_of(&b, level), "the books of shelf {level} changed");
    }
}

/// The cells a model's wood fills, each of them by one brick only, and
/// every brick of it a plain box.
fn wood_cells(model: &Model) -> HashSet<[i32; 3]> {
    let mut all = HashSet::new();
    for b in &model.bricks {
        assert_eq!(b.mask, CUBE, "{b:?} is not a plain box");
        for x in b.min[0]..high(b)[0] {
            for y in b.min[1]..high(b)[1] {
                for z in b.min[2]..high(b)[2] {
                    assert!(all.insert([x, y, z]), "two bricks share a cell");
                }
            }
        }
    }
    all
}

#[test]
fn the_carcass_is_exactly_its_boards_each_one_brick() {
    // Three carcasses worked out by hand, board by board, and written
    // here as which cells are wood: not by the code that lays them.
    let bare = Furniture { books: 0, moulding: false, ..Furniture::of(Kind::Bookcase) };
    type Wood = fn(i32, i32, i32) -> bool;
    let cases: [(Furniture, [i32; 3], usize, Wood); 3] = [
        // 20 by 6 by 30 in boards of 1, two shelves, a back, on the floor: 28 clear inside less 2 shelves is 26, shared 9, 9 and 8 from the bottom
        (Furniture { board: 1, shelves: 2, bay: 0, plinth: 0, back: true, ..bare }, [20, 6, 30], 7, |x, y, z| z == 29 || x == 0 || x == 19 || y == 5 || z == 0 || z == 10 || z == 20),
        // 41 by 8 by 24 in boards of 2, no back, a plinth of 2 with its kick board a cell back: 37 inside is three bays of 11, and 18 inside is two openings of 8
        (Furniture { board: 2, shelves: 0, spacing: 8, bay: 16, plinth: 2, back: false, ..bare }, [41, 8, 24], 10, |x, y, z| {
            let upright = (13..15).contains(&x) || (26..28).contains(&x);
            z >= 22 || !(2..39).contains(&x) || (2..4).contains(&z) || (z < 2 && (1..3).contains(&y)) || (z >= 4 && (upright || (12..14).contains(&z)))
        }),
        // 15 by 4 by 17 in boards of 1, a back, a plinth of 1: 14 inside is three openings of 4
        (Furniture { board: 1, shelves: 0, spacing: 4, bay: 0, plinth: 1, back: true, ..bare }, [15, 4, 17], 8, |x, y, z| z == 16 || x == 0 || x == 14 || y == 3 || z == 1 || z == 6 || z == 11 || (z == 0 && y == 1)),
    ];
    for (f, [w, d, h], boards, wood) in cases {
        let model = made(&f, w, d, h);
        let wanted: HashSet<[i32; 3]> = (0..w).flat_map(|x| (0..d).flat_map(move |y| (0..h).map(move |z| [x, y, z]))).filter(|c| wood(c[0], c[1], c[2])).collect();
        let got = wood_cells(&model);
        let (missing, extra): (Vec<_>, Vec<_>) = (wanted.difference(&got).take(5).collect(), got.difference(&wanted).take(5).collect());
        assert!(missing.is_empty() && extra.is_empty(), "{w} by {d} by {h}: missing {missing:?}, extra {extra:?}");
        assert_eq!(model.bricks.len(), boards, "{w} by {d} by {h}: {:?}", model.bricks);
        assert_eq!(model.volume, wanted.len() as f64);
    }
    // a board longer than a brick may be is cut into as few as that allows
    let long = Model::new(&Furniture { bay: 0, shelves: 1, ..bare }, [(0.0, 200.0), (0.0, 10.0), (0.0, 40.0)], 80, true).unwrap();
    assert!(long.bricks.iter().all(|b| b.ext.iter().all(|e| *e <= 80)));
    // (top, bottom and shelf in three lengths each, the back in three, and the two sides)
    assert_eq!(long.bricks.len(), 3 * 3 + 3 + 2 + 3);
    assert_eq!(wood_cells(&long).len() as f64, long.volume);
}

#[test]
fn a_build_of_it_is_exact_and_coloured_wherever_it_stands() {
    for (n, (f, [w, d, h])) in cases().into_iter().enumerate().filter(|(n, _)| n % 5 == 0) {
        // about a point on or off a cell's centre, an even size or an odd one
        let middle = DVec3::new(3.5, -7.0, 20.0 + (n % 2) as f64 * 0.5);
        let spec = Spec { form: Form::Furniture, points: vec![middle], radius: d as f64 / 2.0, stretch: DVec3::new(w as f64 / d as f64, 1.0, h as f64 / d as f64), furniture: f, ..Spec::default() };
        let built = build(&spec).unwrap_or_else(|e| panic!("{f:?}: {e}"));
        let what = format!("{f:?} at {w} by {d} by {h}");
        assert_eq!((built.error, built.unmeasured, built.grain), (0.0, false, 1), "{what}");
        assert_eq!(built.shades.len(), built.bricks.len(), "{what}");
        // the box asked for, to the cell
        let sides = crate::spec::cube_sides(&spec.snapped(), spec.snapped().points[0]);
        for k in 0..3 {
            let (low, top) = (built.bricks.iter().map(|b| b.min[k]).min().unwrap(), built.bricks.iter().map(|b| high(b)[k]).max().unwrap());
            assert_eq!((low, top, top - low), (sides[k].0 as i32, sides[k].1 as i32, [w, d, h][k]), "{what}, axis {k}");
        }
        assert_eq!(built.solid, built.bricks.iter().map(CellBrick::volume).sum::<f64>(), "{what}");
        // Measured against the form as anything else is, sample by sample
        // and knowing nothing of how it was laid: no cell differs. (The
        // small ones only: it is the slow way, which a build does not go.)
        if w * d * h < 60_000 {
            let probe = crate::spec::Probe::new(&spec, 1).unwrap();
            let (volume, wrong) = crate::report::measure(&probe, &built, crate::par::Threads::available()).unwrap();
            // (the volume to within what 64 samples of a cell can say of a corner piece, which is a third of its box)
            assert!(wrong == 0.0 && (volume - built.solid).abs() < 0.5, "{what}: measured {volume} with {wrong} wrong, built {}", built.solid);
        }
        // the brick size and the budget are not for furniture: it is the same at any
        let coarse = build(&Spec { grain: 4, budget: 10, ..spec.clone() }).unwrap();
        assert_eq!((coarse.bricks, coarse.grain), (built.bricks.clone(), 1), "{what}");
        // turned and flipped it is the same bricks, each still a brick
        let turned = build(&Spec { turned: 3, flip: [true, false, true], ..spec.clone() }).unwrap();
        assert_eq!((turned.bricks.len(), turned.error), (built.bricks.len(), 0.0), "{what}");
        assert!(turned.bricks.iter().all(|b| shape_info(b.mask).is_some()), "{what}");
    }
}

#[test]
fn mouldings_are_sloped_pieces_inside_the_box_and_square_bands_without_slopes() {
    let f = Furniture { moulding: true, books: 0, ..Furniture::of(Kind::Bookcase) };
    let (with, without) = (made(&f, 36, 10, 60), made(&Furniture { moulding: false, ..f }, 36, 10, 60));
    let sloped = |m: &Model| m.bricks.iter().filter(|b| b.mask != CUBE).count();
    // a strip along the front and each side, and a corner piece at each front corner: under the top, and round the foot
    assert_eq!((sloped(&with), sloped(&without)), (10, 0));
    // the cornice widens upwards to meet the overhang, so each piece of it has the whole of its top; and the skirting's chamfer the whole of its foot
    for b in with.bricks.iter().filter(|b| b.mask != CUBE) {
        let (top, foot) = (b.mask & 0xf0 == 0xf0, b.mask & 0x0f == 0x0f);
        assert!(if b.min[2] > 30 { top && !foot } else { foot && !top }, "{b:?} is the wrong way up");
        // and each stands out from the carcass by one cell, as half a board of 2 does
        assert!(b.min[0] == 0 || high(b)[0] == 36 || b.min[1] == 0, "{b:?} is not at the outside");
    }
    // the size is the size of the whole, so the carcass is narrower by the mouldings and its top overhangs
    let widest = |m: &Model, z: i32| m.bricks.iter().filter(|b| b.mask == CUBE && b.min[2] <= z && high(b)[2] > z).map(|b| high(b)[0]).max().unwrap() - m.bricks.iter().filter(|b| b.mask == CUBE && b.min[2] <= z && high(b)[2] > z).map(|b| b.min[0]).min().unwrap();
    assert_eq!((widest(&with, 30), widest(&with, 59), widest(&without, 30)), (34, 36, 36));
    // with no sloped pieces it is boxes only, in the same box; and so it is where a brick may not be as long as the moulding stands out
    let square = Model::new(&f, [(0.0, 36.0), (0.0, 10.0), (0.0, 60.0)], 80, false).unwrap();
    assert!(sloped(&square) == 0 && square.bricks.len() > without.bricks.len());
    let short = Model::new(&Furniture { board: 6, ..f }, [(0.0, 60.0), (0.0, 20.0), (0.0, 80.0)], 2, true).unwrap();
    assert!(sloped(&short) == 0 && short.bricks.iter().all(|b| b.ext.iter().all(|e| *e <= 2)));
    // too small for mouldings and a carcass inside them, it has none and is still built
    assert_eq!(sloped(&made(&f, 6, 2, 8)), 0);
}

#[test]
fn how_far_a_place_is_from_a_sloped_piece_has_the_sign_the_piece_has() {
    // every sloped piece that is laid: the two ramps of a book leaning each way, and the strips and corners of both mouldings
    let f = Furniture { moulding: true, books: 60, board: 4, ..Furniture::of(Kind::Bookcase) };
    let mut masks = HashSet::new();
    for seed in 0..30 {
        let model = made(&Furniture { seed, ..f }, 70, 12, 70);
        for b in model.bricks.iter().filter(|b| b.mask != CUBE) {
            masks.insert(b.mask);
            let info = shape_info(b.mask).unwrap();
            for n in 0..7 * 7 * 7 {
                // (at uneven places across the box, so that none is on a sloped face itself, where neither answer is wrong)
                let t = [(n % 7, 0.31), (n / 7 % 7, 0.47), (n / 49, 0.59)].map(|(i, off)| (i as f64 + off) / 7.0);
                let p = DVec3::new(b.min[0] as f64 + t[0] * b.ext[0] as f64, b.min[1] as f64 + t[1] * b.ext[1] as f64, b.min[2] as f64 + t[2] * b.ext[2] as f64);
                let inside = crate::shapes::inside_shape(info, t[0], t[1], t[2]);
                assert_eq!(brick_distance(b, p) < 0.0, inside, "{b:?} at {t:?}");
                // and of the whole: a place inside the piece is inside the furniture
                assert!(!inside || model.distance(p) < 0.0, "{b:?} at {t:?}");
            }
        }
        // well outside its box it is as far as from the box, and inside a board it is inside
        assert!((model.distance(DVec3::new(-5.0, 6.0, 30.0)) - 5.0).abs() < 1e-9);
        assert!(model.distance(DVec3::new(35.0, 6.0, 69.0)) < 0.0 && model.distance(DVec3::new(35.0, 1.5, 62.0)) > 0.0);
    }
    // The four ramps of books leaning each way, which are also the four
    // strips along the sides of the mouldings; the two strips along the
    // front; and the four corner pieces.
    assert_eq!(masks.len(), 10, "{masks:?}");
}

#[test]
fn hostile_sizes_and_settings_are_brought_within_bounds_or_refused_and_never_hang() {
    let started = std::time::Instant::now();
    // every setting at both ends of what a number can be
    for extreme in [i32::MIN, -1, 0, 1, i32::MAX] {
        let f = Furniture { kind: Kind::Bookcase, seed: u64::MAX, board: extreme, shelves: extreme, spacing: extreme, bay: extreme, plinth: extreme, back: true, moulding: true, books: extreme, ..crate::furniture::Furniture::of(crate::furniture::Kind::Bookcase) };
        let within = f.clamped();
        assert!((1..=MOST_BOARD).contains(&within.board) && (0..=MOST_SHELVES).contains(&within.shelves) && (LEAST_SPACING..=MOST_SPACING).contains(&within.spacing) && (within.bay == 0 || (LEAST_BAY..=MOST_BAY).contains(&within.bay)) && (0..=MOST_PLINTH).contains(&within.plinth) && (0..=100).contains(&within.books));
        // unclamped, as a program might hand it over: built as if clamped, at every size from a cell to a hall
        for size in [[1, 1, 1], [2, 2, 2], [3, 1, 3], [3, 3, 3], [5, 2, 4], [7, 300, 9], [400, 1, 400], [36, 10, 60]] {
            let model = made(&f, size[0], size[1], size[2]);
            assert_eq!(model.bricks, made(&within, size[0], size[1], size[2]).bricks);
            assert!(!model.bricks.is_empty() && model.bricks.iter().all(|b| (0..3).all(|k| b.min[k] >= 0 && high(b)[k] <= size[k])));
        }
    }
    // every small size there is, with everything asked for: something is built
    let all = Furniture { moulding: true, books: 100, ..Furniture::of(Kind::Bookcase) };
    for w in 1..12 {
        for d in 1..6 {
            for h in 1..14 {
                assert!(!made(&all, w, d, h).bricks.is_empty());
            }
        }
    }
    // as large as a form may be, at the closest spacing: refused, and at once
    let close = Furniture { spacing: LEAST_SPACING, bay: LEAST_BAY, board: 1, books: 100, ..all };
    let refused = Model::new(&close, [(0.0, 2000.0), (0.0, 2000.0), (0.0, 2000.0)], 80, true).err().expect("a wall of 400,000 openings was built");
    assert!(refused.contains("too large") && refused.contains("openings"), "{refused}");
    let crowded = Model::new(&Furniture { bay: 0, spacing: 20, ..close }, [(0.0, 2000.0), (0.0, 20.0), (0.0, 2000.0)], 80, true).err().expect("a hundred and ninety thousand books were built");
    assert!(crowded.contains("books"), "{crowded}");
    // sizes that are no sizes
    for bad in [f64::NAN, f64::INFINITY, 1e12] {
        assert!(Model::new(&all, [(0.0, bad), (0.0, 10.0), (0.0, 60.0)], 80, true).is_err());
    }
    // through a build: a radius or a stretch past what a form may be
    let spec = Spec { form: Form::Furniture, points: vec![DVec3::splat(0.5)], radius: 30.0, furniture: close, ..Spec::default() };
    for bad in [Spec { radius: 1e9, ..spec.clone() }, Spec { stretch: DVec3::new(1e9, 1.0, 1.0), ..spec.clone() }, Spec { radius: f64::NAN, ..spec.clone() }, Spec { points: vec![DVec3::new(f64::NAN, 0.0, 0.0)], ..spec.clone() }, Spec { points: Vec::new(), ..spec.clone() }, Spec { radius: 1000.0, ..spec.clone() }] {
        assert!(build(&bad).is_err(), "{:?} with radius {} was built", bad.stretch, bad.radius);
    }
    // the largest the editor's dots can drag it to, full, at the usual spacing, is built; and a brick one cell long at most still builds it
    let large = Model::new(&all, [(0.0, 800.0), (0.0, 20.0), (0.0, 800.0)], 80, true).unwrap();
    assert!(large.bricks.len() > 10_000 && large.bricks.len() < 80_000, "{} bricks", large.bricks.len());
    let single = Model::new(&all, [(0.0, 36.0), (0.0, 10.0), (0.0, 60.0)], 1, true).unwrap();
    assert!(single.bricks.len() > 5000 && single.bricks.iter().all(|b| b.ext == [1, 1, 1]));
    assert!(started.elapsed().as_secs() < 20, "it took {:?}", started.elapsed());
}

#[test]
fn a_bookcase_is_built_in_a_moment() {
    // (a guard against a slow way of laying it being put in, not a measurement: see docs/performance.md for those)
    let f = Furniture::of(Kind::Bookcase);
    let spec = |w: f64, d: f64, h: f64| Spec { form: Form::Furniture, points: vec![DVec3::new(0.5, 0.5, h / 2.0)], radius: d / 2.0, stretch: DVec3::new(w / d, 1.0, h / d), furniture: f, ..Spec::default() };
    let started = std::time::Instant::now();
    for _ in 0..20 {
        assert!(build(&spec(400.0, 12.0, 240.0)).unwrap().bricks.len() > 3000);
    }
    assert!(started.elapsed().as_millis() < 4000, "twenty library walls took {:?}", started.elapsed());
}

/// Prints how long bookcases of three sizes take to build and how many
/// bricks each is: the figures in docs/performance.md. It tests nothing,
/// so it is run by name, with `--release -- --ignored --nocapture`.
#[test]
#[ignore]
fn how_long_a_bookcase_takes() {
    let f = Furniture::of(Kind::Bookcase);
    for (name, [w, d, h]) in [("a bedside shelf", [20.0, 8.0, 20.0]), ("the usual bookcase", Furniture::usual_size(Kind::Bookcase)), ("a library wall", [400.0, 12.0, 240.0]), ("the largest the dots drag out", [800.0, 20.0, 800.0])] {
        let spec = Spec { form: Form::Furniture, points: vec![DVec3::new(0.5, 0.5, h / 2.0)], radius: d / 2.0, stretch: DVec3::new(w / d, 1.0, h / d), furniture: f, ..Spec::default() };
        let rounds = 200;
        let started = std::time::Instant::now();
        let mut built = build(&spec).unwrap();
        for _ in 1..rounds {
            built = build(&spec).unwrap();
        }
        let each = started.elapsed().as_secs_f64() * 1000.0 / rounds as f64;
        let count = Model::new(&f, crate::spec::cube_sides(&spec.snapped(), spec.points[0]), 80, true).unwrap().count;
        println!("{name}, {w} by {d} by {h}: {} bricks ({} shelves in each of {} bays, {} books and ornaments), {each:.3} ms, discrepancy {}", built.bricks.len(), count.shelves, count.bays, count.books, built.error);
    }
}
