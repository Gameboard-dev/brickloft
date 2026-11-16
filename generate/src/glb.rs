//! The mesh file: written through Lux3D, and read back for checking.
//!
//! Writing is Lux3D's (`lux3d_core::export`): binary glTF with one mesh of
//! positions, a colour for each vertex and 32-bit indices. `mesh` says why
//! the colours are written as they are.
//!
//! The reader here reads only that layout. It is for the tests, which check
//! that what was written is what was meant, and for `examples/look.rs`,
//! which draws a mesh so that a person can see it. The editor has its own
//! reader, which reads the whole of what a `.glb` may hold and trusts none
//! of it.
//! https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#glb-file-format-specification

use crate::error::{Error, Result};
use crate::files;
use crate::level::Mesh;
use lux3d_core::export::TripoExportStage;
use lux3d_core::geometry::TripoMeshCpu;
use serde_json::Value;
use std::path::Path;

/// Writes a mesh to `path`, whose name must end in `.glb`.
pub fn write(mesh: &Mesh, path: &Path) -> Result<()> {
    let flat = TripoMeshCpu {
        // the two settings the mesh was drawn with, which the writer does not use
        resolution: 0,
        threshold: 0.0,
        vertices: mesh.points.iter().flatten().copied().collect(),
        faces: mesh.triangles.iter().flatten().map(|&i| i as i64).collect(),
        vertex_colors: mesh.colours.iter().flatten().copied().collect(),
    };
    TripoExportStage.write_mesh(&flat, path).map(|_| ()).map_err(|e| Error::Model { step: "writing the mesh", why: e.to_string() })
}

/// The longest file `read` takes: a mesh at the finest setting is about 10 MB.
const MAX_BYTES: u64 = 256 << 20;

/// Reads a mesh this program wrote.
pub fn read(path: &Path) -> Result<Mesh> {
    let bad = |why: &str| Error::Model { step: "reading the mesh", why: format!("{}: {why}", path.display()) };
    let bytes = files::read_small(path, MAX_BYTES)?;
    let word = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    // twelve bytes of header, then chunks: a length, a kind and that many bytes
    if bytes.get(..4) != Some(b"glTF") || word(4) != Some(2) {
        return Err(bad("it is not a version 2 .glb file"));
    }
    let json_length = word(12).filter(|_| bytes.get(16..20) == Some(b"JSON")).ok_or_else(|| bad("its first chunk is not JSON"))?;
    let json = bytes.get(20..20 + json_length).ok_or_else(|| bad("its JSON is cut short"))?;
    let bin_at = 20 + json_length;
    let bin_length = word(bin_at).filter(|_| bytes.get(bin_at + 4..bin_at + 8) == Some(b"BIN\0")).ok_or_else(|| bad("it has no binary chunk"))?;
    let bin = bytes.get(bin_at + 8..bin_at + 8 + bin_length).ok_or_else(|| bad("its binary chunk is cut short"))?;
    let doc: Value = serde_json::from_slice(json).map_err(|e| bad(&e.to_string()))?;

    // An accessor's numbers: `parts` to an item, of the given component type (5126 is a 32-bit float, 5125 a 32-bit whole number).
    let numbers = |accessor: &Value, kind: u64, parts: usize| -> Option<Vec<[u8; 4]>> {
        let accessor = &doc["accessors"][accessor.as_u64()? as usize];
        let view = &doc["bufferViews"][accessor["bufferView"].as_u64()? as usize];
        if accessor["componentType"].as_u64()? != kind {
            return None;
        }
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = (accessor["count"].as_u64()? as usize).checked_mul(parts)?;
        let data = bin.get(start..start.checked_add(count.checked_mul(4)?)?)?;
        Some(data.chunks_exact(4).map(|b| [b[0], b[1], b[2], b[3]]).collect())
    };
    let primitive = &doc["meshes"][0]["primitives"][0];
    let floats = |name: &str| numbers(&primitive["attributes"][name], 5126, 3).map(|n| n.chunks_exact(3).map(|c| [f32::from_le_bytes(c[0]), f32::from_le_bytes(c[1]), f32::from_le_bytes(c[2])]).collect::<Vec<_>>());
    let points = floats("POSITION").ok_or_else(|| bad("it has no positions as 32-bit floats"))?;
    let colours = floats("COLOR_0").ok_or_else(|| bad("it has no colours as 32-bit floats"))?;
    let indices = numbers(&primitive["indices"], 5125, 1).ok_or_else(|| bad("it has no 32-bit indices"))?;
    let triangles: Vec<[u32; 3]> = indices.chunks_exact(3).map(|t| [u32::from_le_bytes(t[0]), u32::from_le_bytes(t[1]), u32::from_le_bytes(t[2])]).collect();
    if colours.len() != points.len() || triangles.iter().flatten().any(|&i| i as usize >= points.len()) {
        return Err(bad("its colours or its triangles do not match its points"));
    }
    Ok(Mesh { points, colours, triangles })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::Scratch;
    use crate::level::tests::box_mesh;

    #[test]
    fn a_mesh_written_is_the_mesh_read_back_with_its_colours() {
        let scratch = Scratch::new("glb");
        let mut mesh = box_mesh([1.0, 0.8, 0.6], 0.2);
        for (i, colour) in mesh.colours.iter_mut().enumerate() {
            *colour = [i as f32 / 1000.0, 0.25, 0.75];
        }
        mesh.to_y_up();
        let path = scratch.0.join("box.glb");
        write(&mesh, &path).unwrap();
        assert_eq!(read(&path).unwrap(), mesh);

        // what a reader of .glb files goes by: the header, a mesh with COLOR_0, and the box's bounds with Y as its height
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"glTF");
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize, bytes.len(), "the header gives the file's length");
        let json_length = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        let doc: Value = serde_json::from_slice(&bytes[20..20 + json_length]).unwrap();
        let primitive = &doc["meshes"][0]["primitives"][0];
        assert!(primitive["attributes"]["COLOR_0"].is_u64() && primitive["attributes"]["POSITION"].is_u64());
        assert_eq!(primitive["attributes"].as_object().unwrap().len(), 2, "positions and colours, and no normals");
        let position = &doc["accessors"][primitive["attributes"]["POSITION"].as_u64().unwrap() as usize];
        let (min, max): (Vec<f64>, Vec<f64>) = (serde_json::from_value(position["min"].clone()).unwrap(), serde_json::from_value(position["max"].clone()).unwrap());
        assert!((min[1] - 0.0).abs() < 1e-6 && (max[1] - 0.6).abs() < 1e-6, "Y from the ground to the box's height: {min:?} {max:?}");
    }

    #[test]
    fn a_file_that_is_not_a_mesh_is_refused() {
        let scratch = Scratch::new("glb-bad");
        let path = scratch.0.join("bad.glb");
        for bytes in [&b""[..], b"glTF", b"not a mesh at all, only some text that is long enough", b"glTF\x02\0\0\0\x20\0\0\0\xff\xff\xff\x7fJSON{}"] {
            std::fs::write(&path, bytes).unwrap();
            assert!(read(&path).is_err());
        }
    }
}
