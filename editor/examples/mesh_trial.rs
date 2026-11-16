//! Fit the meshes made by rule (`brickloft_core::mesh::made`) each way that
//! is on trial (`brickloft_core::mesh::trial`), print what each fit
//! measures as, and draw each as a picture:
//!
//!   cargo run --release -p brickloft-editor --example mesh_trial -- <folder for pictures> [only this mesh]
//!
//! and, for how long a large mesh takes and how much memory, one fit alone:
//!
//!   cargo run --release -p brickloft-editor --example mesh_trial -- large <triangles> <size> <plain|guided|blocks> <threads>

use brickloft_core::mesh::trial::{fit, measure, Measured, Route, Stand};
use brickloft_core::mesh::{made, Mesh};
use brickloft_core::par::Threads;
use brickloft_core::shapes::CellBrick;
use brickloft_core::DVec3;

#[path = "support/canvas.rs"]
mod canvas;
use canvas::Canvas;

const SIZE: usize = 1100;

fn draw(bricks: &[CellBrick], to: &std::path::Path, turn: f64, tilt: f64) {
    let corner = |pick: fn(i32, i32) -> i32, far: bool| DVec3::from_array([0, 1, 2].map(|k| bricks.iter().map(|b| b.min[k] + if far { b.ext[k] } else { 0 }).reduce(pick).unwrap_or(0) as f64));
    let (lo, hi) = (corner(i32::min, false), corner(i32::max, true));
    let (turn, tilt) = (turn.to_radians(), tilt.to_radians());
    let towards = DVec3::new(turn.cos() * tilt.cos(), turn.sin() * tilt.cos(), tilt.sin());
    let mut canvas = Canvas::new(SIZE, 2, (lo + hi) / 2.0, SIZE as f64 / (hi - lo).length() / 1.02, towards);
    for brick in bricks {
        // boxes a little greyer than sloped pieces, so that a slope built of steps shows as one
        canvas.brick = if brick.mask == 0xff { [214.0, 196.0, 150.0] } else { [242.0, 170.0, 60.0] };
        canvas.shape(brick.mask, DVec3::from_array(brick.min.map(|v| v as f64)), DVec3::from_array(brick.ext.map(|v| v as f64)));
    }
    let file = std::io::BufWriter::new(std::fs::File::create(to).unwrap_or_else(|e| panic!("could not make {}: {e}", to.display())));
    let mut encoder = png::Encoder::new(file, SIZE as u32, SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().expect("could not start the picture").write_image_data(&canvas.pixels()).expect("could not write the picture");
}

fn ms(since: std::time::Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn row(name: &str, size: f64, way: &str, m: &Measured, one: f64, all: f64) {
    println!(
        "| {name} | {size} | {way} | {} | {:.2}% | {:.2} | {:.2} | {:.3} | {} | {} | {:.0} | {:.0} | {:.1} |",
        m.bricks,
        100.0 * m.wrong / m.solid.max(1e-9),
        m.worst_gap,
        m.mean_gap,
        m.steps / m.surface.max(1) as f64,
        m.unit_sloped,
        m.alone,
        one,
        all,
        one / all.max(1e-9)
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all = Threads::available();
    if args.first().map(String::as_str) == Some("large") {
        let number = |k: usize, usual: f64| args.get(k).and_then(|a| a.parse::<f64>().ok()).unwrap_or(usual);
        let route = match args.get(3).map(String::as_str) {
            Some("blocks") => Route::Blocks,
            Some("plain") => Route::Plain,
            Some("guided") => Route::Guided,
            _ => Route::Scan,
        };
        let start = std::time::Instant::now();
        let mesh = made::large(number(1, 500_000.0) as usize);
        println!("{} triangles made and indexed in {:.0} ms; the mesh and its hierarchy take {:.1} MB", mesh.triangles(), ms(start), mesh.bytes() as f64 / 1e6);
        let stand = Stand { mesh: &mesh, size: number(2, 200.0) };
        let start = std::time::Instant::now();
        let fitted = fit(&stand, route, 80, Threads::at_most(number(4, all.count() as f64) as usize));
        println!("{route:?} at size {}: {} bricks in {:.0} ms on {} threads, discrepancy {:.2}%", stand.size, fitted.bricks.len(), ms(start), number(4, all.count() as f64), 100.0 * fitted.error / fitted.solid);
        for (stage, took) in &fitted.stages {
            println!("  {stage}: {took:.0} ms");
        }
        return;
    }
    let folder = std::path::PathBuf::from(args.first().cloned().unwrap_or_else(|| ".".into()));
    let only = args.get(1).filter(|o| o.as_str() != "all");
    let meshes: Vec<(&str, Mesh, [f64; 2], f64, f64)> = vec![
        ("box turned 20", made::turned_box(20.0, 0.0), [48.0, 96.0], 30.0, 35.0),
        ("box turned and tilted", made::turned_box(20.0, 25.0), [48.0, 96.0], 30.0, 35.0),
        ("sphere", made::sphere(48), [48.0, 96.0], 35.0, 35.0),
        ("torus", made::torus(24), [64.0, 128.0], 35.0, 40.0),
        ("gem", made::gem(), [48.0, 77.0], 25.0, 30.0),
        ("house", made::house(), [56.0, 90.0], 30.0, 30.0),
        ("lumpy", made::lumpy(64), [48.0, 96.0], 35.0, 30.0),
    ];
    println!("| mesh | size | route | bricks | discrepancy | worst gap | mean gap | steps per surface cell | unit sloped | of them alone | ms, 1 thread | ms, {} threads | speed-up |", all.count());
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (name, mesh, sizes, turn, tilt) in &meshes {
        if only.is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        for &size in sizes {
            let stand = Stand { mesh, size };
            let n = stand.cells().map(|v| v as usize);
            for (route, way) in [(Route::Blocks, "blocks"), (Route::Plain, "plain"), (Route::Guided, "guided"), (Route::Scan, "scanned")] {
                // (the three that ask the distance everywhere take minutes between them: left out with `scan` as a third argument)
                if route != Route::Scan && args.get(2).is_some_and(|a| a == "scan") {
                    continue;
                }
                let start = std::time::Instant::now();
                let single = fit(&stand, route, 80, Threads::ONE);
                let one = ms(start);
                let start = std::time::Instant::now();
                let fitted = fit(&stand, route, 80, all);
                let many = ms(start);
                assert_eq!(single.bricks, fitted.bricks, "{name} at {size}, {way}: the bricks differ with the number of threads");
                let measured = measure(&|p| stand.distance(p), n, &fitted.bricks, all);
                row(name, size, way, &measured, one, many);
                draw(&fitted.bricks, &folder.join(format!("{}-{size}-{way}.png", name.replace(' ', "-"))), *turn, *tilt);
            }
            // the editor's own form of the same solid, where it has one, measured against that form
            let native = match *name {
                "sphere" => Some(brickloft_core::spec::Spec { form: brickloft_core::spec::Form::Sphere, points: vec![DVec3::splat(size / 2.0 + 2.0)], radius: size / 2.0, fill: true, ..Default::default() }),
                "torus" => Some(brickloft_core::spec::Spec { form: brickloft_core::spec::Form::Circle, points: vec![DVec3::new(size / 2.0 + 2.0, size / 2.0 + 2.0, size / 8.0 + 2.0)], radius: 3.0 * size / 8.0, thick: size / 4.0, tube: true, ..Default::default() }),
                _ => None,
            };
            if let Some(spec) = native {
                let start = std::time::Instant::now();
                let single = brickloft_core::spec::build_with(&spec, false, Threads::ONE).expect("the editor's own form builds");
                let one = ms(start);
                let start = std::time::Instant::now();
                let built = brickloft_core::spec::build_with(&spec, false, all).expect("the editor's own form builds");
                let many = ms(start);
                assert_eq!(single.bricks, built.bricks);
                let probe = brickloft_core::spec::Probe::new(&spec, built.grain).expect("the editor's own form");
                row(name, size, "the editor's own form", &measure(&|p| probe.distance(p), n, &built.bricks, all), one, many);
                draw(&built.bricks, &folder.join(format!("{}-{size}-own.png", name.replace(' ', "-"))), *turn, *tilt);
            }
        }
    }
}
