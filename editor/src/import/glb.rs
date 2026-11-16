//! Binary glTF (a `.glb` file) read into a mesh's parts.
//!
//! The format: <https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html>.
//! A file is a 12-byte header and then chunks, the first of them JSON that
//! describes the scene and the second the bytes it refers to. What is read
//! of it is what bricks can use: each mesh's triangles with their points,
//! colours and places on a picture, set where the scene's nodes put them;
//! each material's base colour; and a base-colour picture if it is a PNG
//! kept inside the file.
//!
//! Written here and not taken from the `gltf` crate. That crate reads the
//! whole of the format, and with it would come its JSON types, an image
//! decoder for three formats and base64, to read the tenth of the format
//! used here; and its reading of accessors trusts counts that this reader
//! has to check, since the file may be anyone's. The JSON itself is read
//! by `serde_json`, which the editor already has.
//!
//! Every count and offset is checked against the bytes that are there
//! before anything is allocated for it, in 64-bit arithmetic that cannot
//! wrap: a file that says it has four billion points in a hundred bytes
//! is refused for being cut short, having cost nothing.

use super::srgb;
use brickloft_core::mesh::{Count, Image, MeshError, Parts, MAX_TRIANGLES, MAX_VERTICES, NO_IMAGE};
use serde_json::Value;

const KIND: &str = "glTF";
/// The most JSON read. A scene's JSON is a few kilobytes, or a few
/// megabytes for one of many thousand nodes; read into `serde_json`'s
/// values it takes several times its own size.
const MAX_JSON: u64 = 16 << 20;
/// The most nodes, meshes, accessors and other listed things, in the file
/// and as met while the scene is walked (a mesh used by many nodes is met
/// once for each).
const MAX_PARTS: u64 = 100_000;
/// How deep nodes may be nested. A real scene is a handful deep; a node
/// that is its own ancestor is found by running out of this.
const MAX_DEPTH: u64 = 64;
/// The most pixels in one picture, and in all of them together.
#[cfg(not(target_arch = "wasm32"))]
const MAX_PIXELS: u64 = 4096 * 4096;
#[cfg(not(target_arch = "wasm32"))]
const MAX_ALL_PIXELS: u64 = 4 * MAX_PIXELS;

fn bad(why: impl Into<String>) -> MeshError {
    MeshError::Malformed { kind: KIND, why: why.into() }
}

/// A list in the JSON by its name, or an empty one.
fn list<'a>(doc: &'a Value, name: &str) -> &'a [Value] {
    doc.get(name).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn whole(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

/// A transform as glTF writes one: sixteen numbers, a column at a time.
type Matrix = [f64; 16];

const SAME: Matrix = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// `outer` after `inner`: the transform of a child within its parent's.
fn times(outer: &Matrix, inner: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| outer[k * 4 + row] * inner[column * 4 + k]).sum();
        }
    }
    out
}

fn numbers<const N: usize>(value: Option<&Value>) -> Option<[f64; N]> {
    let given: Vec<f64> = value?.as_array()?.iter().filter_map(Value::as_f64).collect();
    <[f64; N]>::try_from(given).ok()
}

/// A node's own transform: its matrix, or its move, turn and scale put
/// together (<https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#transformations>).
fn transform_of(node: &Value, index: usize) -> Result<Matrix, MeshError> {
    let matrix = match numbers::<16>(node.get("matrix")) {
        Some(matrix) => matrix,
        None => {
            let [x, y, z] = numbers::<3>(node.get("translation")).unwrap_or([0.0; 3]);
            let [qx, qy, qz, qw] = numbers::<4>(node.get("rotation")).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let [sx, sy, sz] = numbers::<3>(node.get("scale")).unwrap_or([1.0; 3]);
            // a quaternion as a matrix: https://en.wikipedia.org/wiki/Quaternions_and_spatial_rotation#Quaternion-derived_rotation_matrix
            [
                (1.0 - 2.0 * (qy * qy + qz * qz)) * sx, 2.0 * (qx * qy + qz * qw) * sx, 2.0 * (qx * qz - qy * qw) * sx, 0.0,
                2.0 * (qx * qy - qz * qw) * sy, (1.0 - 2.0 * (qx * qx + qz * qz)) * sy, 2.0 * (qy * qz + qx * qw) * sy, 0.0,
                2.0 * (qx * qz + qy * qw) * sz, 2.0 * (qy * qz - qx * qw) * sz, (1.0 - 2.0 * (qx * qx + qy * qy)) * sz, 0.0,
                x, y, z, 1.0,
            ]
        }
    };
    if matrix.iter().any(|v| !v.is_finite()) {
        return Err(MeshError::NotANumber { part: "nodes", at: index as u64 });
    }
    Ok(matrix)
}

/// The numbers one accessor describes, where they lie in the file's bytes.
struct Numbers<'a> {
    data: &'a [u8],
    stride: usize,
    count: usize,
    /// glTF's number for the type of each: 5120 to 5126
    kind: u64,
    normalized: bool,
}

impl Numbers<'_> {
    fn size(kind: u64) -> Option<usize> {
        match kind {
            5120 | 5121 => Some(1),
            5122 | 5123 => Some(2),
            5125 | 5126 => Some(4),
            _ => None,
        }
    }

    /// Number `part` of item `item`, as it is written. (Within the bytes:
    /// `accessor` checked that the last item is.)
    fn raw(&self, item: usize, part: usize) -> (f64, f64) {
        let size = Numbers::size(self.kind).unwrap_or(1);
        let at = item * self.stride + part * size;
        let b = &self.data[at..at + size];
        // the number, and the largest its type holds (for the types that stand for 0 to 1)
        match self.kind {
            5120 => (b[0] as i8 as f64, 127.0),
            5121 => (b[0] as f64, 255.0),
            5122 => (i16::from_le_bytes([b[0], b[1]]) as f64, 32767.0),
            5123 => (u16::from_le_bytes([b[0], b[1]]) as f64, 65535.0),
            5125 => (u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64, 4294967295.0),
            _ => (f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64, 1.0),
        }
    }

    /// As a measure: a float as it is, a whole number as it is or, where
    /// the accessor says so, as a share of the largest its type holds.
    fn float(&self, item: usize, part: usize) -> f32 {
        let (value, most) = self.raw(item, part);
        (if self.normalized && self.kind != 5126 { (value / most).max(-1.0) } else { value }) as f32
    }

    /// As a share of the whole, 0 to 255: a colour's channel. A float is
    /// taken to run from 0 to 1 and a whole number over its type's range.
    fn byte(&self, item: usize, part: usize) -> u8 {
        let (value, most) = self.raw(item, part);
        (value / most * 255.0).round().clamp(0.0, 255.0) as u8
    }

    fn index(&self, item: usize) -> u32 {
        self.raw(item, 0).0 as u32
    }
}

struct Reader<'a> {
    doc: &'a Value,
    bin: Option<&'a [u8]>,
    parts: Parts,
    /// whether any point has been given a colour that is not white, and any triangle a picture
    coloured: bool,
    /// each of the file's images, once it has been asked for: its place in `parts.images`, or none if it is not read
    pictures: Vec<Option<Option<u16>>>,
    pixels: u64,
    met: u64,
}

impl<'a> Reader<'a> {
    /// The numbers of accessor `index`, which are to be `width` to an
    /// item; `part` is what they are, for saying what was wrong.
    fn accessor(&self, index: u64, width: usize, part: &'static str) -> Result<Numbers<'a>, MeshError> {
        let accessor = list(self.doc, "accessors").get(index as usize).ok_or_else(|| bad(format!("its {part} are accessor {index}, which is not there")))?;
        if accessor.get("sparse").is_some() {
            return Err(MeshError::Unsupported { what: "sparse accessors" });
        }
        let named = match accessor.get("type").and_then(Value::as_str) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            _ => 0,
        };
        // (a colour may come with or without its fourth number, which is how see-through it is and is not read)
        if named != width && !(part == "colours" && named == 4) {
            return Err(bad(format!("its {part} are {named} numbers each, where {width} are needed")));
        }
        let kind = whole(accessor.get("componentType")).unwrap_or(0);
        let size = Numbers::size(kind).ok_or_else(|| bad(format!("its {part} are of a number type that is not one of glTF's")))? as u64;
        let count = whole(accessor.get("count")).ok_or_else(|| bad(format!("its {part} do not say how many they are")))?;
        let view = whole(accessor.get("bufferView")).and_then(|v| list(self.doc, "bufferViews").get(v as usize)).ok_or_else(|| bad(format!("its {part} have no bytes of their own in the file")))?;
        let buffer = whole(view.get("buffer")).unwrap_or(0);
        // only the bytes inside the file: the first buffer, which says nothing of where else it might be kept
        if buffer != 0 || list(self.doc, "buffers").first().is_some_and(|b| b.get("uri").is_some()) {
            return Err(MeshError::Unsupported { what: "a buffer kept outside the file" });
        }
        let bin = self.bin.unwrap_or(&[]);
        let (from, length) = (whole(view.get("byteOffset")).unwrap_or(0), whole(view.get("byteLength")).unwrap_or(0));
        let end = from.checked_add(length).ok_or_else(|| bad("a size in it is too large to add up"))?;
        if end > bin.len() as u64 {
            return Err(MeshError::CutShort { part, needs: end, has: bin.len() as u64 });
        }
        let item = size * named as u64;
        let stride = whole(view.get("byteStride")).unwrap_or(item);
        if stride < item || stride > 4096 {
            return Err(bad(format!("its {part} are {item} bytes each and are said to be {stride} apart")));
        }
        let within = whole(accessor.get("byteOffset")).unwrap_or(0);
        // where the last item ends, within the view
        let needs = count.checked_sub(1).map_or(Some(0), |last| last.checked_mul(stride)?.checked_add(item)?.checked_add(within)).ok_or_else(|| bad("a size in it is too large to add up"))?;
        if needs > length {
            return Err(MeshError::CutShort { part, needs, has: length });
        }
        let data = if count == 0 { &[][..] } else { &bin[(from + within) as usize..(from + within + needs - within) as usize] };
        Ok(Numbers { data, stride: stride as usize, count: count as usize, kind, normalized: accessor.get("normalized").and_then(Value::as_bool).unwrap_or(false) })
    }

    /// The picture a material's base colour is painted with, if it has one
    /// that is read: a PNG kept inside the file, laid out by the points'
    /// first set of places.
    fn picture_of(&mut self, material: &Value) -> Result<Option<u16>, MeshError> {
        let Some(texture) = material.pointer("/pbrMetallicRoughness/baseColorTexture") else { return Ok(None) };
        if whole(texture.get("texCoord")).unwrap_or(0) != 0 {
            return Ok(None);
        }
        let Some(image) = whole(texture.get("index")).and_then(|t| list(self.doc, "textures").get(t as usize)).and_then(|t| whole(t.get("source"))) else { return Ok(None) };
        let image = image as usize;
        if self.pictures.len() <= image {
            self.pictures.resize(list(self.doc, "images").len().max(image + 1).min(MAX_PARTS as usize), None);
        }
        if let Some(known) = self.pictures.get(image).copied().flatten() {
            return Ok(known);
        }
        let read = self.picture(image)?;
        let place = match read {
            Some(picture) => {
                self.parts.images.push(picture);
                Some((self.parts.images.len() - 1) as u16)
            }
            None => None,
        };
        if let Some(slot) = self.pictures.get_mut(image) {
            *slot = Some(place);
        }
        Ok(place)
    }

    /// Image `index` of the file as a picture, or `None` if it is of a
    /// kind that is not read (JPEG, WebP, or kept outside the file).
    fn picture(&mut self, index: usize) -> Result<Option<Image>, MeshError> {
        let Some(view) = list(self.doc, "images").get(index).and_then(|i| whole(i.get("bufferView"))).and_then(|v| list(self.doc, "bufferViews").get(v as usize)) else { return Ok(None) };
        let bin = self.bin.unwrap_or(&[]);
        let (from, length) = (whole(view.get("byteOffset")).unwrap_or(0), whole(view.get("byteLength")).unwrap_or(0));
        let end = from.checked_add(length).ok_or_else(|| bad("a size in it is too large to add up"))?;
        if whole(view.get("buffer")).unwrap_or(0) != 0 {
            return Ok(None);
        }
        if end > bin.len() as u64 {
            return Err(MeshError::CutShort { part: "picture", needs: end, has: bin.len() as u64 });
        }
        let bytes = &bin[from as usize..end as usize];
        // (by how the bytes begin and not by what the file calls them: https://www.w3.org/TR/png/#5PNG-file-signature)
        if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Ok(None);
        }
        png_picture(bytes, &mut self.pixels)
    }

    /// The triangles of mesh `index`, set where `place` puts them, added
    /// to what has been read.
    fn mesh(&mut self, index: u64, place: &Matrix) -> Result<(), MeshError> {
        let mesh = list(self.doc, "meshes").get(index as usize).ok_or_else(|| bad(format!("a node is mesh {index}, which is not there")))?;
        for primitive in list(mesh, "primitives") {
            self.met += 1;
            if self.met > MAX_PARTS {
                return Err(MeshError::TooMany { what: Count::Parts, found: self.met, most: MAX_PARTS });
            }
            if primitive.pointer("/extensions/KHR_draco_mesh_compression").is_some() {
                return Err(MeshError::Unsupported { what: "Draco compression" });
            }
            // (4 is triangles and what is meant where nothing is said, 5 a strip and 6 a fan; the rest are points and lines, which are no solid)
            let mode = whole(primitive.get("mode")).unwrap_or(4);
            let Some(positions) = whole(primitive.pointer("/attributes/POSITION")).filter(|_| (4..=6).contains(&mode)) else { continue };
            let positions = self.accessor(positions, 3, "points")?;
            let first = self.parts.positions.len();
            if first + positions.count > MAX_VERTICES {
                return Err(MeshError::TooMany { what: Count::Points, found: (first + positions.count) as u64, most: MAX_VERTICES as u64 });
            }
            let indices = whole(primitive.get("indices")).map(|i| self.accessor(i, 1, "triangles")).transpose()?;
            let corners = indices.as_ref().map_or(positions.count, |i| i.count);
            let triangles = if mode == 4 { corners / 3 } else { corners.saturating_sub(2) };
            if self.parts.triangles.len() + triangles > MAX_TRIANGLES {
                return Err(MeshError::TooMany { what: Count::Triangles, found: (self.parts.triangles.len() + triangles) as u64, most: MAX_TRIANGLES as u64 });
            }
            let colours = whole(primitive.pointer("/attributes/COLOR_0")).map(|c| self.accessor(c, 3, "colours")).transpose()?;
            let places = whole(primitive.pointer("/attributes/TEXCOORD_0")).map(|c| self.accessor(c, 2, "places on its picture")).transpose()?;
            for (part, found) in [("colours", colours.as_ref().map(|c| c.count)), ("places on its picture", places.as_ref().map(|c| c.count))] {
                if found.is_some_and(|found| found != positions.count) {
                    return Err(MeshError::Uneven { part, found: found.unwrap_or(0) as u64, points: positions.count as u64 });
                }
            }
            let material = whole(primitive.get("material")).and_then(|m| list(self.doc, "materials").get(m as usize));
            // A material's base colour is given as light, and is brought to the screen's bytes (see `srgb`).
            let tint = material.and_then(|m| numbers::<4>(m.pointer("/pbrMetallicRoughness/baseColorFactor"))).map_or([255; 3], |c| [0, 1, 2].map(|k| srgb::byte(c[k] as f32)));
            let picture = match (material, &places) {
                (Some(material), Some(_)) => self.picture_of(material)?,
                _ => None,
            };
            self.coloured |= colours.is_some() || tint != [255; 3];
            for i in 0..positions.count {
                let p = [0, 1, 2].map(|k| positions.float(i, k) as f64);
                self.parts.positions.push([0, 1, 2].map(|row| (place[row] * p[0] + place[4 + row] * p[1] + place[8 + row] * p[2] + place[12 + row]) as f32));
                // A point's own colour is taken as written. The format says it too is light; the programs whose
                // files are opened here (a reconstruction model's exporter, trimesh) write the screen's bytes
                // there, as OBJ and PLY files do, and brought through the curve those come out pale.
                let own = colours.as_ref().map_or([255; 3], |c| [0, 1, 2].map(|k| c.byte(i, k)));
                self.parts.colours.push([0, 1, 2].map(|k| ((own[k] as u32 * tint[k] as u32 + 127) / 255) as u8));
                self.parts.uvs.push(places.as_ref().map_or([0.0; 2], |c| [c.float(i, 0), c.float(i, 1)]));
            }
            let corner = |n: usize| -> Result<u32, MeshError> {
                let point = indices.as_ref().map_or(n as u32, |i| i.index(n));
                if point as usize >= positions.count {
                    return Err(MeshError::NoSuchPoint { index: point as u64, points: positions.count as u64 });
                }
                Ok(first as u32 + point)
            };
            for t in 0..triangles {
                let of = match mode {
                    4 => [3 * t, 3 * t + 1, 3 * t + 2],
                    // (every other triangle of a strip is written the other way round)
                    5 if t % 2 == 1 => [t + 1, t, t + 2],
                    5 => [t, t + 1, t + 2],
                    _ => [0, t + 1, t + 2],
                };
                self.parts.triangles.push([corner(of[0])?, corner(of[1])?, corner(of[2])?]);
                self.parts.image_of.push(picture.unwrap_or(NO_IMAGE));
            }
        }
        Ok(())
    }
}

/// A PNG file's bytes as a picture. `so_far` is the pixels of the pictures
/// read before it, which this one's are added to.
#[cfg(not(target_arch = "wasm32"))]
fn png_picture(bytes: &[u8], so_far: &mut u64) -> Result<Option<Image>, MeshError> {
    let png_bad = |e: png::DecodingError| MeshError::Malformed { kind: "PNG picture", why: e.to_string() };
    let mut decoder = png::Decoder::new(bytes);
    // (sixteen bits a channel brought to eight, and a palette or a grey of few bits spread out: https://docs.rs/png/0.17/png/struct.Transformations.html)
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(png_bad)?;
    let (width, height) = (reader.info().width, reader.info().height);
    // by the size its header gives, before the room for its pixels is asked for
    let pixels = width as u64 * height as u64;
    if pixels > MAX_PIXELS || *so_far + pixels > MAX_ALL_PIXELS {
        return Err(MeshError::TooMany { what: Count::Pixels, found: *so_far + pixels, most: if pixels > MAX_PIXELS { MAX_PIXELS } else { MAX_ALL_PIXELS } });
    }
    *so_far += pixels;
    let mut raw = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut raw).map_err(png_bad)?;
    let each = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Ok(None),
    };
    let raw = raw.get(..pixels as usize * each).ok_or_else(|| MeshError::Malformed { kind: "PNG picture", why: "it has fewer pixels than its size says".into() })?;
    let rgb = raw.chunks_exact(each).flat_map(|p| if each < 3 { [p[0]; 3] } else { [p[0], p[1], p[2]] }).collect();
    Ok(Some(Image { width, height, rgb }))
}

/// In the browser's build there is no PNG reader (it is the desktop's, for
/// writing screenshots), and a page opens no files in any case: a picture
/// is left out as a JPEG is.
#[cfg(target_arch = "wasm32")]
fn png_picture(_: &[u8], _: &mut u64) -> Result<Option<Image>, MeshError> {
    Ok(None)
}

/// The parts of the mesh in a `.glb` file's bytes.
pub fn read(bytes: &[u8]) -> Result<Parts, MeshError> {
    let word = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (Some(magic), Some(version), Some(length)) = (word(0), word(4), word(8)) else {
        return Err(MeshError::CutShort { part: "header", needs: 12, has: bytes.len() as u64 });
    };
    // the letters "glTF": https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#binary-header
    if magic != 0x4654_6C67 {
        return Err(bad("it does not begin with the letters glTF"));
    }
    if version != 2 {
        return Err(MeshError::Unsupported { what: "a version of glTF other than 2" });
    }
    if length as usize > bytes.len() {
        return Err(MeshError::CutShort { part: "chunks", needs: length as u64, has: bytes.len() as u64 });
    }
    let (mut json, mut bin) = (None, None);
    let mut at = 12usize;
    while at + 8 <= length as usize {
        let (Some(size), Some(kind)) = (word(at), word(at + 4)) else { break };
        let end = at as u64 + 8 + size as u64;
        if end > length as u64 {
            return Err(MeshError::CutShort { part: "chunks", needs: end, has: length as u64 });
        }
        let chunk = &bytes[at + 8..end as usize];
        match kind {
            // "JSON" and "BIN", the first of each
            0x4E4F_534A if json.is_none() => json = Some(chunk),
            0x004E_4942 if bin.is_none() => bin = Some(chunk),
            _ => {}
        }
        at = end as usize;
    }
    let json = json.ok_or_else(|| bad("it has no JSON chunk"))?;
    if json.len() as u64 > MAX_JSON {
        return Err(MeshError::TooMany { what: Count::Bytes, found: json.len() as u64, most: MAX_JSON });
    }
    // (serde_json stops at 128 levels of nesting of its own accord: https://docs.rs/serde_json/latest/serde_json/struct.Deserializer.html#method.disable_recursion_limit)
    let doc: Value = serde_json::from_slice(json).map_err(|e| bad(format!("its JSON cannot be read ({e})")))?;
    let listed: u64 = ["nodes", "meshes", "accessors", "bufferViews", "materials", "textures", "images", "scenes"].iter().map(|name| list(&doc, name).len() as u64).sum::<u64>() + list(&doc, "meshes").iter().map(|m| list(m, "primitives").len() as u64).sum::<u64>();
    if listed > MAX_PARTS {
        return Err(MeshError::TooMany { what: Count::Parts, found: listed, most: MAX_PARTS });
    }
    // What the file says cannot be left out. The two ways of packing a mesh's numbers are not unpacked here. Whatever
    // only changes how a surface looks (a material's or a texture's extension, a light) can be left out all the same:
    // the mesh is the same mesh, in plainer colours. Whole numbers standing for measures are read as any accessor is.
    for required in list(&doc, "extensionsRequired").iter().filter_map(Value::as_str) {
        match required {
            "KHR_draco_mesh_compression" => return Err(MeshError::Unsupported { what: "Draco compression" }),
            "EXT_meshopt_compression" | "KHR_meshopt_compression" => return Err(MeshError::Unsupported { what: "meshopt compression" }),
            "KHR_mesh_quantization" => {}
            other if other.starts_with("KHR_materials_") || other.starts_with("KHR_texture_") || other.starts_with("EXT_texture_") || other == "KHR_lights_punctual" => {}
            _ => return Err(MeshError::Unsupported { what: "an extension it says it cannot be read without" }),
        }
    }
    let mut reader = Reader { doc: &doc, bin, parts: Parts::default(), coloured: false, pictures: Vec::new(), pixels: 0, met: 0 };
    let nodes = list(&doc, "nodes");
    // where to begin: the nodes of the scene shown first, or, in a file with no scene, every node that is no other's child
    let scene = whole(doc.get("scene")).unwrap_or(0) as usize;
    let mut walk: Vec<(u64, Matrix, u64)> = match list(&doc, "scenes").get(scene) {
        Some(scene) => list(scene, "nodes").iter().filter_map(|n| n.as_u64()).map(|n| (n, SAME, 0)).collect(),
        None => {
            let mut child = vec![false; nodes.len()];
            for inner in nodes.iter().flat_map(|n| list(n, "children")).filter_map(|c| c.as_u64()) {
                if let Some(slot) = child.get_mut(inner as usize) {
                    *slot = true;
                }
            }
            (0..nodes.len()).filter(|n| !child[*n]).map(|n| (n as u64, SAME, 0)).collect()
        }
    };
    let mut placed = false;
    while let Some((index, within, depth)) = walk.pop() {
        reader.met += 1;
        if reader.met > MAX_PARTS {
            return Err(MeshError::TooMany { what: Count::Parts, found: reader.met, most: MAX_PARTS });
        }
        if depth > MAX_DEPTH {
            return Err(MeshError::TooDeep { most: MAX_DEPTH });
        }
        let node = nodes.get(index as usize).ok_or_else(|| bad(format!("node {index} is named and is not there")))?;
        let place = times(&within, &transform_of(node, index as usize)?);
        if let Some(mesh) = whole(node.get("mesh")) {
            reader.mesh(mesh, &place)?;
            placed = true;
        }
        walk.extend(list(node, "children").iter().filter_map(|c| c.as_u64()).map(|c| (c, place, depth + 1)));
    }
    // (a file of meshes that no node places: each as it is)
    if !placed {
        for index in 0..list(&doc, "meshes").len() {
            reader.mesh(index as u64, &SAME)?;
        }
    }
    let mut parts = reader.parts;
    if !reader.coloured {
        parts.colours.clear();
    }
    if parts.images.is_empty() {
        parts.uvs.clear();
        parts.image_of.clear();
    }
    Ok(parts)
}
