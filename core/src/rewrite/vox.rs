//! Reading a MagicaVoxel .vox file: a box of cells, each empty or one of
//! 255 colours. Tilesets keep their tiles in these, and rules their
//! larger patterns.
//!
//! The format is a tree of chunks, each a four-letter name, the length of
//! its own bytes and the length of the chunks inside it:
//! https://github.com/ephtracy/voxel-model/blob/master/MagicaVoxel-file-format-vox.txt
//! Only two kinds are read: `SIZE`, a model's size, and `XYZI`, its filled
//! cells. The original interpreter finds them by looking through the file
//! for the letters S, I, Z, E (`VoxHelper.LoadVox`), which reads the same
//! models from a sound file and anything at all from another. Here the
//! chunks are walked by their lengths, every length is checked against
//! the bytes there are, and a cell outside the size given is a damaged
//! file. Where a file holds several models the last is taken, as the
//! original takes it.

use super::fault::{within, Fault, Limits, Thing};

/// A box of cells: -1 for empty, or a colour number from 1 to 255. The
/// cell at `x, y, z` is `cells[x + y * size[0] + z * size[0] * size[1]]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Vox {
    pub size: [usize; 3],
    pub cells: Vec<i32>,
}

fn word(bytes: &[u8], at: usize) -> Option<usize> {
    let four = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([four[0], four[1], four[2], four[3]]) as usize)
}

pub fn read(file: &str, bytes: &[u8], limits: &Limits) -> Result<Vox, Fault> {
    within(Thing::File, bytes.len() as u64, limits.vox, file, 0)?;
    let bad = || Fault::NotVox { file: file.to_string() };
    if bytes.get(0..4) != Some(b"VOX ") || bytes.get(8..12) != Some(b"MAIN") {
        return Err(bad());
    }
    // past the MAIN chunk's own header (name, two lengths) and its own bytes, to the chunks inside it
    let mut at = 20 + word(bytes, 12).ok_or_else(bad)?;
    let mut size: Option<[usize; 3]> = None;
    let mut found: Option<Vox> = None;
    while at < bytes.len() {
        let name = bytes.get(at..at + 4).ok_or_else(bad)?;
        let (own, inside) = (word(bytes, at + 4).ok_or_else(bad)?, word(bytes, at + 8).ok_or_else(bad)?);
        let body = at + 12;
        let end = body.checked_add(own).filter(|end| *end <= bytes.len()).ok_or_else(bad)?;
        if name == b"SIZE" {
            let s = [word(bytes, body).ok_or_else(bad)?, word(bytes, body + 4).ok_or_else(bad)?, word(bytes, body + 8).ok_or_else(bad)?];
            if s.iter().any(|side| *side == 0) || end < body + 12 {
                return Err(bad());
            }
            for side in s {
                within(Thing::Tile, side as u64, 256, file, 0)?;
            }
            size = Some(s);
        } else if name == b"XYZI" {
            let s = size.ok_or_else(bad)?;
            let count = word(bytes, body).ok_or_else(bad)?;
            if count.checked_mul(4).and_then(|n| n.checked_add(4)).is_none_or(|n| body + n > end) {
                return Err(bad());
            }
            let mut cells = vec![-1; s[0] * s[1] * s[2]];
            for n in 0..count {
                let c = &bytes[body + 4 + n * 4..body + 8 + n * 4];
                let (x, y, z) = (c[0] as usize, c[1] as usize, c[2] as usize);
                if x >= s[0] || y >= s[1] || z >= s[2] {
                    return Err(bad());
                }
                cells[x + y * s[0] + z * s[0] * s[1]] = c[3] as i32;
            }
            found = Some(Vox { size: s, cells });
        }
        // (the chunks inside this one follow it and are walked in their turn)
        let _ = inside;
        at = end;
    }
    found.ok_or_else(bad)
}

/// Write a .vox file of one model: for tests, and for giving a grid to
/// another program. `colour` is from 1 to 255, 0 for empty.
pub fn write(size: [usize; 3], colour: impl Fn(usize, usize, usize) -> u8) -> Vec<u8> {
    let mut cells = Vec::new();
    for z in 0..size[2] {
        for y in 0..size[1] {
            for x in 0..size[0] {
                let c = colour(x, y, z);
                if c != 0 {
                    cells.extend_from_slice(&[x as u8, y as u8, z as u8, c]);
                }
            }
        }
    }
    let mut out = Vec::new();
    let put = |out: &mut Vec<u8>, n: usize| out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(b"VOX ");
    put(&mut out, 150);
    out.extend_from_slice(b"MAIN");
    put(&mut out, 0);
    put(&mut out, 24 + 16 + cells.len());
    out.extend_from_slice(b"SIZE");
    put(&mut out, 12);
    put(&mut out, 0);
    for side in size {
        put(&mut out, side);
    }
    out.extend_from_slice(b"XYZI");
    put(&mut out, 4 + cells.len());
    put(&mut out, 0);
    put(&mut out, cells.len() / 4);
    out.extend_from_slice(&cells);
    out
}
