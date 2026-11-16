//! A Wavefront OBJ file read into a mesh's parts.
//!
//! The format: <https://paulbourke.net/dataformats/obj/> (the original
//! specification, as kept by Paul Bourke). It is lines of text, each a
//! word and some numbers. Two kinds of line are read:
//!
//! - `v x y z`, a point. With three more numbers after it, `v x y z r g b`,
//!   they are its colour from 0 to 1: no part of the specification, and
//!   what MeshLab, trimesh and reconstruction models write.
//! - `f a b c ...`, a face through the points of those numbers, counted
//!   from 1, or from the last point backwards if negative. Each may be
//!   followed by `/` and the numbers of a texture place and a normal,
//!   which are passed over. A face of more than three points is cut into
//!   a fan of triangles from its first.
//!
//! Everything else is passed over: normals, groups, smoothing, and the
//! materials, which live in a second file beside this one with pictures
//! in others. Only what is in the one file opened is read, so an OBJ's
//! colours are its points' colours or none.
//!
//! The file is gone through twice: once to count its points and
//! triangles, which are held to the limits before any room is made for
//! them, and once to read them.

use brickloft_core::mesh::{Count, MeshError, Parts, MAX_TRIANGLES, MAX_VERTICES};

/// The lines of a text file with their numbers from 1, each without its
/// ending and without what follows a `#`.
fn lines(bytes: &[u8]) -> impl Iterator<Item = (u64, &[u8])> {
    bytes.split(|b| *b == b'\n').enumerate().map(|(n, line)| (n as u64 + 1, line.split(|b| *b == b'#').next().unwrap_or(&[])))
}

fn words(line: &[u8]) -> impl Iterator<Item = &[u8]> {
    line.split(|b| b.is_ascii_whitespace()).filter(|w| !w.is_empty())
}

fn number<T: std::str::FromStr>(word: &[u8]) -> Option<T> {
    std::str::from_utf8(word).ok()?.parse().ok()
}

/// The parts of the mesh in an OBJ file's bytes.
pub fn read(bytes: &[u8]) -> Result<Parts, MeshError> {
    let (mut points, mut triangles) = (0u64, 0u64);
    for (_, line) in lines(bytes) {
        let mut words = words(line);
        match words.next() {
            Some(b"v") => points += 1,
            Some(b"f") => triangles += (words.count() as u64).saturating_sub(2),
            _ => {}
        }
    }
    for (what, found, most) in [(Count::Points, points, MAX_VERTICES as u64), (Count::Triangles, triangles, MAX_TRIANGLES as u64)] {
        if found > most {
            return Err(MeshError::TooMany { what, found, most });
        }
    }
    let mut parts = Parts { positions: Vec::with_capacity(points as usize), triangles: Vec::with_capacity(triangles as usize), ..Parts::default() };
    let mut coloured = 0u64;
    let mut corners: Vec<u32> = Vec::new();
    for (at, line) in lines(bytes) {
        let mut words = words(line);
        match words.next() {
            Some(b"v") => {
                let mut given = [0f32; 6];
                let mut count = 0;
                for word in words.take(7) {
                    // (a seventh number is the weight `w` of a point with a colour after it, which nothing writes: left unread)
                    if count < 6 {
                        given[count] = number(word).ok_or(MeshError::NotANumber { part: "points", at })?;
                    }
                    count += 1;
                }
                if count < 3 {
                    return Err(MeshError::Malformed { kind: "OBJ", why: format!("the point on line {at} has {count} numbers") });
                }
                parts.positions.push([given[0], given[1], given[2]]);
                // (four numbers are a point and its weight; six, a point and its colour)
                if count >= 6 {
                    coloured += 1;
                    parts.colours.resize(parts.positions.len() - 1, [255; 3]);
                    parts.colours.push([3, 4, 5].map(|k| (given[k] * 255.0).round().clamp(0.0, 255.0) as u8));
                }
            }
            Some(b"f") => {
                corners.clear();
                for word in words {
                    let first = word.split(|b| *b == b'/').next().unwrap_or(&[]);
                    let named: i64 = number(first).ok_or(MeshError::NotANumber { part: "faces", at })?;
                    let so_far = parts.positions.len() as i64;
                    // from 1, or from the last point read so far backwards
                    let point = if named < 0 { so_far + named } else { named - 1 };
                    if point < 0 || point >= points as i64 {
                        return Err(MeshError::NoSuchPoint { index: named.unsigned_abs(), points });
                    }
                    corners.push(point as u32);
                }
                for k in 1..corners.len().saturating_sub(1) {
                    parts.triangles.push([corners[0], corners[k], corners[k + 1]]);
                }
            }
            _ => {}
        }
    }
    // colours for every point or for none: a file with them on some lines only is not one any program writes
    if coloured != 0 && coloured != points {
        return Err(MeshError::Uneven { part: "colours", found: coloured, points });
    }
    Ok(parts)
}
