//! The two models, run for real. Every test here needs the weights and a
//! graphics card, so each is ignored unless asked for by name:
//!
//!   set BRICKLOFT_WEIGHTS=<the weights folder>
//!   cargo test --release --test models -- --ignored --test-threads 1
//!
//! One at a time (`--test-threads 1`): two models loaded at once do not fit
//! an 8 GB card.

use generate::args::Want;
use generate::protocol::Out;
use generate::{device, glb, mesh, picture};
use std::path::PathBuf;

fn weights() -> PathBuf {
    PathBuf::from(std::env::var_os("BRICKLOFT_WEIGHTS").expect("set BRICKLOFT_WEIGHTS to the folder the weights were fetched into"))
}

fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("brickloft-generate-models-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Somewhere for the lines to go that is not the test's own output.
fn quiet() -> Out {
    Out::to(Box::new(std::io::sink()))
}

#[test]
#[ignore = "needs the Stable Diffusion weights"]
fn the_tokenizer_put_together_here_cuts_a_prompt_as_clips_own_does() {
    let tokenizer = picture::tokenizer(&weights().join("sd15")).unwrap();
    // The numbers the ready-made tokenizer file gives for this text (the one Candle's example downloads, which the
    // trial used and this program does not fetch: see `models`). Worked out with that file once, and written here.
    let ids = picture::pieces(&tokenizer, "a photo of a cat", 77).unwrap();
    assert_eq!(ids[..7], [49406, 320, 1125, 539, 320, 2368, 49407]);
    assert!(ids[7..].iter().all(|&id| id == 49407) && ids.len() == 77, "filled out with the end mark");
    // capitals, runs of spaces and punctuation are handled as the original does
    assert_eq!(picture::pieces(&tokenizer, "A  PHOTO of a cat", 77).unwrap(), ids);
    let wrapped = picture::pieces(&tokenizer, &picture::wrapped("a dragon statue"), 77).unwrap();
    assert_eq!(wrapped[..27], [49406, 320, 16377, 5879, 2863, 539, 320, 5471, 8222, 525, 320, 1579, 5994, 267, 3155, 4306, 1125, 267, 2954, 14115, 8626, 267, 2097, 268, 6632, 1093, 49407], "hyphens and commas are word-pieces of their own");
    // a prompt at the bound on characters can still be more word-pieces than the model reads
    let long = "supercalifragilistic ".repeat(23);
    assert!(long.len() <= 500);
    assert!(matches!(picture::pieces(&tokenizer, &long, 77), Err(generate::error::Error::PromptTooManyPieces { most: 77, .. })));
}

#[test]
#[ignore = "needs the TripoSR weights and a graphics card"]
fn a_made_picture_becomes_a_mesh_standing_on_the_ground() {
    let folder = scratch("mesh");
    // a red box on a grey studio ground, drawn here
    let made = image::RgbaImage::from_fn(512, 512, |x, y| {
        if (170..340).contains(&x) && (140..380).contains(&y) {
            image::Rgba([190, 40 + ((x + y) % 9) as u8, 30, 255])
        } else {
            let v = 225 - (y / 12) as u8;
            image::Rgba([v, v, v, 255])
        }
    });
    let picture = folder.join("box.png");
    made.save(&picture).unwrap();
    let chosen = device::choose(Want::Auto).unwrap();
    let sculptor = mesh::Sculptor::load(&weights(), &chosen, &quiet()).unwrap();
    let to = folder.join("box.glb");
    let result = sculptor.mesh(&picture, &to, true, &mesh::Tidy { level: true, seen_from: None }, 64, &quiet()).unwrap();
    assert_eq!(result["up"], "y");
    assert_eq!(result["cut_by"], "ground");
    let read = glb::read(&to).unwrap();
    assert_eq!(read.triangles.len() as u64, result["triangles"].as_u64().unwrap());
    assert!(read.triangles.len() > 500, "{}", read.triangles.len());
    let (lo, hi) = read.bounds();
    assert!(lo[1].abs() < 1e-4, "its lowest point is on the ground");
    assert!(hi[1] - lo[1] > hi[0] - lo[0], "a box taller than wide gives a mesh taller than wide: {lo:?} {hi:?}");
    // reddish all over, as the box was (the sides the picture did not show are the model's guess, and paler)
    let mean = read.colours.iter().fold([0f32; 3], |sum, c| [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]]).map(|v| v / read.colours.len() as f32);
    assert!(mean[0] > 1.3 * mean[1] && mean[0] > 1.3 * mean[2], "{mean:?}");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2, "the picture and the mesh, and no part-file left");
}

#[test]
#[ignore = "needs the Stable Diffusion weights and a graphics card"]
fn the_same_seed_gives_the_same_picture_and_candidates_come_ranked() {
    let folder = scratch("picture");
    let chosen = device::choose(Want::Auto).unwrap();
    let painter = picture::Painter::load(&weights(), &chosen, &quiet()).unwrap();
    // few steps: the picture is rough, and what is checked is that it is the same twice
    let first = painter.candidates("an oak barrel", 7, 2, 8, &folder.join("a"), &quiet()).unwrap();
    let again = painter.candidates("an oak barrel", 8, 1, 8, &folder.join("b"), &quiet()).unwrap();
    let candidates = first["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["rank"], 1);
    assert!(candidates[0]["score"].as_f64() >= candidates[1]["score"].as_f64());
    for candidate in candidates {
        assert_eq!(image::open(candidate["file"].as_str().unwrap()).unwrap().into_rgb8().dimensions(), (512, 512));
    }
    // seed 8 as the second of two from seed 7, and as the first of one from seed 8
    let bytes = |folder: &str| std::fs::read(self::scratch_path("picture").join(folder).join("candidate-8.png")).unwrap();
    assert_eq!(bytes("a"), bytes("b"));
    assert_eq!(again["candidates"][0]["seed"], 8);
    assert_eq!(first["prompt_sent"], "a miniature scale model of an oak barrel on a white background, studio product photo, whole object visible, three-quarter view");
}

fn scratch_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("brickloft-generate-models-{}-{name}", std::process::id()))
}
