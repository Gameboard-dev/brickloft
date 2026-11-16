//! Tests of bringing a program in: from a folder laid out as a checkout
//! of the original is, into a folder that is not anyone's Documents.
//! The programs written for these tests are in `fixtures/`, as files.

use super::*;
use brickloft_core::spec::{build, Form, Spec};
use std::sync::OnceLock;

/// A folder of this run's own for everything the tests write, and the library within it, told to the core.
fn rooms() -> &'static (PathBuf, PathBuf) {
    static ROOMS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();
    ROOMS.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("brickloft-programs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let library = home.join("library");
        std::fs::create_dir_all(&library).unwrap();
        form::set_library(library.clone());
        (home, library)
    })
}

fn upstream() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/models/markov")
}

/// A folder as the docs say to lay one out: the program beside a
/// `resources` folder and a list of models. The program is the
/// original's block of flats under another name, with its tileset.
fn checkout(under: &str) -> PathBuf {
    let from = rooms().0.join(under);
    let tiles = from.join("resources/tilesets/Paths");
    std::fs::create_dir_all(&tiles).unwrap();
    std::fs::copy(upstream().join("models/Apartemazements.xml"), from.join("MyFlats.xml")).unwrap();
    std::fs::copy(upstream().join("resources/tilesets/Paths.xml"), from.join("resources/tilesets/Paths.xml")).unwrap();
    for tile in std::fs::read_dir(upstream().join("resources/tilesets/Paths")).unwrap().flatten() {
        std::fs::copy(tile.path(), tiles.join(tile.file_name())).unwrap();
    }
    std::fs::write(from.join("models.xml"), include_bytes!("fixtures/models.xml")).unwrap();
    std::fs::write(from.join("notes of my own.txt"), "not the program's").unwrap();
    std::fs::write(from.join("resources/tilesets/Unused.xml"), "<tileset/>").unwrap();
    from
}

#[test]
fn a_program_is_brought_in_with_the_files_it_reads_and_runs_from_the_copy() {
    let library = rooms().1.clone();
    let from = checkout("checkout");
    assert_eq!(import(&from.join("MyFlats.xml")), Ok("MyFlats".to_string()));
    // the program, the tileset it names and that tileset's tiles, and its size; nothing else that lay beside it
    let mine = library.join("MyFlats");
    for copied in ["MyFlats.xml", "tilesets/Paths.xml", "tilesets/Paths/Line.vox", "tilesets/Paths/Up.vox", "size.txt"] {
        assert!(mine.join(copied).is_file(), "{copied} was not copied");
    }
    assert!(!mine.join("notes of my own.txt").exists() && !mine.join("tilesets/Unused.xml").exists() && !mine.join("models.xml").exists());
    // (the original's palette, which was not beside it, is the one that comes with the editor)
    assert!(!mine.join("palette.xml").exists());
    assert_eq!((usual_size("MyFlats"), imported().contains(&"MyFlats".to_string())), (Some([5, 5, 5]), true));
    // where it came from is gone, and it runs: the same building the original's program makes, in the same colours
    std::fs::remove_dir_all(&from).unwrap();
    let brought = Program { size: [5, 5, 5], seed: 3, ..Program::of("MyFlats", true) };
    let original = Program { size: [5, 5, 5], seed: 3, ..Program::of("Apartemazements", false) };
    form::forget("MyFlats");
    let (a, b) = (form::grown(&brought).unwrap(), form::grown(&original).unwrap());
    assert!(a.cells == b.cells && a.cells.iter().any(|value| *value != 0));
    assert_eq!(form::colours(&brought).unwrap(), form::colours(&original).unwrap());
    // as a form, from a save's line and through MCP, by its name
    let spec = brickloft_core::report::parse_spec("form = program\npoints = 0.5,0.5,0.5\nprogram = imported:MyFlats,5,5,5,2,3").unwrap();
    assert!(spec.program.imported && !build(&spec).unwrap().bricks.is_empty());
    let mut state = crate::state::State::new();
    let added = crate::mcp::apply(&mut state, "add_form", &serde_json::json!({ "settings": { "form": "program", "program": "imported:MyFlats" } }), || 0.0).unwrap();
    let added: serde_json::Value = serde_json::from_str(&added).unwrap();
    // (named and no more, it is the size it was brought in at)
    assert_eq!(added["forms"][0]["settings"]["program"], "imported:MyFlats,5,5,5,4,1,wedges,50000,end");
    assert!(added["forms"][0]["bricks"].as_u64().unwrap() > 50 && state.spec.form == Form::Program);
    // brought in again from a file that is another program, it is that program: nothing is remembered of the first
    let again = rooms().0.join("second");
    std::fs::create_dir_all(&again).unwrap();
    std::fs::write(again.join("MyFlats.xml"), include_bytes!("fixtures/Flat.xml")).unwrap();
    assert_eq!(import(&again.join("MyFlats.xml")), Ok("MyFlats".to_string()));
    assert!(!mine.join("tilesets").exists() && usual_size("MyFlats") == Some(form::USUAL_SIZE));
    assert_ne!(form::grown(&brought).unwrap().cells, a.cells);
}

#[test]
fn a_flat_program_is_brought_in_at_the_size_the_list_of_models_gives_it() {
    rooms();
    let from = rooms().0.join("flat");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("Flat.xml"), include_bytes!("fixtures/Flat.xml")).unwrap();
    std::fs::write(from.join("models.xml"), include_bytes!("fixtures/models.xml")).unwrap();
    assert_eq!(import(&from.join("Flat.xml")), Ok("Flat".to_string()));
    // (a size and no `d` is a square one cell deep, as the original reads it)
    assert_eq!(usual_size("Flat"), Some([24, 24, 1]));
    // the editor makes the form that program, at that size, in its colours, and it builds a maze one cell high
    let mut state = crate::state::State::new();
    state.grow.watch = false;
    assert!(state.import_program(&from.join("Flat.xml")) && state.form == crate::state::PROGRAM);
    assert_eq!((state.spec.program.name.as_str(), state.spec.program.imported, state.spec.program.size), ("Flat", true, [24, 24, 1]));
    assert!(state.grow.imported.contains(&"Flat".to_string()) && state.grow.import_note.contains("Imported Flat"));
    let built = build(&Spec { points: vec![brickloft_core::DVec3::new(0.5, 0.5, 0.5)], ..state.full_spec() }).unwrap();
    let cell = state.spec.program.cell;
    assert!(built.bricks.len() > 20 && built.bricks.iter().all(|brick| brick.min[2] == 0 && brick.ext[2] == cell));
    assert_eq!(state.paint.colours.len(), 3);
    // a file dropped on the window is taken if it is a program's, and left for others if it is not
    assert!(state.drop_file(&from.join("Flat.xml")) && !state.drop_file(&from.join("model.glb")) && state.alert.is_none());
}

#[test]
fn what_cannot_be_brought_in_says_why_and_leaves_nothing_behind() {
    let (home, library) = rooms().clone();
    let from = home.join("refused");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::write(from.join("Broken.xml"), include_bytes!("fixtures/Broken.xml")).unwrap();
    std::fs::write(from.join("Lost.xml"), include_bytes!("fixtures/Lost.xml")).unwrap();
    std::fs::write(from.join("Sampled.xml"), include_bytes!("fixtures/Sampled.xml")).unwrap();
    std::fs::write(from.join("my;program.xml"), include_bytes!("fixtures/Flat.xml")).unwrap();
    std::fs::write(from.join("Large.xml"), vec![b' '; 2 << 20]).unwrap();
    std::fs::write(from.join("picture.png"), b"not xml").unwrap();
    let said = |file: &str, words: &[&str]| {
        let why = import(&from.join(file)).unwrap_err();
        for word in words {
            assert!(why.contains(word), "{file}: \"{word}\" is not in: {why}");
        }
    };
    said("Broken.xml", &["could not be read", "line 3 of Broken.xml"]);
    said("Lost.xml", &["needs the file tilesets/Nowhere.xml", "beside"]);
    said("Sampled.xml", &["\"convchain\" at line 3 of Sampled.xml", "does not run yet"]);
    said("my;program.xml", &["cannot be a program's name", "rename the file"]);
    said("Large.xml", &["2097152 bytes", "the most allowed is 1048576"]);
    said("picture.png", &["is not a rule program", ".xml"]);
    said("Not there.xml", &["could not be read", "Check that it is there"]);
    for name in ["Broken", "Lost", "Sampled", "Large", "Not there", "picture"] {
        assert!(!library.join(name).exists(), "{name} was left in the library");
    }
    // and in the editor the reason is said beside the path, the form left as it was
    let mut state = crate::state::State::new();
    let before = state.full_spec();
    assert!(!state.import_program(&from.join("Lost.xml")) && state.grow.import_note.contains("tilesets/Nowhere.xml"));
    assert_eq!(state.full_spec(), before);
    // dropped on the window, it is a notice to be read
    assert!(state.drop_file(&from.join("Broken.xml")) && state.alert.as_ref().is_some_and(|(title, why)| *title == "Not imported" && why.contains("line 3")));
}
