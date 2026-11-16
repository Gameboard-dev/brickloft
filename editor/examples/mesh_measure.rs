//! Build one of the meshes made by rule (`brickloft_core::mesh::made`) as
//! the editor's Imported form builds it, and print the six things the
//! fitter is judged on, as one row of a table:
//!
//!   cargo run --release -p brickloft-editor --example mesh_measure -- <mesh> <size> [smoothed|blocks] [key=value ...] [picture.png turn tilt]
//!
//! - closeness: the volume where bricks and mesh disagree, as a share of
//!   the mesh's volume; how far the bricks' outer faces stand from the
//!   surface (the worst and the mean), in microbricks; and the share of
//!   the mesh's surface with no brick within a cell of it, which is what
//!   shows a thin part that was left out
//! - smoothness: the area of steps the bricks have that the mesh has not,
//!   for each cell the surface crosses, and how many sloped bricks are one
//!   cell each way
//! - the number of bricks
//! - speed: milliseconds on one thread and on all of them
//! - processor use: what share of all the threads' time went into the fit,
//!   which is the time on one thread over the time on all of them times
//!   their number
//! - memory: the mesh with its hierarchy, in megabytes. The most the
//!   process held at once is the system's to say: `Get-Process` gives it as
//!   PeakWorkingSet64 on Windows, and the script that makes the tables in
//!   docs/algorithm.md runs each row as a process of its own and reads it.
//!
//! `key=value` sets `simplify`, `loose`, `shell` (a thickness), `colours`
//! or, for a file, `smooth` (the bricks are then fitted to the smoothed
//! surface and measured against the surface as the file has it). With `header` as the mesh, the table's heading is printed.

use brickloft_core::mesh::{made, Fitter, Import, Mesh, Stood};
use brickloft_core::par::Threads;
use brickloft_core::DVec3;
use std::sync::Arc;

#[path = "support/canvas.rs"]
mod canvas;

/// The meshes by name, each with the size at which its flat faces are on the lattice (or 96).
fn mesh(name: &str) -> Option<Mesh> {
    Some(match name {
        "box" => made::turned_box(20.0, 0.0),
        "tilted" => made::turned_box(20.0, 25.0),
        "sphere" => made::sphere(48),
        "torus" => made::torus(24),
        "gem" => made::gem(),
        "house" => made::house(),
        "lumpy" => made::lumpy(64),
        "scattered" => made::scattered(),
        "figure" => made::figure(96),
        "bust" => made::bust(96),
        "holed" => made::holed_ball(48),
        "snowman" => made::snowman(),
        "thin" => made::thin(0.6),
        "thinner" => made::thin(0.3),
        "large" => made::large(500_000),
        _ => return None,
    })
}

fn ms(since: std::time::Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all = Threads::available();
    if args.first().map(String::as_str) == Some("header") {
        println!("| Mesh | Size | Fitter | Bricks | Discrepancy | Worst gap | Mean gap | Bare surface | Steps per surface cell | Unit sloped | ms, 1 thread | ms, {} threads | Processor use | Mesh, MB |", all.count());
        println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
        return;
    }
    let (Some(name), Some(size)) = (args.first(), args.get(1).and_then(|a| a.parse::<f64>().ok())) else {
        eprintln!("usage: mesh_measure <mesh> <size> [smoothed|blocks] [simplify=0.25 loose=8 shell=2 colours=12] [picture.png turn tilt]");
        std::process::exit(2);
    };
    let setting = |key: &str| args.iter().find_map(|a| a.strip_prefix(key)?.strip_prefix('=')?.parse::<f64>().ok());
    let fitter = if args.iter().any(|a| a == "blocks") { Fitter::Blocks } else { Fitter::Smoothed };
    // (a mesh made by rule by its name, or a file by its path, stood the way `up=x|y|z` says or the way it is guessed to stand)
    let mesh = Arc::new(mesh(name).unwrap_or_else(|| {
        let kind = brickloft_editor::import::Kind::of(name).unwrap_or_else(|e| panic!("{e}"));
        let bytes = std::fs::read(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let parts = brickloft_editor::import::read(&bytes, kind).unwrap_or_else(|e| panic!("{e}"));
        let guessed = Some(brickloft_editor::import::up_of(&parts, kind));
        eprintln!("looks made by a network: {}", brickloft_editor::import::looks_reconstructed(&parts));
        let asked = args.iter().find_map(|a| brickloft_core::mesh::form::UPS.iter().find(|u| a.strip_prefix("up=") == Some(u.1)).map(|u| u.0));
        eprintln!("{name}: up guessed as {guessed:?}");
        brickloft_editor::import::mesh_of(&bytes, kind, asked.or(guessed).unwrap_or(kind.usual_up()), setting("smooth").unwrap_or(0.0) as i32).unwrap_or_else(|e| panic!("{e}"))
    }));
    // the same file as it is, to measure against: bricks fitted to a smoothed surface are judged by the surface that was given
    let given = (setting("smooth").unwrap_or(0.0) > 0.0 && std::path::Path::new(name).is_file()).then(|| {
        let kind = brickloft_editor::import::Kind::of(name).unwrap_or_else(|e| panic!("{e}"));
        let bytes = std::fs::read(name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let parts = brickloft_editor::import::read(&bytes, kind).unwrap_or_else(|e| panic!("{e}"));
        let asked = args.iter().find_map(|a| brickloft_core::mesh::form::UPS.iter().find(|u| a.strip_prefix("up=") == Some(u.1)).map(|u| u.0));
        Arc::new(brickloft_editor::import::mesh_of(&bytes, kind, asked.unwrap_or(brickloft_editor::import::up_of(&parts, kind)), 0).unwrap_or_else(|e| panic!("{e}")))
    });
    eprintln!("{} triangles, {} open edges", mesh.triangles(), mesh.open_edges);
    let megabytes = mesh.bytes() as f64 / 1e6;
    let import = Import { mesh: Some(mesh), fitter, loose: setting("loose").unwrap_or(0.0) as i32, colours: setting("colours").unwrap_or(0.0) as i32, ..Import::default() };
    let stood = Stood::new(&import, DVec3::new(0.5, 0.5, 0.5), size / 2.0, setting("shell")).unwrap_or_else(|e| panic!("{e}"));
    let price = setting("simplify").unwrap_or(0.0);
    let start = std::time::Instant::now();
    let single = stood.built(true, 80, price, Threads::ONE);
    let one = ms(start);
    let start = std::time::Instant::now();
    let laid = stood.built(true, 80, price, all);
    let many = ms(start);
    assert_eq!((&single.bricks, &single.shades), (&laid.bricks, &laid.shades), "the bricks differ with the number of threads");
    let m = match &given {
        Some(given) => Stood::new(&Import { mesh: Some(given.clone()), ..import.clone() }, DVec3::new(0.5, 0.5, 0.5), size / 2.0, setting("shell")).unwrap_or_else(|e| panic!("{e}")).measured(&laid, all),
        None => stood.measured(&laid, all),
    };
    println!(
        "| {} | {size} | {} | {} | {:.2}% | {:.2} | {:.2} | {:.2}% | {:.3} | {} | {:.0} | {:.0} | {:.0}% | {:.1} |",
        std::path::Path::new(name).file_name().map_or(name.clone(), |n| n.to_string_lossy().into_owned()),
        if fitter == Fitter::Blocks { "Blocks" } else { "Smoothed" },
        m.bricks,
        100.0 * m.wrong / m.solid.max(1e-9),
        m.worst_gap,
        m.mean_gap,
        100.0 * stood.bare(&laid),
        m.steps / m.surface.max(1) as f64,
        m.unit_sloped,
        one,
        many,
        100.0 * one / (many * all.count() as f64).max(1e-9),
        megabytes
    );
    if args.iter().any(|a| a == "stages") {
        for ((stage, took), (_, alone)) in laid.stages.iter().zip(&single.stages) {
            println!("  {stage}: {alone:.0} ms on one thread, {took:.0} ms on {}", all.count());
        }
    }
    if let Some(at) = args.iter().position(|a| a.ends_with(".png")) {
        let number = |k: usize, usual: f64| args.get(at + k).and_then(|a| a.parse::<f64>().ok()).unwrap_or(usual);
        let (turn, tilt) = (number(1, 35.0).to_radians(), number(2, 30.0).to_radians());
        let corner = |pick: fn(i32, i32) -> i32, far: bool| DVec3::from_array([0, 1, 2].map(|k| laid.bricks.iter().map(|b| b.min[k] + if far { b.ext[k] } else { 0 }).reduce(pick).unwrap_or(0) as f64));
        let (lo, hi) = (corner(i32::min, false), corner(i32::max, true));
        const SIDE: usize = 1100;
        let mut canvas = canvas::Canvas::new(SIDE, 2, (lo + hi) / 2.0, SIDE as f64 / (hi - lo).length() / 1.02, DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin()));
        let palette = stood.palette();
        for (n, brick) in laid.bricks.iter().enumerate() {
            // in its own colours if it has any; else boxes a little greyer than sloped pieces, so that a slope built of steps shows as one
            canvas.brick = match laid.shades.get(n) {
                Some(shade) => palette[*shade as usize].map(f64::from),
                None if brick.mask == 0xff => [214.0, 196.0, 150.0],
                None => [242.0, 170.0, 60.0],
            };
            canvas.shape(brick.mask, DVec3::from_array(brick.min.map(|v| v as f64)), DVec3::from_array(brick.ext.map(|v| v as f64)));
        }
        let file = std::io::BufWriter::new(std::fs::File::create(&args[at]).unwrap_or_else(|e| panic!("could not make {}: {e}", args[at])));
        let mut encoder = png::Encoder::new(file, SIDE as u32, SIDE as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().expect("could not start the picture").write_image_data(&canvas.pixels()).expect("could not write the picture");
    }
}
