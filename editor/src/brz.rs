//! The workspace as a Brickadia prefab file (.brz), and that file on the
//! clipboard, to try a build in the game without the plugin.
//!
//! Desktop only. A web page may put text and images on the clipboard and
//! nothing else, so there is no way for the browser editor to offer a file.
//! https://developer.mozilla.org/en-US/docs/Web/API/Clipboard/write
//!
//! The file is written with the `brdb` crate, which is the game community's
//! own reader and writer of the format.

use crate::paint::{Paint, MODES};
use crate::state::Painted;
use arcs_core::report::{parse_spec, spec_text};
use arcs_core::spec::Spec;
use arcs_core::orient::{Models, Solver};
use arcs_core::shapes::Kind;
use brdb::{BString, Brick, BrickSize, BrickType, Color, Direction, Rotation, World};

/// The game's brick asset for each shape: the plugin's defaults (`doc.json`).
/// The plugin can be configured otherwise; a file made here cannot ask it.
const fn asset(kind: Kind) -> &'static str {
    match kind {
        Kind::Cube => "PB_DefaultMicroBrick",
        Kind::Ramp => "PB_DefaultMicroWedge",
        Kind::Tet => "PB_DefaultMicroWedgeTriangleCorner",
        Kind::Cut => "PB_DefaultMicroWedgeOuterCorner",
    }
}

/// What a brick with no colour of its own gets in a file, which has no
/// player to take a colour from: the neutral one the editor draws it in.
const UNPAINTED: [u8; 3] = [214, 208, 196];

/// The line that begins each form in a file's description.
const FORM: &str = "[arcs form]";

/// Largest file a paste will read. A prefab of a million bricks is a few
/// megabytes; anything near this is not one of ours.
const MAX_FILE: u64 = 64 << 20;

/// The workspace's forms as text, kept in the file's description.
///
/// A .brz holds bricks, and bricks cannot be turned back into the form they
/// were fitted to. With the forms written beside them, a file copied from
/// the editor pastes back as forms that can still be reshaped, not as a
/// fixed lump; the game ignores the description.
fn describe(forms: &[(Spec, Paint)]) -> String {
    let mut text = String::new();
    for (spec, paint) in forms {
        let mode = MODES.iter().position(|m| m.0 == paint.mode).unwrap_or(0);
        let colours: Vec<String> = paint.colours.iter().map(|c| format!("{},{},{}", c[0], c[1], c[2])).collect();
        text += &format!("{FORM}\npaint {mode} {} {} {}\n{}", paint.axis, paint.width, colours.join(" "), spec_text(spec));
    }
    text
}

/// The reverse of [`describe`]. The text is from a file, which may be anyone's.
fn forms_in(description: &str) -> Result<Vec<(Spec, Paint)>, String> {
    let mut forms = Vec::new();
    for part in description.split(FORM).skip(1) {
        let (first, spec) = part.trim_start().split_once('\n').ok_or("a form in it is cut short")?;
        let mut words = first.strip_prefix("paint ").ok_or("a form in it has no colours")?.split_whitespace();
        let mut number = || words.next().and_then(|w| w.parse::<usize>().ok());
        let (Some(mode), Some(axis), Some(width)) = (number(), number(), number()) else { return Err("a form's colours cannot be read".into()) };
        let colours: Option<Vec<[u8; 3]>> = words
            .map(|w| {
                let parts: Vec<u8> = w.split(',').filter_map(|v| v.parse().ok()).collect();
                <[u8; 3]>::try_from(parts).ok()
            })
            .collect();
        let colours = colours.filter(|c| c.len() <= 4).ok_or("a form's colours cannot be read")?;
        let paint = Paint { mode: MODES.get(mode).ok_or("a form's colouring is not one this editor has")?.0, colours, axis: axis.min(2), width: width.clamp(1, 1000) as i32 };
        forms.push((parse_spec(spec)?, paint));
    }
    Ok(forms)
}

/// The bricks as a prefab, with the forms they were built from in its
/// description. `Err` names the first brick that cannot be placed.
pub fn prefab(bricks: &[Painted], forms: &[(Spec, Paint)]) -> Result<World, String> {
    let solver = Solver::new(&Models::default());
    let mut world = World::new();
    world.meta.bundle.description = describe(forms);
    for (cell, colour, _) in bricks {
        let placed = solver.place(cell).ok_or_else(|| format!("no brick fits the piece at {:?}", cell.min))?;
        // a brick's size in the file is its half-extent in game units, which
        // for a microbrick is its size in cells
        let size = placed.size.map(|s| u16::try_from(s).ok().filter(|s| *s >= 1));
        let [Some(x), Some(y), Some(z)] = size else {
            return Err(format!("the piece at {:?} is too large for one brick", cell.min));
        };
        let [r, g, b] = colour.unwrap_or(UNPAINTED);
        // the game numbers orientations as direction * 4 + rotation
        let direction = [Direction::XPositive, Direction::XNegative, Direction::YPositive, Direction::YNegative, Direction::ZPositive, Direction::ZNegative][placed.orientation as usize / 4];
        let rotation = [Rotation::Deg0, Rotation::Deg90, Rotation::Deg180, Rotation::Deg270][placed.orientation as usize % 4];
        world.bricks.push(Brick {
            asset: BrickType::Procedural { asset: BString::str(asset(placed.kind)), size: BrickSize { x, y, z } },
            position: (placed.pos[0], placed.pos[1], placed.pos[2]).into(),
            direction,
            rotation,
            color: Color::new(r, g, b),
            ..Default::default()
        });
    }
    world.make_prefab();
    Ok(world)
}

/// Write the workspace to `arcs-build.brz` beside where the editor was
/// started, and put that file on the clipboard. Returns what to tell the
/// player.
pub fn copy(bricks: &[Painted], forms: &[(Spec, Paint)]) -> String {
    if bricks.is_empty() {
        return "There are no bricks to copy.".into();
    }
    let bytes = match prefab(bricks, forms).and_then(|world| world.to_brz_vec().map_err(|e| e.to_string())) {
        Ok(bytes) => bytes,
        Err(e) => return format!("Could not make the file: {e}"),
    };
    // One fixed name, replaced each time: what is on the clipboard is a
    // reference to the file, so it has to exist somewhere, and a fresh name
    // for every copy would leave a pile of them.
    let path = std::env::current_dir().unwrap_or_default().join("arcs-build.brz");
    if let Err(e) = std::fs::write(&path, &bytes) {
        return format!("Could not write {}: {e}. Start the editor from a folder you can write to.", path.display());
    }
    let size = format!("{} bricks, {} bytes", bricks.len(), bytes.len());
    match place_on_clipboard(&path) {
        Ok(()) => format!("Copied {} to the clipboard as a file ({size}).", path.display()),
        Err(e) => format!("Saved {} ({size}), but could not put it on the clipboard: {e}", path.display()),
    }
}

/// Put a file on the clipboard the way a file manager's Copy does: as a
/// reference to the file, which a paste then reads.
#[cfg(windows)]
fn place_on_clipboard(path: &std::path::Path) -> Result<(), String> {
    use clipboard_win::{formats, Clipboard, Setter};
    let name = path.to_str().ok_or("its path is not text the clipboard can hold")?;
    // opening can fail while another program holds the clipboard: try a few times
    let _open = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
    formats::FileList.write_clipboard(&[name]).map_err(|e| e.to_string())
}

/// The forms in a .brz file's bytes, to add to the workspace.
///
/// A file holding any brick that is not one of the four microbricks is
/// refused, naming them: the editor has no such bricks to show. One that
/// holds only microbricks but was not copied from this editor is refused
/// too, since it carries no forms.
pub fn read(bytes: &[u8]) -> Result<Vec<(Spec, Paint)>, String> {
    use brdb::{Brz, IntoReader};
    let file = Brz::read_slice(bytes).map_err(|e| format!("it is not a .brz file the editor can read ({e})"))?.into_reader();
    let data = file.global_data().map_err(|e| format!("its list of bricks cannot be read ({e})"))?;
    let ours = [Kind::Cube, Kind::Ramp, Kind::Tet, Kind::Cut].map(asset);
    let mut foreign: Vec<&str> = data.basic_brick_asset_names.iter().chain(&data.procedural_brick_asset_names).map(String::as_str).filter(|name| !ours.contains(name)).collect();
    if !foreign.is_empty() {
        let more = foreign.len().saturating_sub(6);
        foreign.truncate(6);
        let more = if more > 0 { format!(" and {more} more") } else { String::new() };
        return Err(format!("it holds bricks this editor does not have: {}{more}. Only the four microbricks can be shown.", foreign.join(", ")));
    }
    let bundle = file.bundle_json().map_err(|e| format!("its description cannot be read ({e})"))?;
    let forms = forms_in(&bundle.description)?;
    if forms.is_empty() {
        return Err("it was not copied from this editor. Its bricks are all microbricks, but it carries no forms, and bricks alone cannot be turned back into a form to edit.".into());
    }
    Ok(forms)
}

/// The forms in the .brz file on the clipboard. `Ok(None)` if what is on the
/// clipboard is not a .brz file at all, which is nothing to complain about.
pub fn paste() -> Result<Option<Vec<(Spec, Paint)>>, String> {
    let Some(path) = file_on_clipboard() else { return Ok(None) };
    let name = path.display();
    let size = std::fs::metadata(&path).map_err(|e| format!("{name} cannot be read: {e}"))?.len();
    if size > MAX_FILE {
        return Err(format!("{name} is {size} bytes, more than a paste will read"));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{name} cannot be read: {e}"))?;
    read(&bytes).map(Some).map_err(|e| format!("{name} cannot be pasted: {e}"))
}

/// The first .brz file among the files on the clipboard.
#[cfg(windows)]
fn file_on_clipboard() -> Option<std::path::PathBuf> {
    use clipboard_win::{formats, Clipboard, Getter};
    let _open = Clipboard::new_attempts(10).ok()?;
    let mut files: Vec<String> = Vec::new();
    formats::FileList.read_clipboard(&mut files).ok()?;
    files.into_iter().map(std::path::PathBuf::from).find(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("brz")))
}

#[cfg(not(windows))]
fn file_on_clipboard() -> Option<std::path::PathBuf> {
    None
}

#[cfg(not(windows))]
fn place_on_clipboard(_: &std::path::Path) -> Result<(), String> {
    Err("copying a file to the clipboard is only done on Windows so far".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcs_core::shapes::{all_masks, CellBrick};
    use brdb::{BrFsReader, Brz, IntoReader};

    #[test]
    fn every_shape_is_written_and_read_back() {
        // one brick of each pose, each a different size and place, two painted
        let bricks: Vec<Painted> = all_masks()
            .into_iter()
            .enumerate()
            .map(|(i, mask)| {
                let i = i as i32;
                (CellBrick { mask, min: [i * 7, -3, i % 4], ext: [1 + i % 3, 2, 1 + i % 2] }, (i % 2 == 0).then_some([200, 10, i as u8]), 0)
            })
            .collect();
        let arch = Spec { points: vec![arcs_core::DVec3::new(-7.5, 0.5, 0.5), arcs_core::DVec3::new(8.5, 0.5, 0.5)], thick: 3.0, flip: [false, true, false], ..Spec::default() };
        let blend = Paint { mode: crate::paint::Mode::Stripes, colours: vec![[1, 2, 3], [250, 0, 9]], axis: 0, width: 3 };
        let forms = vec![(arch, blend), (Spec { form: arcs_core::spec::Form::Sphere, points: vec![arcs_core::DVec3::splat(0.5)], ..Spec::default() }, Paint::default())];
        let world = prefab(&bricks, &forms).unwrap();
        assert_eq!(world.bricks.len(), 29);
        assert_eq!(world.meta.bundle.level_type, "Prefab");
        let solver = Solver::new(&Models::default());
        for (brick, (cell, colour, _)) in world.bricks.iter().zip(&bricks) {
            let placed = solver.place(cell).unwrap();
            assert_eq!(brdb::orientation_to_byte(brick.direction, brick.rotation), placed.orientation, "{cell:?}");
            assert_eq!((brick.position.x, brick.position.y, brick.position.z), (placed.pos[0], placed.pos[1], placed.pos[2]));
            let [r, g, b] = colour.unwrap_or(UNPAINTED);
            assert_eq!((brick.color.r, brick.color.g, brick.color.b), (r, g, b));
            let BrickType::Procedural { asset: name, size } = &brick.asset else { panic!("{cell:?} is not a sized brick") };
            assert_eq!(name.to_string(), asset(placed.kind));
            assert_eq!([size.x, size.y, size.z].map(i32::from), placed.size);
        }
        // the bytes are a .brz the format's own reader accepts, with the bricks in it
        let bytes = world.to_brz_vec().unwrap();
        let archive = Brz::read_slice(&bytes).unwrap().into_reader();
        let listing = archive.get_fs().unwrap().render();
        assert!(listing.contains("Bricks"), "{listing}");
        // and pasted back, the forms are the ones copied, to the last setting
        assert_eq!(read(&bytes).unwrap(), forms);
        // a file with any other brick in it is refused by name
        let mut other = World::new();
        other.bricks.push(Brick { asset: BrickType::Procedural { asset: BString::str("PB_DefaultBrick"), size: BrickSize { x: 5, y: 5, z: 2 } }, ..Default::default() });
        other.bricks.push(Brick { asset: BrickType::str("B_1x1_Round"), ..Default::default() });
        other.meta.bundle.description = describe(&forms);
        let refused = read(&other.to_brz_vec().unwrap()).unwrap_err();
        assert!(refused.contains("PB_DefaultBrick") && refused.contains("B_1x1_Round"), "{refused}");
        // microbricks from somewhere else carry no forms
        let mut bare = prefab(&bricks, &[]).unwrap();
        bare.meta.bundle.description = "a build".into();
        assert!(read(&bare.to_brz_vec().unwrap()).unwrap_err().contains("not copied from this editor"));
        assert!(read(b"not a brz").is_err());
        // a piece no single brick can be is refused, not written wrong
        let odd = (CellBrick { mask: 0b0001_1111, min: [0; 3], ext: [1; 3] }, None, 0);
        assert!(prefab(&[odd], &[]).err().unwrap().contains("no brick fits"));
    }
}
