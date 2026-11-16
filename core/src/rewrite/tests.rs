//! Tests of the interpreter: the language's own behaviour on small
//! programs, the programs that come from the original, the bricks, and a
//! program or a run to provoke each [`Fault`].

use super::fault::{Expected, Thing};
use super::*;

const NONE: Resources = Resources { roots: Vec::new(), bundled: &[] };

fn load(text: &str, size: [usize; 3]) -> Result<Machine, Fault> {
    Machine::load("test.xml", text.as_bytes(), size, &NONE, &Limits::USUAL)
}

/// A program run to its end from a seed: the letter in each cell.
fn run(text: &str, size: [usize; 3], seed: u64) -> String {
    let mut machine = load(text, size).unwrap_or_else(|fault| panic!("{fault}"));
    machine.start(seed);
    machine.finish().unwrap_or_else(|fault| panic!("{fault}"));
    let view = machine.view();
    view.cells.iter().map(|v| view.letters[*v as usize]).collect()
}

/// Files held in memory, for programs that refer to others. (Leaked: a test's few bytes, to give them the lifetime compiled-in files have.)
fn bundled(files: Vec<(&'static str, Vec<u8>)>) -> Resources {
    let table: Vec<(&'static str, &'static [u8])> = files.into_iter().map(|(name, bytes)| (name, &*Box::leak(bytes.into_boxed_slice()))).collect();
    Resources { roots: Vec::new(), bundled: Box::leak(table.into_boxed_slice()) }
}

/// The programs and tiles copied from the original, beside this crate.
fn upstream() -> (std::path::PathBuf, Resources) {
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("models/markov");
    let home = if home.is_dir() { home } else { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/models/markov") };
    (home.clone(), Resources { roots: vec![home], bundled: &[] })
}

#[test]
fn the_reader_keeps_names_attributes_children_and_lines_and_skips_the_rest() {
    let text = "\u{feff}<?xml version=\"1.0\"?>\n<!-- a note -->\n<sequence values=\"BW\" note='a &lt; b &amp; &#65;'>\n  stray text\n  <one in=\"B\" out=\"W\"/> z\n  <all>\n    <rule in=\"W\" out=\"B\"/>\n  </all>\n</sequence>\n";
    let root = xml::read("t.xml", text.as_bytes(), &Limits::USUAL).unwrap();
    assert_eq!((root.name.as_str(), root.line, root.get("values"), root.get("note")), ("sequence", 3, Some("BW"), Some("a < b & A")));
    assert_eq!(root.children.iter().map(|c| (c.name.as_str(), c.line)).collect::<Vec<_>>(), [("one", 5), ("all", 6)]);
    assert_eq!(root.children[1].children[0].get("in"), Some("W"));
}

#[test]
fn a_vox_file_written_is_read_back_and_a_damaged_one_is_refused() {
    let bytes = vox::write([3, 2, 4], |x, y, z| if (x + y + z) % 2 == 0 { (1 + x + 3 * y) as u8 } else { 0 });
    let read = vox::read("a.vox", &bytes, &Limits::USUAL).unwrap();
    assert_eq!(read.size, [3, 2, 4]);
    assert_eq!((read.cells[0], read.cells[1], read.cells[2 + 3 + 6]), (1, -1, 6));
    // cut short, a cell outside the size given, and not a .vox file at all
    for damaged in [bytes[..bytes.len() - 3].to_vec(), { let mut b = bytes.clone(); b[60] = 200; b }, b"<sequence/>".to_vec()] {
        assert_eq!(vox::read("a.vox", &damaged, &Limits::USUAL), Err(Fault::NotVox { file: "a.vox".into() }));
    }
}

#[test]
fn one_rule_applied_until_nothing_matches_fills_the_grid_a_cell_a_turn() {
    let mut machine = load("<one values=\"BW\" in=\"B\" out=\"W\"/>", [4, 3, 1]).unwrap();
    machine.start(7);
    let mut turns = 0;
    while machine.step().unwrap() {
        turns += 1;
        assert_eq!(machine.view().cells.iter().filter(|v| **v == 1).count(), turns.min(12));
    }
    // twelve cells, one turn each; then a turn that finds nothing, and the turns in which the program ends
    assert!(machine.view().cells.iter().all(|v| *v == 1) && (12..=14).contains(&machine.steps()), "{} turns", machine.steps());
    assert!(!machine.running());
}

#[test]
fn a_seed_gives_one_result_every_time_and_another_seed_another() {
    // a maze: a white cell reaches two cells over black ones
    let maze = "<one values=\"BWA\" origin=\"True\" in=\"WBB\" out=\"WAW\"/>";
    let (a, b, c) = (run(maze, [15, 15, 1], 3), run(maze, [15, 15, 1], 3), run(maze, [15, 15, 1], 4));
    assert_eq!(a, b);
    assert_ne!(a, c);
    // every other cell of every other row is reached, and none of the cells between rows of both
    let at = |x: usize, y: usize| a.as_bytes()[x + y * 15] as char;
    assert!((1..15).step_by(2).all(|y| (1..15).step_by(2).all(|x| at(x, y) == 'W')), "{a}");
    assert!((0..15).step_by(2).all(|y| (0..15).step_by(2).all(|x| at(x, y) == 'B')), "{a}");
}

#[test]
fn rules_are_copied_by_the_symmetries_asked_for() {
    // one white cell at the middle grows a line: every way, along x only, or one way only
    let grow = |symmetry: &str| run(&format!("<one values=\"BW\" origin=\"True\" in=\"WB\" out=\"WW\" {symmetry}/>"), [5, 5, 1], 1);
    assert!(grow("").chars().all(|c| c == 'W'));
    assert_eq!(grow("symmetry=\"(x)\""), "BBBBBBBBBBWWWWWBBBBBBBBBB");
    assert_eq!(grow("symmetry=\"()\""), "BBBBBBBBBBBBWWWBBBBBBBBBB");
    // in space a rule along x has six copies, and the middle of a cube grows to fill it
    assert!(run("<one values=\"BW\" origin=\"True\" in=\"WB\" out=\"WW\"/>", [3, 3, 3], 1).chars().all(|c| c == 'W'));
    // a pattern's layers are written top first: white on top of black, turned no way, is found only that way up
    assert_eq!(run("<sequence values=\"BWR\"><prl in=\"B\" out=\"W\" steps=\"1\"/><prl in=\"W *\" out=\"R *\" symmetry=\"()\"/></sequence>", [1, 1, 3], 1), "WRR");
}

#[test]
fn all_writes_every_match_that_does_not_overlap_and_prl_every_match() {
    // pairs of black cells in a row of five: `all` can turn two pairs at most in a turn, and leaves the odd cell
    let all = run("<all values=\"BW\" in=\"BB\" out=\"WW\" steps=\"1\" symmetry=\"()\"/>", [5, 1, 1], 2);
    assert_eq!(all.matches('W').count(), 4, "{all}");
    // `prl` writes all four overlapping matches
    assert_eq!(run("<prl values=\"BW\" in=\"BB\" out=\"WW\" steps=\"1\" symmetry=\"()\"/>", [5, 1, 1], 2), "WWWWW");
    // and with a chance of nothing, writes none
    assert_eq!(run("<prl values=\"BW\" in=\"B\" out=\"W\" p=\"0\" steps=\"3\"/>", [5, 1, 1], 2), "BBBBB");
}

#[test]
fn a_sequence_runs_each_child_out_and_a_markov_goes_back_to_the_first() {
    // sequence: all cells red, then one made white, and no more since the first child is not gone back to
    assert_eq!(run("<sequence values=\"BRW\"><one in=\"B\" out=\"R\"/><one in=\"R\" out=\"W\" steps=\"1\"/><one in=\"W\" out=\"B\" steps=\"1\"/></sequence>", [4, 1, 1], 1).matches('R').count(), 3);
    // markov: the first child that can act does, so white always goes back to black before another red can turn
    let text = "<markov values=\"BRW\"><one in=\"W\" out=\"B\"/><one in=\"R\" out=\"W\"/></markov>";
    let mut machine = load(&text.replace("values=\"BRW\"", "values=\"RBW\""), [4, 1, 1]).unwrap();
    machine.start(1);
    while machine.step().unwrap() {
        assert!(machine.view().cells.iter().filter(|v| **v == 2).count() <= 1);
    }
    assert!(machine.view().cells.iter().all(|v| *v == 1));
    // unions, and elements that are not nodes, which are passed over
    assert_eq!(run("<sequence values=\"BRW\"><union symbol=\"?\" values=\"BR\"/><altone in=\"B\" out=\"R\"/><one in=\"B\" out=\"R\" steps=\"2\"/><prl in=\"?\" out=\"W\"/></sequence>", [4, 1, 1], 1), "WWWW");
}

#[test]
fn a_path_is_drawn_and_a_convolution_counts_neighbours() {
    // from the red cell to the white one, through black, in green: the shortest way is the row between them
    let ends = "<prl in=\"B****\" out=\"R****\" symmetry=\"()\" steps=\"1\"/><prl in=\"****B\" out=\"****W\" symmetry=\"()\" steps=\"1\"/>";
    assert_eq!(run(&format!("<sequence values=\"BRWG\">{ends}<path from=\"R\" to=\"W\" on=\"B\" color=\"G\"/></sequence>"), [5, 1, 1], 1), "RGGGW");
    // and round a wall: three rows, the middle of the middle row walled off in a value the path may not cross
    let walled = run(&format!("<sequence values=\"BRWGA\"><prl in=\"*****/**B**/*****\" out=\"*****/**A**/*****\" symmetry=\"()\" steps=\"1\"/><prl in=\"*****/B****/*****\" out=\"*****/R****/*****\" symmetry=\"()\" steps=\"1\"/><prl in=\"*****/****B/*****\" out=\"*****/****W/*****\" symmetry=\"()\" steps=\"1\"/><path from=\"R\" to=\"W\" on=\"B\" color=\"G\"/></sequence>"), [5, 3, 1], 1);
    // (a path node draws again each turn while there is still a way through, as the original's does: here a second way round the other side)
    assert!(walled.matches('G').count() >= 5 && &walled[5..10] == "RGAGW", "{walled}");
    // a cell with exactly two white face neighbours turns white: the middle of W B W
    assert_eq!(run("<sequence values=\"BW\"><prl in=\"B*B\" out=\"W*W\" steps=\"1\" symmetry=\"()\"/><convolution neighborhood=\"VonNeumann\" steps=\"1\"><rule in=\"B\" out=\"W\" sum=\"2\" values=\"W\"/></convolution></sequence>", [3, 1, 1], 1), "WWW");
    // a map doubles the grid: each white cell becomes two by two
    assert_eq!(run("<sequence values=\"BW\"><one in=\"B\" out=\"W\" steps=\"1\"/><map scale=\"2 2 1\" values=\"BR\"><rule in=\"W\" out=\"RR/RR\"/></map></sequence>", [1, 1, 1], 1), "RRRR");
}

#[test]
fn a_field_leads_a_rule_towards_its_goal() {
    // the red cell walks over black to the white one by the shortest way, where chance alone would wander: seven turns for seven cells
    let text = "<sequence values=\"BRW\"><prl in=\"B*******\" out=\"R*******\" symmetry=\"()\" steps=\"1\"/><prl in=\"*******B\" out=\"*******W\" symmetry=\"()\" steps=\"1\"/><one in=\"RB\" out=\"BR\"><field for=\"R\" to=\"W\" on=\"B\" recompute=\"True\"/></one></sequence>";
    let mut machine = load(text, [8, 1, 1]).unwrap();
    machine.start(5);
    machine.finish().unwrap();
    assert_eq!(machine.view().cells, [0, 0, 0, 0, 0, 0, 1, 2]);
    assert!(machine.steps() <= 14, "{} turns", machine.steps());
}

/// Two tiles one cell each, which may only alternate along x and y and repeat upwards.
fn chess() -> Resources {
    let set = "<tileset><tiles><tile name=\"a\"/><tile name=\"b\"/></tiles><neighbors><neighbor left=\"a\" right=\"b\"/><neighbor top=\"a\" bottom=\"a\"/><neighbor top=\"b\" bottom=\"b\"/></neighbors></tileset>";
    bundled(vec![("tilesets/chess.xml", set.as_bytes().to_vec()), ("tilesets/chess/a.vox", vox::write([1, 1, 1], |_, _, _| 1)), ("tilesets/chess/b.vox", vox::write([1, 1, 1], |_, _, _| 2))])
}

#[test]
fn wave_function_collapse_lays_tiles_only_as_the_tileset_allows() {
    let resources = chess();
    let mut machine = Machine::load("t.xml", b"<wfc values=\"RG\" tileset=\"chess\"/>" as &[u8], [1, 1, 1], &resources, &Limits::USUAL);
    assert!(machine.is_ok());
    machine = Machine::load("t.xml", b"<sequence values=\"B\"><wfc values=\"RG\" tileset=\"chess\"/></sequence>" as &[u8], [6, 5, 2], &resources, &Limits::USUAL);
    let mut machine = machine.unwrap_or_else(|fault| panic!("{fault}"));
    machine.start(9);
    machine.finish().unwrap();
    let view = machine.view();
    assert_eq!(view.size, [6, 5, 2]);
    let first = view.cells[0];
    for (i, v) in view.cells.iter().enumerate() {
        assert_eq!(*v, (first as usize + i % 6 + i / 6 % 5) as u8 % 2, "cell {i}");
    }
    // a cell fixed beforehand is kept: the rule lets a white cell be only tile b
    let fixed = b"<sequence values=\"BW\"><one in=\"B\" out=\"W\" steps=\"1\"/><wfc values=\"RG\" tileset=\"chess\"><rule in=\"W\" out=\"b\"/></wfc></sequence>" as &[u8];
    let mut machine = Machine::load("t.xml", fixed, [4, 4, 1], &resources, &Limits::USUAL).unwrap();
    machine.start(2);
    machine.step().unwrap();
    let white = machine.view().cells.iter().position(|v| *v == 1).unwrap();
    machine.finish().unwrap();
    assert_eq!(machine.view().cells[white], 1);
}

#[test]
fn the_logarithm_and_power_done_by_arithmetic_agree_with_the_library() {
    for x in [1e-9, 0.001, 0.3, 0.9999, 1.0, 1.5, 2.0, 10.0, 12345.678, 1e12] {
        assert!((nodes::ln(x) - x.ln()).abs() <= 1e-12 * x.ln().abs().max(1.0), "ln {x}: {} against {}", nodes::ln(x), x.ln());
    }
    for x in [-700.0, -20.0, -1.0, -1e-9, 0.0, 0.5, 1.0, 3.3, 50.0, 700.0] {
        assert!((nodes::exp(x) / x.exp() - 1.0).abs() <= 1e-12, "exp {x}: {} against {}", nodes::exp(x), x.exp());
    }
}

#[test]
fn the_original_programs_load_run_to_the_end_and_repeat() {
    let (home, resources) = upstream();
    for (name, size) in [("Apartemazements", [4, 4, 4]), ("PillarsOfEternity", [4, 4, 4]), ("Escher", [4, 4, 4]), ("StrangeDungeon", [15, 15, 15]), ("Hills", [40, 40, 12])] {
        let text = std::fs::read(home.join(format!("models/{name}.xml"))).unwrap();
        let cells = |seed: u64| {
            let mut machine = Machine::load(name, &text, size, &resources, &Limits::USUAL).unwrap_or_else(|fault| panic!("{name}: {fault}"));
            machine.start(seed);
            machine.finish().unwrap_or_else(|fault| panic!("{name}: {fault}"));
            let view = machine.view();
            (view.size, view.cells.to_vec())
        };
        let (first, again, other) = (cells(1), cells(1), cells(2));
        assert!(first == again, "{name}: one seed gave two results");
        assert!(first.1 != other.1, "{name}: two seeds gave one result");
        assert!(first.1.iter().any(|v| *v != 0), "{name} made nothing");
    }
    // a block of flats: tiles five cells across, so four tiles are twenty cells; and it has floors, columns and windows
    let text = std::fs::read(home.join("models/Apartemazements.xml")).unwrap();
    let mut machine = Machine::load("Apartemazements.xml", &text, [4, 4, 4], &resources, &Limits::USUAL).unwrap();
    machine.start(3);
    machine.finish().unwrap();
    let view = machine.view();
    assert_eq!(view.size, [20, 20, 20]);
    // what the rules after the wfc node drew is still there when the grid is looked at: grass on the ground, and no value the last rules erase
    let count = |letter: char| view.cells.iter().filter(|v| view.letters[**v as usize] == letter).count();
    assert!(count('E') > 100 && count('A') > 10 && count('F') + count('D') + count('P') == 0, "{} grass, {} concrete", count('E'), count('A'));
}

#[test]
fn a_plain_finish_is_the_cells_joined_and_wedges_fill_only_flights_of_steps() {
    let limits = Limits::USUAL;
    // a flight of four steps along x, three cells wide, with a wall at its top; and a single slab lying on the floor apart from it
    let size = [8usize, 9, 8];
    let solid = |x: usize, y: usize, z: usize| (y < 3 && x < 5 && z <= x) || (y < 3 && x == 5) || (y >= 6 && x < 3 && z < 2);
    let cells: Vec<u8> = (0..size[0] * size[1] * size[2]).map(|i| solid(i % 8, i / 8 % 9, i / 72) as u8).collect();
    let letters = ['B', 'W'];
    let view = View { size, cells: &cells, letters: &letters };
    let plain = bricks(&view, 2, Finish::Plain, 80, &limits).unwrap();
    let wedged = bricks(&view, 2, Finish::Wedged, 80, &limits).unwrap();
    assert!(plain.bricks.iter().all(|b| b.mask == crate::shapes::CUBE) && plain.wedges == 0);
    let volume = |made: &Bricks| made.bricks.iter().map(|b| b.volume()).sum::<f64>();
    assert_eq!(volume(&plain), 8.0 * cells.iter().filter(|v| **v == 1).count() as f64);
    // the flight has four notches (above steps 0 to 3), each a ramp three cells long: one brick each once joined, and half a cell of volume a cell of length
    let ramps: Vec<_> = wedged.bricks.iter().filter(|b| b.mask != crate::shapes::CUBE).collect();
    assert_eq!((ramps.len(), wedged.wedges), (4, 4), "{ramps:?}");
    assert!(ramps.iter().all(|b| b.ext == [2, 6, 2] && crate::shapes::shape_info(b.mask).is_some_and(|s| s.kind == crate::shapes::Kind::Ramp)));
    assert_eq!(volume(&wedged) - volume(&plain), 4.0 * 3.0 * 4.0);
    // nothing beside the slab or the wall: a single step is not a flight
    assert!(ramps.iter().all(|b| b.min[1] < 6 && b.min[0] < 10));
    // A slab one cell high lying on a floor has a step all round it, and no flight: it stays square.
    let floor: Vec<u8> = (0..6 * 6 * 4).map(|i| { let (x, y, z) = (i % 6, i / 6 % 6, i / 36); (z == 0 || (z == 1 && (2..4).contains(&x) && (2..4).contains(&y))) as u8 }).collect();
    assert_eq!(bricks(&View { size: [6, 6, 4], cells: &floor, letters: &letters }, 1, Finish::Wedged, 80, &limits).unwrap().wedges, 0);
    // no two bricks share a cell
    let mut held = std::collections::HashSet::new();
    for b in &wedged.bricks {
        for x in 0..b.ext[0] {
            for y in 0..b.ext[1] {
                for z in 0..b.ext[2] {
                    assert!(held.insert((b.min[0] + x, b.min[1] + y, b.min[2] + z)), "two bricks at {:?}", b.min);
                }
            }
        }
    }
}

#[test]
fn stepped_pyramids_get_corner_pieces_and_a_flight_is_closed_underneath() {
    let limits = Limits::USUAL;
    // a pyramid of three steps: its edges are flights, its four corners hips
    let size = [7usize, 7, 4];
    let cells: Vec<u8> = (0..7 * 7 * 4).map(|i| { let (x, y, z) = ((i % 7) as i32, (i / 7 % 7) as i32, (i / 49) as i32); ((x - 3).abs().max((y - 3).abs()) <= 3 - z) as u8 }).collect();
    let view = View { size, cells: &cells, letters: &['B', 'W'] };
    let made = bricks(&view, 1, Finish::Wedged, 80, &limits).unwrap();
    let kinds = |kind| made.bricks.iter().filter(|b| crate::shapes::shape_info(b.mask).is_some_and(|s| s.kind == kind)).count();
    assert_eq!((kinds(crate::shapes::Kind::Hip), kinds(crate::shapes::Kind::Ramp)), (12, 12), "three levels of four corners and four sides");
    // the same pit upside down, hollowed: a valley piece in each inside corner
    let pit: Vec<u8> = cells.iter().map(|v| 1 - v).collect();
    let made = bricks(&View { size, cells: &pit, letters: &['B', 'W'] }, 1, Finish::Wedged, 80, &limits).unwrap();
    assert!(made.bricks.iter().filter(|b| crate::shapes::shape_info(b.mask).is_some_and(|s| s.kind == crate::shapes::Kind::Inner)).count() >= 4, "{}", made.wedges);
    // a thin flight in the air, one cell thick: ramps above it and below it
    let thin: Vec<u8> = (0..6 * 1 * 6).map(|i| ((i % 6) == (i / 6)) as u8).collect();
    let made = bricks(&View { size: [6, 1, 6], cells: &thin, letters: &['B', 'W'] }, 1, Finish::Wedged, 80, &limits).unwrap();
    assert_eq!(made.wedges, 10, "five notches above and five below");
}

#[test]
fn every_fault_is_provoked_and_says_what_to_do() {
    let usual = Limits::USUAL;
    let fails = |text: &str, size: [usize; 3]| load(text, size).err().unwrap_or_else(|| panic!("loaded: {text}"));
    let said = |fault: &Fault, words: &[&str]| {
        let text = fault.to_string();
        assert!(words.iter().all(|w| text.contains(w)), "{fault:?} says: {text}");
        // (and the same words as plain text, at the boundary with the rest of the core)
        assert_eq!(String::from(fault.clone()), text);
    };
    // the file is not XML, each way a reader can find that out
    for (text, line, expected) in [("", 1, Expected::Element), ("<one", 1, Expected::EndOfTag), ("<one in>", 1, Expected::Equals), ("<one in=B>", 1, Expected::Quote), ("<one>\n<all>\n</one>", 3, Expected::CloseOf), ("<one>\n<!-- never ends", 2, Expected::EndOfComment), ("<one/><one/>", 1, Expected::OneRoot), ("<one>\n< />", 2, Expected::Name)] {
        let fault = fails(text, [2, 2, 1]);
        assert_eq!(fault, Fault::Syntax { file: "test.xml".into(), line, expected }, "{text}");
        said(&fault, &["could not be read", &format!("line {line} of test.xml")]);
    }
    assert_eq!(Machine::load("test.xml", &[0xff, 0xfe, 0x00], [2, 2, 1], &NONE, &usual).err(), Some(Fault::Syntax { file: "test.xml".into(), line: 0, expected: Expected::Text }));
    // sizes over their limits
    let too = |fault: Fault, thing: Thing, words: &[&str]| {
        assert!(matches!(&fault, Fault::TooLarge { thing: t, figure, limit, .. } if *t == thing && figure > limit), "{fault:?}");
        said(&fault, words);
    };
    too(fails("<one values=\"BW\" in=\"B\" out=\"W\"/>", [2000, 2000, 2]), Thing::Grid, &["8000000 cells", "4000000", "make the size smaller"]);
    too(fails("<one values=\"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh\" in=\"A\" out=\"B\"/>", [2, 2, 1]), Thing::Values, &["34 values", "32"]);
    too(Machine::load("big.xml", &vec![b' '; 2 << 20], [2, 2, 1], &NONE, &usual).err().unwrap(), Thing::File, &["2097152 bytes", "1048576"]);
    too(fails(&format!("<one values=\"BW\" in=\"{0}\" out=\"{0}\"/>", "B".repeat(5000)), [2, 2, 1]), Thing::Pattern, &["5000 cells", "line 1"]);
    too(fails(&format!("{}<one values=\"BW\" in=\"B\" out=\"W\"/>{}", "<sequence values=\"BW\">".repeat(70), "</sequence>".repeat(70)), [2, 2, 1]), Thing::Depth, &["nested"]);
    too(Machine::load("t.xml", format!("<sequence values=\"BW\">{}</sequence>", "<one in=\"B\" out=\"W\"/>".repeat(30)).as_bytes(), [2, 2, 1], &NONE, &Limits { nodes: 20, ..usual }).err().unwrap(), Thing::Nodes, &["nodes", "20"]);
    too(Machine::load("t.xml", b"<one values=\"BWR\" in=\"BW/RB BB/WR\" out=\"BW/RB BB/WR\"/>" as &[u8], [4, 4, 4], &NONE, &Limits { rules: 20, ..usual }).err().unwrap(), Thing::Rules, &["rules", "symmetry"]);
    let mut crowded = Machine::load("t.xml", b"<one values=\"BW\" in=\"B\" out=\"W\"/>" as &[u8], [40, 40, 1], &NONE, &Limits { matches: 1000, ..usual }).unwrap();
    too(crowded.step().err().unwrap(), Thing::Matches, &["1600 cells times rules", "1000"]);
    let view = View { size: [4, 4, 4], cells: &(0..64).map(|i| 1 + ((i % 4 + i / 4 % 4 + i / 16) % 2) as u8).collect::<Vec<u8>>(), letters: &['B', 'W', 'R'] };
    too(bricks(&view, 1, Finish::Plain, 80, &Limits { bricks: 10, ..usual }).err().unwrap(), Thing::Bricks, &["64 bricks", "10", "plain finish"]);
    too(bricks(&view, 50, Finish::Plain, 80, &usual).err().unwrap(), Thing::Scale, &["50 microbricks", "20"]);
    // a program that is not of the language, or uses what is not run
    said(&fails("<castle values=\"BW\"/>", [2, 2, 1]), &["\"castle\" at line 1 of test.xml is not a node"]);
    assert!(matches!(fails("<sequence values=\"BW\">\n<wall/>\n<convchain sample=\"x\"/></sequence>", [2, 2, 1]), Fault::NotImplemented { line: 3, ref node, .. } if node == "convchain"));
    said(&fails("<wfc values=\"BW\" sample=\"Flowers\" n=\"3\"/>", [2, 2, 1]), &["wfc with a sample", "does not run yet"]);
    assert!(matches!(fails("<one values=\"BW\" in=\"B\" out=\"W\">\n<observe value=\"B\" to=\"W\"/></one>", [2, 2, 1]), Fault::NotImplemented { line: 2, ref node, .. } if node == "observe"));
    said(&fails("<one values=\"BW\" fin=\"Room\" out=\"W\" legend=\"BW\"/>", [2, 2, 1]), &[".png"]);
    // attributes missing, unreadable, or naming what is not there
    said(&fails("<one in=\"B\" out=\"W\"/>", [2, 2, 1]), &["\"one\" element", "needs a \"values\" attribute"]);
    said(&fails("<one values=\"BW\" in=\"B\"/>", [2, 2, 1]), &["needs a \"out\" attribute"]);
    said(&fails("<sequence values=\"BW\"><map values=\"BW\"/></sequence>", [2, 2, 1]), &["\"map\"", "\"scale\""]);
    said(&fails("<one values=\"BW\" in=\"B\" out=\"W\" steps=\"many\"/>", [2, 2, 1]), &["steps=\"many\"", "line 1"]);
    said(&fails("<one values=\"BW\" in=\"B\" out=\"W\" symmetry=\"(q)\"/>", [2, 2, 1]), &["symmetry=\"(q)\""]);
    said(&fails("<one values=\"BW\" in=\"B\" out=\"X\"/>", [2, 2, 1]), &["The letter \"X\"", "neither one of the grid's values nor a union"]);
    said(&fails("<one values=\"BWB\" in=\"B\" out=\"W\"/>", [2, 2, 1]), &["\"B\" is given twice"]);
    assert_eq!(fails("<one values=\"BW\" in=\"BB/B\" out=\"WW/WW\"/>", [2, 2, 1]), Fault::Pattern { file: "test.xml".into(), line: 1, uneven: true });
    said(&fails("<one values=\"BW\" in=\"BB\" out=\"W\"/>", [2, 2, 1]), &["different sizes", "map"]);
    said(&fails("<one values=\"BW\" in=\"B\" out=\"W\"/>", [3, 0, 1]), &["3 by 0 by 1", "at least one cell"]);
    // files a program refers to
    said(&fails("<wfc values=\"BW\" tileset=\"Nowhere\"/>", [2, 2, 2]), &["tilesets/Nowhere.xml", "was not found", "beside"]);
    for name in ["../secret", "a/b", "C:\\x", ".hidden", ""] {
        let fault = fails(&format!("<wfc values=\"BW\" tileset=\"{name}\"/>"), [2, 2, 2]);
        assert!(matches!(fault, Fault::Outside { .. }), "{name}: {fault:?}");
        said(&fault, &["not a plain file name"]);
    }
    let odd = bundled(vec![("tilesets/odd.xml", b"<tileset><tiles><tile name=\"a\"/></tiles><neighbors><neighbor left=\"a\" right=\"zz b\"/></neighbors></tileset>".to_vec()), ("tilesets/odd/a.vox", vox::write([2, 3, 2], |_, _, _| 1)), ("tilesets/sq.xml", b"<tileset><tiles><tile name=\"a\"/></tiles><neighbors><neighbor left=\"a\" right=\"b\"/></neighbors></tileset>".to_vec()), ("tilesets/sq/a.vox", vox::write([2, 2, 2], |x, _, _| 1 + x as u8)), ("tilesets/torn.xml", b"<tileset><tiles><tile name=\"a\"/></tiles></tileset>".to_vec()), ("tilesets/torn/a.vox", b"VOX nothing more".to_vec()), ("rules/Room.vox", vox::write([2, 1, 1], |x, _, _| 1 + x as u8))]);
    let with = |text: &str| Machine::load("t.xml", text.as_bytes(), [2, 2, 2], &odd, &usual).err().unwrap_or_else(|| panic!("loaded: {text}"));
    said(&with("<wfc values=\"BW\" tileset=\"odd\"/>"), &["2 by 3 cells", "must be square"]);
    said(&with("<wfc values=\"BW\" tileset=\"sq\"/>"), &["The tile \"b\"", "line 1 of tilesets/sq.xml"]);
    said(&with("<wfc values=\"BW\" tileset=\"sq\"><rule in=\"B\" out=\"Empty\"/></wfc>"), &["The tile \"Empty\"", "line 1 of t.xml"]);
    said(&with("<wfc values=\"B\" tileset=\"sq\"/>"), &["2 colours", "letters for 1"]);
    said(&with("<wfc values=\"BW\" tileset=\"torn\"/>"), &["tilesets/torn/a.vox is not a MagicaVoxel .vox file"]);
    said(&with("<one values=\"BW\" file=\"Room\" legend=\"B\"/>"), &["rules/Room.vox has 2 colours"]);
    said(&Fault::TileShape { file: "t.vox".into(), size: [2, 2, 3], cube: true }, &["2 by 2 by 3", "cubes"]);
    said(&Fault::Unreadable { name: "tilesets/a.xml".into(), why: "access denied".into() }, &["could not be read: access denied"]);
    // a wfc node that cannot be satisfied: before it starts, and after every try
    let fixed = b"<sequence values=\"BW\"><prl in=\"B\" out=\"W\"/><wfc values=\"RG\" tileset=\"chess\"><rule in=\"W\" out=\"b\"/></wfc></sequence>" as &[u8];
    let mut machine = Machine::load("t.xml", fixed, [2, 1, 1], &chess(), &usual).unwrap();
    machine.start(1);
    let fault = machine.finish().err().unwrap();
    assert_eq!(fault, Fault::Contradiction { file: "t.xml".into(), line: 1, tries: 0, at_start: true });
    said(&fault, &["cannot be satisfied at this size", "another size"]);
    said(&Fault::Contradiction { file: "t.xml".into(), line: 4, tries: 1000, at_start: false }, &["no arrangement of its tiles in 1000 tries", "another seed"]);
    // a program that never ends is stopped at the limit on steps, and what it made is still there
    let mut machine = Machine::load("t.xml", b"<markov values=\"BW\"><one in=\"B\" out=\"W\"/><one in=\"W\" out=\"B\"/></markov>" as &[u8], [3, 3, 1], &NONE, &Limits { steps: 500, ..usual }).unwrap();
    machine.start(1);
    let fault = machine.finish().err().unwrap();
    assert_eq!(fault, Fault::StepLimit { steps: 500 });
    said(&fault, &["Stopped early", "500 steps", "What it had made is shown"]);
    assert!(machine.running() && machine.view().cells.iter().any(|v| *v == 1));
    said(&Fault::TimeLimit { steps: 9000, seconds: 30 }, &["Stopped early", "30 seconds", "9000 steps"]);
}

#[test]
fn a_link_out_of_the_folder_and_a_file_too_large_are_refused() {
    let folder = std::env::temp_dir().join(format!("brickloft-rewrite-test-{}", std::process::id()));
    std::fs::create_dir_all(folder.join("tilesets")).unwrap();
    std::fs::write(folder.join("tilesets/Big.xml"), vec![b' '; 3000]).unwrap();
    let resources = Resources { roots: vec![folder.clone()], bundled: &[] };
    assert!(matches!(resources.read(&["tilesets", "Big.xml"], 2000), Err(Fault::TooLarge { thing: Thing::File, figure: 3000, limit: 2000, .. })));
    assert_eq!(resources.read(&["tilesets", "Big.xml"], 4000).map(|(name, bytes)| (name, bytes.len())), Ok(("tilesets/Big.xml".to_string(), 3000)));
    assert_eq!(resources.read(&["tilesets", "None.xml"], 4000), Err(Fault::NoResource { name: "tilesets/None.xml".into() }));
    assert_eq!(resources.read(&["..", "x"], 4000), Err(Fault::Outside { name: "../x".into() }));
    let _ = std::fs::remove_dir_all(folder);
}
