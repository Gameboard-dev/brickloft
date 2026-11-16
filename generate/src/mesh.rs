//! A picture made into a mesh, with TripoSR through Lux3D.
//!
//! The steps: read the picture (bounded, since it is anyone's file); cut the
//! object out and centre it (`cutout`); run the model, which turns the
//! picture into a field of density and colour; draw the surface of that
//! field as triangles at a chosen fineness; tidy the mesh (`level`); write
//! it as a `.glb`.
//!
//! Lux3D is used for the model and for writing the file (`glb`). Its own program
//! (`lux3d-cli`) does the same steps in one call; they are taken apart here
//! so that the mesh can be levelled between drawing and writing, and so
//! that progress can be said between them.
//!
//! **The file.** Binary glTF with one mesh: positions, a colour for each
//! vertex and 32-bit indices, no normals. Y is up and the object's front
//! faces +Z, as the format says. The colours are written as TripoSR gives
//! them, which is as they are to be shown on a screen, and not converted to
//! linear light as the format's text asks of `COLOR_0`: the editor's reader
//! takes them as shown, as do the programs TripoSR's own output is made for,
//! so converting them here would make every reader that does so show them
//! dark. The result says so (`"colours": "srgb"`).
//! https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#meshes

use crate::cutout::{self, Cut};
use crate::device::Chosen;
use crate::error::{Error, Result};
use crate::files::{self, Part};
use crate::glb;
use crate::level::{self, Mesh, Tilt};
use crate::models::TRIPOSR;
use crate::protocol::Out;
use crate::rank;
use lux3d_core::runtime::TripoSrPipeline;
use lux3d_core::ModelAssetOptions;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The longest file that is read as a picture.
pub const MAX_PICTURE_BYTES: u64 = 64 << 20;
/// The most pixels along a side. A phone's camera gives 4000 to 8000.
pub const MAX_PICTURE_SIDE: u32 = 8192;
/// The density at which the field is taken to be the object's surface, the model's own setting.
const SURFACE: f32 = 25.0;
/// How many points of the field are asked of the card at once, Lux3D's own setting.
const AT_ONCE: usize = 8192;
/// A picture with at least this share of see-through pixels has been cut out already.
const ALREADY_CUT: f32 = 0.02;

fn failed(step: &'static str) -> impl Fn(lux3d_core::Lux3dError) -> Error {
    move |e| Error::Model { step, why: e.to_string() }
}

/// Reads a picture from a file that may be anything.
pub fn read_picture(path: &Path) -> Result<image::RgbaImage> {
    let bytes = std::fs::metadata(path).map_err(|source| Error::Io { doing: "read", path: path.to_path_buf(), source })?.len();
    if bytes > MAX_PICTURE_BYTES {
        return Err(Error::PictureFileTooLarge { path: path.to_path_buf(), bytes, most: MAX_PICTURE_BYTES });
    }
    let unreadable = |e: image::ImageError| Error::PictureUnreadable { path: path.to_path_buf(), why: e.to_string() };
    let open = || -> Result<image::ImageReader<std::io::BufReader<std::fs::File>>> {
        // by what the file begins with, not by its name
        image::ImageReader::open(path).and_then(|r| r.with_guessed_format()).map_err(|source| Error::Io { doing: "read", path: path.to_path_buf(), source })
    };
    // The size is in the file's first bytes: it is read, and refused if too large, before any pixel is.
    let (width, height) = open()?.into_dimensions().map_err(unreadable)?;
    if width > MAX_PICTURE_SIDE || height > MAX_PICTURE_SIDE {
        return Err(Error::PictureTooLarge { path: path.to_path_buf(), width, height, most: MAX_PICTURE_SIDE });
    }
    let mut reader = open()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PICTURE_SIDE);
    limits.max_image_height = Some(MAX_PICTURE_SIDE);
    // what the decoder may hold while it works: the largest picture allowed, as 16-bit colour with transparency, and as much again
    limits.max_alloc = Some(2 * 8 * MAX_PICTURE_SIDE as u64 * MAX_PICTURE_SIDE as u64);
    reader.limits(limits);
    Ok(reader.decode().map_err(unreadable)?.into_rgba8())
}

/// The model's files, each checked to be there and the length it should be.
pub fn weights_in(weights: &Path) -> Result<PathBuf> {
    let folder = TRIPOSR.folder(weights);
    for file in TRIPOSR.files {
        let path = folder.join(file.local);
        match files::length(&path) {
            None => return Err(Error::WeightsMissing { model: TRIPOSR.name, file: path }),
            Some(found) if found != file.bytes => return Err(Error::WeightsWrongSize { file: path, expected: file.bytes, found }),
            Some(_) => {}
        }
    }
    Ok(folder)
}

/// How the object was told from its ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutBy {
    /// By the colours (`cutout::cut_out`).
    Ground,
    /// By the picture's own transparency.
    Transparency,
}

/// The object alone, ready for the model. `asked` is false for `--no-cutout`.
pub fn prepared(picture: &image::RgbaImage, path: &Path, asked: bool) -> Result<(Cut, CutBy)> {
    let see_through = picture.pixels().filter(|p| p[3] < 128).count() as f32 / (picture.width() as f32 * picture.height() as f32).max(1.0);
    if !asked || see_through >= ALREADY_CUT {
        // A picture that is partly see-through has been cut out by someone: their cut is better than a guess from the
        // colours, and the colours of see-through pixels are whatever the program that made them left there.
        return cutout::already_cut(picture).map(|cut| (cut, CutBy::Transparency)).ok_or_else(|| if see_through > 0.0 { Error::NothingToCutOut { path: path.to_path_buf() } } else { Error::NoTransparency { path: path.to_path_buf() } });
    }
    cutout::cut_out(picture).map(|cut| (cut, CutBy::Ground)).ok_or_else(|| Error::NothingToCutOut { path: path.to_path_buf() })
}

/// What is to be done with a mesh once the model has drawn it.
pub struct Tidy {
    pub level: bool,
    /// How far above level the picture looks to have been taken from, in degrees, if it looks taken from high up
    /// (see `cutout::rise`): what `level::flattest` goes by when the mesh has no flat base.
    pub seen_from: Option<f32>,
}

/// What tidying did, for the result.
#[derive(Debug, PartialEq)]
pub struct Tidied {
    pub pieces_dropped: usize,
    pub tilt: Tilt,
    /// Whether the mesh was turned to stand level.
    pub levelled: bool,
    /// The tip that was undone by laying the mesh flattest, when it had no flat base to go by.
    pub laid_flat: Option<f32>,
}

/// Drops crumbs, stands the mesh level if it can be told how, sets it on the
/// ground and turns it Y up. In the model's frame on the way in.
pub fn tidy(mesh: &mut Mesh, how: &Tidy) -> Tidied {
    let pieces_dropped = mesh.drop_loose_pieces();
    let tilt = level::tilt(mesh);
    let mut laid_flat = None;
    let levelled = match tilt {
        Tilt::Tipped(degrees) if how.level => {
            mesh.tip(-degrees);
            true
        }
        Tilt::Unknown if how.level => {
            laid_flat = how.seen_from.and_then(|seen_from| level::flattest(mesh, seen_from));
            if let Some(degrees) = laid_flat {
                mesh.tip(-degrees);
            }
            laid_flat.is_some()
        }
        _ => false,
    };
    mesh.set_on_ground();
    mesh.to_y_up();
    Tidied { pieces_dropped, tilt, levelled, laid_flat }
}

/// What the result says about the tilt, and a sentence when there is something a person should know.
fn tilt_json(tidied: &Tidied, view: rank::View) -> (Value, Option<String>) {
    match tidied.tilt {
        Tilt::Level => (json!({ "state": "level", "degrees": 0.0 }), None),
        Tilt::Tipped(degrees) if tidied.levelled => (json!({ "state": "levelled", "degrees": (degrees as f64 * 10.0).round() / 10.0 }), Some(format!("the mesh came out tipped by {:.0} degrees, as from a picture taken from above, and was stood level", degrees.abs()))),
        Tilt::Tipped(degrees) => (json!({ "state": "tilted", "degrees": (degrees as f64 * 10.0).round() / 10.0 }), Some(format!("the mesh is tipped by {:.0} degrees and was left so, as asked", degrees.abs()))),
        Tilt::Unknown if tidied.laid_flat.is_some() => {
            let degrees = tidied.laid_flat.unwrap_or(0.0);
            (json!({ "state": "levelled", "degrees": (degrees as f64 * 10.0).round() / 10.0, "by": "lying flattest" }), Some(format!("the picture looks taken from high above and the mesh came out standing on end; it has no flat base, so it was laid the way it lies lowest ({degrees:.0} degrees), which may not be exactly level")))
        }
        Tilt::Unknown => {
            let note = (view == rank::View::High).then(|| "the picture looks taken from too high, and the mesh has no flat base to level it by: it may be tilted".to_string());
            (json!({ "state": "unknown", "degrees": null }), note)
        }
    }
}

/// The model, loaded: what `serve` keeps between requests.
pub struct Sculptor {
    pipeline: TripoSrPipeline,
    device: candle_core::Device,
}

impl Sculptor {
    pub fn load(weights: &Path, chosen: &Chosen, out: &Out) -> Result<Sculptor> {
        let folder = weights_in(weights)?;
        out.progress("load", 0, 1, json!({ "what": "weights" }));
        // `canonical_dir` given: Lux3D then reads that folder and nothing else. Without it, it would look in its own
        // cache and download what it did not find.
        let pipeline = TripoSrPipeline::load(ModelAssetOptions { canonical_dir: Some(folder), cache_dir: None }).map_err(failed("loading the weights"))?;
        out.progress("load", 1, 1, json!({}));
        Ok(Sculptor { pipeline, device: chosen.device.clone() })
    }

    /// The mesh of the object in `cut`, in the model's frame (Z up, front towards +X).
    /// And how many cells of scraps were cleared from the field before it was drawn.
    fn sculpted(&self, cut: &Cut, beside: &Path, resolution: u32, out: &Out) -> Result<(Mesh, usize)> {
        // Lux3D reads its picture from a file. It is written beside the mesh that is being made, where it is known
        // that files can be written, under a part-file's name, and removed when it has been read.
        let ready = Part::of(&beside.with_extension("png"))?;
        cut.picture.save_with_format(ready.path(), image::ImageFormat::Png).map_err(|e| Error::Model { step: "writing the cut-out", why: e.to_string() })?;
        out.progress("reconstruct", 0, 3, json!({ "what": "reading the picture" }));
        let inputs = self.pipeline.prepare_inputs_from_path(ready.path(), &self.device).map_err(failed("reading the cut-out"))?;
        drop(ready);
        let (_, _, _, _, field) = self.pipeline.scene_codes(&inputs.preprocessed_image).map_err(failed("making the mesh"))?;
        out.progress("reconstruct", 1, 3, json!({ "what": "drawing the surface" }));
        let mut grid = self.pipeline.neural.build_density_grid_cpu(&self.pipeline, &field, resolution, SURFACE, AT_ONCE).map_err(failed("drawing the surface"))?;
        let scraps = level::clear_scraps(&mut grid.density_values, resolution as usize, SURFACE);
        let surface = self.pipeline.geometry.assemble_cpu(&grid).map_err(failed("drawing the surface"))?;
        if surface.faces.is_empty() {
            return Err(Error::EmptyMesh);
        }
        out.progress("reconstruct", 2, 3, json!({ "what": "colouring" }));
        let colours = self.pipeline.neural.sample_vertex_colors_cpu(&self.pipeline, &field, &surface.vertices, AT_ONCE).map_err(failed("colouring the mesh"))?;
        out.progress("reconstruct", 3, 3, json!({}));
        let mesh = Mesh::from_flat(&surface.vertices, &surface.faces, &colours).ok_or(Error::Model { step: "making the mesh", why: "the model gave numbers that do not make a mesh".into() })?;
        Ok((mesh, scraps))
    }

    /// Reads `picture`, makes its mesh and writes it to `to`.
    pub fn mesh(&self, picture: &Path, to: &Path, cutout_asked: bool, how: &Tidy, resolution: u32, out: &Out) -> Result<Value> {
        let read = read_picture(picture)?;
        let (width, height) = read.dimensions();
        let (cut, cut_by) = prepared(&read, picture, cutout_asked)?;
        out.progress("cutout", 1, 1, json!({}));
        let judged = rank::judge(Some(&cut.report));

        let (mut mesh, scraps) = self.sculpted(&cut, to, resolution, out)?;
        // the sine of the camera's height is what the lower outline's rise estimates
        let seen_from = (rank::View::of(cut.report.rise) == rank::View::High).then(|| cut.report.rise.min(1.0).asin().to_degrees());
        let tidied = tidy(&mut mesh, &Tidy { level: how.level, seen_from });
        if mesh.triangles.is_empty() {
            return Err(Error::EmptyMesh);
        }
        let (tilt, tilt_note) = tilt_json(&tidied, rank::View::of(cut.report.rise));
        let (lo, hi) = mesh.bounds();

        let part = Part::of(to)?;
        glb::write(&mesh, part.path())?;
        let bytes = files::length(part.path()).unwrap_or(0);
        part.keep()?;

        let mut notes: Vec<String> = judged.problems.iter().map(|p| format!("the picture: {p}")).collect();
        notes.extend(tilt_note);
        let round = |v: f32| (v as f64 * 10_000.0).round() / 10_000.0;
        Ok(json!({
            "file": to,
            "bytes": bytes,
            "vertices": mesh.points.len(),
            "triangles": mesh.triangles.len(),
            "resolution": resolution,
            "up": "y",
            "front": "+z",
            "colours": "srgb",
            "size": [round(hi[0] - lo[0]), round(hi[1] - lo[1]), round(hi[2] - lo[2])],
            "picture": { "file": picture, "width": width, "height": height },
            "cut_by": match cut_by { CutBy::Ground => "ground", CutBy::Transparency => "transparency" },
            "cutout": rank::report_json(&cut.report),
            "pieces_dropped": tidied.pieces_dropped,
            "scrap_cells_cleared": scraps,
            "tilt": tilt,
            "notes": notes,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cutout::tests::{boxed, studio};
    use crate::files::tests::Scratch;
    use crate::level::tests::{box_mesh, tipped};

    #[test]
    fn a_tipped_mesh_is_stood_level_set_down_and_turned_y_up() {
        let mut mesh = tipped(box_mesh([1.0, 0.8, 0.6], 0.02), 25.0);
        let tidied = tidy(&mut mesh, &Tidy { level: true, seen_from: None });
        assert!(tidied.levelled && matches!(tidied.tilt, Tilt::Tipped(d) if (d - 25.0).abs() <= 1.0), "{tidied:?}");
        let (lo, hi) = mesh.bounds();
        assert!(lo[1].abs() < 1e-4, "it stands on the ground: Y is up and its lowest point is at 0");
        assert!((hi[1] - 0.6).abs() < 0.03 && (hi[0] - lo[0] - 0.8).abs() < 0.03 && (hi[2] - lo[2] - 1.0).abs() < 0.03, "its own height, width and depth again: {lo:?} {hi:?}");
        let (tilt, note) = tilt_json(&tidied, rank::View::High);
        assert_eq!(tilt["state"], "levelled");
        let Tilt::Tipped(found) = tidied.tilt else { unreachable!() };
        assert_eq!(note.unwrap(), format!("the mesh came out tipped by {found:.0} degrees, as from a picture taken from above, and was stood level"));

        // asked not to level: found and reported, and left tipped
        let mut mesh = tipped(box_mesh([1.0, 0.8, 0.6], 0.02), 25.0);
        let tidied = tidy(&mut mesh, &Tidy { level: false, seen_from: Some(25.0) });
        assert!(!tidied.levelled);
        let (lo, hi) = mesh.bounds();
        assert!(hi[1] - lo[1] > 0.8, "still tipped: taller than the box is");
        assert_eq!(tilt_json(&tidied, rank::View::High).0["state"], "tilted");
    }

    #[test]
    fn a_mesh_with_no_base_from_a_high_picture_is_said_to_be_perhaps_tilted() {
        let tidied = Tidied { pieces_dropped: 0, tilt: Tilt::Unknown, levelled: false, laid_flat: None };
        let (tilt, note) = tilt_json(&tidied, rank::View::High);
        assert_eq!(tilt, json!({ "state": "unknown", "degrees": null }));
        assert_eq!(note.unwrap(), "the picture looks taken from too high, and the mesh has no flat base to level it by: it may be tilted");
        assert_eq!(tilt_json(&tidied, rank::View::Level).1, None);

        // no flat base, but laid the way it lies lowest (`level::flattest`): said as levelled, and how
        let laid = Tidied { pieces_dropped: 0, tilt: Tilt::Unknown, levelled: true, laid_flat: Some(50.0) };
        let (tilt, note) = tilt_json(&laid, rank::View::High);
        assert_eq!(tilt, json!({ "state": "levelled", "degrees": 50.0, "by": "lying flattest" }));
        assert!(note.unwrap().ends_with("it was laid the way it lies lowest (50 degrees), which may not be exactly level"));
        assert_eq!(tilt_json(&Tidied { pieces_dropped: 0, tilt: Tilt::Level, levelled: false, laid_flat: None }, rank::View::High), (json!({ "state": "level", "degrees": 0.0 }), None));
    }

    #[test]
    fn a_picture_is_cut_by_its_transparency_when_it_has_some() {
        let path = Path::new("picture.png");
        assert_eq!(prepared(&boxed(), path, true).unwrap().1, CutBy::Ground);
        assert!(matches!(prepared(&boxed(), path, false), Err(Error::NoTransparency { .. })), "--no-cutout on a picture with no transparency");
        assert!(matches!(prepared(&studio(128, 128), path, true), Err(Error::NothingToCutOut { .. })));
        let mut cut = image::RgbaImage::new(200, 200);
        for y in 50..150 {
            for x in 60..140 {
                cut.put_pixel(x, y, image::Rgba([200, 30, 30, 255]));
            }
        }
        assert_eq!(prepared(&cut, path, true).unwrap().1, CutBy::Transparency, "found without being told");
        assert_eq!(prepared(&cut, path, false).unwrap().1, CutBy::Transparency);
    }

    #[test]
    fn a_file_that_is_not_a_picture_or_is_too_large_is_refused() {
        let scratch = Scratch::new("pictures");
        let good = scratch.0.join("good.png");
        boxed().save(&good).unwrap();
        assert_eq!(read_picture(&good).unwrap().dimensions(), (256, 256));
        // a PNG under a name that says JPEG is read by what it is
        let misnamed = scratch.0.join("misnamed.jpg");
        std::fs::copy(&good, &misnamed).unwrap();
        assert!(read_picture(&misnamed).is_ok());

        let text = scratch.0.join("text.png");
        std::fs::write(&text, b"this is not a picture").unwrap();
        assert!(matches!(read_picture(&text), Err(Error::PictureUnreadable { .. })));
        assert!(matches!(read_picture(&scratch.0.join("missing.png")), Err(Error::Io { .. })));

        // a PNG that says it is 60 000 pixels a side, in 100 bytes: refused by its header, with nothing allocated for it
        let mut huge = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut header = b"IHDR".to_vec();
        header.extend_from_slice(&60_000u32.to_be_bytes());
        header.extend_from_slice(&60_000u32.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        // the header, then the start of the pixel data: a chunk is its length, its name and bytes, and their checksum
        for (length, chunk) in [(13u32, header), (4, [&b"IDAT"[..], &[0x78, 0x9C, 0x03, 0x00]].concat())] {
            huge.extend_from_slice(&length.to_be_bytes());
            huge.extend_from_slice(&chunk);
            huge.extend_from_slice(&png_crc(&chunk).to_be_bytes());
        }
        let path = scratch.0.join("huge.png");
        std::fs::write(&path, &huge).unwrap();
        match read_picture(&path) {
            Err(Error::PictureTooLarge { width: 60_000, height: 60_000, most: MAX_PICTURE_SIDE, .. }) => {}
            other => panic!("expected too large, got {:?}", other.map(|p| p.dimensions())),
        }
    }

    /// The checksum a PNG chunk ends with (CRC-32, https://www.w3.org/TR/png/#5CRC-algorithm).
    fn png_crc(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = if crc & 1 == 1 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
            }
        }
        !crc
    }
}
