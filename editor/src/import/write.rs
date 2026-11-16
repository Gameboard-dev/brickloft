//! A mesh's parts written out as a `.glb`, `.obj` or `.ply` file.
//!
//! The editor reads these files and has no use for writing them. This is
//! here for its tests and for `examples/mesh_files.rs`: no model made by
//! anyone else is kept in the repository, so every file a reader is tried
//! on is written by rule from parts made by rule (`brickloft_core::mesh::made`).
//! Each writer is the plainest file of its kind that says everything the
//! parts hold.

use brickloft_core::mesh::Parts;
use serde_json::json;

/// A chunk of a `.glb` file: its length, its kind, and its bytes padded to
/// a multiple of four with `pad`.
fn chunk(kind: u32, mut bytes: Vec<u8>, pad: u8) -> Vec<u8> {
    while bytes.len() % 4 != 0 {
        bytes.push(pad);
    }
    let mut out = (bytes.len() as u32).to_le_bytes().to_vec();
    out.extend(kind.to_le_bytes());
    out.extend(bytes);
    out
}

/// A `.glb` file from its JSON and its bytes: for a test that wants to
/// write the JSON itself, wrongly.
pub fn glb_of(json: &serde_json::Value, bin: &[u8]) -> Vec<u8> {
    let mut chunks = chunk(0x4E4F_534A, json.to_string().into_bytes(), b' ');
    if !bin.is_empty() {
        chunks.extend(chunk(0x004E_4942, bin.to_vec(), 0));
    }
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((12 + chunks.len() as u32).to_le_bytes());
    out.extend(chunks);
    out
}

/// The parts as a `.glb` file: one mesh under one node, its points as
/// floats, its triangles as 32-bit numbers, and its colours, if it has
/// them, as a byte a channel.
pub fn glb(parts: &Parts) -> Vec<u8> {
    let mut bin: Vec<u8> = parts.positions.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    let points = bin.len();
    bin.extend(parts.triangles.iter().flatten().flat_map(|v| v.to_le_bytes()));
    let triangles = bin.len() - points;
    bin.extend(parts.colours.iter().flatten());
    let mut views = vec![json!({ "buffer": 0, "byteOffset": 0, "byteLength": points }), json!({ "buffer": 0, "byteOffset": points, "byteLength": triangles })];
    let mut accessors = vec![json!({ "bufferView": 0, "componentType": 5126, "count": parts.positions.len(), "type": "VEC3" }), json!({ "bufferView": 1, "componentType": 5125, "count": parts.triangles.len() * 3, "type": "SCALAR" })];
    let mut attributes = json!({ "POSITION": 0 });
    if !parts.colours.is_empty() {
        views.push(json!({ "buffer": 0, "byteOffset": points + triangles, "byteLength": parts.colours.len() * 3, "byteStride": 3 }));
        accessors.push(json!({ "bufferView": 2, "componentType": 5121, "normalized": true, "count": parts.colours.len(), "type": "VEC3" }));
        attributes["COLOR_0"] = json!(2);
    }
    let doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": attributes, "indices": 1 }] }],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{ "byteLength": bin.len() }],
    });
    glb_of(&doc, &bin)
}

/// The parts as an OBJ file: a `v` line for each point, with its colour
/// after it if it has one, and an `f` line for each triangle.
pub fn obj(parts: &Parts) -> Vec<u8> {
    let mut text = String::from("# written by Brickloft\n");
    for (n, p) in parts.positions.iter().enumerate() {
        text += &format!("v {} {} {}", p[0], p[1], p[2]);
        if let Some(c) = parts.colours.get(n) {
            text += &format!(" {} {} {}", c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
        }
        text.push('\n');
    }
    for t in &parts.triangles {
        text += &format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1);
    }
    text.into_bytes()
}

/// The parts as a PLY file, as bytes or as text.
pub fn ply(parts: &Parts, binary: bool) -> Vec<u8> {
    let coloured = !parts.colours.is_empty();
    let mut out = format!("ply\nformat {} 1.0\ncomment written by Brickloft\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\n", if binary { "binary_little_endian" } else { "ascii" }, parts.positions.len());
    if coloured {
        out += "property uchar red\nproperty uchar green\nproperty uchar blue\n";
    }
    out += &format!("element face {}\nproperty list uchar int vertex_indices\nend_header\n", parts.triangles.len());
    let mut out = out.into_bytes();
    for (n, p) in parts.positions.iter().enumerate() {
        let colour = parts.colours.get(n).filter(|_| coloured);
        if binary {
            out.extend(p.iter().flat_map(|v| v.to_le_bytes()));
            out.extend(colour.into_iter().flatten());
        } else {
            out.extend(format!("{} {} {}", p[0], p[1], p[2]).bytes());
            if let Some(c) = colour {
                out.extend(format!(" {} {} {}", c[0], c[1], c[2]).bytes());
            }
            out.push(b'\n');
        }
    }
    for t in &parts.triangles {
        if binary {
            out.push(3);
            out.extend(t.iter().flat_map(|v| (*v as i32).to_le_bytes()));
        } else {
            out.extend(format!("3 {} {} {}\n", t[0], t[1], t[2]).bytes());
        }
    }
    out
}
