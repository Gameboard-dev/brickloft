//! Write the meshes made by rule (`brickloft_core::mesh::made`) out as
//! files, to open in the editor and to try its readers on:
//!
//!   cargo run --release -p brickloft-editor --example mesh_files -- <folder> [adopt]
//!
//! One file for each mesh, in a kind of file chosen so that all three
//! kinds are among them. With `adopt`, each is also taken into the
//! editor's imports folder, as opening it in the editor would, and its
//! name there is printed: the name a spec file's `import` line, or a
//! program driving the editor, gives.
//!
//! No model made by anyone else is kept in this repository, so these are
//! what the feature is tried on here. They are not what a reconstruction
//! model makes: see docs/algorithm.md, section 7.13, for which of such a
//! model's faults each one stands for.

use brickloft_core::mesh::{made, Parts};
use brickloft_editor::import::{adopt, write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(folder) = args.first().map(std::path::PathBuf::from) else {
        eprintln!("usage: mesh_files <folder> [adopt]");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&folder).unwrap_or_else(|e| panic!("could not make {}: {e}", folder.display()));
    // each mesh at the size its flat faces are whole numbers at, where it has any
    let all: Vec<(&str, Parts)> = vec![
        ("house.glb", made::house().parts(56.0)),
        ("gem.obj", made::gem().parts(48.0)),
        ("sphere.ply", made::sphere(48).parts(96.0)),
        ("torus.obj", made::torus(24).parts(128.0)),
        ("lumpy.glb", made::lumpy(48).parts(96.0)),
        ("turned_box.ply", made::turned_box(20.0, 25.0).parts(96.0)),
        ("painted_ball.glb", made::painted_ball(48).parts(96.0)),
        ("rainbow_ball.ply", made::rainbow_ball(48).parts(96.0)),
        ("scattered.glb", made::scattered().parts(100.0)),
        ("figure.glb", made::figure(96).parts(102.0)),
    ];
    for (name, parts) in &all {
        let bytes = match name.rsplit('.').next() {
            Some("glb") => write::glb(parts),
            Some("obj") => write::obj(parts),
            _ => write::ply(parts, true),
        };
        let path = folder.join(name);
        std::fs::write(&path, &bytes).unwrap_or_else(|e| panic!("could not write {}: {e}", path.display()));
        if args.iter().any(|a| a == "adopt") {
            match adopt(&path) {
                Ok((kept, _)) => println!("{name}: {} triangles, {} bytes, kept as {kept}", parts.triangles.len(), bytes.len()),
                Err(why) => println!("{name}: not taken in: {why}"),
            }
        } else {
            println!("{name}: {} triangles, {} bytes", parts.triangles.len(), bytes.len());
        }
    }
}
