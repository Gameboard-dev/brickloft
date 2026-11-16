//! A PLY file read into a mesh's parts.
//!
//! The format: <https://paulbourke.net/dataformats/ply/> (Greg Turk's
//! description, as kept by Paul Bourke). A header of text lines names the
//! file's elements (`vertex`, `face` and whatever else), how many of each
//! there are and what numbers each is made of; the elements follow, as
//! text or as bytes in the order the header gave.
//!
//! Read: text files and little-endian binary ones; a vertex's `x`, `y`
//! and `z` and its `red`, `green` and `blue`; a face's list of vertex
//! numbers, cut into a fan of triangles from its first. Any other
//! property or element is stepped over by its size. A big-endian file is
//! refused: nothing met writes one now, and it would be a second copy of
//! every read to keep right for no file.
//!
//! The counts in the header are checked against the bytes that follow
//! before room is made for anything: a vertex takes at least the bytes of
//! its numbers, so a header that promises more of them than the file
//! could hold is refused as cut short.

use brickloft_core::mesh::{Count, MeshError, Parts, MAX_TRIANGLES, MAX_VERTICES};

const KIND: &str = "PLY";
/// The most header that is read, and the most elements and properties in
/// it: a real header is a few hundred bytes and a dozen lines.
const MAX_HEADER: usize = 64 << 10;
const MAX_NAMED: usize = 256;

fn bad(why: impl Into<String>) -> MeshError {
    MeshError::Malformed { kind: KIND, why: why.into() }
}

/// A number's type, as the header names it.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Type {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    F64,
}

impl Type {
    fn named(name: &str) -> Option<Type> {
        Some(match name {
            "char" | "int8" => Type::I8,
            "uchar" | "uint8" => Type::U8,
            "short" | "int16" => Type::I16,
            "ushort" | "uint16" => Type::U16,
            "int" | "int32" => Type::I32,
            "uint" | "uint32" => Type::U32,
            "float" | "float32" => Type::F32,
            "double" | "float64" => Type::F64,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Type::I8 | Type::U8 => 1,
            Type::I16 | Type::U16 => 2,
            Type::I32 | Type::U32 | Type::F32 => 4,
            Type::F64 => 8,
        }
    }

    /// The number in the first bytes of `b`, which are at least its size.
    fn from_bytes(self, b: &[u8]) -> f64 {
        match self {
            Type::I8 => b[0] as i8 as f64,
            Type::U8 => b[0] as f64,
            Type::I16 => i16::from_le_bytes([b[0], b[1]]) as f64,
            Type::U16 => u16::from_le_bytes([b[0], b[1]]) as f64,
            Type::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            Type::U32 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            Type::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64,
            Type::F64 => f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        }
    }

    /// A colour's channel of this type as a byte: a whole number over its
    /// type's range, a float from 0 to 1.
    fn channel(self, value: f64) -> u8 {
        let most = match self {
            Type::U8 | Type::I8 => 255.0,
            Type::U16 | Type::I16 => 65535.0,
            Type::U32 | Type::I32 => 4294967295.0,
            Type::F32 | Type::F64 => 1.0,
        };
        (value / most * 255.0).round().clamp(0.0, 255.0) as u8
    }
}

struct Property {
    name: String,
    of: Type,
    /// for a list: the type its length is written in
    count: Option<Type>,
}

struct Element {
    name: String,
    count: u64,
    properties: Vec<Property>,
}

/// Where the numbers come from: the rest of the file as bytes, or as words.
enum Source<'a> {
    Bytes(&'a [u8]),
    Words(Box<dyn Iterator<Item = &'a [u8]> + 'a>),
}

impl Source<'_> {
    /// The next number, of type `of`. `part` and `at` are for saying where the file ended or what was not a number.
    fn next(&mut self, of: Type, part: &'static str, at: u64) -> Result<f64, MeshError> {
        match self {
            Source::Bytes(rest) => {
                if rest.len() < of.size() {
                    return Err(MeshError::CutShort { part, needs: of.size() as u64, has: rest.len() as u64 });
                }
                let (now, after) = rest.split_at(of.size());
                *rest = after;
                Ok(of.from_bytes(now))
            }
            Source::Words(words) => {
                let word = words.next().ok_or(MeshError::CutShort { part, needs: 1, has: 0 })?;
                std::str::from_utf8(word).ok().and_then(|w| w.parse::<f64>().ok()).ok_or(MeshError::NotANumber { part, at })
            }
        }
    }

    /// How many bytes are left, for judging whether a count could be true.
    fn left(&self, all: usize) -> u64 {
        match self {
            Source::Bytes(rest) => rest.len() as u64,
            Source::Words(_) => all as u64,
        }
    }
}

/// The parts of the mesh in a PLY file's bytes.
pub fn read(bytes: &[u8]) -> Result<Parts, MeshError> {
    if !bytes.starts_with(b"ply") {
        return Err(bad("it does not begin with the word ply"));
    }
    // the header: lines of text, up to and with the one that says `end_header`
    let head = &bytes[..bytes.len().min(MAX_HEADER)];
    let ends = head.windows(10).position(|w| w == b"end_header").ok_or_else(|| bad(format!("its header does not end within {MAX_HEADER} bytes")))?;
    let body = ends + 10 + head[ends + 10..].iter().position(|b| *b == b'\n').map_or(head.len() - ends - 10, |n| n + 1);
    let header = std::str::from_utf8(&bytes[..ends]).map_err(|_| bad("its header is not text"))?;
    let mut binary = None;
    let mut elements: Vec<Element> = Vec::new();
    for line in header.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["format", "ascii", ..] => binary = Some(false),
            ["format", "binary_little_endian", ..] => binary = Some(true),
            ["format", "binary_big_endian", ..] => return Err(MeshError::Unsupported { what: "big-endian numbers" }),
            ["format", ..] => return Err(bad("its format is not one of PLY's three")),
            ["element", name, count] => {
                if elements.len() >= MAX_NAMED {
                    return Err(MeshError::TooMany { what: Count::Parts, found: elements.len() as u64 + 1, most: MAX_NAMED as u64 });
                }
                elements.push(Element { name: name.to_string(), count: count.parse().map_err(|_| bad(format!("how many `{name}` there are is not a whole number")))?, properties: Vec::new() });
            }
            ["property", rest @ ..] => {
                let element = elements.last_mut().ok_or_else(|| bad("a property comes before any element"))?;
                if element.properties.len() >= MAX_NAMED {
                    return Err(MeshError::TooMany { what: Count::Parts, found: element.properties.len() as u64 + 1, most: MAX_NAMED as u64 });
                }
                let unknown = || bad(format!("a property of `{}` is of a type that is not one of PLY's", element.name));
                let property = match rest {
                    ["list", count, of, name] => Property { name: name.to_string(), of: Type::named(of).ok_or_else(unknown)?, count: Some(Type::named(count).filter(|t| !matches!(t, Type::F32 | Type::F64)).ok_or_else(unknown)?) },
                    [of, name] => Property { name: name.to_string(), of: Type::named(of).ok_or_else(unknown)?, count: None },
                    _ => return Err(bad("a property is not a type and a name")),
                };
                element.properties.push(property);
            }
            _ => {}
        }
    }
    let binary = binary.ok_or_else(|| bad("its header does not say whether it is text or bytes"))?;
    let rest = bytes.get(body..).unwrap_or(&[]);
    let mut source = if binary { Source::Bytes(rest) } else { Source::Words(Box::new(rest.split(|b| b.is_ascii_whitespace()).filter(|w| !w.is_empty()))) };
    let mut parts = Parts::default();
    let mut coloured = false;
    for element in &elements {
        // The fewest bytes one of these can take: each number's own size, or in a text file a digit and a space.
        let least: u64 = element.properties.iter().map(|p| if binary { p.count.map_or(p.of.size(), Type::size) as u64 } else { 2 }).sum::<u64>().max(1);
        let left = source.left(rest.len());
        let part: &'static str = match element.name.as_str() {
            "vertex" => "points",
            "face" => "faces",
            _ => "other elements",
        };
        if element.count.checked_mul(least).is_none_or(|needs| needs > left) {
            return Err(MeshError::CutShort { part, needs: element.count.saturating_mul(least), has: left });
        }
        match element.name.as_str() {
            "vertex" => {
                if element.count > MAX_VERTICES as u64 {
                    return Err(MeshError::TooMany { what: Count::Points, found: element.count, most: MAX_VERTICES as u64 });
                }
                let place = |name: &str| element.properties.iter().position(|p| p.name == name && p.count.is_none());
                let (Some(x), Some(y), Some(z)) = (place("x"), place("y"), place("z")) else { return Err(bad("its vertices have no x, y and z")) };
                let rgb = [place("red"), place("green"), place("blue")];
                coloured = rgb.iter().all(Option::is_some);
                parts.positions.reserve(element.count as usize);
                let mut given = vec![0f64; element.properties.len()];
                for at in 0..element.count {
                    for (n, property) in element.properties.iter().enumerate() {
                        match property.count {
                            None => given[n] = source.next(property.of, part, at)?,
                            // (a list on a vertex is nothing read here: stepped over)
                            Some(count) => {
                                for _ in 0..source.next(count, part, at)?.max(0.0) as u64 {
                                    source.next(property.of, part, at)?;
                                }
                            }
                        }
                    }
                    parts.positions.push([given[x] as f32, given[y] as f32, given[z] as f32]);
                    if let [Some(r), Some(g), Some(b)] = rgb {
                        parts.colours.push([r, g, b].map(|k| element.properties[k].of.channel(given[k])));
                    }
                }
            }
            "face" => {
                let mut corners: Vec<u32> = Vec::new();
                for at in 0..element.count {
                    for property in &element.properties {
                        let Some(count) = property.count else {
                            source.next(property.of, part, at)?;
                            continue;
                        };
                        let length = source.next(count, part, at)?.max(0.0) as u64;
                        let indices = matches!(property.name.as_str(), "vertex_indices" | "vertex_index");
                        corners.clear();
                        for _ in 0..length {
                            let index = source.next(property.of, part, at)?;
                            if !indices {
                                continue;
                            }
                            if !(0.0..parts.positions.len() as f64).contains(&index) {
                                return Err(MeshError::NoSuchPoint { index: index.max(0.0) as u64, points: parts.positions.len() as u64 });
                            }
                            corners.push(index as u32);
                        }
                        if parts.triangles.len() + corners.len().saturating_sub(2) > MAX_TRIANGLES {
                            return Err(MeshError::TooMany { what: Count::Triangles, found: (parts.triangles.len() + corners.len() - 2) as u64, most: MAX_TRIANGLES as u64 });
                        }
                        for k in 1..corners.len().saturating_sub(1) {
                            parts.triangles.push([corners[0], corners[k], corners[k + 1]]);
                        }
                    }
                }
            }
            // anything else is stepped over, number by number
            _ => {
                for at in 0..element.count {
                    for property in &element.properties {
                        let length = match property.count {
                            Some(count) => source.next(count, part, at)?.max(0.0) as u64,
                            None => 1,
                        };
                        for _ in 0..length {
                            source.next(property.of, part, at)?;
                        }
                    }
                }
            }
        }
    }
    if !coloured {
        parts.colours.clear();
    }
    Ok(parts)
}
