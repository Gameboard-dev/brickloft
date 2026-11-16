//! How long representative forms take to fit, and how well they fit.
//! `cargo run --release -p brickloft-core --example timing`

use brickloft_core::curves::ArchType;
use brickloft_core::spec::{build, build_as, Form, Spec};
use brickloft_core::DVec3;
use std::time::Instant;

fn main() {
    // `-- <a saved report> [scale]`: time the form in that report instead,
    // measured and unmeasured, at its own size or so many times larger
    let args: Vec<String> = std::env::args().skip(1).collect();
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
