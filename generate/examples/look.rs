//! Pictures for a person to judge the generator by. Not part of the program.
//!
//!   look views <mesh.glb> <out.png>
//!       The mesh from five sides: as the picture saw it, from its left,
//!       from behind, from behind and to one side, and from above.
//!   look cut <picture> <out.png>
//!       The picture with its ground cut away, as TripoSR is given it, over
//!       the mid grey TripoSR puts behind it. Prints what the cut-out found.
//!   look sheet <out.png> <columns> <tile> <in.png>...
//!       The pictures side by side, each scaled to `tile` pixels.
//!   look stack <out.png> <in.png>...
//!       The pictures one under another at one width: several meshes' views
//!       to be compared.
//!   look birefnet <model.safetensors> <picture> <out.png>
//!       The picture cut out by BiRefNet (https://huggingface.co/ZhengPeng7/BiRefNet,
//!       MIT), a model made for telling an object from its ground, run by
//!       the code Lux3D has for a model of the same design. The generator
//!       does not use it; this is how it was tried (docs/generate.md says
//!       what came of it). The weights are not fetched by anything here.
//!
//! Whether a mesh is good is decided by looking at it, and from the back,
//! which the picture never showed. These are the views that decision was
//! made from for docs/generate.md; the trial drew the same views with a
//! throwaway tool of its own, from which the rasteriser here is taken.
//!
//!   cargo run --release --example look -- views barrel.glb barrel-views.png

use generate::level::Mesh;
use generate::{cutout, glb, mesh, rank};
use image::{Rgba, RgbaImage};
use std::path::Path;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

const TILE: u32 = 384;
const GROUND: Rgba<u8> = Rgba([40, 44, 52, 255]);

/// Draws the mesh into one tile, looking from `yaw` degrees round (0 is from the front, +Z) and `pitch` degrees above
/// level, without perspective. Both sides of a triangle are lit, so a mesh wound the wrong way still shows.
fn draw(sheet: &mut RgbaImage, left: u32, mesh: &Mesh, yaw: f32, pitch: f32) {
    let (lo, hi) = mesh.bounds();
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let size = mesh.size().max(1e-9);
    let (sy, cy) = yaw.to_radians().sin_cos();
    let (sp, cp) = pitch.to_radians().sin_cos();
    // into the viewer's frame: x to the right, y up, z towards the viewer
    let view = |p: V3| -> V3 {
        let p = [(p[0] - centre[0]) / size, (p[1] - centre[1]) / size, (p[2] - centre[2]) / size];
        let x = p[0] * cy - p[2] * sy;
        let z = p[0] * sy + p[2] * cy;
        [x, p[1] * cp - z * sp, p[1] * sp + z * cp]
    };
    let scale = TILE as f32 * 0.62;
    let half = TILE as f32 * 0.5;
    let mut depth = vec![f32::MIN; (TILE * TILE) as usize];
    let light = unit([0.4, 0.7, 0.6]);
    for t in &mesh.triangles {
        let w = t.map(|i| view(mesh.points[i as usize]));
        let c = t.map(|i| mesh.colours[i as usize]);
        let shade = 0.35 + 0.65 * dot(unit(cross(sub(w[1], w[0]), sub(w[2], w[0]))), light).abs();
        let s = w.map(|p| [half + p[0] * scale, half - p[1] * scale]);
        let area = (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
        if area.abs() < 1e-9 {
            continue;
        }
        let bound = |k: usize, most: bool| {
            let v = s.iter().map(|p| p[k]).fold(if most { f32::MIN } else { f32::MAX }, if most { f32::max } else { f32::min });
            (if most { v.ceil() } else { v.floor() }).clamp(0.0, TILE as f32 - 1.0) as u32
        };
        for y in bound(1, false)..=bound(1, true) {
            for x in bound(0, false)..=bound(0, true) {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let b0 = ((s[1][0] - px) * (s[2][1] - py) - (s[2][0] - px) * (s[1][1] - py)) / area;
                let b1 = ((s[2][0] - px) * (s[0][1] - py) - (s[0][0] - px) * (s[2][1] - py)) / area;
                let b2 = 1.0 - b0 - b1;
                if b0 < 0.0 || b1 < 0.0 || b2 < 0.0 {
                    continue;
                }
                let z = b0 * w[0][2] + b1 * w[1][2] + b2 * w[2][2];
                let i = (y * TILE + x) as usize;
                if z <= depth[i] {
                    continue;
                }
                depth[i] = z;
                let channel = |k: usize| ((b0 * c[0][k] + b1 * c[1][k] + b2 * c[2][k]) * shade * 255.0).clamp(0.0, 255.0) as u8;
                sheet.put_pixel(left + x, y, Rgba([channel(0), channel(1), channel(2), 255]));
            }
        }
    }
    // the ground the mesh stands on (y = 0 in the file), as a line across the level views
    if pitch.abs() < 45.0 {
        let y = (half - ((0.0 - centre[1]) / size * cp) * scale).round();
        if (0.0..TILE as f32).contains(&y) {
            for x in (0..TILE).step_by(6) {
                if depth[(y as u32 * TILE + x) as usize] == f32::MIN {
                    sheet.put_pixel(left + x, y as u32, Rgba([120, 126, 138, 255]));
                }
            }
        }
    }
}

/// A picture scaled into a tile, over the mid grey TripoSR composites onto.
fn tile_of(picture: &RgbaImage, tile: u32) -> RgbaImage {
    let small = image::imageops::resize(picture, tile, tile, image::imageops::FilterType::Triangle);
    RgbaImage::from_fn(tile, tile, |x, y| {
        let p = small.get_pixel(x, y);
        let a = p[3] as f32 / 255.0;
        let over = |k: usize| (p[k] as f32 * a + 128.0 * (1.0 - a)) as u8;
        Rgba([over(0), over(1), over(2), 255])
    })
}

fn run(args: &[String]) -> Result<(), String> {
    let wrong = |e: &dyn std::fmt::Display| e.to_string();
    match args.first().map(String::as_str) {
        Some("views") if args.len() == 3 => {
            let mesh = glb::read(Path::new(&args[1])).map_err(|e| wrong(&e))?;
            let views = [(0.0, 15.0), (90.0, 15.0), (180.0, 15.0), (225.0, 25.0), (0.0, 89.0)];
            let mut sheet = RgbaImage::from_pixel(TILE * views.len() as u32, TILE, GROUND);
            for (i, (yaw, pitch)) in views.iter().enumerate() {
                draw(&mut sheet, TILE * i as u32, &mesh, *yaw, *pitch);
            }
            sheet.save(&args[2]).map_err(|e| wrong(&e))?;
            let (lo, hi) = mesh.bounds();
            println!("{}: {} points, {} triangles, {:.3} wide, {:.3} high, {:.3} deep", args[1], mesh.points.len(), mesh.triangles.len(), hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
        }
        Some("cut") if args.len() == 3 => {
            let picture = mesh::read_picture(Path::new(&args[1])).map_err(|e| wrong(&e))?;
            match cutout::cut_out(&picture) {
                Some(cut) => {
                    tile_of(&cut.picture, cutout::SIDE).save(&args[2]).map_err(|e| wrong(&e))?;
                    let judged = rank::judge(Some(&cut.report));
                    println!("{}: score {:.2}, {} {:?}", args[1], judged.score, rank::report_json(&cut.report), judged.problems);
                }
                None => {
                    tile_of(&RgbaImage::new(8, 8), cutout::SIDE).save(&args[2]).map_err(|e| wrong(&e))?;
                    println!("{}: no object found", args[1]);
                }
            }
        }
        Some("sheet") if args.len() >= 5 => {
            let columns: u32 = args[2].parse().map_err(|_| "columns must be a number")?;
            let tile: u32 = args[3].parse().map_err(|_| "tile must be a number")?;
            if columns == 0 || !(16..=1024).contains(&tile) {
                return Err("columns must be at least 1 and tile from 16 to 1024".into());
            }
            let files = &args[4..];
            let rows = (files.len() as u32).div_ceil(columns);
            let mut sheet = RgbaImage::from_pixel(columns * tile, rows * tile, Rgba([128, 128, 128, 255]));
            for (i, file) in files.iter().enumerate() {
                // a picture that is missing leaves its tile grey: a sheet of what there is is still worth looking at
                let Ok(picture) = mesh::read_picture(Path::new(file)) else { continue };
                image::imageops::replace(&mut sheet, &tile_of(&picture, tile), ((i as u32 % columns) * tile).into(), ((i as u32 / columns) * tile).into());
            }
            sheet.save(&args[1]).map_err(|e| wrong(&e))?;
        }
        Some("stack") if args.len() >= 3 => {
            // each picture scaled to one width, one under another
            let width = 1280u32;
            let strips: Vec<RgbaImage> = args[2..].iter().filter_map(|file| mesh::read_picture(Path::new(file)).ok()).map(|p| {
                let height = ((p.height() as f32 * width as f32 / p.width() as f32).round() as u32).max(1);
                image::imageops::resize(&p, width, height, image::imageops::FilterType::Triangle)
            }).collect();
            let mut sheet = RgbaImage::from_pixel(width, strips.iter().map(|s| s.height()).sum::<u32>().max(1), GROUND);
            let mut y = 0;
            for strip in &strips {
                image::imageops::replace(&mut sheet, strip, 0, y.into());
                y += strip.height();
            }
            sheet.save(&args[1]).map_err(|e| wrong(&e))?;
        }
        Some("birefnet") if args.len() == 4 => {
            let chosen = generate::device::choose(generate::args::Want::Auto).map_err(|e| wrong(&e))?;
            let started = std::time::Instant::now();
            let model = lux3d_core::runtime::Rmbg2Model::load_from_path(Path::new(&args[1]), &chosen.device).map_err(|e| wrong(&e))?;
            let loaded = started.elapsed();
            let picture = image::DynamicImage::ImageRgba8(mesh::read_picture(Path::new(&args[2])).map_err(|e| wrong(&e))?).into_rgb8();
            let cut = model.alpha_mask(&picture).map_err(|e| wrong(&e))?;
            tile_of(&cut, cutout::SIDE).save(&args[3]).map_err(|e| wrong(&e))?;
            println!("{}: loaded in {:.1} s, cut in {:.1} s on {}", args[2], loaded.as_secs_f32(), (started.elapsed() - loaded).as_secs_f32(), chosen.kind.name());
        }
        _ => return Err("usage: look views <mesh.glb> <out.png> | look cut <picture> <out.png> | look sheet <out.png> <columns> <tile> <in.png>... | look stack <out.png> <in.png>... | look birefnet <model.safetensors> <picture> <out.png>".into()),
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            std::process::ExitCode::FAILURE
        }
    }
}
