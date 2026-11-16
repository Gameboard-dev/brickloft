//! The readers tried on files written by rule, and on files written
//! wrongly on purpose: cut short, with counts that lie, with numbers that
//! are not numbers, naming points that are not there, nested without end.

use super::write::{glb, glb_of, obj, ply};
use super::*;
use brickloft_core::mesh::{made, Image, MAX_TRIANGLES, MAX_VERTICES};
use serde_json::{json, Value};

/// A ball with a colour at every point, as parts 10 across.
fn ball() -> Parts {
    made::rainbow_ball(6).parts(10.0)
}

/// A folder of this test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("brickloft-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// One triangle's worth of a `.glb`: three points as floats, and whatever
/// `change` does to the JSON that describes them.
fn triangle_glb(change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let points: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let mut bin: Vec<u8> = points.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    bin.extend([0u8, 1, 2, 0]);
    let mut doc = json!({
        "asset": { "version": "2.0" },
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1 }] }],
        "accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" }, { "bufferView": 1, "componentType": 5121, "count": 3, "type": "SCALAR" }],
        "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 36 }, { "buffer": 0, "byteOffset": 36, "byteLength": 3 }],
        "buffers": [{ "byteLength": 40 }],
    });
    change(&mut doc);
    glb_of(&doc, &bin)
}

#[test]
fn each_kind_of_file_gives_back_the_mesh_that_was_written() {
    let parts = ball();
    let near = |a: &Parts, b: &Parts| a.triangles == b.triangles && a.colours == b.colours && a.positions.len() == b.positions.len() && a.positions.iter().zip(&b.positions).all(|(p, q)| (0..3).all(|k| (p[k] - q[k]).abs() < 1e-4));
    for (kind, bytes) in [(Kind::Glb, glb(&parts)), (Kind::Obj, obj(&parts)), (Kind::Ply, ply(&parts, true)), (Kind::Ply, ply(&parts, false))] {
        let back = read(&bytes, kind).unwrap();
        assert!(near(&back, &parts), "{kind:?}: {} points and {} triangles came back for {} and {}", back.positions.len(), back.triangles.len(), parts.positions.len(), parts.triangles.len());
        assert!(back.uvs.is_empty() && back.images.is_empty() && back.image_of.is_empty());
        // and with no colours, none are made up
        let plain = Parts { colours: Vec::new(), ..parts.clone() };
        let bytes = match kind {
            Kind::Glb => glb(&plain),
            Kind::Obj => obj(&plain),
            Kind::Ply => ply(&plain, true),
        };
        assert!(near(&read(&bytes, kind).unwrap(), &plain), "{kind:?} with no colours");
    }
    // a file's kind goes by its ending, whatever its case, and nothing else is read
    assert_eq!((Kind::of("a.GLB"), Kind::of("b.c.obj"), Kind::of("C:\\x\\y.Ply")), (Ok(Kind::Glb), Ok(Kind::Obj), Ok(Kind::Ply)));
    assert_eq!(Kind::of("model.stl"), Err(MeshError::UnknownKind { name: "model.stl".into() }));
    assert!(matches!(Kind::of("glb"), Err(MeshError::UnknownKind { .. })));
}

#[test]
fn a_glb_cut_short_anywhere_is_refused_and_never_read_past_its_end() {
    let whole = glb(&ball());
    assert!(read(&whole, Kind::Glb).is_ok());
    // every length from nothing up to all but the last byte: refused each time, by name
    for keep in (0..whole.len()).step_by(7).chain([11, 12, 13, 19, 20, 21]) {
        let result = read(&whole[..keep], Kind::Glb);
        assert!(matches!(result, Err(MeshError::CutShort { .. }) | Err(MeshError::Malformed { .. })), "cut to {keep} of {}: {result:?}", whole.len());
    }
    assert_eq!(read(&whole[..8], Kind::Glb), Err(MeshError::CutShort { part: "header", needs: 12, has: 8 }));
    // the header says there is more than there is
    let mut longer = whole.clone();
    longer[8..12].copy_from_slice(&(whole.len() as u32 + 100).to_le_bytes());
    assert_eq!(read(&longer, Kind::Glb), Err(MeshError::CutShort { part: "chunks", needs: whole.len() as u64 + 100, has: whole.len() as u64 }));
    // a chunk says it is longer than the file
    let mut chunk = whole.clone();
    chunk[12..16].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
    assert!(matches!(read(&chunk, Kind::Glb), Err(MeshError::CutShort { part: "chunks", .. })));
}

#[test]
fn a_glb_whose_counts_lie_is_refused_before_anything_is_made_for_them() {
    // four billion points in thirty-six bytes
    let lying = triangle_glb(|doc| doc["accessors"][0]["count"] = json!(4_000_000_000u64));
    assert!(matches!(read(&lying, Kind::Glb), Err(MeshError::CutShort { part: "points", has: 36, .. })), "{:?}", read(&lying, Kind::Glb));
    // a count so large that its size cannot be added up at all
    let wrapping = triangle_glb(|doc| doc["accessors"][0]["count"] = json!(u64::MAX));
    assert!(matches!(read(&wrapping, Kind::Glb), Err(MeshError::Malformed { .. })));
    // a view that runs past the bytes there are, and one that starts past them
    let past = triangle_glb(|doc| doc["bufferViews"][0]["byteLength"] = json!(4000));
    assert_eq!(read(&past, Kind::Glb), Err(MeshError::CutShort { part: "points", needs: 4000, has: 40 }));
    let beyond = triangle_glb(|doc| doc["bufferViews"][1]["byteOffset"] = json!(u64::MAX - 1));
    assert!(matches!(read(&beyond, Kind::Glb), Err(MeshError::Malformed { .. })));
    // more triangles than there are numbers for, items closer together than they are wide, an offset into the next view
    let indices = triangle_glb(|doc| doc["accessors"][1]["count"] = json!(3_000_000));
    assert!(matches!(read(&indices, Kind::Glb), Err(MeshError::CutShort { part: "triangles", .. })));
    let stride = triangle_glb(|doc| doc["bufferViews"][0]["byteStride"] = json!(4));
    assert!(matches!(read(&stride, Kind::Glb), Err(MeshError::Malformed { .. })));
    let offset = triangle_glb(|doc| doc["accessors"][0]["byteOffset"] = json!(4));
    assert_eq!(read(&offset, Kind::Glb), Err(MeshError::CutShort { part: "points", needs: 40, has: 36 }));
    // no binary chunk at all
    let mut doc = Value::Null;
    triangle_glb(|d| doc = d.clone());
    assert_eq!(read(&glb_of(&doc, &[]), Kind::Glb), Err(MeshError::CutShort { part: "points", needs: 36, has: 0 }));
    // accessors, views and meshes that are named and are not there, and types that are no type
    for change in [
        (|doc: &mut Value| doc["meshes"][0]["primitives"][0]["attributes"]["POSITION"] = json!(7)) as fn(&mut Value),
        |doc| doc["accessors"][0]["bufferView"] = json!(9),
        |doc| doc["nodes"][0]["mesh"] = json!(5),
        |doc| doc["scenes"][0]["nodes"] = json!([3]),
        |doc| doc["accessors"][0]["componentType"] = json!(5124),
        |doc| doc["accessors"][0]["type"] = json!("VEC2"),
        |doc| doc["accessors"][0]["count"] = json!("three"),
    ] {
        assert!(matches!(read(&triangle_glb(change), Kind::Glb), Err(MeshError::Malformed { kind: "glTF", .. })));
    }
    // more parts listed than are read, by their count alone
    let crowded = triangle_glb(|doc| doc["nodes"] = Value::Array(vec![json!({}); 100_001]));
    assert!(matches!(read(&crowded, Kind::Glb), Err(MeshError::TooMany { what: Count::Parts, most: 100_000, .. })));
}

#[test]
fn a_triangle_that_names_a_point_that_is_not_there_is_refused_in_every_kind_of_file() {
    let glb = triangle_glb(|doc| doc["accessors"][1]["byteOffset"] = json!(0));
    assert!(read(&glb, Kind::Glb).is_ok());
    // the fourth byte of the triangles' view is the padding: three numbers from the second on are 1, 2 and... point 200
    let mut bytes = triangle_glb(|_| {});
    let at = bytes.len() - 2;
    bytes[at] = 200;
    assert_eq!(read(&bytes, Kind::Glb), Err(MeshError::NoSuchPoint { index: 200, points: 3 }));
    assert_eq!(read(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n", Kind::Obj), Err(MeshError::NoSuchPoint { index: 4, points: 3 }));
    assert_eq!(read(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 0\n", Kind::Obj), Err(MeshError::NoSuchPoint { index: 0, points: 3 }));
    assert_eq!(read(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf -1 -2 -4\n", Kind::Obj), Err(MeshError::NoSuchPoint { index: 4, points: 3 }));
    let mut parts = ball();
    parts.triangles[5][1] = parts.positions.len() as u32 + 3;
    for binary in [true, false] {
        assert_eq!(read(&ply(&parts, binary), Kind::Ply), Err(MeshError::NoSuchPoint { index: parts.positions.len() as u64 + 3, points: parts.positions.len() as u64 }));
    }
    // and one that gets past a reader is stopped where the mesh is made
    let mut parts = ball();
    parts.triangles[0][0] = 9999;
    assert_eq!(Mesh::with(parts).unwrap_err(), MeshError::NoSuchPoint { index: 9999, points: ball().positions.len() as u64 });
}

#[test]
fn nodes_nested_without_end_or_inside_themselves_are_refused() {
    // a node that is its own child
    let own = triangle_glb(|doc| doc["nodes"][0]["children"] = json!([0]));
    assert_eq!(read(&own, Kind::Glb), Err(MeshError::TooDeep { most: 64 }));
    // a hundred nodes, each inside the one before
    let chain = triangle_glb(|doc| doc["nodes"] = Value::Array((0..100).map(|n| if n < 99 { json!({ "children": [n + 1] }) } else { json!({ "mesh": 0 }) }).collect()));
    assert_eq!(read(&chain, Kind::Glb), Err(MeshError::TooDeep { most: 64 }));
    // sixty, which is a deep scene and a real one
    let deep = triangle_glb(|doc| doc["nodes"] = Value::Array((0..60).map(|n| if n < 59 { json!({ "children": [n + 1], "translation": [1, 0, 0] }) } else { json!({ "mesh": 0 }) }).collect()));
    assert_eq!(read(&deep, Kind::Glb).unwrap().positions[0], [59.0, 0.0, 0.0]);
    // thirty nodes, each holding the next one twice: a billion places for one mesh, stopped at the count
    let doubling = triangle_glb(|doc| doc["nodes"] = Value::Array((0..30).map(|n| if n < 29 { json!({ "children": [n + 1, n + 1] }) } else { json!({ "mesh": 0 }) }).collect()));
    assert!(matches!(read(&doubling, Kind::Glb), Err(MeshError::TooMany { what: Count::Parts, .. })));
    // JSON nested deeper than a scene is: refused by the JSON reader, without running out of stack
    let nested = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let mut bytes = b"glTF".to_vec();
    bytes.extend(2u32.to_le_bytes());
    bytes.extend((20 + nested.len() as u32).to_le_bytes());
    bytes.extend((nested.len() as u32).to_le_bytes());
    bytes.extend(0x4E4F_534Au32.to_le_bytes());
    bytes.extend(nested.bytes());
    assert!(matches!(read(&bytes, Kind::Glb), Err(MeshError::Malformed { .. })));
}

#[test]
fn what_a_glb_may_hold_that_is_not_read_is_refused_by_name() {
    let unsupported = |change: fn(&mut Value)| match read(&triangle_glb(change), Kind::Glb) {
        Err(MeshError::Unsupported { what }) => what,
        other => panic!("{other:?}"),
    };
    assert_eq!(unsupported(|doc| doc["accessors"][0]["sparse"] = json!({ "count": 1 })), "sparse accessors");
    assert_eq!(unsupported(|doc| doc["extensionsRequired"] = json!(["KHR_draco_mesh_compression"])), "Draco compression");
    assert_eq!(unsupported(|doc| doc["meshes"][0]["primitives"][0]["extensions"] = json!({ "KHR_draco_mesh_compression": { "bufferView": 0 } })), "Draco compression");
    assert_eq!(unsupported(|doc| doc["extensionsRequired"] = json!(["EXT_meshopt_compression"])), "meshopt compression");
    assert_eq!(unsupported(|doc| doc["buffers"][0]["uri"] = json!("model.bin")), "a buffer kept outside the file");
    assert_eq!(unsupported(|doc| doc["bufferViews"][0]["buffer"] = json!(1)), "a buffer kept outside the file");
    assert_eq!(unsupported(|doc| doc["extensionsRequired"] = json!(["VENDOR_something_else"])), "an extension it says it cannot be read without");
    // what changes only how the surface looks is left out, and the mesh read
    assert!(read(&triangle_glb(|doc| doc["extensionsRequired"] = json!(["KHR_materials_unlit", "KHR_texture_transform", "KHR_mesh_quantization"])), Kind::Glb).is_ok());
    // the first version of the format, and a file that is not one at all
    let mut old = triangle_glb(|_| {});
    old[4] = 1;
    assert_eq!(read(&old, Kind::Glb), Err(MeshError::Unsupported { what: "a version of glTF other than 2" }));
    assert!(matches!(read(b"not a glb file at all", Kind::Glb), Err(MeshError::Malformed { .. })));
    assert!(matches!(read(&glb_of(&json!("just a word"), &[]), Kind::Glb).and_then(Mesh::with), Err(MeshError::Empty)));
    let mut garbled = triangle_glb(|_| {});
    garbled[24] = b'}';
    assert!(matches!(read(&garbled, Kind::Glb), Err(MeshError::Malformed { .. })));
    // lines and points are no solid: passed over, which leaves nothing
    assert_eq!(read(&triangle_glb(|doc| doc["meshes"][0]["primitives"][0]["mode"] = json!(1)), Kind::Glb).and_then(Mesh::with).unwrap_err(), MeshError::Empty);
}

#[test]
fn a_glbs_nodes_strips_fans_small_numbers_and_colours_are_read_as_the_format_says() {
    // a node moved and scaled, holding a node turned a quarter turn about z, holding the mesh
    let placed = triangle_glb(|doc| doc["nodes"] = json!([{ "children": [1], "translation": [10, 0, 0], "scale": [2, 2, 2] }, { "mesh": 0, "rotation": [0.0, 0.0, 0.5f64.sqrt(), 0.5f64.sqrt()] }]));
    let back = read(&placed, Kind::Glb).unwrap();
    let near = |p: [f32; 3], q: [f32; 3]| (0..3).all(|k| (p[k] - q[k]).abs() < 1e-5);
    // (1, 0, 0) turned to (0, 1, 0), doubled, and moved along x
    assert!(near(back.positions[0], [10.0, 0.0, 0.0]) && near(back.positions[1], [10.0, 2.0, 0.0]) && near(back.positions[2], [8.0, 0.0, 0.0]), "{:?}", back.positions);
    // the same by a matrix, a column at a time
    let matrix = triangle_glb(|doc| doc["nodes"][0]["matrix"] = json!([0, 2, 0, 0, -2, 0, 0, 0, 0, 0, 2, 0, 10, 0, 0, 1]));
    assert!(read(&matrix, Kind::Glb).unwrap().positions.iter().zip(&back.positions).all(|(p, q)| near(*p, *q)));
    // a transform that is not numbers
    let broken = triangle_glb(|doc| doc["nodes"][0]["scale"] = json!([1e308, 1e308, 1e308]));
    assert!(matches!(read(&broken, Kind::Glb).and_then(Mesh::with), Err(MeshError::NotANumber { .. })));
    // one mesh in two places is two meshes; no scene at all, and every node that is nobody's child is a start
    let twice = triangle_glb(|doc| { doc["nodes"] = json!([{ "mesh": 0 }, { "mesh": 0, "translation": [5, 0, 0] }]); doc["scenes"] = json!([{ "nodes": [0, 1] }]); });
    assert_eq!(read(&twice, Kind::Glb).unwrap().triangles.len(), 2);
    let no_scene = triangle_glb(|doc| { doc["nodes"] = json!([{ "children": [1] }, { "mesh": 0 }]); doc.as_object_mut().unwrap().remove("scenes"); });
    assert_eq!(read(&no_scene, Kind::Glb).unwrap().triangles.len(), 1);
    let no_nodes = triangle_glb(|doc| { doc.as_object_mut().unwrap().remove("nodes"); doc.as_object_mut().unwrap().remove("scenes"); });
    assert_eq!(read(&no_nodes, Kind::Glb).unwrap().triangles.len(), 1);

    // five points in a row of triangles: a strip is three triangles, every other one the other way round, and a fan three from the first point
    let row = |mode: u64, indices: Option<Vec<u16>>| {
        let points: [[f32; 3]; 5] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [0.0, 2.0, 0.0]];
        let mut bin: Vec<u8> = points.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
        let mut doc = json!({
            "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "mode": mode }] }],
            "accessors": [{ "bufferView": 0, "componentType": 5126, "count": 5, "type": "VEC3" }],
            "bufferViews": [{ "buffer": 0, "byteLength": 60 }],
            "buffers": [{ "byteLength": 60 }],
        });
        if let Some(indices) = indices {
            doc["meshes"][0]["primitives"][0]["indices"] = json!(1);
            doc["accessors"].as_array_mut().unwrap().push(json!({ "bufferView": 1, "componentType": 5123, "count": indices.len(), "type": "SCALAR" }));
            doc["bufferViews"].as_array_mut().unwrap().push(json!({ "buffer": 0, "byteOffset": 60, "byteLength": indices.len() * 2 }));
            bin.extend(indices.iter().flat_map(|v| v.to_le_bytes()));
        }
        read(&glb_of(&doc, &bin), Kind::Glb).unwrap().triangles
    };
    assert_eq!(row(5, None), vec![[0, 1, 2], [2, 1, 3], [2, 3, 4]]);
    assert_eq!(row(6, None), vec![[0, 1, 2], [0, 2, 3], [0, 3, 4]]);
    assert_eq!(row(4, Some(vec![4, 3, 2, 2, 1, 0])), vec![[4, 3, 2], [2, 1, 0]]);
    assert_eq!(row(5, Some(vec![4, 3, 2, 1])), vec![[4, 3, 2], [2, 3, 1]]);

    // points as whole numbers standing for measures, and a colour given as light with a see-through part
    let points: [[i16; 3]; 3] = [[0, 0, 0], [32767, 0, 0], [0, -32767, 0]];
    let mut bin: Vec<u8> = points.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    bin.extend([0u8; 2]);
    bin.extend([[65535u16, 0, 0, 65535], [0, 32768, 0, 0], [0, 0, 65535, 65535]].iter().flatten().flat_map(|v| v.to_le_bytes()));
    let doc = json!({
        "extensionsUsed": ["KHR_mesh_quantization"], "extensionsRequired": ["KHR_mesh_quantization"],
        "nodes": [{ "mesh": 0 }],
        "materials": [{ "pbrMetallicRoughness": { "baseColorFactor": [1.0, 0.18, 1.0, 1.0] } }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0, "COLOR_0": 1 }, "material": 0 }] }],
        "accessors": [{ "bufferView": 0, "componentType": 5122, "normalized": true, "count": 3, "type": "VEC3" }, { "bufferView": 1, "componentType": 5123, "normalized": true, "count": 3, "type": "VEC4" }],
        "bufferViews": [{ "buffer": 0, "byteLength": 18 }, { "buffer": 0, "byteOffset": 20, "byteLength": 24 }],
        "buffers": [{ "byteLength": 44 }],
    });
    let small = read(&glb_of(&doc, &bin), Kind::Glb).unwrap();
    assert_eq!(small.positions, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, -1.0, 0.0]]);
    // each point's own colour as it is written, times the material's, which is light (0.18 of it is byte 118)
    assert_eq!(small.colours, vec![[255, 0, 0], [0, 59, 0], [0, 0, 255]]);
    // colours for some of the points only
    let mut uneven = doc.clone();
    uneven["accessors"][1]["count"] = json!(2);
    assert_eq!(read(&glb_of(&uneven, &bin), Kind::Glb), Err(MeshError::Uneven { part: "colours", found: 2, points: 3 }));
}

/// A PNG of `width` by `height` in which pixel (x, y) is `at(x, y)`.
fn png_of(width: u32, height: u32, colour: png::ColorType, depth: png::BitDepth, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(colour);
    encoder.set_depth(depth);
    encoder.write_header().unwrap().write_image_data(data).unwrap();
    out
}

/// The triangle with a picture for its material: the picture's bytes go after the triangle's own.
fn pictured(picture: &[u8]) -> Result<Parts, MeshError> {
    let points: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let places: [[f32; 2]; 3] = [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75]];
    let mut bin: Vec<u8> = points.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    bin.extend(places.iter().flatten().flat_map(|v| v.to_le_bytes()));
    bin.extend(picture);
    let doc = json!({
        "nodes": [{ "mesh": 0 }],
        "images": [{ "bufferView": 2, "mimeType": "image/png" }],
        "textures": [{ "source": 0 }],
        "materials": [{ "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 } } }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0, "TEXCOORD_0": 1 }, "material": 0 }] }],
        "accessors": [{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" }, { "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2" }],
        "bufferViews": [{ "buffer": 0, "byteLength": 36 }, { "buffer": 0, "byteOffset": 36, "byteLength": 24 }, { "buffer": 0, "byteOffset": 60, "byteLength": picture.len() }],
        "buffers": [{ "byteLength": bin.len() }],
    });
    read(&glb_of(&doc, &bin), Kind::Glb)
}

#[test]
fn a_png_picture_inside_a_glb_paints_its_mesh_and_any_other_picture_is_left_out() {
    // two by two: red, green over blue, white
    let rgb = [255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let parts = pictured(&png_of(2, 2, png::ColorType::Rgb, png::BitDepth::Eight, &rgb)).unwrap();
    assert_eq!((parts.images.clone(), parts.image_of.clone(), parts.uvs.len()), (vec![Image { width: 2, height: 2, rgb: rgb.to_vec() }], vec![0], 3));
    let mesh = Mesh::with(parts).unwrap();
    let ([a, b, c], _) = mesh.triangle(0);
    // each corner of the triangle is in a different pixel of it
    let at = |p| mesh.colour(0, p).unwrap();
    let corners = [at(a * 0.98 + b * 0.01 + c * 0.01), at(b * 0.98 + a * 0.01 + c * 0.01), at(c * 0.98 + a * 0.01 + b * 0.01)];
    assert!(corners.contains(&[255, 0, 0]) && corners.contains(&[0, 255, 0]) && corners.contains(&[0, 0, 255]), "{corners:?}");
    // with a see-through part, in greys, and in sixteen bits: each read as colours
    let rgba: Vec<u8> = rgb.chunks(3).flat_map(|p| [p[0], p[1], p[2], 9]).collect();
    assert_eq!(pictured(&png_of(2, 2, png::ColorType::Rgba, png::BitDepth::Eight, &rgba)).unwrap().images[0].rgb, rgb);
    assert_eq!(pictured(&png_of(2, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[7, 200])).unwrap().images[0].rgb, [7, 7, 7, 200, 200, 200]);
    assert_eq!(pictured(&png_of(1, 1, png::ColorType::Rgb, png::BitDepth::Sixteen, &[255, 255, 128, 0, 0, 0])).unwrap().images[0].rgb, [255, 128, 0]);
    // a JPEG, by how it begins, and bytes that are no picture: the mesh is read, in no colours
    for other in [&[0xFFu8, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F'][..], b"RIFF....WEBPVP8 ", b""] {
        let plain = pictured(other).unwrap();
        assert!(plain.images.is_empty() && plain.image_of.is_empty() && plain.uvs.is_empty() && plain.colours.is_empty() && plain.triangles.len() == 1);
    }
    // a PNG cut short is a file that is wrong, and says so
    let whole = png_of(2, 2, png::ColorType::Rgb, png::BitDepth::Eight, &rgb);
    assert!(matches!(pictured(&whole[..whole.len() - 20]), Err(MeshError::Malformed { kind: "PNG picture", .. })));
    // one larger than is read, refused by the size in its header (a bit a pixel, so the file itself is small)
    let side = 4097u32;
    let large = png_of(side, side, png::ColorType::Grayscale, png::BitDepth::One, &vec![0u8; (side as usize).div_ceil(8) * side as usize]);
    assert_eq!(pictured(&large), Err(MeshError::TooMany { what: Count::Pixels, found: side as u64 * side as u64, most: 4096 * 4096 }));
}

#[test]
fn an_obj_is_read_line_by_line_and_what_is_wrong_with_one_is_said_with_its_line() {
    let text = b"# a square and a colour\r\nmtllib other.mtl\r\no square\r\nv 0 0 0 1 0 0\r\nv 1 0 0 0 1 0\nv 1 1 0 0 0 1\nv 0 1 0 1 1 1 # the last\nvn 0 0 1\nvt 0.5 0.5\nusemtl red\ns off\nf 1/1/1 2/1/1 3//1 4\nf -4 -3 -2\n";
    let parts = read(text, Kind::Obj).unwrap();
    assert_eq!(parts.positions, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]);
    assert_eq!(parts.colours, vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]]);
    // a face of four points is a fan of two triangles; points counted back from the last
    assert_eq!(parts.triangles, vec![[0, 1, 2], [0, 2, 3], [0, 1, 2]]);
    assert_eq!(read(b"v 0 0 0\nv 1 zero 0\n", Kind::Obj), Err(MeshError::NotANumber { part: "points", at: 2 }));
    assert_eq!(read(b"v 0 0 0\nv 1 0 0\nv 0 1 0\n\nf 1 2 three\n", Kind::Obj), Err(MeshError::NotANumber { part: "faces", at: 5 }));
    assert!(matches!(read(b"v 0 0\n", Kind::Obj), Err(MeshError::Malformed { kind: "OBJ", .. })));
    assert_eq!(read(b"v 0 0 0 1 1 1\nv 1 0 0\nv 0 1 0\nf 1 2 3\n", Kind::Obj), Err(MeshError::Uneven { part: "colours", found: 1, points: 3 }));
    // numbers that are written as numbers and are none
    for word in ["nan", "inf", "-inf", "1e999"] {
        let text = format!("v 0 0 0\nv 1 0 0\nv 0 {word} 0\nf 1 2 3\n");
        assert_eq!(read(text.as_bytes(), Kind::Obj).and_then(Mesh::with).unwrap_err(), MeshError::NotANumber { part: "points", at: 2 }, "{word}");
    }
    // points and no faces, nothing at all, and bytes that are not text
    assert_eq!(read(b"v 0 0 0\nv 1 0 0\nv 0 1 0\n", Kind::Obj).and_then(Mesh::with).unwrap_err(), MeshError::Empty);
    assert_eq!(read(b"", Kind::Obj).and_then(Mesh::with).unwrap_err(), MeshError::Empty);
    assert_eq!(read(&[b'v', b' ', 0xff, 0xfe, b' ', b'0', b' ', b'0', b'\n'], Kind::Obj), Err(MeshError::NotANumber { part: "points", at: 1 }));
    // more points than are read: counted and refused before room is made for one
    let many = "v 0 0 0\n".repeat(MAX_VERTICES + 1);
    assert_eq!(read(many.as_bytes(), Kind::Obj), Err(MeshError::TooMany { what: Count::Points, found: MAX_VERTICES as u64 + 1, most: MAX_VERTICES as u64 }));
    // and more triangles, as one face of very many corners
    let fan = format!("v 0 0 0\nf{}\n", " 1".repeat(MAX_TRIANGLES + 3));
    assert_eq!(read(fan.as_bytes(), Kind::Obj), Err(MeshError::TooMany { what: Count::Triangles, found: MAX_TRIANGLES as u64 + 1, most: MAX_TRIANGLES as u64 }));
}

#[test]
fn a_ply_is_read_by_its_header_and_one_that_lies_or_is_cut_short_is_refused() {
    let parts = ball();
    for binary in [true, false] {
        let whole = ply(&parts, binary);
        // Cut anywhere after the header: short. (In text a number cut in two may be no number, which is said instead;
        // or still a number, and then what follows it is missing; or, cut within the last number of all, a whole file
        // with one number wrong, which only whoever wrote it could know.)
        let header = whole.windows(10).position(|w| w == b"end_header").unwrap() + 11;
        for keep in (header..whole.len() - 1).step_by(13) {
            let result = read(&whole[..keep], Kind::Ply).and_then(Mesh::with);
            let allowed = matches!(result, Err(MeshError::CutShort { .. })) || (!binary && (matches!(result, Err(MeshError::NotANumber { .. })) || keep > whole.len() - 12));
            assert!(allowed, "cut to {keep} of {}: {:?}", whole.len(), result.err());
        }
        // a header that promises more points than the file could hold
        let text = String::from_utf8_lossy(&whole[..header]).replace(&format!("element vertex {}", parts.positions.len()), "element vertex 4000000000");
        let lying = [text.as_bytes(), &whole[header..]].concat();
        assert!(matches!(read(&lying, Kind::Ply), Err(MeshError::CutShort { part: "points", .. })), "{:?}", read(&lying, Kind::Ply).err());
        // and more faces
        let text = String::from_utf8_lossy(&whole[..header]).replace(&format!("element face {}", parts.triangles.len()), "element face 18446744073709551615");
        let lying = [text.as_bytes(), &whole[header..]].concat();
        assert!(matches!(read(&lying, Kind::Ply), Err(MeshError::CutShort { part: "faces", .. })));
    }
    // other elements and properties are stepped over by their sizes; a face of four points is two triangles; colours as floats
    let mut file = b"ply\nformat binary_little_endian 1.0\ncomment made by rule\nelement camera 1\nproperty double focal\nproperty list uchar short path\nelement vertex 4\nproperty float x\nproperty float nx\nproperty float y\nproperty double z\nproperty float red\nproperty float green\nproperty float blue\nproperty list uchar uchar tags\nelement face 1\nproperty uchar flags\nproperty list uint ushort vertex_index\nelement edge 1\nproperty int a\nproperty int b\nend_header\n".to_vec();
    file.extend(50f64.to_le_bytes());
    file.extend([2u8, 1, 0, 2, 0]);
    for (n, p) in [[0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]].iter().enumerate() {
        file.extend(p[0].to_le_bytes());
        file.extend(9f32.to_le_bytes());
        file.extend(p[1].to_le_bytes());
        file.extend((p[2] as f64).to_le_bytes());
        file.extend([1f32, 0.5, 0.0].iter().flat_map(|v| v.to_le_bytes()));
        file.extend(std::iter::once(n as u8).chain((0..n as u8).map(|t| t + 100)));
    }
    file.push(7);
    file.extend(4u32.to_le_bytes());
    file.extend([0u16, 1, 2, 3].iter().flat_map(|v| v.to_le_bytes()));
    file.extend([1i32, 2].iter().flat_map(|v| v.to_le_bytes()));
    let read_back = read(&file, Kind::Ply).unwrap();
    assert_eq!(read_back.positions, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]]);
    assert_eq!((read_back.triangles, read_back.colours), (vec![[0, 1, 2], [0, 2, 3]], vec![[255, 128, 0]; 4]));
    // what is not read, and what is not a PLY file
    assert_eq!(read(b"ply\nformat binary_big_endian 1.0\nelement vertex 0\nend_header\n", Kind::Ply), Err(MeshError::Unsupported { what: "big-endian numbers" }));
    for wrong in [&b"plain text"[..], b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\n", b"ply\nelement vertex 0\nend_header\n", b"ply\nformat ascii 1.0\nproperty float x\nend_header\n", b"ply\nformat ascii 1.0\nelement vertex many\nend_header\n", b"ply\nformat ascii 1.0\nelement vertex 1\nproperty quad x\nend_header\n", b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nend_header\n1 2\n", b"ply\nformat ascii 1.0\nelement vertex 1\nproperty list float int x\nend_header\n"] {
        assert!(matches!(read(wrong, Kind::Ply), Err(MeshError::Malformed { kind: "PLY", .. })), "{:?}: {:?}", String::from_utf8_lossy(wrong), read(wrong, Kind::Ply).err());
    }
    assert_eq!(read(b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n1 two 3\n", Kind::Ply), Err(MeshError::NotANumber { part: "points", at: 0 }));
    // a header with more in it than a header has
    let crowded = format!("ply\nformat ascii 1.0\n{}end_header\n", "element thing 0\n".repeat(300));
    assert!(matches!(read(crowded.as_bytes(), Kind::Ply), Err(MeshError::TooMany { what: Count::Parts, .. })));
    let endless = format!("ply\nformat ascii 1.0\n{}", "comment on and on\n".repeat(5000));
    assert!(matches!(read(endless.as_bytes(), Kind::Ply), Err(MeshError::Malformed { .. })));
    // more points than are read, in a file long enough to hold them
    let mut many = format!("ply\nformat binary_little_endian 1.0\nelement vertex {}\nproperty uchar x\nproperty uchar y\nproperty uchar z\nend_header\n", MAX_VERTICES + 1).into_bytes();
    many.resize(many.len() + 3 * (MAX_VERTICES + 1), 0);
    assert_eq!(read(&many, Kind::Ply), Err(MeshError::TooMany { what: Count::Points, found: MAX_VERTICES as u64 + 1, most: MAX_VERTICES as u64 }));
}

#[test]
fn a_file_is_taken_in_under_a_name_from_its_content_and_found_again_by_that_name_alone() {
    let (from, kept) = (scratch("from"), scratch("kept"));
    let parts = made::house().parts(56.0);
    let path = from.join("My House (final).GLB");
    std::fs::write(&path, glb(&parts)).unwrap();
    let (name, up, _) = adopt_into(&kept, &path).unwrap();
    // (z is up in it, whatever a .glb is said to be: the house stands on a flat floor that is low along z)
    assert_eq!((Import::named(&name), up, name.ends_with(".glb")), (Ok(()), Up::Z, true));
    assert_eq!(std::fs::read(kept.join(&name)).unwrap(), glb(&parts));
    assert_eq!(name, name_of(&glb(&parts), Kind::Glb));
    // found again from the folder alone, with the file it came from gone; the same mesh, and the same one in hand each time
    std::fs::remove_file(&path).unwrap();
    let mesh = open_from(&kept, &name, Up::Y, 0).unwrap();
    assert!(Arc::ptr_eq(&mesh, &open_from(&kept, &name, Up::Y, 0).unwrap()));
    // smoothed, it is another mesh of the same triangles in the same box
    let smoothed = open_from(&kept, &name, Up::Y, 5).unwrap();
    assert!(smoothed.id != mesh.id && smoothed.triangles() == mesh.triangles() && smoothed.half == mesh.half);
    assert_eq!(mesh.triangles(), parts.triangles.len());
    // y is up in the file, so the house, 30 tall in the file's z, lies on its side until it is said that z is up
    assert!((mesh.half.y - 30.0 / 112.0).abs() < 1e-6 && (open_from(&kept, &name, Up::Z, 0).unwrap().half.z - 30.0 / 112.0).abs() < 1e-6, "{}", mesh.half);
    // taken in again, it is the one file still
    std::fs::write(&path, glb(&parts)).unwrap();
    assert_eq!(adopt_into(&kept, &path).unwrap().0, name);
    assert_eq!(std::fs::read_dir(&kept).unwrap().count(), 1);
    // a different file is a different name, and a PLY file's up is z
    let other = from.join("ball.ply");
    std::fs::write(&other, ply(&ball(), true)).unwrap();
    let (second, up, _) = adopt_into(&kept, &other).unwrap();
    assert!(second != name && second.ends_with(".ply") && up == Up::Z);

    // what is not there, and what is no name of the folder's: nothing outside it is ever looked at
    assert_eq!(open_from(&kept, "0123456789abcdef.glb", Up::Y, 0).err(), Some(MeshError::Missing { name: "0123456789abcdef.glb".into() }));
    std::fs::write(from.join("secret.glb"), glb(&parts)).unwrap();
    for outside in ["../brickloft-test-from/secret.glb", "secret.glb", &from.join("secret.glb").display().to_string(), "..\\secret.glb", "", "0123456789abcdef.glb/../secret.glb"] {
        assert!(matches!(open_from(&kept, outside, Up::Y, 0), Err(MeshError::NotAnImport { .. })), "{outside}");
    }
    // a file that is no mesh is refused and is not copied in; nor one of a kind that is not read, a folder, or nothing
    let bad = from.join("bad.obj");
    std::fs::write(&bad, b"v 0 0 0\nf 1 2 3\n").unwrap();
    assert_eq!(adopt_into(&kept, &bad), Err(MeshError::NoSuchPoint { index: 2, points: 1 }));
    std::fs::write(from.join("model.stl"), b"solid").unwrap();
    assert!(matches!(adopt_into(&kept, &from.join("model.stl")), Err(MeshError::UnknownKind { .. })));
    assert!(matches!(adopt_into(&kept, &from.join("gone.glb")), Err(MeshError::File { .. })));
    std::fs::create_dir_all(from.join("folder.glb")).unwrap();
    assert!(matches!(adopt_into(&kept, &from.join("folder.glb")), Err(MeshError::File { .. })));
    assert_eq!(std::fs::read_dir(&kept).unwrap().count(), 2);
    // a file larger than is read is refused by its size, before any of it is read
    let huge = from.join("huge.ply");
    std::fs::File::create(&huge).unwrap().set_len(MAX_FILE + 1).unwrap();
    assert_eq!(adopt_into(&kept, &huge), Err(MeshError::TooMany { what: Count::Bytes, found: MAX_FILE + 1, most: MAX_FILE }));
    let _ = std::fs::remove_dir_all(&from);
    let _ = std::fs::remove_dir_all(&kept);
}

#[test]
fn a_form_is_given_its_mesh_from_the_folder_and_built_once_for_all_who_ask() {
    let from = scratch("attach");
    let path = from.join("gem.obj");
    std::fs::write(&path, obj(&made::gem().parts(48.0))).unwrap();
    let (name, up, _) = adopt(&path).unwrap();
    let mut spec = Spec { form: Form::Imported, points: vec![brickloft_core::DVec3::new(0.5, 0.5, 0.5)], radius: 24.0, fill: true, import: Import { name: name.clone(), up, ..Import::default() }, ..Spec::default() };
    assert!(brickloft_core::spec::build(&spec).unwrap_err().contains("is not in the imports folder"));
    attach(&mut spec);
    assert!(spec.import.mesh.is_some());
    let first = built(&spec).unwrap();
    assert!(!first.bricks.is_empty() && first.error.abs() < 1e-9, "{} bricks, {} wrong", first.bricks.len(), first.error);
    // Asked again, wherever it stands, it is not built again: the second answer comes at once. (One mesh is
    // remembered and the tests beside this one build theirs at the same time, so it is tried until none came between.)
    let at_once = (0..50).any(|_| {
        let slow = built(&spec).unwrap();
        let started = std::time::Instant::now();
        let again = built(&Spec { grid_at: [3.0, 0.0, 0.0], on_grid: 4, ..spec.clone() }).unwrap();
        again.bricks == first.bricks && slow.bricks == first.bricks && started.elapsed().as_micros() < 2000
    });
    assert!(at_once, "a mesh just built was built again when asked for a second time");
    // a change is a new build
    assert!(built(&Spec { radius: 12.0, ..spec.clone() }).unwrap().bricks != first.bricks);
    // a form that names a mesh that is not there is left without, and one that is no mesh's form is left alone
    let mut missing = Spec { import: Import { name: "0000000000000000.glb".into(), ..Import::default() }, ..spec.clone() };
    attach(&mut missing);
    assert!(missing.import.mesh.is_none() && built(&missing).unwrap_err().contains("0000000000000000.glb"));
    assert!(listed().iter().any(|(listed, size)| *listed == name && *size > 100));
    assert_eq!(paint_of(&spec), None);
    let _ = std::fs::remove_dir_all(&from);
}

/// Let the window's frames go by until the form being edited has been built for what it now is.
fn settle(state: &mut crate::state::State) {
    let started = std::time::Instant::now();
    let now = || started.elapsed().as_secs_f64() * 1000.0;
    for _ in 0..3000 {
        state.refresh_behind(now);
        if !state.fitting() {
            // (one more frame: the first after a change only begins the build)
            state.refresh_behind(now);
            if !state.fitting() {
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the form was not built in half a minute");
}

#[test]
fn a_mesh_is_opened_in_the_editor_built_behind_the_window_and_saved_by_its_name() {
    use crate::paint::Mode;
    use crate::state::{State, IMPORTED};
    let from = scratch("editor");
    let path = from.join("painted ball.glb");
    std::fs::write(&path, glb(&made::painted_ball(16).parts(30.0))).unwrap();
    let mut state = State::new();
    state.form = IMPORTED;
    state.preset();
    // (at single microbricks, where a smaller size is fewer bricks: left to the budget, a larger model is fitted in larger bricks)
    state.spec.grain = 1;
    settle(&mut state);
    // no file yet: nothing is drawn, and the form says why
    assert!(state.built.bricks.is_empty() && state.problem.as_deref() == Some(MeshError::NoFile.to_string().as_str()), "{:?}", state.problem);
    // a path that is no file changes nothing and is said where the form's problem is said
    state.open_mesh(&from.join("nothing.glb").display().to_string(), false);
    assert!(state.problem.as_ref().is_some_and(|p| p.contains("nothing.glb")) && state.spec.import.name.is_empty());
    // the file, by its path as a file manager copies it
    state.open_mesh(&format!(" \"{}\" ", path.display()), false);
    let name = state.spec.import.name.clone();
    assert!(Import::named(&name).is_ok() && state.spec.import.mesh.is_some() && state.problem.is_none());
    // in the mesh's own colours, and not built yet: the window has not been held up for it
    assert_eq!((state.paint.mode, state.paint.colours.len()), (Mode::Pattern, 12));
    assert!(state.built.bricks.is_empty());
    // the first frame after begins its build on another thread and draws on without it
    assert!(!state.refresh_behind(|| 0.0) && state.fitting() && state.built.bricks.is_empty());
    settle(&mut state);
    let first = state.built.bricks.len();
    assert!(first > 50 && state.built.shades.len() == first && state.problem.is_none(), "{first} bricks, {:?}", state.problem);
    let colours: std::collections::HashSet<[u8; 3]> = state.painted().iter().filter_map(|brick| brick.1).collect();
    assert!(colours.len() >= 3 && colours.iter().all(|c| state.paint.colours.contains(c)));
    // its dot is at the end of its longest side, half way up (a ball is as long each way, and the first way is x); it stands on the floor
    let (handle, dot) = *state.handles().iter().find(|h| h.0 == crate::state::Handle::Radius).unwrap();
    let (low, high) = state.bounds.unwrap();
    assert!((dot.x - high[0] as f64).abs() <= 1.0 && dot.z == 24.0 && low[2] == 0, "the dot is at {dot} and the bricks reach from {low:?} to {high:?}");
    // a change of size or of brick size builds it again from the mesh in hand: the file is not read or indexed again
    let mesh = state.spec.import.mesh.clone().unwrap();
    // the dot dragged half way in halves its size
    let eye = dot + brickloft_core::DVec3::new(0.0, -200.0, 0.0);
    state.drag(handle, eye, (dot - brickloft_core::DVec3::X * 12.0 - eye).normalize(), brickloft_core::DVec3::Y);
    assert_eq!(state.spec.radius, 12.0);
    settle(&mut state);
    // and it stands where it stood
    assert_eq!(state.bounds.map(|b| (b.0[2], b.1[2])), Some((0, 24)));
    let smaller = state.built.bricks.len();
    state.spec.grain = 2;
    settle(&mut state);
    assert!(smaller < first && state.built.bricks.len() < smaller && state.built.grain == 2, "{first} bricks, then {smaller}, then {} at brick size {}", state.built.bricks.len(), state.built.grain);
    assert!(Arc::ptr_eq(&mesh, state.spec.import.mesh.as_ref().unwrap()) && Arc::ptr_eq(&mesh, &open(&name, state.spec.import.up, state.spec.import.smooth).unwrap()));
    // fewer colours asked for are the mesh's own again, as many as asked
    state.spec.import.colours = 3;
    state.paint = paint_of(&state.spec).unwrap();
    assert_eq!(state.paint.colours.len(), 3);

    // a save names the mesh and holds no path to it; opened, the form has its mesh back and is the form it was
    let saved = crate::store::describe(&state.forms());
    assert!(saved.contains(&format!("import = {name},y,smoothed,0,3,0")) && !saved.contains("painted ball") && !saved.contains(&from.display().to_string()), "{saved}");
    let opened = crate::store::forms_in(&saved).unwrap();
    assert!(opened[0].0 == state.forms()[0].0 && opened[0].0.import.mesh.is_some() && opened[0].1 == state.paint);
    // on a computer that has not the file, the form opens empty and says what is missing
    let elsewhere = crate::store::forms_in(&saved.replace(&name, "00000000000000aa.glb")).unwrap();
    assert!(elsewhere[0].0.import.mesh.is_none());
    state.open(elsewhere);
    settle(&mut state);
    assert!(state.built.bricks.is_empty() && state.problem.as_deref() == Some(MeshError::Missing { name: "00000000000000aa.glb".into() }.to_string().as_str()), "{:?}", state.problem);
    // and Ctrl+Z brings the mesh back with its bricks
    assert!(state.undo());
    settle(&mut state);
    assert!(state.spec.import.name == name && !state.built.bricks.is_empty() && state.problem.is_none());

    // dropped on the window while another form is being edited, a mesh is a new form beside it
    let mut state = State::new();
    let before = state.others.len();
    state.open_mesh(&path.display().to_string(), true);
    assert_eq!((state.form, state.others.len(), state.spec.import.name.as_str()), (IMPORTED, before + 1, name.as_str()));
    // (and built behind the window like any mesh, though the form it took the place of was quick to build)
    assert!(state.built.bricks.is_empty() && !state.refresh_behind(|| 0.0) && state.fitting());
    settle(&mut state);
    assert!(!state.built.bricks.is_empty() && state.bounds.is_some());
    // what cannot be opened is said in a notice, and the workspace is as it was
    std::fs::write(from.join("broken.glb"), &glb(&ball())[..40]).unwrap();
    state.open_mesh(&from.join("broken.glb").display().to_string(), true);
    assert!(state.alert.as_ref().is_some_and(|(title, why)| *title == "Cannot open the mesh" && why.contains("cut short")) && state.others.len() == before + 1);
    let _ = std::fs::remove_dir_all(&from);
}

#[test]
fn a_program_driving_the_editor_names_a_mesh_in_the_folder_and_can_name_nothing_else() {
    use crate::mcp::apply;
    use crate::state::{State, IMPORTED};
    let from = scratch("mcp");
    let path = from.join("lumpy.ply");
    std::fs::write(&path, ply(&made::rainbow_ball(12).parts(20.0), true)).unwrap();
    let (name, _, _) = adopt(&path).unwrap();
    let outside = from.join("0123456789abcdef.glb");
    std::fs::write(&outside, glb(&ball())).unwrap();
    let mut state = State::new();
    let listed: Value = serde_json::from_str(&apply(&mut state, "list_meshes", &json!({}), || 0.0).unwrap()).unwrap();
    assert!(listed["meshes"].as_array().unwrap().iter().any(|m| m["name"] == name.as_str() && m["bytes"].as_u64() > Some(100)));
    // every way of naming a file that is not a name of the folder's own: refused, and nothing is added
    let outside_text = outside.display().to_string();
    for import in ["../../Windows/win.ini", "C:\\Windows\\win.ini", "/etc/passwd", outside_text.as_str(), "lumpy.ply", "..\\0123456789abcdef.glb", "0123456789abcdef.glb/../../x", "file:///c:/x.glb", ""] {
        let refused = apply(&mut state, "add_form", &json!({ "settings": { "form": "imported", "import": import } }), || 0.0);
        assert!(refused.is_err() && state.others.is_empty(), "{import}: {refused:?}");
    }
    // a name of the right shape that is no file in the folder (though a file of that name is elsewhere), and no name at all
    let absent = apply(&mut state, "add_form", &json!({ "settings": { "form": "imported", "import": "0123456789abcdef.glb" } }), || 0.0);
    assert_eq!(absent, Err(MeshError::Missing { name: "0123456789abcdef.glb".into() }.to_string()));
    assert!(apply(&mut state, "add_form", &json!({ "settings": { "form": "imported" } }), || 0.0).unwrap_err().contains("list_meshes"));
    assert!(state.others.is_empty());
    // by its name it is added, at the size the editor opens one at, filled, in its own colours, and built
    let added: Value = serde_json::from_str(&apply(&mut state, "add_form", &json!({ "settings": { "form": "imported", "import": name } }), || 0.0).unwrap()).unwrap();
    assert_eq!((state.form, state.spec.radius, state.spec.fill, state.others.len()), (IMPORTED, crate::state::USUAL_MESH_RADIUS, true, 1));
    assert!(added["forms"][0]["bricks"].as_u64() > Some(100) && added["forms"][0]["settings"]["import"] == format!("{name},z,smoothed,0,12,0").as_str(), "{}", added["forms"][0]);
    assert_eq!((state.paint.mode, state.paint.colours.len(), state.built.shades.len()), (crate::paint::Mode::Pattern, 12, state.built.bricks.len()));
    // changed: cubes alone, four colours, and its paint follows
    apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "import": format!("{name},z,blocks,0,4") } }), || 0.0).unwrap();
    assert!(state.built.bricks.iter().all(|b| b.mask == brickloft_core::shapes::CUBE) && state.paint.colours.len() == 4 && state.built.shades.iter().all(|s| *s < 4));
    // and a change to another file's path is refused like an addition, leaving it as it was
    let was = state.spec.import.clone();
    assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "import": outside_text } }), || 0.0).is_err());
    assert_eq!(state.spec.import, was);
    let _ = std::fs::remove_dir_all(&from);
}

#[test]
fn which_way_is_up_is_told_from_a_mesh_a_network_made_and_else_from_what_it_stands_on() {
    // the figure as such a program writes it: within a box one unit across, coloured by its points, z up, in a .glb
    let made = made::figure(32).parts(1.0);
    assert!(looks_reconstructed(&made) && up_of(&made, Kind::Glb) == Up::Z && up_of(&made, Kind::Obj) == Up::Z);
    // the same at the size a modelling program would save it, or with no colours, is not taken for one
    assert!(!looks_reconstructed(&made::figure(32).parts(102.0)) && !looks_reconstructed(&Parts { colours: Vec::new(), ..made.clone() }));
    // a house standing on its floor: up is the axis its flat underside is low along, whichever that is in the file
    let house = made::house().parts(56.0);
    assert_eq!((guess_up(&house), up_of(&house, Kind::Glb)), (Some(Up::Z), Up::Z));
    let lying = Parts { positions: house.positions.iter().map(|p| [p[0], p[2], -p[1]]).collect(), ..house.clone() };
    assert_eq!(guess_up(&lying), Some(Up::Y));
    // a ball stands on nothing, and goes by its file's kind
    let ball = made::sphere(12).parts(10.0);
    assert_eq!((guess_up(&ball), up_of(&ball, Kind::Glb), up_of(&ball, Kind::Ply)), (None, Up::Y, Up::Z));
}
