//! How long representative forms take to fit, and how well they fit.
//! `cargo run --release -p arcs-core --example timing`

use arcs_core::curves::ArchType;
use arcs_core::spec::{build, Form, Spec};
use arcs_core::DVec3;
use std::time::Instant;

fn main() {
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
