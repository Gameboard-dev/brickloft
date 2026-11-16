//! The workspace as a Brickadia prefab file (.brz), and that file on the
//! clipboard, for the game to paste: the editor's one route into the game.
//!
//! Desktop only. A web page may put text and images on the clipboard and
//! nothing else, so there is no way for the browser editor to offer a file.
//! https://developer.mozilla.org/en-US/docs/Web/API/Clipboard/write
//!
//! The file is written with the `brdb` crate, which is the game community's
//! own reader and writer of the format.

use crate::state::{Painted, Placed};
use crate::store::{describe, forms_in};
use brickloft_core::orient::{Models, Solver};
use brickloft_core::shapes::Kind;
use brdb::{BString, Brick, BrickSize, BrickType, Color, Direction, Rotation, World};

/// The game's brick asset for each shape.
const fn asset(kind: Kind) -> &'static str {
    match kind {
        Kind::Cube => "PB_DefaultMicroBrick",
        Kind::Ramp => "PB_DefaultMicroWedge",
        Kind::Tet => "PB_DefaultMicroWedgeTriangleCorner",
        Kind::Cut => "PB_DefaultMicroWedgeOuterCorner",
        Kind::Hip => "PB_DefaultMicroWedgeCorner",
        Kind::Inner => "PB_DefaultMicroWedgeInnerCorner",
        Kind::HalfOuter => "PB_DefaultMicroWedgeHalfOuterCorner",
        Kind::HalfInner => "PB_DefaultMicroWedgeHalfInnerCorner",
        Kind::HalfInnerInv => "PB_DefaultMicroWedgeHalfInnerCornerInverted",
    }
}

/// What a brick with no colour of its own gets in a file, which has no
/// player to take a colour from: the neutral one the editor draws it in.
const UNPAINTED: [u8; 3] = [214, 208, 196];

/// Largest file a paste will read. A prefab of a million bricks is a few
/// megabytes; anything near this is not one of ours.
pub(crate) const MAX_FILE: u64 = 64 << 20;

/// The bricks as a prefab, with the forms they were built from in its
/// description. `Err` names the first brick that cannot be placed.
pub fn prefab(bricks: &[Painted], forms: &[Placed]) -> Result<World, String> {
    let solver = Solver::new(&Models::default());
    let mut world = World::new();
    world.meta.bundle.description = describe(forms);
    // Forms whose grid is at an angle go on that grid, frozen where it
    // stands: one grid for all the forms that share it. (That the game takes
    // such a prefab from the clipboard was tried with `tilted_trial`.)
    let mut own: std::collections::BTreeMap<u32, Vec<&Painted>> = std::collections::BTreeMap::new();
    for brick in bricks {
        let (cell, colour, form, place) = brick;
        match forms.get(*form).filter(|f| crate::state::on_own_grid(&f.0)) {
            Some(form) => own.entry(form.0.on_grid).or_default().push(brick),
            None => world.bricks.push(brick_for(&solver, cell, *colour, *place)?),
        }
    }
    if !own.is_empty() {
        world.register_all_components();
    }
    for bricks in own.into_values() {
        let on_grid: Vec<Brick> = bricks.iter().map(|(cell, colour, _, place)| brick_for(&solver, cell, *colour, *place)).collect::<Result<_, _>>()?;
        let spec = &forms[bricks[0].2].0;
        let [x, y, z, w] = spec.grid.map(|v| v as f32);
        // where the grid is, in units, of which there are two to a cell
        let at = spec.grid_at.map(|v| (v * 2.0) as f32);
        let grid = brdb::Entity { frozen: true, location: (at[0], at[1], at[2]).into(), rotation: brdb::Quat4f { x, y, z, w }, ..Default::default() };
        world.add_brick_grid(grid, on_grid);
    }
    world.make_prefab();
    Ok(world)
}

/// One of the editor's bricks as the file has it.
fn brick_for(solver: &Solver, cell: &brickloft_core::shapes::CellBrick, colour: Option<[u8; 3]>, place: [i32; 3]) -> Result<Brick, String> {
    {
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
        Ok(Brick {
            asset: BrickType::Procedural { asset: BString::str(asset(placed.kind)), size: BrickSize { x, y, z } },
            // a brick's centre in units: its cells', and where its form stands
            position: (placed.pos[0] + place[0], placed.pos[1] + place[1], placed.pos[2] + place[2]).into(),
            direction,
            rotation,
            color: Color::new(r, g, b),
            ..Default::default()
        })
    }
}

/// A trial file for brick grids at an angle: a flat ring on the world's own
/// grid, and an arch on a second grid of its own, frozen in place and
/// tilted 30 degrees about x and then 20 about z.
///
/// The game keeps bricks on grids, and a grid other than the world's can
/// stand at any angle. Whether a prefab holding such a grid can be pasted
/// from the clipboard as one of bricks alone can is what this file is for
/// finding out, before the editor is built to turn forms freely.
pub fn tilted_trial() -> Result<Vec<u8>, String> {
    use brickloft_core::spec::{build, Form, Spec};
    use brickloft_core::DVec3;
    let solver = Solver::new(&Models::default());
    let mut world = World::new();
    world.register_all_components();
    let ring = build(&Spec { form: Form::Circle, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: 14.0, thick: 3.0, ..Spec::default() })?;
    for cell in &ring.bricks {
        world.bricks.push(brick_for(&solver, cell, Some([90, 150, 220]), [0; 3])?);
    }
    let arch = build(&Spec { points: vec![DVec3::new(-7.5, 0.5, 0.5), DVec3::new(8.5, 0.5, 0.5)], thick: 3.0, depth: 3, ..Spec::default() })?;
    let on_grid: Vec<Brick> = arch.bricks.iter().map(|cell| brick_for(&solver, cell, Some([230, 150, 60]), [0; 3])).collect::<Result<_, _>>()?;
    // a quarter of a turn is two halves: the turn about x, then the turn about z
    let (half_x, half_z) = (15f32.to_radians(), 10f32.to_radians());
    let (about_x, about_z) = ([half_x.sin(), 0.0, 0.0, half_x.cos()], [0.0, 0.0, half_z.sin(), half_z.cos()]);
    // the product of two quaternions (x, y, z, w): https://en.wikipedia.org/wiki/Quaternion#Hamilton_product
    let both = |a: [f32; 4], b: [f32; 4]| [a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1], a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0], a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3], a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2]];
    let [x, y, z, w] = both(about_z, about_x);
    let grid = brdb::Entity { frozen: true, location: (0.0, 0.0, 60.0).into(), rotation: brdb::Quat4f { x, y, z, w }, ..Default::default() };
    world.add_brick_grid(grid, on_grid);
    world.make_prefab();
    world.to_brz_vec().map_err(|e| e.to_string())
}

/// Write the workspace to `brickloft-build.brz` in the program's temporary
/// folder (see `places`), and put that file on the clipboard. Returns what to tell the
/// player.
pub fn copy(bricks: &[Painted], forms: &[Placed]) -> String {
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
    let path = match crate::places::passing() {
        Ok(folder) => folder.join("brickloft-build.brz"),
        Err(e) => return format!("Could not make the file: {e}"),
    };
    if let Err(e) = std::fs::write(&path, &bytes) {
        return format!("Could not write {}: {e}", path.display());
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
/// A file holding any brick that is not one of the microbricks is
/// refused, naming them: the editor has no such bricks to show. One that
/// holds only microbricks but was not copied from this editor is refused
/// too, since it carries no forms.
pub fn read(bytes: &[u8]) -> Result<Vec<Placed>, String> {
    use brdb::{Brz, IntoReader};
    let file = Brz::read_slice(bytes).map_err(|e| format!("it is not a .brz file the editor can read ({e})"))?.into_reader();
    let data = file.global_data().map_err(|e| format!("its list of bricks cannot be read ({e})"))?;
    let ours = brickloft_core::shapes::KINDS.map(asset);
    let mut foreign: Vec<&str> = data.basic_brick_asset_names.iter().chain(&data.procedural_brick_asset_names).map(String::as_str).filter(|name| !ours.contains(name)).collect();
    if !foreign.is_empty() {
        let more = foreign.len().saturating_sub(6);
        foreign.truncate(6);
        let more = if more > 0 { format!(" and {more} more") } else { String::new() };
        return Err(format!("it holds bricks this editor does not have: {}{more}. Only the five microbricks can be shown.", foreign.join(", ")));
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
pub fn paste() -> Result<Option<Vec<Placed>>, String> {
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
    use brickloft_core::spec::Spec;
    use crate::paint::Paint;
    use brickloft_core::shapes::{all_masks, CellBrick};
    use brdb::{BrFsReader, Brz, IntoReader};

    #[test]
    fn the_tilted_trial_is_a_file_with_two_grids_of_bricks() {
        let bytes = tilted_trial().unwrap();
        let archive = Brz::read_slice(&bytes).unwrap().into_reader();
        // the ring on the world's grid, and the arch on one of its own
        let grids = archive.brick_chunk_index(1).map(|c| c.len()).unwrap_or(0);
        assert!(archive.brick_chunk_index(1).is_ok(), "the world's grid is missing");
        assert!(grids > 0);
        assert!(archive.entity_chunk_index().unwrap().len() >= 1, "no second grid");
    }

    #[test]
    fn a_form_at_an_angle_is_copied_on_a_grid_of_its_own() {
        use brickloft_core::spec::build;
        use brickloft_core::DVec3;
        let arch = Spec { points: vec![DVec3::new(-7.5, 0.5, 0.5), DVec3::new(8.5, 0.5, 0.5)], thick: 3.0, ..Spec::default() };
        let tilted = Spec { grid: [0.0, 0.6, 0.0, 0.8], grid_at: [3.0, 0.0, 10.5], on_grid: 1, ..arch.clone() };
        let cells = build(&arch).unwrap().bricks;
        // the same arch three times: level, and twice more on one grid at an angle
        let bricks: Vec<Painted> = (0..3).flat_map(|form| cells.iter().map(move |c| (*c, None, form, [0, 8 * form as i32, 0]))).collect();
        let forms = vec![(arch, Paint::default(), [0; 3]), (tilted.clone(), Paint::default(), [0, 8, 0]), (tilted, Paint::default(), [0, 16, 0])];
        let world = prefab(&bricks, &forms).unwrap();
        assert_eq!(world.bricks.len(), cells.len(), "the level arch is on the world's grid");
        assert_eq!(world.grids.len(), 1, "the two tilted arches share one grid");
        let (grid, on_it) = &world.grids[0];
        assert_eq!(on_it.len(), 2 * cells.len());
        assert!(grid.frozen && (grid.rotation.y - 0.6).abs() < 1e-6 && (grid.rotation.w - 0.8).abs() < 1e-6);
        // where the grid is, in units; and its bricks where they are on it
        assert_eq!([grid.location.x, grid.location.y, grid.location.z], [6.0, 0.0, 21.0]);
        // (the file's library keeps a grid's bricks half a chunk, 1024 units, from where they are)
        let level: Vec<i32> = world.bricks.iter().map(|b| b.position.y).collect();
        let first: Vec<i32> = on_it[..cells.len()].iter().map(|b| b.position.y + 1024 - 8).collect();
        assert_eq!(first, level);
        // and it comes back from the file as the form it was, angle and all
        let back = read(&world.to_brz_vec().unwrap()).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!((back[2].0.on_grid, back[2].0.grid_at), (1, [3.0, 0.0, 10.5]));
        assert_eq!(back[1].0.grid, [0.0, 0.6, 0.0, 0.8]);
    }

    #[test]
    fn every_shape_is_written_and_read_back() {
        // one brick of each pose, each a different size and place, two painted
        let bricks: Vec<Painted> = all_masks()
            .into_iter()
            .enumerate()
            .map(|(i, mask)| {
                let i = i as i32;
                (CellBrick { mask, min: [i * 7, -3, i % 4], ext: [1 + i % 3, 2, 1 + i % 2] }, (i % 2 == 0).then_some([200, 10, i as u8]), 0, [i % 3, 0, -(i % 2)])
            })
            .collect();
        let arch = Spec { points: vec![brickloft_core::DVec3::new(-7.5, 0.5, 0.5), brickloft_core::DVec3::new(8.5, 0.5, 0.5)], thick: 3.0, flip: [false, true, false], ..Spec::default() };
        let blend = Paint { mode: crate::paint::Mode::Stripes, colours: vec![[1, 2, 3], [250, 0, 9]], axis: 0, width: 3 };
        let forms = vec![(arch, blend, [3, 0, -1]), (Spec { form: brickloft_core::spec::Form::Sphere, points: vec![brickloft_core::DVec3::splat(0.5)], ..Spec::default() }, Paint::default(), [0; 3])];
        let world = prefab(&bricks, &forms).unwrap();
        assert_eq!(world.bricks.len(), 125);
        assert_eq!(world.meta.bundle.level_type, "Prefab");
        let solver = Solver::new(&Models::default());
        for (brick, (cell, colour, _, place)) in world.bricks.iter().zip(&bricks) {
            let placed = solver.place(cell).unwrap();
            assert_eq!(brdb::orientation_to_byte(brick.direction, brick.rotation), placed.orientation, "{cell:?}");
            // its cells' middle in units, and where its form stands
            assert_eq!((brick.position.x, brick.position.y, brick.position.z), (placed.pos[0] + place[0], placed.pos[1] + place[1], placed.pos[2] + place[2]));
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
        let odd = (CellBrick { mask: 0b0000_1001, min: [0; 3], ext: [1; 3] }, None, 0, [0; 3]);
        assert!(prefab(&[odd], &[]).err().unwrap().contains("no brick fits"));
    }
}
