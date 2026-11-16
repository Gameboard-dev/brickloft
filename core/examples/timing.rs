//! How long representative forms take to fit, and how well they fit.
//! `cargo run --release -p brickloft-core --example timing`
//!
//! `-- threads` times the forms whose builds are shared out over threads
//! instead, each on one thread and on all the machine has. `-- maze` does
//! the same for mazes: a small, a usual and a large one of each texture.

use brickloft_core::curves::ArchType;
use brickloft_core::par::Threads;
use brickloft_core::pattern::{Mosaic, Stones};
use brickloft_core::spec::{build, build_as, build_with, Form, Spec};
use brickloft_core::DVec3;
use std::time::Instant;

/// `-- threads [runs]`: large forms of each kind, measured and unmeasured
/// (as when let go and as while dragged), on one thread and on all of
/// them. Each time is the least of `runs` builds (5 unless given), since a
/// desktop doing anything else at all makes any one build slower.
fn threads(runs: usize) {
    let at = |x: f64, y: f64, z: f64| DVec3::new(x + 0.5, y + 0.5, z + 0.5);
    let sphere = |radius: f64, fill: bool, thick: f64| Spec { form: Form::Sphere, points: vec![at(0.0, 0.0, 0.0)], radius, fill, thick, ..Spec::default() };
    let ring = Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, thick: 10.0, tube: true, ..Spec::default() };
    let lift = vec![(at(-60.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 30.0))];
    let wavy: Vec<DVec3> = (0..9).map(|i| at(i as f64 * 40.0 - 160.0, if i % 2 == 0 { -25.0 } else { 25.0 }, 0.0)).collect();
    let band = Spec { form: Form::Spline, points: wavy, thick: 20.0, depth: 2, ..Spec::default() };
    let floor = Spec { form: Form::Circle, points: vec![at(0.0, 0.0, 0.0)], radius: 70.0, fill: true, depth: 3, ..Spec::default() };
    let stones = Some(Stones { size: 10.0, gap: 1.5, uneven: 0.5, seed: 1 });
    let mosaic = |style: usize, bevel: bool| Some(Mosaic { style, size: 2, bevel, border: 0, seed: 1, mortar: true, stagger: -1, upright: false, uneven: [50; 2] });
    let cube = Spec { form: Form::Cube, points: vec![at(0.0, 0.0, 0.0)], radius: 60.0, stretch: DVec3::new(1.0, 0.7, 0.4), fill: true, ..Spec::default() };
    let forms = [
        ("solid sphere, radius 40", sphere(40.0, true, 2.0)),
        ("solid sphere, radius 80", sphere(80.0, true, 2.0)),
        ("hollow sphere, radius 50, wall 2", sphere(50.0, false, 2.0)),
        ("hollow sphere, radius 80, wall 4", sphere(80.0, false, 4.0)),
        ("revolve (a bell, radius 50)", Spec { form: Form::Revolve, points: vec![at(0.0, 0.0, 60.0), at(30.0, 0.0, 40.0), at(50.0, 0.0, 0.0)], thick: 4.0, ..Spec::default() }),
        ("ring as a pipe, radius 60", ring.clone()),
        ("lifted tube, radius 60", Spec { pulls: lift.clone(), pull_reach: 30.0, ..ring.clone() }),
        ("lifted band, radius 60", Spec { tube: false, thick: 6.0, depth: 4, pulls: lift, pull_reach: 30.0, ..ring.clone() }),
        ("torus, radius 40, 8 thick", Spec { form: Form::Torus, points: vec![at(0.0, 0.0, 0.0)], radius: 40.0, thick: 8.0, ..Spec::default() }),
        ("spline band, 320 long, 20 wide", band.clone()),
        ("round arch, span 320", Spec { form: Form::Arch(ArchType::Round), points: vec![at(-160.0, 0.0, 0.0), at(160.0, 0.0, 0.0)], thick: 3.0, depth: 4, ..Spec::default() }),
        ("stepping stones on that band", Spec { stones, ..band.clone() }),
        ("stepping stones on a disc, radius 70", Spec { stones, ..floor.clone() }),
        ("medallions on that disc", Spec { mosaic: mosaic(0, false), ..floor.clone() }),
        ("rug on that disc", Spec { mosaic: mosaic(1, false), ..floor.clone() }),
        ("planks on that disc", Spec { mosaic: mosaic(2, false), ..floor.clone() }),
        ("bricks on that disc", Spec { mosaic: mosaic(3, false), ..floor.clone() }),
        ("bevelled bricks on that disc", Spec { mosaic: mosaic(3, true), ..floor.clone() }),
        ("cobblestone on that disc", Spec { mosaic: mosaic(4, false), ..floor.clone() }),
        ("cobblestone on the band", Spec { mosaic: mosaic(4, false), depth: 3, ..band }),
        ("rug on that disc with a rounded rim", Spec { tube: true, thick: 8.0, mosaic: mosaic(1, false), ..floor.clone() }),
        ("cube 120 by 84 by 48", cube.clone()),
        ("cobblestone on that cube, sides too", Spec { mosaic: mosaic(4, false), ..cube.clone() }),
        ("bevelled bricks on that cube", Spec { mosaic: mosaic(3, true), ..cube }),
    ];
    let all = Threads::available();
    println!("least of {runs} builds; all threads: {}", all.count());
    println!("{:<38} {:>8}  {:>22}  {:>22}", "", "", "let go (measured)", "dragged (unmeasured)");
    println!("{:<38} {:>8}  {:>10} {:>11}  {:>10} {:>11}", "form", "bricks", "one thread", "all threads", "one thread", "all threads");
    for (name, spec) in &forms {
        let mut bricks = 0;
        let mut least = |measured: bool, threads: Threads| {
            (0..runs).fold(f64::INFINITY, |least, _| {
                let start = Instant::now();
                bricks = build_with(spec, measured, threads).expect(name).bricks.len();
                least.min(start.elapsed().as_secs_f64() * 1000.0)
            })
        };
        let times = [(true, Threads::ONE), (true, all), (false, Threads::ONE), (false, all)].map(|(measured, threads)| least(measured, threads));
        println!("{name:<38} {bricks:>8}  {:>8.1}ms {:>9.1}ms  {:>8.1}ms {:>9.1}ms", times[0], times[1], times[2], times[3]);
    }
}

/// `-- maze [runs]`: a small, a usual and a large maze, with walls plain,
/// of bricks, of cobblestone and of hedge, on one thread and on all of
/// them. Each time is the least of `runs` builds (3 unless given).
fn mazes(runs: usize) {
    use brickloft_core::maze::{Kind, Maze};
    let mosaic = |name: &str| {
        let style = brickloft_core::pattern::STYLES.iter().position(|s| s.name == name).expect("no such style");
        Some(Mosaic { style, size: 2, bevel: false, border: 0, seed: 1, mortar: true, stagger: 50, upright: false, uneven: if name == "cobblestone" { [60; 2] } else { [0; 2] } })
    };
    let sizes = [("small: 4 rings, radius 80", Kind::Theta, 4, 80.0), ("usual: 6 rings, radius 120", Kind::Theta, 6, 120.0), ("larger: 9 rings, radius 180", Kind::Theta, 9, 180.0), ("large: 16 rings, radius 320", Kind::Theta, 16, 320.0), ("grid: 6 rings, radius 120", Kind::Grid, 6, 120.0)];
    let all = Threads::available();
    println!("least of {runs} builds; all threads: {}; walls 30 tall, 4 thick (hedge 6)", all.count());
    println!("{:<28} {:<12} {:>8} {:>12} {:>12}", "maze", "texture", "bricks", "one thread", "all threads");
    for (name, kind, rings, radius) in sizes {
        for texture in ["none", "bricks", "cobblestone", "hedge"] {
            let hedge = texture == "hedge";
            let spec = Spec { form: Form::Maze, points: vec![DVec3::splat(0.5)], radius, thick: if hedge { 6.0 } else { 4.0 }, rise: Some(30.0), maze: Maze { kind, rings, hedge, ..Maze::default() }, mosaic: if hedge || texture == "none" { None } else { mosaic(texture) }, ..Spec::default() };
            // (a maze past what may be built is refused, and says so)
            if let Err(why) = build_with(&spec, true, all) {
                println!("{name:<28} {texture:<12} refused: {}", why.split(':').next().unwrap_or_default());
                continue;
            }
            let mut bricks = 0;
            let mut least = |threads: Threads| {
                (0..runs).fold(f64::INFINITY, |least, _| {
                    let start = Instant::now();
                    bricks = build_with(&spec, true, threads).expect(name).bricks.len();
                    least.min(start.elapsed().as_secs_f64() * 1000.0)
                })
            };
            let (one, many) = (least(Threads::ONE), least(all));
            println!("{name:<28} {texture:<12} {bricks:>8} {one:>10.1}ms {many:>10.1}ms");
        }
    }
}

fn main() {
    // `-- <a saved report> [scale]`: time the form in that report instead,
    // measured and unmeasured, at its own size or so many times larger
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "maze") {
        return mazes(args.get(1).map_or(3, |s| s.parse().expect("the number of runs is not a number")));
    }
    if args.first().is_some_and(|a| a == "threads") {
        return threads(args.get(1).map_or(5, |s| s.parse().expect("the number of runs is not a number")));
    }
    if let Some(path) = args.first() {
        let text = std::fs::read_to_string(path).expect("the report could not be read");
        let mut spec = brickloft_core::report::parse_spec(&text).expect("no form in the report");
        let scale: f64 = args.get(1).map_or(1.0, |s| s.parse().expect("the scale is not a number"));
        let about = spec.points[0];
        for p in &mut spec.points {
            *p = about + (*p - about) * scale;
        }
        (spec.radius, spec.thick) = (spec.radius * scale, spec.thick * scale);
        for measured in [true, false] {
            let start = Instant::now();
            let built = build_as(&spec, measured).expect("it does not build");
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            println!("{} {ms:>8.1}ms {:>7} bricks {:>6.2}%", if measured { "measured  " } else { "unmeasured" }, built.bricks.len(), 100.0 * built.error / built.solid.max(1e-9));
        }
        return;
    }
    let at = |x: f64, y: f64, z: f64| DVec3::new(x + 0.5, y + 0.5, z + 0.5);
    let arch = |half: f64| Spec {
        form: Form::Arch(ArchType::Round),
        points: vec![at(-half, 0.0, 0.0), at(half, 0.0, 0.0)],
        thick: 3.0,
        depth: 4,
        ..Spec::default()
    };
    let sphere = |radius: f64, fill: bool| Spec {
        form: Form::Sphere,
        points: vec![at(0.0, 0.0, 0.0)],
        radius,
        fill,
        thick: 2.0,
        ..Spec::default()
    };
    let forms = [
        ("round arch, span 32", arch(16.0)),
        ("round arch, span 160", arch(80.0)),
        (
            "circle band, radius 60",
            Spec {
                form: Form::Circle,
                points: vec![at(0.0, 0.0, 0.0)],
                radius: 60.0,
                thick: 3.0,
                depth: 4,
                ..Spec::default()
            },
        ),
        ("solid sphere, radius 12", sphere(12.0, true)),
        ("solid sphere, radius 50", sphere(50.0, true)),
        ("hollow sphere, radius 50", sphere(50.0, false)),
        (
            "hollow dome, radius 100",
            Spec {
                cut: Some((-0.5, f64::INFINITY)),
                ..sphere(100.0, false)
            },
        ),
        (
            "torus, radius 40",
            Spec {
                form: Form::Torus,
                points: vec![at(0.0, 0.0, 0.0)],
                radius: 40.0,
                thick: 8.0,
                ..Spec::default()
            },
        ),
    ];
    // closest fit, then the editor's balance of fit against brick count
    for (label, simplify, grain) in [
        ("closest fit", 0.0, 1),
        (
            "balanced: simplify 0.25, automatic brick size, budget 2500",
            0.25,
            0,
        ),
    ] {
        println!(
            "
{label}"
        );
        println!(
            "{:<26} {:>9} {:>8} {:>6} {:>13}",
            "form", "time", "bricks", "size", "discrepancy"
        );
        for (name, spec) in &forms {
            let spec = Spec {
                simplify,
                grain,
                ..spec.clone()
            };
            let start = Instant::now();
            let built = build(&spec).expect(name);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            println!(
                "{name:<26} {ms:>7.1}ms {:>8} {:>6} {:>12.2}%",
                built.bricks.len(),
                built.grain,
                100.0 * built.error / built.solid
            );
        }
    }
}
