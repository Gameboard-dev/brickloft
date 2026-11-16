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
use brdb::{BString, Brick, BrickSize, BrickType, Collision, Color, Direction, Rotation, World};

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

/// The intensity water's glass is written with, of 10 (see `brick_for`).
const WATER_INTENSITY: u8 = 6;
const ICE_INTENSITY: u8 = 3;

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
    // (asked once of each form, not once of each brick: a plant is grown again to answer)
    let stuffs: Vec<Box<dyn Fn(&brickloft_core::shapes::CellBrick) -> Stuff>> = forms.iter().map(|f| stuff_of(&f.0)).collect();
    let stuff = |form: usize, cell: &brickloft_core::shapes::CellBrick| stuffs.get(form).map_or(Stuff::Solid, |of| of(cell));
    let mut own: std::collections::BTreeMap<u32, Vec<&Painted>> = std::collections::BTreeMap::new();
    for brick in bricks {
        let (cell, colour, form, place) = brick;
        match forms.get(*form).filter(|f| crate::state::on_own_grid(&f.0)) {
            Some(form) => own.entry(form.0.on_grid).or_default().push(brick),
            None => world.bricks.push(brick_for(&solver, cell, *colour, *place, stuff(*form, cell))?),
        }
    }
    if !own.is_empty() {
        world.register_all_components();
    }
    for bricks in own.into_values() {
        let on_grid: Vec<Brick> = bricks.iter().map(|(cell, colour, form, place)| brick_for(&solver, cell, *colour, *place, stuff(*form, cell))).collect::<Result<_, _>>()?;
        let spec = &forms[bricks[0].2].0;
        let [x, y, z, w] = spec.grid.map(|v| v as f32);
        // where the grid is, in units, of which there are two to a cell
        let at = spec.grid_at.map(|v| (v * 2.0) as f32);
        let grid = brdb::Entity { frozen: true, location: (at[0], at[1], at[2]).into(), rotation: brdb::Quat4f { x, y, z, w }, ..Default::default() };
        world.add_brick_grid(grid, on_grid);
    }
    // (A brick with a light on it names a kind of component, which the file must declare. Only those used: the writer's own note is that its whole catalogue can make the game refuse the file.)
    if world.bricks.iter().any(|b| !b.components.is_empty()) {
        world.register_used_components();
    }
    world.make_prefab();
    Ok(world)
}

/// One of the editor's bricks as the file has it.
/// What a brick of a form is to the game, beyond its shape and colour.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Stuff {
    /// stops everything, as a brick does
    Solid,
    /// a blade of grass: a player walks through it
    Blade,
    /// the water of a pond
    Water,
    /// the same water frozen: a player stands on it
    Ice,
    /// a bauble, tinsel or the star of a dressed pine: the game's metal
    Metal,
    /// a light or a candle's flame on a dressed pine: it glows, and has a light on it
    Lamp,
    /// the same with no light on it, for a tree whose lights are not to shine in the game
    Glow,
}

/// The light on a lamp of a dressed pine: a point light of a warm white,
/// weak and short, since there may be forty of them on a tree and the
/// game draws each. The fields are those the writer's own example of a
/// point light gives (`brdb`, `examples/write_fixtures.rs`), and the
/// component is the one its catalogue calls `Component_PointLight`. Its
/// defaults are a brightness of 20 and a radius of 150.
/// https://docs.rs/brdb
fn lamp_light() -> brdb::assets::LiteralComponent {
    use brdb::schema::as_brdb::AsBrdbValue;
    brdb::assets::LiteralComponent::new("Component_PointLight").with_data([
        ("bMatchBrickShape", Box::new(false) as Box<dyn AsBrdbValue>),
        ("bEnabled", Box::new(true)),
        ("Brightness", Box::new(LAMP_BRIGHTNESS)),
        ("Radius", Box::new(LAMP_RADIUS)),
        ("Color", Box::new(brdb::SavedBrickColor { r: 255, g: 196, b: 120, a: 255 })),
        ("bUseBrickColor", Box::new(false)),
        ("bCastShadows", Box::new(false)),
    ])
}
const LAMP_BRIGHTNESS: f32 = 25.0;
const LAMP_RADIUS: f32 = 120.0;

/// What each brick of a patch of grass is: its blades are walked through
/// (unless the form says they are solid) and its water is water.
///
/// A brick in the game's file has a flag for each thing it may stop: the
/// player, weapons, interaction and physics (`brdb::Collision`, which
/// writes them as the file's `CollisionFlags_Player`, `_Weapon`,
/// `_Interaction` and `_Physics`). A lawn of blades a few microbricks
/// high that stopped the player would be a lawn to trip over, and a pond
/// that did would be a pane of glass.
///
/// Which part a brick is, is asked of the plant itself, grown again from
/// the form (see `brickloft_core::spec::plant`): the part its middle is
/// in, as its colour was decided. It was first worked out from how high
/// the brick stood over the ground, which told blades from soil and
/// could not tell water from a stone lying in it.
///
/// Any other plant's greenery is walked through too if the form says so,
/// and its wood is not: a crown to walk through, limbs to stand on.
///
/// Only a plant as it was grown: turned or flipped, its bricks are no
/// longer where the plant is, and all are solid. So are those of a plant
/// too large for single microbricks, which is built at a larger size.
fn stuff_of(spec: &brickloft_core::spec::Spec) -> Box<dyn Fn(&brickloft_core::shapes::CellBrick) -> Stuff> {
    use brickloft_core::{spec::Form, tree, tree::Kind, DVec3};
    if spec.form == Form::Furniture {
        return furniture_stuff(spec);
    }
    let as_grown = spec.form == Form::Tree && spec.turned == brickloft_core::shapes::UPRIGHT && !spec.flip.contains(&true);
    let kind = spec.tree.kind;
    // Grass's blades are walked through unless it says they are solid
    // (its `extras`); any other plant's greenery is solid unless it says
    // it is walked through (`tube`, which a plant has no other use for).
    // A pond is frozen if the grass says so (`fill`, likewise).
    let (grass, soft, frozen) = (kind == Kind::Grass, if kind == Kind::Grass { !spec.tree.extras } else { spec.tube }, spec.fill);
    // (a pine with anything hung on it: its decorations are metal and its lights lit)
    let dressed = kind == Kind::Pine && (spec.tree.extras || spec.tree.baubles > 0 || spec.tree.tinsel || spec.tree.lights > 0);
    let shine = spec.tree.shine;
    // (how many lights have been written: never more than the bound, though a bulb cut in two by a bough is two bricks)
    let lit = std::sync::atomic::AtomicUsize::new(0);
    if !as_grown || !(grass || soft || dressed) {
        return Box::new(|_| Stuff::Solid);
    }
    let Some(plant) = brickloft_core::spec::plant(spec) else { return Box::new(|_| Stuff::Solid) };
    Box::new(move |cell| {
        // (asked where its colour was: at the middle of a sloped brick, and of a box at the middle of one of its cells, all of which are one part)
        let ext = if cell.mask == brickloft_core::shapes::CUBE { [1; 3] } else { cell.ext };
        let middle = DVec3::new(cell.min[0] as f64 + ext[0] as f64 / 2.0, cell.min[1] as f64 + ext[1] as f64 / 2.0, cell.min[2] as f64 + ext[2] as f64 / 2.0);
        // What is wood, by the part and not by its colour, which a
        // player may change: the trunk and the limbs of anything; the
        // marks on a birch's bark; the fibre under a palm's crown; and in
        // grass its soil, sand, stones and the bed of its pond. The rest
        // is greenery: leaves, needles, fronds, petals, berries, baubles.
        // A brick is of one part all through (see `tree::Model::colour`),
        // so none is half wood and half leaf.
        match (kind, plant.shade(middle)) {
            (Kind::Grass, tree::WATER) => if frozen { Stuff::Ice } else { Stuff::Water },
            (Kind::Grass, tree::LEAF | tree::LEAF_DARK | tree::LEAF_LIGHT) if soft => Stuff::Blade,
            (Kind::Grass, _) => Stuff::Solid,
            (_, tree::TRUNK | tree::LIMB) => Stuff::Solid,
            // A dressed pine's decorations, by the part and not the colour. They are solid whether or not its needles are walked through: a bauble is a thing to bump into.
            (Kind::Pine, tree::STAR | tree::EXTRA | tree::BAUBLE_GOLD | tree::BAUBLE_PALE | tree::TINSEL) => Stuff::Metal,
            (Kind::Pine, tree::FLAME) => if shine && lit.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < tree::MOST_LIGHTS { Stuff::Lamp } else { Stuff::Glow },
            (Kind::Pine, tree::WAX) => Stuff::Solid,
            _ if !soft => Stuff::Solid,
            // (and the bands on a cherry's bark)
            (Kind::Birch, tree::EXTRA) | (Kind::Palm | Kind::DatePalm | Kind::Cherry, tree::STAR) => Stuff::Solid,
            _ => Stuff::Blade,
        }
    })
}

/// What each brick of a piece of furniture is: its metal the game's
/// metal, and its lights glowing, with a real light on each that the
/// furniture says carries one (`brickloft_core::furniture::Model::lamps`:
/// one to a lamp, one to each flame of a chandelier, eight at most) if
/// the piece says its lights shine.
///
/// By the colour number the furniture gave each brick, which is a number
/// for what the brick is made of and not for the colour a player has
/// painted it. The piece is built again here as the workspace built it,
/// turned and flipped as it stands, and each brick of it looked up by
/// where it is; so a turned lamp is still lit, where a turned plant's
/// parts are lost. A brick that a join has cut is no longer one of
/// those, and is written as plain plastic.
///
/// Not a bookcase, whose numbers above the two woods are its books.
fn furniture_stuff(spec: &brickloft_core::spec::Spec) -> Box<dyn Fn(&brickloft_core::shapes::CellBrick) -> Stuff> {
    use brickloft_core::furniture::{Kind, Model, LIGHT, METAL};
    let snapped = spec.snapped();
    let lone = brickloft_core::spec::Spec { blend: None, ..spec.clone() };
    let (false, Ok(built), Some(middle)) = (spec.furniture.kind == Kind::Bookcase, brickloft_core::spec::build(&lone), snapped.points.first()) else { return Box::new(|_| Stuff::Solid) };
    // (the same bricks in the same order, before they were turned: which of them carry a light)
    let lamps = Model::new(&snapped.furniture, brickloft_core::spec::cube_sides(&snapped, *middle), snapped.max_ext, snapped.wedges).map(|model| model.lamps).unwrap_or_default();
    let shine = spec.furniture.shine;
    let mut made: std::collections::HashMap<(u8, [i32; 3], [i32; 3]), Stuff> = std::collections::HashMap::new();
    for (n, (brick, shade)) in built.bricks.iter().zip(&built.shades).enumerate() {
        let stuff = match *shade {
            METAL => Stuff::Metal,
            LIGHT if shine && lamps.contains(&n) => Stuff::Lamp,
            LIGHT => Stuff::Glow,
            _ => continue,
        };
        made.insert((brick.mask, brick.min, brick.ext), stuff);
    }
    Box::new(move |cell| made.get(&(cell.mask, cell.min, cell.ext)).copied().unwrap_or(Stuff::Solid))
}

fn brick_for(solver: &Solver, cell: &brickloft_core::shapes::CellBrick, colour: Option<[u8; 3]>, place: [i32; 3], stuff: Stuff) -> Result<Brick, String> {
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
        let brick = Brick {
            asset: BrickType::Procedural { asset: BString::str(asset(placed.kind)), size: BrickSize { x, y, z } },
            // a brick's centre in units: its cells', and where its form stands
            position: (placed.pos[0] + place[0], placed.pos[1] + place[1], placed.pos[2] + place[2]).into(),
            direction,
            rotation,
            color: Color::new(r, g, b),
            // A blade stops nothing of the player's, no weapon and no
            // physics object, and can still be pointed at. Water stops
            // nothing at all, pointing included, so that what lies in a
            // pond can be reached through it.
            collision: match stuff {
                Stuff::Solid | Stuff::Ice | Stuff::Metal | Stuff::Lamp | Stuff::Glow => Collision::default(),
                Stuff::Blade => Collision { player: false, weapon: false, physics: false, ..Collision::default() },
                Stuff::Water => Collision { player: false, weapon: false, physics: false, interact: false, ..Collision::default() },
            },
            // Water is the game's glass. No brick or material of the
            // game's is water that this file's writer knows of (its
            // materials are plastic, glass, translucent plastic, glow,
            // metallic and hologram). A material has an intensity from 0
            // to 10, kept in the fourth byte of the brick's colour, 5
            // unless said; 6 is the 0.6 asked for. Whether more is more
            // see-through or less on glass is not something the writer's
            // source says.
            // Ice is the same glass at 3: by the reading that makes water's
            // 6 a transparency of 0.6, half as see-through.
            // A dressed pine's baubles, tinsel and star are the game's metallic material, and its lights the glowing one.
            material: match stuff {
                Stuff::Water | Stuff::Ice => brdb::assets::materials::GLASS,
                Stuff::Metal => brdb::assets::materials::METALLIC,
                Stuff::Lamp | Stuff::Glow => brdb::assets::materials::GLOW,
                _ => brdb::assets::materials::PLASTIC,
            },
            material_intensity: match stuff {
                Stuff::Water => WATER_INTENSITY,
                Stuff::Ice => ICE_INTENSITY,
                // (as bright as a glowing brick goes)
                Stuff::Lamp | Stuff::Glow => 10,
                _ => 5,
            },
            ..Default::default()
        };
        Ok(if stuff == Stuff::Lamp { brick.with_component(lamp_light()) } else { brick })
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
        world.bricks.push(brick_for(&solver, cell, Some([90, 150, 220]), [0; 3], Stuff::Solid)?);
    }
    let arch = build(&Spec { points: vec![DVec3::new(-7.5, 0.5, 0.5), DVec3::new(8.5, 0.5, 0.5)], thick: 3.0, depth: 3, ..Spec::default() })?;
    let on_grid: Vec<Brick> = arch.bricks.iter().map(|cell| brick_for(&solver, cell, Some([230, 150, 60]), [0; 3], Stuff::Solid)).collect::<Result<_, _>>()?;
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

    /// Read back out of the file itself, chunk by chunk: what the game is given.
    #[test]
    fn blades_of_grass_are_written_to_be_walked_through_and_the_soil_is_not() {
        use brickloft_core::spec::{build, Form};
        use brickloft_core::tree::{Kind, Tree, LEAF, LEAF_DARK, LEAF_LIGHT, TRUNK};
        use brickloft_core::DVec3;
        let grass = Spec { form: Form::Tree, points: vec![DVec3::new(-15.5, 0.5, 0.5), DVec3::new(0.5, 0.5, 0.5), DVec3::new(15.5, 0.5, 0.5)], radius: 10.0, rise: Some(8.0), tree: Tree::of(Kind::Grass), pulls: vec![(DVec3::new(4.5, 0.5, 0.5), DVec3::new(14.0, 0.0, 9.0))], ..Spec::default() };
        let built = build(&grass).unwrap();
        let written = |spec: &Spec| {
            let bricks: Vec<Painted> = built.bricks.iter().map(|c| (*c, None, 0, [0; 3])).collect();
            let world = prefab(&bricks, &[(spec.clone(), Paint::default(), [0; 3])]).unwrap();
            let archive = Brz::read_slice(&world.to_brz_vec().unwrap()).unwrap().into_reader();
            let data = archive.read_global_data().unwrap();
            let mut all = Vec::new();
            for chunk in archive.brick_chunk_index(1).unwrap() {
                let soa = archive.brick_chunk_soa(1, chunk.index).unwrap();
                all.extend(soa.iter_bricks(chunk.index, data.clone()).map(|b| b.unwrap()));
            }
            assert_eq!(all.len(), built.bricks.len());
            all
        };
        let file = written(&grass);
        let (soft, solid) = (file.iter().filter(|b| !b.collision.player).count(), file.iter().filter(|b| b.collision.player).count());
        let (blades, soil) = (built.shades.iter().filter(|s| matches!(**s, LEAF | LEAF_DARK | LEAF_LIGHT)).count(), built.shades.iter().filter(|s| **s == TRUNK).count());
        // every brick of a blade is walked through, and every brick of soil is solid, on the mound as on the flat
        assert!(soft == blades && solid >= soil, "{soft} walked through of {blades} of blade; {solid} solid for {soil} of soil");
        let of = stuff_of(&grass);
        for (cell, shade) in built.bricks.iter().zip(&built.shades) {
            assert_eq!(of(cell) == Stuff::Blade, matches!(*shade, LEAF | LEAF_DARK | LEAF_LIGHT), "a brick of colour {shade} at {:?}", cell.min);
        }

        // A pond: dug below the water's level, its water is written as glass that stops nothing, in few large bricks; sand, bed and stones are solid plastic.
        let pond = Spec { pulls: vec![(DVec3::new(0.5, 0.5, 0.5), DVec3::new(16.0, 0.0, -9.0))], radius: 16.0, ..grass.clone() };
        let dug = build(&pond).unwrap();
        let bricks: Vec<Painted> = dug.bricks.iter().map(|c| (*c, None, 0, [0; 3])).collect();
        let world = prefab(&bricks, &[(pond.clone(), Paint::default(), [0; 3])]).unwrap();
        let water: Vec<(&Brick, &brickloft_core::shapes::CellBrick)> = world.bricks.iter().zip(&dug.bricks).zip(&dug.shades).filter(|(_, s)| **s == brickloft_core::tree::WATER).map(|(b, _)| b).collect();
        assert!(!water.is_empty() && water.len() < 60, "{} bricks of water", water.len());
        let cells: i32 = water.iter().map(|(_, c)| c.ext[0] * c.ext[1] * c.ext[2]).sum();
        assert!(cells > 300 && cells / water.len() as i32 >= 8, "{cells} cells of water in {} bricks", water.len());
        for (brick, _) in &water {
            assert_eq!(brick.material, brdb::assets::materials::GLASS);
            assert_eq!(brick.material_intensity, 6);
            assert!(!brick.collision.player && !brick.collision.weapon && !brick.collision.physics && !brick.collision.interact);
        }
        // frozen, the same bricks are ice: glass still, less see-through, and they stop the player
        let frozen = prefab(&bricks, &[(Spec { fill: true, ..pond.clone() }, Paint::default(), [0; 3])]).unwrap();
        let ice: Vec<&Brick> = frozen.bricks.iter().zip(&dug.shades).filter(|(_, s)| **s == brickloft_core::tree::WATER).map(|(b, _)| b).collect();
        assert_eq!(ice.len(), water.len());
        assert!(ice.iter().all(|b| b.collision.player && b.collision.physics && b.collision.weapon && b.material == brdb::assets::materials::GLASS && b.material_intensity == 3));

        {
        // A dressed pine: its baubles, tinsel and star are metal, each of its lights glows and carries a light, and unticked they glow without one. Read back out of the file.
        let pine = Spec { form: Form::Tree, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: 20.0, rise: Some(70.0), tree: Tree { extras: true, baubles: 60, tinsel: true, lights: 1, ..Tree::of(Kind::Pine) }, ..Spec::default() };
        let tree = build(&pine).unwrap();
        let bricks: Vec<Painted> = tree.bricks.iter().map(|c| (*c, None, 0, [0; 3])).collect();
        let world = prefab(&bricks, &[(pine.clone(), Paint::default(), [0; 3])]).unwrap();
        let of = |shade: u8| -> Vec<&Brick> { world.bricks.iter().zip(&tree.shades).filter(|(_, s)| **s == shade).map(|(b, _)| b).collect() };
        for shade in [brickloft_core::tree::STAR, brickloft_core::tree::EXTRA, brickloft_core::tree::BAUBLE_GOLD, brickloft_core::tree::TINSEL] {
            // (All but a few in a hundred. What a sloped brick is, is asked at its middle here and where it is deepest in the tree when it is coloured, and at the edge of a part the two may differ.)
            let metal = of(shade).iter().filter(|b| b.material == brdb::assets::materials::METALLIC && b.components.is_empty() && b.collision.player).count();
            assert!(of(shade).len() > 3 && metal * 100 >= of(shade).len() * 94, "part {shade}: {} bricks, {metal} of them metal", of(shade).len());
        }
        let lamps = of(brickloft_core::tree::FLAME);
        assert!((3..=brickloft_core::tree::MOST_LIGHTS).contains(&lamps.len()) && lamps.iter().all(|b| b.material == brdb::assets::materials::GLOW && b.components.len() == 1), "{} lamps", lamps.len());
        assert!(of(LEAF).iter().all(|b| b.material == brdb::assets::materials::PLASTIC && b.components.is_empty()));
        let archive = Brz::read_slice(&world.to_brz_vec().unwrap()).unwrap().into_reader();
        let mut lights = Vec::new();
        for chunk in archive.brick_chunk_index(1).unwrap() {
            if chunk.num_components > 0 {
                lights.extend(archive.component_chunk_soa(1, chunk.index).unwrap().1.into_iter().map(|c| c.to_string()));
            }
        }
        assert!(lights.len() >= lamps.len() && lights.len() <= brickloft_core::tree::MOST_LIGHTS && lights.iter().all(|c| c.contains("PointLight") && c.contains("Brightness: 25f32")), "{} lights in the file for {} lamps: {:?}", lights.len(), lamps.len(), lights.first());
        let dark = prefab(&bricks, &[(Spec { tree: Tree { shine: false, ..pine.tree }, ..pine.clone() }, Paint::default(), [0; 3])]).unwrap();
        assert!(dark.bricks.iter().all(|b| b.components.is_empty()) && dark.bricks.iter().filter(|b| b.material == brdb::assets::materials::GLOW).count() >= lamps.len());
        }

        // Two levels: a stream's water is water like a pond's, and ice with it when frozen.
        let touch = |x: f64, range: f64, amount: f64| (DVec3::new(x + 0.5, 0.5, 0.5), DVec3::new(range, 0.0, amount));
        let hilly = Spec { points: vec![DVec3::new(-55.5, 0.5, 0.5), DVec3::new(55.5, 0.5, 0.5)], radius: 30.0, tube: true, pulls: vec![touch(-25.0, 34.0, 16.0), touch(-25.0, 16.0, -14.0), touch(30.0, 20.0, -10.0)], ..grass.clone() };
        let stepped = build(&hilly).unwrap();
        let bricks: Vec<Painted> = stepped.bricks.iter().map(|c| (*c, None, 0, [0; 3])).collect();
        let of = |spec: Spec| -> Vec<Brick> { prefab(&bricks, &[(spec, Paint::default(), [0; 3])]).unwrap().bricks.into_iter().zip(&stepped.bricks).zip(&stepped.shades).filter(|((_, c), s)| **s == brickloft_core::tree::WATER && c.min[0] > -10 && c.min[0] + c.ext[0] < 8).map(|((b, _), _)| b).collect() };
        let (running, still) = (of(hilly.clone()), of(Spec { fill: true, ..hilly.clone() }));
        assert!(running.len() > 5 && running.iter().all(|b| !b.collision.player && b.material == brdb::assets::materials::GLASS && b.material_intensity == 6), "{} bricks of stream", running.len());
        assert!(still.len() == running.len() && still.iter().all(|b| b.collision.player && b.material_intensity == 3));
        let back = Brz::read_slice(&frozen.to_brz_vec().unwrap()).unwrap().into_reader();
        let data = back.read_global_data().unwrap();
        let read: Vec<Brick> = back.brick_chunk_index(1).unwrap().into_iter().flat_map(|chunk| back.brick_chunk_soa(1, chunk.index).unwrap().iter_bricks(chunk.index, data.clone()).map(|b| b.unwrap()).collect::<Vec<_>>()).collect();
        assert_eq!(read.iter().filter(|b| b.material == brdb::assets::materials::GLASS && b.collision.player && b.material_intensity == 3).count(), ice.len(), "the ice as the file has it");

        // A tree whose greenery is walked through: every brick of leaf stops nothing of the player's and every brick of trunk and limb does; solid all through unless asked.
        let oak = Spec { form: Form::Tree, points: vec![DVec3::new(0.5, 0.5, 0.5)], radius: 30.0, rise: Some(56.0), tree: Tree::of(Kind::Oak), ..Spec::default() };
        let grown = build(&oak).unwrap();
        let bricks: Vec<Painted> = grown.bricks.iter().map(|c| (*c, None, 0, [0; 3])).collect();
        let solid = prefab(&bricks, &[(oak.clone(), Paint::default(), [0; 3])]).unwrap();
        assert!(solid.bricks.iter().all(|b| b.collision.player));
        let soft = prefab(&bricks, &[(Spec { tube: true, ..oak.clone() }, Paint::default(), [0; 3])]).unwrap();
        let back = Brz::read_slice(&soft.to_brz_vec().unwrap()).unwrap().into_reader();
        let data = back.read_global_data().unwrap();
        let read: Vec<Brick> = back.brick_chunk_index(1).unwrap().into_iter().flat_map(|chunk| back.brick_chunk_soa(1, chunk.index).unwrap().iter_bricks(chunk.index, data.clone()).map(|b| b.unwrap()).collect::<Vec<_>>()).collect();
        let (leaf, wood) = (grown.shades.iter().filter(|s| matches!(**s, LEAF | LEAF_DARK | LEAF_LIGHT)).count(), grown.shades.iter().filter(|s| matches!(**s, TRUNK | brickloft_core::tree::LIMB)).count());
        assert!(leaf > 300 && wood > 20, "{leaf} of leaf, {wood} of wood");
        assert_eq!((read.iter().filter(|b| !b.collision.player).count(), read.iter().filter(|b| b.collision.player).count()), (leaf, wood), "an oak's leaves and wood in the file");
        for (brick, shade) in soft.bricks.iter().zip(&grown.shades) {
            assert_eq!(brick.collision.player, matches!(*shade, TRUNK | brickloft_core::tree::LIMB), "a brick of colour {shade}");
        }
        for ((brick, _), shade) in world.bricks.iter().zip(&dug.bricks).zip(&dug.shades).filter(|(_, s)| matches!(**s, brickloft_core::tree::SAND | brickloft_core::tree::STONE | brickloft_core::tree::BED | TRUNK)) {
            // (the heads of the reeds are the bed's colour and solid too)
            assert!(brick.collision.player && brick.material == brdb::assets::materials::PLASTIC && brick.material_intensity == 5, "a brick of colour {shade}");
        }
        // a brick walked through stops no weapon and no physics object either, and can still be pointed at
        assert!(file.iter().filter(|b| !b.collision.player).all(|b| !b.collision.weapon && !b.collision.physics && b.collision.interact));
        assert!(file.iter().filter(|b| b.collision.player).all(|b| b.collision.weapon && b.collision.physics && b.collision.interact));
        // asked to be solid, turned, or not grass at all: every brick stops the player
        for other in [Spec { tree: Tree { extras: true, ..grass.tree }, ..grass.clone() }, Spec { turned: 3, ..grass.clone() }, Spec { tree: Tree::of(Kind::Reeds), ..grass.clone() }] {
            assert!(written(&other).iter().all(|b| b.collision.player));
        }
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
