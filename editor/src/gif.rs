//! Recording the window as an animated GIF.
//!
//! A recording is the window as drawn, taken at a fixed number of pictures a
//! second by the clock, made smaller, reduced to 256 colours and written to
//! a file in the pictures folder (see `places`) while it is being made.
//!
//! ## Who does what
//!
//! The frame loop only reads the window's pixels back, which it alone can
//! do, and hands them over ([`Controls::give`]). A second thread does the
//! rest ([`work`]): making the picture smaller, finding what changed,
//! choosing colours, compressing and writing. The two are joined by a queue
//! of at most [`QUEUE`] pictures. When the second thread has not kept up the
//! frame loop skips a picture, and the one before it is shown for longer: a
//! recording falls short of its rate before the editor waits for it or the
//! queue grows.
//!
//! ## What it costs
//!
//! Measured on ten seconds of a camera flying round two forms, 20 pictures
//! a second, in a window of 1920 by 1009 pixels, in a release build:
//!
//! | | full size | half | a third |
//! |---|---|---|---|
//! | frame loop, for each picture taken | 8 ms | 8 ms | 7 ms |
//! | second thread, for each picture | 13 ms | 8 ms | 5 ms |
//! | memory held while recording | 26 MB | 15 MB | 19 MB |
//! | file | 16.9 MB | 7.1 MB | 4.2 MB |
//!
//! The frame loop's 8 ms is one call, three-d's `read_color`, which is the
//! call a screenshot makes: it waits for the graphics card to finish the
//! frame, then copies the window's 7.7 MB out, again into the type asked
//! for, and turns the rows the right way up. It is the same at every size
//! because the whole window is read whatever is kept of it. The wait is
//! most of it and is not the editor's to shorten: with a second copy of
//! the editor open on the same graphics card, the same program took a
//! steady 24 ms for each read. Reading back
//! fewer pixels would take the picture made smaller on the graphics card
//! (which rounds differently from one card to the next) or OpenGL's
//! asynchronous read into a pixel buffer, which three-d does not offer
//! and which is `unsafe` to call beneath it.
//! https://docs.rs/three-d/latest/three_d/core/render_target/struct.RenderTarget.html#method.read_color
//!
//! In a build without `--release` the same recording took the frame loop
//! 74 ms a picture and the second thread 92 ms at half size, and so kept
//! 11 pictures a second of the 20: the loops below, and three-d's generic
//! `read_color`, are then compiled unoptimised with the rest of the editor
//! (see `[profile.dev]` in the workspace's Cargo.toml).
//!
//! ## The file
//!
//! GIF89a, written here: https://www.w3.org/Graphics/GIF/spec-gif89a.txt
//! The `gif` crate (https://docs.rs/gif) would write the container and do
//! the compression, about 150 of the lines below, and leave the rest to be
//! written anyway: it reduces no colours, finds no changed rectangle and
//! keeps no time. Its compression comes from a second crate, `weezl`, so it
//! is two crates to build for a fifth of the work, and the format has not
//! changed since 1989, so there is nothing a maintained crate would keep up
//! with for us. What a crate would bring that is hard to get right, the
//! compression, is held here to the format's published sample and to a
//! reader written apart from it (see the tests).
//!
//! What keeps a recording small is that most of the window is the same from
//! one picture to the next. Only the rectangle that holds every changed
//! pixel is written, and within it a pixel that did not change is written
//! as "leave what is there" (the format's transparent colour, with each
//! picture left in place under the next). A long run of those compresses to
//! almost nothing, and a picture in which nothing changed is not written at
//! all: the one before it is shown for longer. That is also what a
//! recording does while nothing is happening.
//!
//! ## Colours
//!
//! A GIF picture has at most 256 colours. They are fitted to the recording
//! ([`fit`]) and not fixed in advance: see [`Palette`] for the comparison.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How much smaller than the window a recording may be made: what the panel
/// calls each choice, and how many of the window's pixels across and down
/// become one. Whole numbers only: each pixel is then the average of a
/// square of the window's, the same on every machine. A resize by any other
/// amount weighs pixels by fractions, which smears the thin dark edges of
/// the bricks and rounds differently from one processor to the next.
pub const SIZES: [(&str, usize); 3] = [("Full size", 1), ("Half size", 2), ("A third", 3)];
/// The size a recording has unless another is chosen: a quarter of the
/// pixels of the window. Ten seconds of a camera flying round two forms, in
/// a window of 1920 by 1009, came to 16.5 MB at full size, 6.9 MB at half
/// and 4.2 MB at a third; text in the panel is still read at half size.
pub const USUAL_SIZE: usize = 1;

/// How often a picture may be taken: what the panel calls each choice, and
/// the time between two pictures in hundredths of a second, which is the
/// unit a GIF keeps its times in. Only rates that are a whole number of
/// hundredths apart are offered. Fifteen a second is not one (6.67
/// hundredths), and would play as pictures 6 and 7 hundredths apart by
/// turns. Nothing under 2 hundredths is offered either: browsers play such
/// a picture for 10.
pub const RATES: [(&str, u32); 3] = [("10 a second", 10), ("20 a second", 5), ("25 a second", 4)];
/// The rate a recording has unless another is chosen. Twenty a second is
/// every third frame of a display that shows 60, so the pictures are evenly
/// spaced; ten a second shows a turning camera in jerks.
pub const USUAL_RATE: usize = 1;

/// The longest a recording may be, in milliseconds, and the largest its
/// file may be, in bytes. At either it stops by itself and says so. A
/// recording left running by mistake would otherwise fill the disk. Sixty
/// seconds of a camera that never rests came to 41 MB at half size, so the
/// usual recording ends by time; at full size it ends by size, after about
/// 40 seconds.
pub const LONGEST_MS: f64 = 60_000.0;
pub const LARGEST: u64 = 64 << 20;

/// How many pictures may wait for the second thread. Each is the whole
/// window at four bytes a pixel (8 MB for a window of 1920 by 1080), so the
/// most a recording holds at once is this many, the one being worked on,
/// and two of the smaller pictures made from them.
pub const QUEUE: usize = 2;

/// The bounds of a recording: [`LONGEST_MS`] and [`LARGEST`], or smaller
/// ones for a test.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub ms: f64,
    pub bytes: u64,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { ms: LONGEST_MS, bytes: LARGEST }
    }
}

// ---------------------------------------------------------------- compression

/// How many places the table of [`lzw`] has: a power of two, and twice the
/// 4096 strings the format allows, so that a search seldom has far to go.
const SLOTS: usize = 8192;

/// Compress a picture's colour numbers as GIF does, and add them to `out`
/// as the format's blocks of at most 255 bytes, ready to follow a picture's
/// header. `bits` is how many bits a colour number takes (8 for 256 colours).
///
/// The method is LZW with codes that grow from `bits + 1` to 12 bits, as
/// the specification's appendix F has it:
/// https://www.w3.org/Graphics/GIF/spec-gif89a.txt
/// Each string of colours seen so far has a code. The table that finds a
/// string's code is a hash table searched from the hashed place onward,
/// and not an array of 4096 by 256 codes indexed directly: that array is
/// 2 MB to empty every time the table fills, which for a picture with much
/// detail is hundreds of times.
fn lzw(bits: u8, pixels: &[u8], out: &mut Vec<u8>) {
    let clear = 1u32 << bits;
    let end = clear + 1;
    let mut keys = vec![0u32; SLOTS];
    let mut codes = vec![0u16; SLOTS];
    let mut raw: Vec<u8> = Vec::with_capacity(pixels.len() / 2 + 16);
    // codes are packed into bytes from the low bit up
    let (mut bank, mut banked) = (0u64, 0u32);
    let mut width = bits as u32 + 1;
    let mut next = end + 1;
    let mut put = |code: u32, width: u32| {
        bank |= (code as u64) << banked;
        banked += width;
        while banked >= 8 {
            raw.push(bank as u8);
            bank >>= 8;
            banked -= 8;
        }
    };
    put(clear, width);
    if let Some((&first, rest)) = pixels.split_first() {
        // the code of the string read so far
        let mut so_far = first as u32;
        'pixels: for &pixel in rest {
            // (one more than the string and its next colour, so that 0 can mean an empty place)
            let key = (so_far << 8 | pixel as u32) + 1;
            // https://en.wikipedia.org/wiki/Hash_function#Fibonacci_hashing
            let mut slot = (key.wrapping_mul(0x9E37_79B1) >> 19) as usize;
            while keys[slot] != 0 {
                if keys[slot] == key {
                    so_far = codes[slot] as u32;
                    continue 'pixels;
                }
                slot = (slot + 1) & (SLOTS - 1);
            }
            put(so_far, width);
            if next < 4096 {
                keys[slot] = key;
                codes[slot] = next as u16;
                // a code that needs one more bit: every code after this one is written that wide
                if next == 1 << width {
                    width += 1;
                }
                next += 1;
            } else {
                // the table is full: start again with an empty one
                put(clear, width);
                keys.fill(0);
                width = bits as u32 + 1;
                next = end + 1;
            }
            so_far = pixel as u32;
        }
        put(so_far, width);
        // whoever reads the file adds a string for that last code as for
        // any other, and widens its codes at the same count
        if next < 4096 && next == 1 << width {
            width += 1;
        }
    }
    put(end, width);
    // (whatever is left of a byte)
    put(0, 7);
    out.push(bits);
    for block in raw.chunks(255) {
        out.push(block.len() as u8);
        out.extend_from_slice(block);
    }
    out.push(0);
}

// ---------------------------------------------------------------- the file

/// A table of 256 colours, as red, green and blue.
pub type Colours = [[u8; 3]; 256];

/// The colour number that means "leave what is there". The last one, so
/// that the colours fitted are simply the first 255.
const LEFT: u8 = 255;

/// Where in the picture a rectangle is: its left, top, width and height.
type Place = [u16; 4];

/// A GIF file being written: its header, then one picture at a time, then
/// its end. Counts the bytes it has written.
struct Writer<W: Write> {
    out: W,
    written: u64,
}

impl<W: Write> Writer<W> {
    fn put(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.out.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    /// The header: the size of the whole picture, the colours of every
    /// picture that brings none of its own, and that the recording plays
    /// round and round.
    fn begin(out: W, wide: u16, tall: u16, colours: &Colours) -> std::io::Result<Writer<W>> {
        let mut file = Writer { out, written: 0 };
        let mut head = Vec::with_capacity(13 + 768 + 19);
        head.extend_from_slice(b"GIF89a");
        head.extend_from_slice(&wide.to_le_bytes());
        head.extend_from_slice(&tall.to_le_bytes());
        // a table of colours follows, of 2^(7+1) colours of 8 bits each;
        // then the background colour and the shape of a pixel, neither used
        head.extend_from_slice(&[0xF7, 0, 0]);
        head.extend(colours.iter().flatten());
        // Play it again when it ends, for ever (the two zero bytes are the
        // number of times, 0 for no end). Not in the specification: it is
        // Netscape's addition, which every program that shows GIFs follows.
        // https://en.wikipedia.org/wiki/GIF#Animated_GIF
        head.extend_from_slice(b"\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00");
        file.put(&head)?;
        Ok(file)
    }

    /// One picture: shown for `hundredths` of a second, covering `place`,
    /// with colours of its own if `own` is given, and with [`LEFT`] meaning
    /// "leave what is there" if `leaves` is set. `data` is what [`lzw`]
    /// made of its colour numbers.
    fn picture(&mut self, hundredths: u16, leaves: bool, place: Place, own: Option<&Colours>, data: &[u8]) -> std::io::Result<()> {
        let mut head = Vec::with_capacity(18 + 768);
        // How the picture is shown (the specification's Graphic Control
        // Extension). The 4 is "leave this picture in place when the next
        // is drawn" (disposal method 1), which is what lets the next one
        // hold only what changed. The alternative, putting back the
        // background (method 2), has every picture written whole.
        head.extend_from_slice(&[0x21, 0xF9, 4, 4 | leaves as u8]);
        head.extend_from_slice(&hundredths.to_le_bytes());
        head.extend_from_slice(&[LEFT, 0]);
        // where it goes (the Image Descriptor)
        head.push(0x2C);
        for side in place {
            head.extend_from_slice(&side.to_le_bytes());
        }
        match own {
            Some(colours) => {
                head.push(0x87);
                head.extend(colours.iter().flatten());
            }
            None => head.push(0),
        }
        self.put(&head)?;
        self.put(data)
    }

    /// The file's last byte, and everything still held back written out.
    fn end(mut self) -> std::io::Result<u64> {
        self.put(&[0x3B])?;
        self.out.flush()?;
        Ok(self.written)
    }
}

// ---------------------------------------------------------------- colours

/// A colour as one number: red, green and blue from the high byte down.
type Rgb = u32;

fn parts(colour: Rgb) -> [i32; 3] {
    [(colour >> 16 & 255) as i32, (colour >> 8 & 255) as i32, (colour & 255) as i32]
}

/// A cache entry of [`Palette`] not yet worked out.
const UNKNOWN: u16 = u16::MAX;
/// Set in a cache entry whose colour is far from every colour of the table.
const FAR: u16 = 1 << 8;
/// How far that is, as a squared distance: 8 levels out in each of red,
/// green and blue, about where a flat face is seen to be the wrong colour.
const FAR_SQUARED: i32 = 3 * 8 * 8;

/// The 255 colours of a recording, and which of them is nearest any colour.
///
/// The colours are fitted to a picture of the recording ([`fit`]). The
/// alternative is a table fixed in advance (six levels of red and blue and
/// seven of green) with the picture dithered onto it in a fixed pattern
/// (https://en.wikipedia.org/wiki/Ordered_dithering): nothing to fit, and
/// the same colour number for a pixel whatever else is drawn. Both were
/// tried on recordings of this editor at half size, a camera flown round a
/// patterned cube and a sphere for ten seconds and a form being changed:
///
/// | | fixed table, dithered | fitted table |
/// |---|---|---|
/// | flat faces, the background, the panel | a cross-hatch of two table colours, and off their own colour | their own colours, flat |
/// | the floor, lit unevenly | cross-hatched | smooth: no bands to be seen at three times the size |
/// | thin dark edges of bricks | frayed by the hatch | kept |
/// | the flight's file | 7.1 MB | 7.0 MB |
///
/// The editor's pictures are a few flat colours and their shades, which is
/// what a fitted table serves best and a fixed one worst. A light dither
/// on the fitted table was tried as well, against bands in the floor:
/// there were none to hide, it put a speckle on every flat face, and the
/// flight's file was 8.5 MB.
///
/// The risk of fitting is that the colours change from one picture to the
/// next, so that a still wall flickers. Three things prevent it. A pixel
/// the window did not change is never written again, whatever the table
/// (see the module's notes). The table is fitted once and kept, and fitted
/// again only when a picture brings colours it serves badly ([`Film::add`]),
/// such as a form of a new colour; the new table goes in the file with that
/// picture. And there is no dither, whose pattern is fixed to the screen
/// and so crawls over anything that moves.
///
/// Finding the nearest of 255 colours for every pixel would be 255
/// comparisons a pixel. The answer is kept instead for each colour once it
/// has been asked for, to 6 bits each of red, green and blue: 262,144
/// entries, of which a picture uses a few thousand. Colours that differ
/// only below those bits share an answer, which moves a pixel at most two
/// levels.
pub struct Palette {
    pub colours: Colours,
    /// how many of `colours` are in use, from the first
    used: usize,
    /// for each colour to 6 bits a part: the nearest of `colours`, with
    /// [`FAR`] set if even that is far off, or [`UNKNOWN`]
    near: Vec<u16>,
}

impl Palette {
    fn of(colours: Colours, used: usize) -> Palette {
        Palette { colours, used: used.clamp(1, 255), near: vec![UNKNOWN; 1 << 18] }
    }

    /// The nearest colour's number, with [`FAR`] set if it is far off.
    fn nearest(&mut self, colour: Rgb) -> u16 {
        let key = ((colour >> 18 & 63) << 12 | (colour >> 10 & 63) << 6 | (colour >> 2 & 63)) as usize;
        let known = self.near[key];
        if known != UNKNOWN {
            return known;
        }
        // (the middle of the colours that share this entry, so that the
        // answer does not depend on which of them happened to ask first)
        let [r, g, b] = parts(colour).map(|part| part & !3 | 2);
        let (mut best, mut least) = (0usize, i32::MAX);
        for (number, [pr, pg, pb]) in self.colours[..self.used].iter().enumerate() {
            let (dr, dg, db) = (*pr as i32 - r, *pg as i32 - g, *pb as i32 - b);
            let apart = dr * dr + dg * dg + db * db;
            if apart < least {
                (best, least) = (number, apart);
            }
        }
        let entry = best as u16 | if least > FAR_SQUARED { FAR } else { 0 };
        self.near[key] = entry;
        entry
    }
}

/// The colours of a picture that fall in one small cube of colour (to 5
/// bits each of red, green and blue): how many pixels, and their sum.
#[derive(Clone, Copy, Default)]
struct Bin {
    count: u64,
    sum: [u64; 3],
}

impl Bin {
    fn mean(&self, part: usize) -> f64 {
        self.sum[part] as f64 / self.count as f64
    }
}

/// A group of [`Bin`]s that will share one colour of the table: which bins
/// (a stretch of the list), what they hold together, how far their colours
/// lie from their mean, and along which of red, green and blue the most.
struct Group {
    from: usize,
    to: usize,
    all: Bin,
    spread: f64,
    widest: usize,
}

fn group(bins: &[Bin], from: usize, to: usize) -> Group {
    let mut all = Bin::default();
    for bin in &bins[from..to] {
        all.count += bin.count;
        for part in 0..3 {
            all.sum[part] += bin.sum[part];
        }
    }
    let mut apart = [0.0f64; 3];
    for bin in &bins[from..to] {
        for part in 0..3 {
            let off = bin.mean(part) - all.mean(part);
            apart[part] += bin.count as f64 * off * off;
        }
    }
    let widest = if apart[0] >= apart[1] && apart[0] >= apart[2] { 0 } else if apart[1] >= apart[2] { 1 } else { 2 };
    Group { from, to, all, spread: apart[0] + apart[1] + apart[2], widest }
}

/// Choose 255 colours for a picture.
///
/// By median cut (Heckbert, 1982, https://doi.org/10.1145/965145.801294):
/// all the picture's colours start as one group, and the group is split in
/// two again and again until there are 255, each of which gives the table
/// the mean of its colours. Two departures from the paper, both to suit
/// pictures that are mostly a few flat colours. The group split next is the
/// one whose colours lie furthest from their mean, summed over its pixels,
/// and not the one with the most pixels. By pixels alone the floor, which
/// is most of the picture and nearly one colour, is split into ever finer
/// shades while the dots and arrows, a few hundred pixels of strong
/// colours, are left sharing one colour between them. And a group is split
/// at its mean, not its median, so that the cut falls between two clusters
/// of colour and not through the larger.
///
/// The colours are first counted in cubes of 8 levels a side ([`Bin`]),
/// 32,768 of them, so that the splitting works on a few thousand cubes and
/// not on half a million pixels. A cube holding one colour gives exactly
/// that colour, because the sum of its pixels is kept with their count.
pub fn fit(picture: &[Rgb]) -> Palette {
    let mut cubes = vec![Bin::default(); 1 << 15];
    for &colour in picture {
        let cube = &mut cubes[((colour >> 19 & 31) << 10 | (colour >> 11 & 31) << 5 | (colour >> 3 & 31)) as usize];
        cube.count += 1;
        cube.sum[0] += (colour >> 16 & 255) as u64;
        cube.sum[1] += (colour >> 8 & 255) as u64;
        cube.sum[2] += (colour & 255) as u64;
    }
    let mut bins: Vec<Bin> = cubes.into_iter().filter(|bin| bin.count > 0).collect();
    let mut colours = [[0u8; 3]; 256];
    if bins.is_empty() {
        return Palette::of(colours, 1);
    }
    let mut groups = vec![group(&bins, 0, bins.len())];
    while groups.len() < 255 {
        let Some(which) = (0..groups.len()).filter(|&g| groups[g].spread > 0.0 && groups[g].to - groups[g].from > 1).max_by(|&a, &b| groups[a].spread.total_cmp(&groups[b].spread)) else { break };
        let Group { from, to, all, widest, .. } = groups.swap_remove(which);
        // the bins on the near side of the mean are moved to the front
        let mean = all.mean(widest);
        let mut middle = from;
        for at in from..to {
            if bins[at].mean(widest) <= mean {
                bins.swap(at, middle);
                middle += 1;
            }
        }
        // (a group with any spread has bins on both sides of its mean; one
        // that rounding left with none on a side is cut in half instead)
        if middle == from || middle == to {
            middle = (from + to) / 2;
        }
        groups.push(group(&bins, from, middle));
        groups.push(group(&bins, middle, to));
    }
    // In a fixed order, darkest first: the order the splitting leaves them
    // in means nothing, and a file is easier to examine with its table in
    // an order.
    let light = |group: &Group, other: &Group| group.all.sum.iter().sum::<u64>() as u128 * other.all.count as u128;
    groups.sort_by(|a, b| light(a, b).cmp(&light(b, a)).then(a.from.cmp(&b.from)));
    for (colour, group) in colours.iter_mut().zip(&groups) {
        *colour = [0, 1, 2].map(|part| ((group.all.sum[part] + group.all.count / 2) / group.all.count) as u8);
    }
    Palette::of(colours, groups.len())
}

// ---------------------------------------------------------------- a picture made smaller

/// The window's pixels (rows from the top, four bytes each) as a picture
/// `by` times smaller each way, each of its pixels the average of a square
/// of `by` by `by` of the window's, rounded to the nearest. Columns and
/// rows left over at the right and bottom, fewer than `by`, are dropped.
/// Returns the smaller picture's width and height.
pub fn shrink(wide: usize, tall: usize, pixels: &[[u8; 4]], by: usize, into: &mut Vec<Rgb>) -> (usize, usize) {
    let (w, h) = (wide / by, tall / by);
    into.clear();
    into.reserve(w * h);
    if by == 1 {
        into.extend(pixels.iter().map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32));
        return (w, h);
    }
    let square = (by * by) as u32;
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for down in 0..by {
                let row = (y * by + down) * wide + x * by;
                for pixel in &pixels[row..row + by] {
                    sum[0] += pixel[0] as u32;
                    sum[1] += pixel[1] as u32;
                    sum[2] += pixel[2] as u32;
                }
            }
            let [r, g, b] = sum.map(|part| (part + square / 2) / square);
            into.push(r << 16 | g << 8 | b);
        }
    }
    (w, h)
}

/// The smallest rectangle that holds every pixel in which two pictures of
/// the same size differ, as its first and last column and row; `None` if
/// they are the same picture.
fn changed(before: &[Rgb], after: &[Rgb], wide: usize) -> Option<[usize; 4]> {
    let mut found: Option<[usize; 4]> = None;
    for (y, (old, new)) in before.chunks_exact(wide).zip(after.chunks_exact(wide)).enumerate() {
        if old == new {
            continue;
        }
        let differs = |x: &usize| old[*x] != new[*x];
        let (first, last) = ((0..wide).find(differs).unwrap_or(0), (0..wide).rev().find(differs).unwrap_or(wide - 1));
        found = Some(match found {
            None => [first, y, last, y],
            Some([left, top, right, _]) => [left.min(first), top, right.max(last), y],
        });
    }
    found
}

// ---------------------------------------------------------------- the recording, a picture at a time

/// A picture compressed and waiting to be written: a GIF says how long a
/// picture is shown before the picture itself, and that is not known until
/// the next one comes.
struct Held {
    /// when it was taken, in hundredths of a second from the first
    at: u64,
    leaves: bool,
    place: Place,
    own: Option<Box<Colours>>,
    data: Vec<u8>,
}

/// What a recording came to.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// pictures written to the file
    pub pictures: u32,
    /// how long it plays, in hundredths of a second
    pub hundredths: u64,
    pub bytes: u64,
    /// how many times the table of colours was fitted: once, and again for each picture it served badly
    pub fitted: u32,
}

/// A recording being made: takes the window's pictures one at a time with
/// the time each was taken, and writes the file as they come.
pub struct Film<W: Write> {
    /// where the file goes, until the first picture says how large it is
    blank: Option<W>,
    file: Option<Writer<W>>,
    by: usize,
    /// the usual time between pictures, in hundredths of a second: how long the last one is shown
    every: u32,
    /// the window's size, as the first picture had it
    window: (usize, usize),
    wide: usize,
    tall: usize,
    /// the picture before this one and this one, made smaller
    before: Vec<Rgb>,
    now: Vec<Rgb>,
    palette: Palette,
    /// whether the table has been fitted again since the one in the file's header
    refitted: bool,
    /// how many pixels the table served badly when it was fitted
    far_when_fitted: usize,
    held: Option<Held>,
    /// when the first picture was taken, and the latest, in milliseconds
    first_ms: f64,
    last_ms: f64,
    pictures: u32,
    fitted: u32,
    /// the colour numbers of the rectangle being written
    numbers: Vec<u8>,
}

impl<W: Write> Film<W> {
    /// A recording into `out`, `by` times smaller than the window each
    /// way, with pictures usually `every` hundredths of a second apart.
    pub fn new(out: W, by: usize, every: u32) -> Film<W> {
        Film { blank: Some(out), file: None, by: by.max(1), every: every.max(2), window: (0, 0), wide: 0, tall: 0, before: Vec::new(), now: Vec::new(), palette: Palette::of([[0; 3]; 256], 1), refitted: false, far_when_fitted: 0, held: None, first_ms: 0.0, last_ms: 0.0, pictures: 0, fitted: 0, numbers: Vec::new() }
    }

    /// How many bytes the recording is so far, with what is waiting to be written.
    pub fn bytes(&self) -> u64 {
        self.file.as_ref().map_or(0, |file| file.written) + self.held.as_ref().map_or(0, |held| held.data.len() as u64 + 18 + if held.own.is_some() { 768 } else { 0 })
    }

    /// When the first picture was taken, if one has been.
    pub fn began_ms(&self) -> Option<f64> {
        self.file.is_some().then_some(self.first_ms)
    }

    /// When the latest picture was taken.
    pub fn latest_ms(&self) -> f64 {
        self.last_ms
    }

    fn hundredths(&self, at_ms: f64) -> u64 {
        ((at_ms - self.first_ms).max(0.0) / 10.0).round() as u64
    }

    /// Colour numbers for the rectangle `[left, top, right, bottom]` of the
    /// picture, into `self.numbers`: every pixel if `whole`, and otherwise
    /// [`LEFT`] for each that is as it was. Returns how many pixels were
    /// given a colour, and how many of those the table served badly.
    fn number(&mut self, [left, top, right, bottom]: [usize; 4], whole: bool) -> (usize, usize) {
        self.numbers.clear();
        let (mut coloured, mut far) = (0, 0);
        for y in top..=bottom {
            let row = y * self.wide;
            for at in row + left..=row + right {
                let colour = self.now[at];
                if !whole && colour == self.before[at] {
                    self.numbers.push(LEFT);
                } else {
                    let entry = self.palette.nearest(colour);
                    coloured += 1;
                    far += (entry >> 8) as usize;
                    self.numbers.push(entry as u8);
                }
            }
        }
        (coloured, far)
    }

    /// Fit the table to the picture in hand, and note how well it serves it.
    fn refit(&mut self) {
        self.palette = fit(&self.now);
        self.fitted += 1;
        let mut far = 0;
        for at in 0..self.now.len() {
            far += (self.palette.nearest(self.now[at]) >> 8) as usize;
        }
        self.far_when_fitted = far;
    }

    /// Write the picture that was waiting, now that the time of the next,
    /// `until` in hundredths of a second, says how long it is shown.
    fn release(&mut self, until: u64) -> Result<(), String> {
        if let (Some(held), Some(file)) = (self.held.take(), self.file.as_mut()) {
            // (never under 2 hundredths, which browsers play as 10, nor more than the two bytes it is written in hold)
            let shown = until.saturating_sub(held.at).clamp(2, u16::MAX as u64) as u16;
            file.picture(shown, held.leaves, held.place, held.own.as_deref(), &held.data).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The next picture: the window's pixels, rows from the top, taken at
    /// `at_ms`. `Err` says why the recording cannot go on.
    pub fn add(&mut self, wide: usize, tall: usize, pixels: &[[u8; 4]], at_ms: f64) -> Result<(), String> {
        if pixels.len() != wide * tall {
            return Err("a picture read back from the window is not the size of the window".into());
        }
        if let Some(out) = self.blank.take() {
            // the first picture: its size is the recording's, and it is written whole
            let (w, h) = (wide / self.by, tall / self.by);
            if w == 0 || h == 0 || w > u16::MAX as usize || h > u16::MAX as usize {
                return Err(format!("a window of {wide} by {tall} pixels cannot be recorded"));
            }
            (self.window, self.wide, self.tall, self.first_ms, self.last_ms) = ((wide, tall), w, h, at_ms, at_ms);
            shrink(wide, tall, pixels, self.by, &mut self.now);
            self.refit();
            self.file = Some(Writer::begin(out, w as u16, h as u16, &self.palette.colours).map_err(|e| e.to_string())?);
            self.number([0, 0, w - 1, h - 1], true);
            let mut data = Vec::with_capacity(self.numbers.len() / 4);
            lzw(8, &self.numbers, &mut data);
            self.held = Some(Held { at: 0, leaves: false, place: [0, 0, w as u16, h as u16], own: None, data });
            self.pictures = 1;
            std::mem::swap(&mut self.before, &mut self.now);
            return Ok(());
        }
        if (wide, tall) != self.window {
            return Err("the window changed size".into());
        }
        self.last_ms = self.last_ms.max(at_ms);
        shrink(wide, tall, pixels, self.by, &mut self.now);
        // nothing changed: the picture before is shown for as long again
        let Some(rect) = changed(&self.before, &self.now, self.wide) else { return Ok(()) };
        let (coloured, far) = self.number(rect, false);
        // Colours the table serves badly: more than a thousandth of the
        // picture, over twice the share it served badly when it was fitted
        // (the odd pixel along an edge). That is a form of a new colour,
        // or the light on a turned face. The table is fitted to this
        // picture, and goes in the file with it. Any picture that fell
        // short of its colours at all would be the simpler rule, and would
        // fit a table for nearly every picture of a turning camera, each a
        // little different from the last: a shimmer over everything that
        // moves.
        if far > self.now.len() / 1000 + 2 * self.far_when_fitted * coloured / self.now.len() {
            self.refit();
            self.refitted = true;
            self.number(rect, false);
        }
        let mut data = Vec::with_capacity(self.numbers.len() / 4);
        lzw(8, &self.numbers, &mut data);
        let at = self.hundredths(at_ms);
        self.release(at)?;
        let [left, top, right, bottom] = rect;
        self.held = Some(Held { at, leaves: true, place: [left as u16, top as u16, (right - left + 1) as u16, (bottom - top + 1) as u16], own: self.refitted.then(|| Box::new(self.palette.colours)), data });
        self.pictures += 1;
        std::mem::swap(&mut self.before, &mut self.now);
        Ok(())
    }

    /// End the recording at `end_ms`: the last picture is shown until
    /// then, or for the usual time if that is longer, so that a recording
    /// played round does not skip it.
    pub fn finish(mut self, end_ms: f64) -> Result<Totals, String> {
        let Some(last) = self.held.as_ref().map(|held| held.at) else { return Ok(Totals::default()) };
        let until = self.hundredths(end_ms).max(last + self.every as u64);
        self.release(until)?;
        let bytes = self.file.take().map_or(Ok(0), Writer::end).map_err(|e| e.to_string())?;
        Ok(Totals { pictures: self.pictures, hundredths: until, bytes, fitted: self.fitted })
    }
}

// ---------------------------------------------------------------- when to take a picture

/// Says when the next picture of a recording is due, by the clock.
///
/// Pictures are due at whole numbers of the interval after the first. A
/// frame that comes late takes the picture then, and the one after is due
/// at the next whole interval still ahead: pictures are neither bunched up
/// to make good the delay, nor does a slow frame move every later one
/// back. Counting frames instead (every third frame, say) would tie the
/// rate to the display's, and play a stretch of slow frames speeded up.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    every_ms: f64,
    began_ms: f64,
    due_ms: f64,
}

/// How much before its time a picture may be taken, in milliseconds. A
/// display's frames come a fraction of a millisecond early as often as
/// late, and a picture due at 50 ms would otherwise wait out a frame that
/// came at 49.9.
const EARLY_MS: f64 = 2.0;

impl Pace {
    pub fn new(every_ms: f64, now_ms: f64) -> Pace {
        Pace { every_ms, began_ms: now_ms, due_ms: now_ms }
    }

    /// Whether to take a picture in the frame drawn at `now_ms`. Saying
    /// yes sets when the next is due.
    pub fn due(&mut self, now_ms: f64) -> bool {
        if now_ms + EARLY_MS < self.due_ms {
            return false;
        }
        let taken = ((now_ms + EARLY_MS - self.began_ms) / self.every_ms).floor();
        self.due_ms = self.began_ms + (taken + 1.0) * self.every_ms;
        true
    }
}

// ---------------------------------------------------------------- the second thread

/// One picture of the window on its way to the second thread.
struct Taken {
    wide: usize,
    tall: usize,
    pixels: Vec<[u8; 4]>,
    at_ms: f64,
}

/// Why a recording ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Ended {
    /// it was stopped
    Asked,
    /// it reached the longest a recording may be
    Time,
    /// its file reached the largest a recording may be
    Size,
    /// it could not go on, and why
    Failed(String),
}

/// What the second thread gives back when a recording is over.
#[derive(Clone, Debug)]
pub struct Done {
    pub totals: Totals,
    pub ended: Ended,
    /// how long the thread worked on a picture, in milliseconds: all told, and the longest
    pub worked_ms: (f64, f64),
    /// how many pictures it was given
    pub given: u32,
}

/// What the frame loop and the second thread both see.
#[derive(Default)]
struct Shared {
    /// how many pictures are waiting or being worked on
    waiting: AtomicUsize,
    /// when the recording was stopped, once it has been
    stopped_ms: Mutex<Option<f64>>,
}

/// The second thread's work: every picture sent, into `film`, until the
/// sender is dropped or a bound is reached.
fn work<W: Write>(mut film: Film<W>, pictures: Receiver<Taken>, shared: &Shared, limits: Limits) -> Done {
    let started = Instant::now();
    let (mut worked_ms, mut given) = ((0.0f64, 0.0f64), 0u32);
    let mut ended = Ended::Asked;
    // when the recording ends, if not when it was stopped
    let mut end_ms: Option<f64> = None;
    loop {
        // If the frame loop sends nothing more and never says stop (the
        // editor redraws only when there is something to draw), the
        // recording still ends at its longest, by this thread's own clock.
        // (half a second over, so that a frame loop that is still running is the one to end it, at the time it knows)
        let left = Duration::try_from_secs_f64((limits.ms / 1000.0 - started.elapsed().as_secs_f64()).max(0.0) + 0.5).unwrap_or(Duration::MAX);
        let taken = match pictures.recv_timeout(left) {
            Ok(taken) => taken,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                (ended, end_ms) = (Ended::Time, film.began_ms().map(|began| began + limits.ms));
                break;
            }
        };
        given += 1;
        if let Some(began) = film.began_ms().filter(|began| taken.at_ms - began >= limits.ms) {
            shared.waiting.fetch_sub(1, Ordering::Relaxed);
            (ended, end_ms) = (Ended::Time, Some(began + limits.ms));
            break;
        }
        let at = Instant::now();
        let added = film.add(taken.wide, taken.tall, &taken.pixels, taken.at_ms);
        drop(taken);
        shared.waiting.fetch_sub(1, Ordering::Relaxed);
        let took = at.elapsed().as_secs_f64() * 1000.0;
        worked_ms = (worked_ms.0 + took, worked_ms.1.max(took));
        if let Err(why) = added {
            // (what has been written is kept and closed: the recording up to here)
            ended = Ended::Failed(why);
            break;
        }
        if film.bytes() >= limits.bytes {
            ended = Ended::Size;
            break;
        }
    }
    // Stop taking pictures before the file is closed: the frame loop sees
    // the queue gone and sends no more.
    drop(pictures);
    let stopped = shared.stopped_ms.lock().ok().and_then(|at| *at);
    let began = film.began_ms();
    let mut end = end_ms.or(stopped).unwrap_or(film.latest_ms());
    if let Some(began) = began {
        if end - began >= limits.ms {
            end = began + limits.ms;
            if ended == Ended::Asked {
                ended = Ended::Time;
            }
        }
    }
    match film.finish(end) {
        Ok(totals) => Done { totals, ended, worked_ms, given },
        Err(why) => Done { totals: Totals::default(), ended: if matches!(ended, Ended::Failed(_)) { ended } else { Ended::Failed(why) }, worked_ms, given },
    }
}

// ---------------------------------------------------------------- what the frame loop holds

/// A recording under way, as the frame loop sees it.
struct Running {
    /// `None` once the recording has been stopped: dropping it is what tells the second thread
    pictures: Option<SyncSender<Taken>>,
    worker: JoinHandle<Done>,
    shared: Arc<Shared>,
    path: PathBuf,
    panel: bool,
    pace: Pace,
    began_ms: f64,
    /// the window's size when the first picture was taken
    window: Option<(u32, u32)>,
    /// pictures handed over, and pictures skipped because the second thread had not kept up
    given: u32,
    skipped: u32,
    /// how long reading the window's pixels took, in milliseconds: all told, and the longest
    read_ms: (f64, f64),
    /// why it was stopped, if not by being asked
    because: Option<&'static str>,
}

/// Something to tell the player: in passing, or to be read and closed.
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    Done(String),
    Failed(String),
}

/// What the mark over the view shows while a recording is made.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub seconds: f64,
    pub pictures: u32,
    /// said beside the mark for the first few seconds: where the recording is going
    pub notice: Option<String>,
}

/// GIF recording as the editor holds it: the two settings, and the
/// recording under way if there is one.
pub struct Controls {
    /// which of [`SIZES`] and of [`RATES`] the next recording has
    pub size: usize,
    pub rate: usize,
    limits: Limits,
    running: Option<Running>,
    /// what the last recording cost, for whoever asks (see `mcp`)
    pub cost: String,
}

impl Default for Controls {
    fn default() -> Controls {
        Controls { size: USUAL_SIZE, rate: USUAL_RATE, limits: Limits::default(), running: None, cost: String::new() }
    }
}

/// How long the notice of where a recording is going stays beside the mark, in milliseconds.
const NOTICE_MS: f64 = 4000.0;

/// A size in bytes, in words.
fn size_said(bytes: u64) -> String {
    if bytes >= 1 << 20 { format!("{:.1} MB", bytes as f64 / (1 << 20) as f64) } else { format!("{} KB", bytes.div_ceil(1024)) }
}

/// The name of a recording begun `seconds` after the start of 1970 by
/// universal time: `brickloft-2026-11-16-143055.gif`.
///
/// Universal time and not the time on the player's clock: the standard
/// library knows the first and not the second, which takes a call to each
/// operating system or a crate for dates (`chrono`, `time`), for a name
/// whose purpose is to sort recordings in the order they were made.
/// The date is worked out from the count of days as in
/// https://howardhinnant.github.io/date_algorithms.html#civil_from_days
pub fn name_at(seconds: u64) -> String {
    let (days, of_day) = ((seconds / 86_400) as i64, seconds % 86_400);
    // years are counted from 1 March of year 0, in eras of 400 years, so that a leap day is the last day of its year
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    let year = year_of_era + era * 400 + (month <= 2) as i64;
    format!("brickloft-{year:04}-{month:02}-{day:02}-{:02}{:02}{:02}.gif", of_day / 3600, of_day / 60 % 60, of_day % 60)
}

impl Controls {
    /// The same with other bounds than the usual, for a test.
    pub fn bounded(limits: Limits) -> Controls {
        let mut controls = Controls::default();
        controls.limits = limits;
        controls
    }

    pub fn recording(&self) -> bool {
        self.running.as_ref().is_some_and(|running| running.pictures.is_some())
    }

    /// Start recording into the pictures folder, the window with its panel
    /// or the view alone. Returns where the recording is going, or why
    /// there is none.
    pub fn start(&mut self, panel: bool, now_ms: f64) -> Result<String, String> {
        let folder = crate::places::screenshots().map_err(|e| format!("No recording: {e}"))?;
        self.start_in(&folder, panel, now_ms)
    }

    /// [`Controls::start`] into a folder of the caller's choosing.
    pub fn start_in(&mut self, folder: &Path, panel: bool, now_ms: f64) -> Result<String, String> {
        if let Some(running) = &self.running {
            return Err(if running.pictures.is_some() { format!("A recording is already being made: {}", running.path.display()) } else { "The last recording is still being saved; try again in a moment".into() });
        }
        let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs());
        let name = name_at(seconds);
        // (a second recording begun within the same second: -2, -3, ...)
        let stem = name.trim_end_matches(".gif");
        let path = (1..1000).map(|n| folder.join(if n == 1 { name.clone() } else { format!("{stem}-{n}.gif") })).find(|path| !path.exists()).ok_or_else(|| format!("No recording: {} is full of recordings made this second", folder.display()))?;
        // made here and not by the second thread, so that a folder that
        // cannot be written is said at once; never over a file already there
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| format!("No recording: could not make {}: {e}. Check that the folder can be written to and that the disk has room.", path.display()))?;
        // (64 KB at a time: the file grows as the recording does, without a call to the system for every few bytes)
        self.start_into(std::io::BufWriter::with_capacity(64 << 10, file), path.clone(), panel, now_ms);
        Ok(format!("Recording to {}", path.display()))
    }

    /// Start recording into anything that can be written to; `path` is
    /// what the recording is called in what is said of it.
    fn start_into(&mut self, out: impl Write + Send + 'static, path: PathBuf, panel: bool, now_ms: f64) {
        let (by, every) = (SIZES[self.size.min(SIZES.len() - 1)].1, RATES[self.rate.min(RATES.len() - 1)].1);
        let (pictures, taken) = sync_channel::<Taken>(QUEUE);
        let shared = Arc::new(Shared::default());
        let (theirs, limits) = (shared.clone(), self.limits);
        let worker = std::thread::spawn(move || work(Film::new(out, by, every), taken, &theirs, limits));
        self.running = Some(Running { pictures: Some(pictures), worker, shared, path, panel, pace: Pace::new(every as f64 * 10.0, now_ms), began_ms: now_ms, window: None, given: 0, skipped: 0, read_ms: (0.0, 0.0), because: None });
    }

    /// Whether a picture is to be taken in the frame drawn at `now_ms`:
    /// with the panel drawn, or before it is. Saying yes counts as taking
    /// it: ask once a frame for each.
    pub fn wants(&mut self, panel: bool, now_ms: f64) -> bool {
        match &mut self.running {
            Some(running) if running.pictures.is_some() && running.panel == panel => running.pace.due(now_ms),
            _ => false,
        }
    }

    /// Hand over the window as just drawn, `wide` by `tall`, taken at
    /// `now_ms`. `read` reads its pixels back, rows from the top, and is
    /// not called if the picture would only be dropped. Never waits.
    pub fn give(&mut self, wide: u32, tall: u32, read: impl FnOnce() -> Vec<[u8; 4]>, now_ms: f64) {
        let Some(running) = self.running.as_mut().filter(|running| running.pictures.is_some()) else { return };
        // A minimised window has no size and nothing drawn in it. The
        // recording goes on, showing the last picture, until the window is
        // back: ending it there would end a recording because its window
        // was put away for a moment.
        if wide == 0 || tall == 0 {
            return;
        }
        if *running.window.get_or_insert((wide, tall)) != (wide, tall) {
            // a GIF is one size throughout
            running.because = Some("the window changed size");
            self.stop(now_ms);
            return;
        }
        // The second thread has not kept up. The picture is skipped before
        // it is read, which is the costly part, and the one before it is
        // shown for longer.
        if running.shared.waiting.load(Ordering::Relaxed) > QUEUE {
            running.skipped += 1;
            return;
        }
        let Some(pictures) = &running.pictures else { return };
        let at = Instant::now();
        let pixels = read();
        let took = at.elapsed().as_secs_f64() * 1000.0;
        running.read_ms = (running.read_ms.0 + took, running.read_ms.1.max(took));
        running.shared.waiting.fetch_add(1, Ordering::Relaxed);
        match pictures.try_send(Taken { wide: wide as usize, tall: tall as usize, pixels, at_ms: now_ms }) {
            Ok(()) => running.given += 1,
            Err(TrySendError::Full(_)) => {
                running.shared.waiting.fetch_sub(1, Ordering::Relaxed);
                running.skipped += 1;
            }
            // the second thread has ended the recording itself: `poll` says why
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    /// Stop recording. The file is finished by the second thread; `poll`
    /// says when it has been.
    pub fn stop(&mut self, now_ms: f64) {
        if let Some(running) = &mut self.running {
            if let Ok(mut stopped) = running.shared.stopped_ms.lock() {
                stopped.get_or_insert(now_ms);
            }
            running.pictures = None;
        }
    }

    /// Start if nothing is being recorded, stop if something is: the key
    /// and the button.
    pub fn toggle(&mut self, panel: bool, now_ms: f64) -> Option<Said> {
        if self.recording() {
            self.stop(now_ms);
            None
        } else {
            // (where it is going is said beside the mark, which is not in the recording as a notice over the view would be)
            self.start(panel, now_ms).err().map(Said::Failed)
        }
    }

    /// What the mark over the view is to show, while recording.
    pub fn status(&self, now_ms: f64) -> Option<Status> {
        let running = self.running.as_ref().filter(|running| running.pictures.is_some())?;
        let notice = (now_ms - running.began_ms < NOTICE_MS).then(|| format!("Recording to {}", running.path.display()));
        Some(Status { seconds: (now_ms - running.began_ms) / 1000.0, pictures: running.given, notice })
    }

    /// Called every frame: ends a recording that has reached its longest,
    /// and returns what there is to say of one that is over.
    pub fn poll(&mut self, now_ms: f64) -> Option<Said> {
        let running = self.running.as_ref()?;
        if running.pictures.is_some() && now_ms - running.began_ms >= self.limits.ms {
            self.stop(now_ms);
        }
        self.running.as_ref().is_some_and(|running| running.worker.is_finished()).then(|| self.wait(now_ms)).flatten()
    }

    /// Stop recording if it has not been, wait for the file to be
    /// finished, and return what there is to say of it. The wait is as
    /// long as the pictures still in the queue take, a few hundredths of a
    /// second: `poll` is for the frame loop, which does not wait.
    pub fn wait(&mut self, now_ms: f64) -> Option<Said> {
        self.stop(now_ms);
        let Running { worker, path, given, skipped, read_ms, because, .. } = self.running.take()?;
        let Ok(done) = worker.join() else { return Some(Said::Failed(format!("The recording stopped: its thread failed. What was written is in {}", path.display()))) };
        let (pictures, seconds) = (done.totals.pictures, done.totals.hundredths as f64 / 100.0);
        self.cost = format!(
            "{given} pictures taken and {skipped} skipped; {pictures} written, the rest being the same as the one before; colours fitted {} time(s). Reading the window took {:.1} ms a picture (at most {:.1}) in the frame loop; the rest took {:.1} ms a picture (at most {:.1}) on the second thread.",
            done.totals.fitted,
            read_ms.0 / given.max(1) as f64,
            read_ms.1,
            done.worked_ms.0 / done.given.max(1) as f64,
            done.worked_ms.1
        );
        if pictures == 0 {
            // nothing was drawn while it ran, or it could not be written: an empty file is no recording
            let _ = std::fs::remove_file(&path);
            return Some(match done.ended {
                Ended::Failed(why) => Said::Failed(format!("No recording was saved to {}: {why}. Check that the disk has room and that the folder can be written to.", path.display())),
                _ => Said::Done("Nothing was recorded: the window was not drawn while the recording ran".into()),
            });
        }
        let saved = format!("saved to {} ({seconds:.1} seconds, {pictures} pictures, {})", path.display(), size_said(done.totals.bytes));
        Some(match (done.ended, because) {
            (Ended::Failed(why), _) => Said::Failed(format!("The recording stopped: {why}. What had been recorded was {saved}.")),
            (Ended::Time, _) => Said::Done(format!("Recording stopped at {:.0} seconds, the longest it may be, and {saved}", self.limits.ms / 1000.0)),
            (Ended::Size, _) => Said::Done(format!("Recording stopped at {}, the largest it may be, and {saved}", size_said(self.limits.bytes))),
            (Ended::Asked, Some(because)) => Said::Done(format!("Recording stopped because {because}, and {saved}")),
            (Ended::Asked, None) => Said::Done(format!("Recording {saved}")),
        })
    }
}

/// A recording still being made when the editor closes is ended and its
/// file finished, as if it had been stopped.
impl Drop for Controls {
    fn drop(&mut self) {
        if let Some(mut running) = self.running.take() {
            running.pictures = None;
            let _ = running.worker.join();
        }
    }
}

// ---------------------------------------------------------------- the mark over the view

/// The mark shown while recording: a red dot, how long the recording has
/// run and how many pictures it has taken.
///
/// It has a painter of its own, apart from the panel's, so that it can be
/// drawn after the picture for the recording has been read back, and is
/// never in the recording. Drawn with the panel it would be in every
/// recording that has the panel; left out of just the frames recorded it
/// would flicker, at the rate of the recording.
pub struct Mark(three_d::GUI);

impl Mark {
    pub fn new(context: &three_d::Context) -> Mark {
        Mark(three_d::GUI::new(context))
    }

    /// Draw it over what is on `screen`, in the top right corner.
    pub fn draw(&mut self, screen: &three_d::RenderTarget, time_ms: f64, viewport: three_d::Viewport, pixels_per_point: f32, status: &Status) {
        use three_d::egui;
        let whole = status.seconds.max(0.0) as u64;
        let counted = format!("{}:{:02}   {} picture{}", whole / 60, whole % 60, status.pictures, if status.pictures == 1 { "" } else { "s" });
        self.0.update(&mut [], time_ms, viewport, pixels_per_point, |ui| {
            // (under where the progress of a search for the best fit is shown)
            egui::Area::new(egui::Id::new("recording")).anchor(egui::Align2::RIGHT_TOP, [-12.0, 44.0]).interactable(false).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 7.0, egui::Color32::from_rgb(235, 40, 40));
                        ui.add(egui::Label::new(egui::RichText::new(counted.as_str()).strong()).wrap_mode(egui::TextWrapMode::Extend));
                    });
                    if let Some(notice) = &status.notice {
                        ui.add(egui::Label::new(notice.as_str()).wrap_mode(egui::TextWrapMode::Extend));
                    }
                });
            });
        });
        if let Err(e) = screen.write(|| self.0.render()) {
            eprintln!("brickloft editor: could not draw the recording mark: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------ reading a GIF back
    //
    // Written apart from the code above, and sharing nothing with it but
    // the constants of the format: what the recorder writes is held to
    // what a reader of GIFs makes of it.

    /// One picture of a file read back.
    struct Shown {
        hundredths: u16,
        /// what is done with the picture when the next is drawn (1: it is left in place)
        after: u8,
        /// the colour number that means "leave what is there", if any does
        leaves: Option<u8>,
        /// left, top, width, height
        place: [usize; 4],
        own_colours: bool,
        colours: Vec<[u8; 3]>,
        numbers: Vec<u8>,
    }

    struct Read {
        wide: usize,
        tall: usize,
        /// how many times it plays, if the file says (0: for ever)
        plays: Option<u16>,
        pictures: Vec<Shown>,
        /// whether the file has its last byte
        ended: bool,
    }

    /// Undo GIF's compression: `data` is the blocks' contents joined up.
    ///
    /// The data must end in the byte its end code ends in. A reader that
    /// widens its codes when the writer did not reads the end code a bit
    /// longer, and only sees it at all because the bits after it are zero;
    /// here that is caught when the longer code runs past the data.
    fn unlzw(bits: u8, data: &[u8]) -> Vec<u8> {
        let clear = 1usize << bits;
        let end = clear + 1;
        // each string is the string before it and one colour more
        let mut table: Vec<(usize, u8)> = vec![(0, 0); 4096];
        let mut out = Vec::new();
        let (mut width, mut next, mut before) = (bits as usize + 1, end + 1, None::<usize>);
        let mut at = 0usize;
        let string = |table: &[(usize, u8)], mut code: usize| {
            let mut backwards = Vec::new();
            while code >= clear {
                backwards.push(table[code].1);
                code = table[code].0;
            }
            backwards.push(code as u8);
            backwards.reverse();
            backwards
        };
        loop {
            assert!(at + width <= data.len() * 8, "the compressed picture stops without its end code");
            let code = (0..width).fold(0usize, |code, bit| code | ((data[(at + bit) / 8] >> ((at + bit) % 8) & 1) as usize) << bit);
            at += width;
            if code == clear {
                (width, next, before) = (bits as usize + 1, end + 1, None);
                continue;
            }
            if code == end {
                assert_eq!(data.len(), at.div_ceil(8), "bytes follow the end code");
                return out;
            }
            let said = match before {
                _ if code < next => string(&table, code),
                Some(before) if code == next => {
                    let mut said = string(&table, before);
                    said.push(said[0]);
                    said
                }
                _ => panic!("code {code} is not in the table of {next}"),
            };
            if let Some(before) = before {
                if next < 4096 {
                    table[next] = (before, said[0]);
                    next += 1;
                    if next == 1 << width && width < 12 {
                        width += 1;
                    }
                }
            }
            out.extend_from_slice(&said);
            before = Some(code);
        }
    }

    /// Read a GIF file, panicking on anything a reader could not follow.
    fn read(file: &[u8]) -> Read {
        assert_eq!(&file[..6], b"GIF89a");
        let two = |at: usize| file[at] as usize | (file[at + 1] as usize) << 8;
        let table = |at: usize, packed: u8| -> Vec<[u8; 3]> { file[at..at + 3 * (2 << (packed & 7))].chunks(3).map(|c| [c[0], c[1], c[2]]).collect() };
        let mut read = Read { wide: two(6), tall: two(8), plays: None, pictures: Vec::new(), ended: false };
        assert!(file[10] & 0x80 != 0, "the file has no table of colours of its own");
        let global = table(13, file[10]);
        let mut at = 13 + 3 * global.len();
        // how the next picture is shown, from the block before it
        let mut control: Option<(u16, u8, Option<u8>)> = None;
        // the contents of the blocks starting at `at`, and where they end
        let blocks = |mut at: usize| {
            let mut all = Vec::new();
            while file[at] != 0 {
                all.extend_from_slice(&file[at + 1..at + 1 + file[at] as usize]);
                at += 1 + file[at] as usize;
            }
            (all, at + 1)
        };
        while at < file.len() {
            assert!(!read.ended, "bytes follow the end of the file");
            match file[at] {
                0x3B => {
                    read.ended = true;
                    at += 1;
                }
                0x21 if file[at + 1] == 0xF9 => {
                    assert_eq!(file[at + 2], 4);
                    control = Some((two(at + 4) as u16, file[at + 3] >> 2 & 7, (file[at + 3] & 1 != 0).then_some(file[at + 6])));
                    assert_eq!(file[at + 7], 0);
                    at += 8;
                }
                0x21 if file[at + 1] == 0xFF => {
                    assert_eq!(&file[at + 2..at + 14], b"\x0BNETSCAPE2.0");
                    let (said, after) = blocks(at + 14);
                    assert_eq!(said[0], 1);
                    read.plays = Some(said[1] as u16 | (said[2] as u16) << 8);
                    at = after;
                }
                0x2C => {
                    let place = [two(at + 1), two(at + 3), two(at + 5), two(at + 7)];
                    let packed = file[at + 9];
                    assert_eq!(packed & 0x40, 0, "the picture is interlaced");
                    at += 10;
                    let own_colours = packed & 0x80 != 0;
                    let colours = if own_colours { table(at, packed) } else { global.clone() };
                    at += if own_colours { 3 * colours.len() } else { 0 };
                    let bits = file[at];
                    let (data, after) = blocks(at + 1);
                    at = after;
                    let numbers = unlzw(bits, &data);
                    assert_eq!(numbers.len(), place[2] * place[3], "a picture does not fill its rectangle");
                    assert!(place[0] + place[2] <= read.wide && place[1] + place[3] <= read.tall, "a picture reaches outside the file's own size");
                    let (hundredths, after, leaves) = control.take().expect("a picture with nothing said of how it is shown");
                    read.pictures.push(Shown { hundredths, after, leaves, place, own_colours, colours, numbers });
                }
                other => panic!("byte {other:#x} at {at} begins nothing"),
            }
        }
        read
    }

    /// What is on show after each picture of a file: every picture drawn
    /// over the ones before, as a program showing the file does.
    fn shown(read: &Read) -> Vec<Vec<[u8; 3]>> {
        let mut canvas = vec![[0u8; 3]; read.wide * read.tall];
        let mut all = Vec::new();
        for picture in &read.pictures {
            assert_eq!(picture.after, 1, "only pictures left in place can be drawn over one another");
            let [left, top, wide, tall] = picture.place;
            for y in 0..tall {
                for x in 0..wide {
                    let number = picture.numbers[y * wide + x];
                    if Some(number) != picture.leaves {
                        canvas[(top + y) * read.wide + left + x] = picture.colours[number as usize];
                    }
                }
            }
            all.push(canvas.clone());
        }
        all
    }

    /// Somewhere to write that can be read afterwards, and made to fail.
    #[derive(Clone, Default)]
    struct Tape {
        bytes: Arc<Mutex<Vec<u8>>>,
        /// refuse everything past this many bytes, as a full disk does
        room: Option<usize>,
        /// wait, on every write, until this is set
        held_until: Option<Arc<(Mutex<bool>, std::sync::Condvar)>>,
        /// set once a write is waiting
        held: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Write for Tape {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(gate) = &self.held_until {
                self.held.store(true, Ordering::Relaxed);
                let mut open = gate.0.lock().unwrap();
                while !*open {
                    open = gate.1.wait(open).unwrap();
                }
            }
            let mut held = self.bytes.lock().unwrap();
            if self.room.is_some_and(|room| held.len() + bytes.len() > room) {
                // https://doc.rust-lang.org/std/io/enum.ErrorKind.html#variant.StorageFull
                return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "no space left on the disk"));
            }
            held.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Tape {
        fn bytes(&self) -> Vec<u8> {
            self.bytes.lock().unwrap().clone()
        }
    }

    /// A picture `wide` by `tall` whose pixel at (x, y) is `colour(x, y)`.
    fn picture(wide: usize, tall: usize, mut colour: impl FnMut(usize, usize) -> [u8; 3]) -> Vec<[u8; 4]> {
        (0..wide * tall).map(|at| colour(at % wide, at / wide)).map(|[r, g, b]| [r, g, b, 255]).collect()
    }

    fn solid(pixels: &[[u8; 4]]) -> Vec<[u8; 3]> {
        pixels.iter().map(|p| [p[0], p[1], p[2]]).collect()
    }

    /// Numbers that look random and are the same every time.
    /// https://en.wikipedia.org/wiki/Linear_congruential_generator
    fn noise(seed: &mut u64) -> u32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*seed >> 33) as u32
    }

    // ------------------------------------------------------------ compression

    #[test]
    fn the_formats_own_sample_picture_compresses_to_its_known_bytes() {
        // The 10 by 10 picture of four colours that explanations of the
        // format use, and the bytes it is known to compress to:
        // https://www.matthewflickinger.com/lab/whatsinagif/bits_and_bytes.asp
        let row = |a: [u8; 2], middle: bool| -> Vec<u8> { (0..10).map(|x| if middle && (3..7).contains(&x) { 0 } else if x < 5 { a[0] } else { a[1] }).collect() };
        let pixels: Vec<u8> = (0..10).flat_map(|y| row(if y < 5 { [1, 2] } else { [2, 1] }, (3..7).contains(&y))).collect();
        let mut out = Vec::new();
        lzw(2, &pixels, &mut out);
        let known: [u8; 25] = [0x02, 0x16, 0x8C, 0x2D, 0x99, 0x87, 0x2A, 0x1C, 0xDC, 0x33, 0xA0, 0x02, 0x75, 0xEC, 0x95, 0xFA, 0xA8, 0xDE, 0x60, 0x8C, 0x04, 0x91, 0x4C, 0x01, 0x00];
        assert_eq!(out, known);
    }

    /// The contents of the blocks `lzw` made, joined up.
    fn joined(blocks: &[u8]) -> (u8, Vec<u8>) {
        let (mut at, mut all) = (1, Vec::new());
        while blocks[at] != 0 {
            all.extend_from_slice(&blocks[at + 1..at + 1 + blocks[at] as usize]);
            at += 1 + blocks[at] as usize;
        }
        assert_eq!(at + 1, blocks.len());
        (blocks[0], all)
    }

    #[test]
    fn what_is_compressed_reads_back_as_it_was() {
        let mut seed = 7;
        // every length up to where the codes first grow wider, so that a
        // picture ending exactly there is among them; then long ones, plain
        // and noisy, that fill the table of strings and start it again
        let mut tried: Vec<Vec<u8>> = (0..600).map(|length| (0..length).map(|_| (noise(&mut seed) % 256) as u8).collect()).collect();
        tried.push(vec![9; 100_000]);
        tried.push((0..100_000).map(|_| (noise(&mut seed) % 256) as u8).collect());
        tried.push((0..100_000).map(|at| if at / 50 % 2 == 0 { 255 } else { (noise(&mut seed) % 7) as u8 }).collect());
        for pixels in tried {
            let mut out = Vec::new();
            lzw(8, &pixels, &mut out);
            let (bits, data) = joined(&out);
            assert_eq!(unlzw(bits, &data), pixels, "a picture of {} pixels", pixels.len());
        }
        // And of fewer colours, where the codes start narrower, at every
        // short length several times over. A picture that ends just as the
        // codes are due to widen has its end code written wider, and a
        // reader notices one written too narrow only when it then falls at
        // the very end of a byte. Noise of 256 colours reaches that point
        // at one length, with the end code always at the same place in its
        // byte; these reach it at many lengths and places.
        for bits in 2..8u8 {
            let lengths = (0..400).flat_map(|length| [length; 5]).chain([20_000]);
            for length in lengths {
                let pixels: Vec<u8> = (0..length).map(|_| (noise(&mut seed) % (1 << bits)) as u8).collect();
                let mut out = Vec::new();
                lzw(bits, &pixels, &mut out);
                let (bits, data) = joined(&out);
                assert_eq!(unlzw(bits, &data), pixels, "a picture of {length} pixels of {bits} bits");
            }
        }
    }

    // ------------------------------------------------------------ the file

    /// Four pictures of a window 12 by 8, of few colours: the second
    /// changes two pixels some way apart, the third is the second again,
    /// the fourth changes one corner.
    fn four() -> Vec<Vec<[u8; 4]>> {
        let first = picture(12, 8, |x, y| [(x * 20) as u8, (y * 30) as u8, 90]);
        let mut second = first.clone();
        second[2 * 12 + 3] = [255, 0, 0, 255];
        second[3 * 12 + 5] = [0, 255, 0, 255];
        let mut fourth = second.clone();
        fourth[7 * 12 + 11] = [0, 0, 255, 255];
        vec![first, second.clone(), second, fourth]
    }

    #[test]
    fn a_recording_reads_back_as_the_pictures_given_with_their_times_and_only_what_changed() {
        let tape = Tape::default();
        let mut film = Film::new(tape.clone(), 1, 5);
        let pictures = four();
        // taken at uneven times, as frames come
        for (pixels, at_ms) in pictures.iter().zip([1000.0, 1050.0, 1100.0, 1250.0]) {
            film.add(12, 8, pixels, at_ms).unwrap();
        }
        let totals = film.finish(1400.0).unwrap();
        let file = tape.bytes();
        let read = read(&file);
        assert!(read.ended, "the file has no last byte");
        assert_eq!((read.wide, read.tall), (12, 8));
        assert_eq!(read.plays, Some(0), "the file does not say to play for ever");
        // the third picture, the same as the second, is not in the file:
        // the second is shown through its time as well
        assert_eq!(read.pictures.iter().map(|p| p.hundredths).collect::<Vec<_>>(), [5, 20, 15]);
        // (the colours were fitted to the first picture, and again for each that brought a colour it had not)
        assert_eq!(totals, Totals { pictures: 3, hundredths: 40, bytes: file.len() as u64, fitted: 3 });
        // only the rectangle that changed, and in it only the pixels that did
        assert_eq!(read.pictures.iter().map(|p| p.place).collect::<Vec<_>>(), [[0, 0, 12, 8], [3, 2, 3, 2], [11, 7, 1, 1]]);
        assert_eq!(read.pictures[0].leaves, None);
        let second = &read.pictures[1];
        assert_eq!(second.numbers.iter().filter(|&&n| Some(n) != second.leaves).count(), 2);
        // and shown one over another they are the pictures given, to the colour
        let shown = shown(&read);
        for (shown, given) in shown.iter().zip([&pictures[0], &pictures[1], &pictures[3]]) {
            assert_eq!(shown, &solid(given));
        }
    }

    #[test]
    fn a_pixel_that_did_not_change_keeps_its_colour_when_the_colours_are_fitted_again() {
        // a picture of more colours than a table holds (4096), so that each pixel is near its colour and not at it
        let first = picture(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, 90]);
        // then two blocks of strong colours it has none of, in opposite corners
        let mut second = first.clone();
        for y in 0..64 {
            for x in 0..64 {
                if (x < 12 && y < 12) || (x >= 52 && y >= 52) {
                    second[y * 64 + x] = [255, (x * 4) as u8, 255 - (y * 4) as u8, 255];
                }
            }
        }
        let tape = Tape::default();
        let mut film = Film::new(tape.clone(), 1, 5);
        film.add(64, 64, &first, 0.0).unwrap();
        film.add(64, 64, &second, 50.0).unwrap();
        film.finish(100.0).unwrap();
        let read = read(&tape.bytes());
        assert!(read.pictures[1].own_colours, "the table was not fitted again for colours it had none of");
        assert_eq!(read.pictures[1].place, [0, 0, 64, 64]);
        let shown = shown(&read);
        let (mut same, mut worst) = (0, 0);
        for at in 0..64 * 64 {
            if first[at] == second[at] {
                assert_eq!(shown[0][at], shown[1][at], "pixel {at} did not change and is shown in another colour");
                same += 1;
            } else {
                worst = worst.max((0..3).map(|part| (shown[1][at][part] as i32 - second[at][part] as i32).abs()).max().unwrap());
            }
        }
        assert_eq!(same, 64 * 64 - 2 * 144);
        assert!(worst <= 16, "a changed pixel is {worst} levels from its colour");
    }

    #[test]
    fn only_the_pixels_that_changed_are_written_when_the_colours_are_kept() {
        // a picture of more colours than a table holds, and a second that
        // moves two of its own colours to pixels some way apart: nothing
        // the table does not serve already, so it is kept
        let first = picture(64, 64, |x, y| [(x * 4) as u8, (y * 4) as u8, 90]);
        let mut second = first.clone();
        second[10 * 64 + 10] = first[40 * 64 + 50];
        second[30 * 64 + 20] = first[5 * 64 + 60];
        let tape = Tape::default();
        let mut film = Film::new(tape.clone(), 1, 5);
        film.add(64, 64, &first, 0.0).unwrap();
        film.add(64, 64, &second, 50.0).unwrap();
        assert_eq!(film.finish(100.0).unwrap().fitted, 1);
        let read = read(&tape.bytes());
        let changed = &read.pictures[1];
        assert!(!changed.own_colours);
        assert_eq!(changed.place, [10, 10, 11, 21]);
        // of the 231 pixels of the rectangle, the two that changed and no other
        assert_eq!(changed.numbers.iter().filter(|&&n| Some(n) != changed.leaves).count(), 2);
        let shown = shown(&read);
        assert_eq!((0..64 * 64).filter(|&at| shown[0][at] != shown[1][at]).collect::<Vec<_>>(), [10 * 64 + 10, 30 * 64 + 20]);
    }

    #[test]
    fn a_few_pixels_of_strong_colours_keep_them_beside_a_floor_of_many_shades() {
        // a floor of 2000 shades, 50 pixels of each, and eight strong colours of 20 pixels each, as dots and arrows are
        let mut picture: Vec<Rgb> = Vec::new();
        for shade in 0..2000u32 {
            let colour = (shade % 20 * 8 + 4) << 16 | (shade / 20 % 20 * 8 + 4) << 8 | (shade / 400 * 8 + 4);
            picture.extend(std::iter::repeat(colour).take(50));
        }
        let strong: [Rgb; 8] = [0xFF2020, 0x20FF20, 0x2020FF, 0xFFDC3C, 0x46E6F0, 0xFF8C28, 0xFAFAFA, 0xE040E0];
        for colour in strong {
            picture.extend(std::iter::repeat(colour).take(20));
        }
        let mut palette = fit(&picture);
        for colour in strong {
            assert_eq!(palette.colours[palette.nearest(colour) as u8 as usize], parts(colour).map(|part| part as u8), "{colour:#08x} is shown as another colour");
        }
    }

    #[test]
    fn colours_are_fitted_exactly_where_there_are_few_and_flat_ones_stay_flat() {
        // a wall of one colour beside a gradient of many: the wall keeps its own colour
        let wall: Rgb = 43 << 16 | 48 << 8 | 59;
        let mut seed = 11;
        let picture: Vec<Rgb> = (0..40_000u32).map(|at| if at % 200 < 120 { wall } else { noise(&mut seed) & 0xFF_FFFF }).collect();
        let mut palette = fit(&picture);
        assert_eq!(palette.used, 255);
        assert_eq!(palette.colours[palette.nearest(wall) as u8 as usize], [43, 48, 59]);
        // and a picture of 200 colours has every one of them
        let few: Vec<Rgb> = (0..200u32).map(|n| (n % 6 * 50) << 16 | (n / 6 % 6 * 50) << 8 | (n / 36 * 40)).collect();
        let mut palette = fit(&few);
        assert_eq!(palette.used, 200);
        for colour in few {
            assert_eq!(palette.colours[palette.nearest(colour) as u8 as usize], parts(colour).map(|part| part as u8));
        }
    }

    #[test]
    fn a_smaller_picture_is_the_rounded_average_of_whole_squares() {
        // 5 by 4, halved: the fifth column is dropped
        let pixels = picture(5, 4, |x, y| [(x + 10 * y) as u8, 255, if (x + y) % 2 == 0 { 0 } else { 3 }]);
        let mut small = Vec::new();
        assert_eq!(shrink(5, 4, &pixels, 2, &mut small), (2, 2));
        // red: (0 + 1 + 10 + 11) / 4 = 5.5, rounded up; blue: 6 / 4 = 1.5, rounded up
        assert_eq!(small, [6 << 16 | 255 << 8 | 2, 8 << 16 | 255 << 8 | 2, 26 << 16 | 255 << 8 | 2, 28 << 16 | 255 << 8 | 2]);
        // by three: one pixel, the average of nine
        let pixels = picture(3, 3, |x, y| [(x + 3 * y) as u8 * 10, 0, 7]);
        assert_eq!(shrink(3, 3, &pixels, 3, &mut small), (1, 1));
        assert_eq!(small, [40 << 16 | 7]);
        // and at full size nothing is averaged
        assert_eq!(shrink(3, 3, &pixels, 1, &mut small), (3, 3));
        assert_eq!(small[4], 40 << 16 | 7);
    }

    // ------------------------------------------------------------ time

    #[test]
    fn pictures_are_taken_by_the_clock_whatever_the_rate_of_frames() {
        // 20 a second, for one second of frames at each of three rates
        let taken = |frame_ms: f64| -> Vec<f64> {
            let mut pace = Pace::new(50.0, 0.0);
            (0..).map(|frame| frame as f64 * frame_ms).take_while(|at| *at < 999.0).filter(|at| pace.due(*at)).collect()
        };
        assert_eq!(taken(1000.0 / 60.0).len(), 20, "at 60 frames a second");
        assert_eq!(taken(1000.0 / 144.0).len(), 20, "at 144 frames a second");
        // frames slower than the rate: each is taken, and none twice to make up
        assert_eq!(taken(180.0), [0.0, 180.0, 360.0, 540.0, 720.0, 900.0]);
        // one slow frame among quick ones costs the pictures it covered and moves no other
        let mut pace = Pace::new(50.0, 0.0);
        let frames = [0.0, 16.0, 33.0, 50.0, 400.0, 416.0, 433.0, 450.0, 466.0, 483.0, 500.0];
        assert_eq!(frames.into_iter().filter(|at| pace.due(*at)).collect::<Vec<_>>(), [0.0, 50.0, 400.0, 450.0, 500.0]);
    }

    #[test]
    fn a_recording_plays_for_as_long_as_it_took_however_late_its_pictures_came() {
        // pictures that all differ, taken when slow frames let them be
        let tape = Tape::default();
        let mut film = Film::new(tape.clone(), 1, 5);
        let times = [0.0, 52.0, 97.0, 431.0, 449.0, 1003.0];
        for (n, at_ms) in times.into_iter().enumerate() {
            film.add(4, 4, &picture(4, 4, |x, y| [(n * 40) as u8, x as u8, y as u8]), at_ms).unwrap();
        }
        let totals = film.finish(1100.0).unwrap();
        let read = read(&tape.bytes());
        let shown: Vec<u16> = read.pictures.iter().map(|p| p.hundredths).collect();
        // each for the time until the next, to the hundredth, with no rounding carried over
        assert_eq!(shown, [5, 5, 33, 2, 55, 10]);
        assert_eq!(shown.iter().map(|&h| h as u64).sum::<u64>(), 110);
        assert_eq!(totals.hundredths, 110);
    }

    #[test]
    fn a_recording_is_named_by_the_date_and_time() {
        assert_eq!(name_at(1_700_000_000), "brickloft-2023-11-14-221320.gif");
        // the last day of February in a leap year, and the first second of a year
        assert_eq!(name_at(1_709_164_800 + 86_399), "brickloft-2024-02-29-235959.gif");
        assert_eq!(name_at(1_767_225_600), "brickloft-2026-01-01-000000.gif");
        assert_eq!(name_at(0), "brickloft-1970-01-01-000000.gif");
    }

    // ------------------------------------------------------------ bounds, and what goes wrong

    /// Pictures of a window 32 by 32 that are all different, and noisy
    /// enough not to compress to nothing.
    fn busy(n: usize) -> Vec<[u8; 4]> {
        let mut seed = n as u64 + 1;
        picture(32, 32, |_, _| [(noise(&mut seed) % 256) as u8, (noise(&mut seed) % 256) as u8, 0])
    }

    /// Run the second thread's work on pictures taken at the times given.
    fn worked(tape: &Tape, limits: Limits, times: &[f64], stopped_ms: Option<f64>) -> Done {
        let (pictures, taken) = sync_channel(times.len());
        let shared = Shared::default();
        for (n, at_ms) in times.iter().enumerate() {
            shared.waiting.fetch_add(1, Ordering::Relaxed);
            pictures.send(Taken { wide: 32, tall: 32, pixels: busy(n), at_ms: *at_ms }).unwrap();
        }
        *shared.stopped_ms.lock().unwrap() = stopped_ms;
        drop(pictures);
        work(Film::new(tape.clone(), 1, 5), taken, &shared, limits)
    }

    #[test]
    fn a_recording_stops_by_itself_at_its_longest() {
        let tape = Tape::default();
        let done = worked(&tape, Limits { ms: 1000.0, bytes: u64::MAX }, &[5000.0, 5400.0, 5800.0, 6200.0, 6600.0], None);
        assert_eq!(done.ended, Ended::Time);
        // the pictures taken within the second, played for the second exactly
        assert_eq!((done.totals.pictures, done.totals.hundredths), (3, 100));
        let read = read(&tape.bytes());
        assert!(read.ended);
        assert_eq!(read.pictures.iter().map(|p| p.hundredths).collect::<Vec<_>>(), [40, 40, 20]);
        // stopped by hand a moment past its longest, it is no longer than that either
        let done = worked(&Tape::default(), Limits { ms: 1000.0, bytes: u64::MAX }, &[0.0, 400.0], Some(1016.0));
        assert_eq!((done.ended, done.totals.hundredths), (Ended::Time, 100));
        // and stopped before, it ends when it was stopped
        let done = worked(&Tape::default(), Limits { ms: 1000.0, bytes: u64::MAX }, &[0.0, 400.0], Some(700.0));
        assert_eq!((done.ended, done.totals.hundredths), (Ended::Asked, 70));
    }

    #[test]
    fn a_recording_that_is_sent_nothing_more_still_ends_at_its_longest() {
        // the frame loop goes quiet without stopping the recording: the sender is kept, and nothing sent
        let tape = Tape::default();
        let (pictures, taken) = sync_channel(1);
        let shared = Arc::new(Shared::default());
        shared.waiting.fetch_add(1, Ordering::Relaxed);
        pictures.send(Taken { wide: 32, tall: 32, pixels: busy(0), at_ms: 0.0 }).unwrap();
        let (theirs, into) = (shared.clone(), tape.clone());
        let worker = std::thread::spawn(move || work(Film::new(into, 1, 5), taken, &theirs, Limits { ms: 200.0, bytes: u64::MAX }));
        let done = worker.join().unwrap();
        assert_eq!((done.ended, done.totals.pictures, done.totals.hundredths), (Ended::Time, 1, 20));
        assert!(read(&tape.bytes()).ended);
        drop(pictures);
    }

    #[test]
    fn a_recording_stops_by_itself_at_its_largest() {
        let tape = Tape::default();
        let times: Vec<f64> = (0..40).map(|n| n as f64 * 50.0).collect();
        let done = worked(&tape, Limits { ms: f64::MAX, bytes: 6000 }, &times, None);
        assert_eq!(done.ended, Ended::Size);
        let file = tape.bytes();
        // (a picture of 32 by 32 of noise is about 1300 bytes, and 800 more with colours of its own: it stops within one picture of the bound)
        assert!(done.totals.pictures < 10 && (6000..8200).contains(&file.len()), "{} pictures, {} bytes", done.totals.pictures, file.len());
        let read = read(&file);
        assert!(read.ended && read.pictures.len() == done.totals.pictures as usize);
    }

    #[test]
    fn a_disk_that_fills_ends_the_recording_with_the_reason_and_no_panic() {
        let tape = Tape { room: Some(3000), ..Tape::default() };
        let times: Vec<f64> = (0..10).map(|n| n as f64 * 50.0).collect();
        let done = worked(&tape, Limits::default(), &times, None);
        assert_eq!(done.ended, Ended::Failed("no space left on the disk".into()));
        // and through the editor's own hold on a recording it is said in words
        let mut controls = Controls::default();
        controls.start_into(Tape { room: Some(3000), ..Tape::default() }, PathBuf::from("the-recording.gif"), true, 0.0);
        for n in 0..10 {
            controls.give(32, 32, || busy(n), n as f64 * 50.0);
            // (given no faster than they are worked on, so that none is skipped)
            while controls.running.as_ref().is_some_and(|r| r.shared.waiting.load(Ordering::Relaxed) > 0 && !r.worker.is_finished()) {
                std::thread::yield_now();
            }
        }
        let Some(Said::Failed(said)) = controls.wait(500.0) else { panic!("a recording that could not be written was not said to have failed") };
        assert!(said.contains("no space left on the disk") && said.contains("the-recording.gif"), "{said}");
    }

    #[test]
    fn a_folder_that_cannot_be_written_to_is_said_and_nothing_is_started() {
        let nowhere = std::env::temp_dir().join(format!("brickloft-gif-test-{}", std::process::id())).join("not").join("made");
        let mut controls = Controls::default();
        let said = controls.start_in(&nowhere, true, 0.0).expect_err("a recording was started in a folder that is not there");
        assert!(said.starts_with("No recording: could not make") && said.contains("brickloft-") && !controls.recording(), "{said}");
    }

    #[test]
    fn pictures_are_skipped_and_never_waited_for_when_the_second_thread_falls_behind() {
        // the second thread is held at its first write, as by a disk that has stopped answering
        let gate = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let tape = Tape { held_until: Some(gate.clone()), ..Tape::default() };
        let mut controls = Controls::default();
        controls.start_into(tape.clone(), PathBuf::from("held.gif"), true, 0.0);
        let mut read_back = 0;
        controls.give(32, 32, || { read_back += 1; busy(0) }, 0.0);
        while !tape.held.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        let began = Instant::now();
        for n in 1..50 {
            controls.give(32, 32, || { read_back += 1; busy(n) }, n as f64 * 50.0);
        }
        let took = began.elapsed();
        let running = controls.running.as_ref().unwrap();
        let counted = (running.given, running.skipped, read_back);
        // (let go before anything is held to, so that a failure here does not leave the thread waiting for ever)
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        assert!(took < Duration::from_secs(2), "handing pictures over waited for the second thread");
        // one being worked on and the queue full: everything after is skipped, without being read
        assert_eq!(counted, (QUEUE as u32 + 1, 50 - QUEUE as u32 - 1, QUEUE + 1));
        let Some(Said::Done(said)) = controls.wait(2500.0) else { panic!("the recording was not finished") };
        // the last picture written is shown for all the time of those skipped after it
        let read = read(&tape.bytes());
        assert!(read.ended, "{said}");
        assert_eq!(read.pictures.iter().map(|p| p.hundredths).collect::<Vec<_>>(), [5, 5, 240]);
    }

    #[test]
    fn a_recording_is_made_in_a_folder_and_closed_when_the_editor_is() {
        let folder = std::env::temp_dir().join(format!("brickloft-gif-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let mut controls = Controls::default();
        (controls.size, controls.rate) = (1, 1);
        let said = controls.start_in(&folder, false, 100.0).unwrap();
        let path = PathBuf::from(said.strip_prefix("Recording to ").expect(&said));
        assert_eq!(path.parent(), Some(folder.as_path()));
        // one recording at a time
        assert!(controls.start_in(&folder, false, 100.0).is_err_and(|e| e.contains("already")));
        // pictures are taken for the view alone, as asked, when they are due
        assert!(!controls.wants(true, 100.0) && controls.wants(false, 100.0));
        controls.give(32, 32, || busy(0), 100.0);
        assert!(!controls.wants(false, 120.0) && controls.wants(false, 150.0));
        controls.give(32, 32, || busy(1), 150.0);
        assert_eq!(controls.status(1100.0).map(|s| (s.seconds, s.pictures)), Some((1.0, 2)));
        // the window minimised has no size: nothing is taken, and the recording goes on
        assert!(controls.wants(false, 200.0));
        controls.give(0, 0, || panic!("a window of no size was read back"), 200.0);
        assert!(controls.recording());
        // the window made larger: a GIF is one size, so the recording ends
        assert!(controls.wants(false, 250.0));
        controls.give(40, 32, || busy(2), 250.0);
        assert!(!controls.recording() && controls.status(250.0).is_none());
        let Some(Said::Done(said)) = controls.wait(250.0) else { panic!("nothing was said of the recording") };
        assert!(said.starts_with("Recording stopped because the window changed size") && said.contains(&path.display().to_string()), "{said}");
        let read_back = read(&std::fs::read(&path).unwrap());
        // at half size, and for as long as it ran
        assert_eq!((read_back.wide, read_back.tall, read_back.pictures.len(), read_back.ended), (16, 16, 2, true));
        assert_eq!(read_back.pictures.iter().map(|p| p.hundredths).collect::<Vec<_>>(), [5, 10]);

        // a recording still running when the editor closes is finished as it closes
        let said = controls.start_in(&folder, true, 0.0).unwrap();
        let path = PathBuf::from(said.strip_prefix("Recording to ").unwrap());
        controls.give(32, 32, || busy(0), 0.0);
        controls.give(32, 32, || busy(1), 50.0);
        drop(controls);
        let read_back = read(&std::fs::read(&path).unwrap());
        assert!(read_back.ended && read_back.pictures.len() == 2);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_recording_ends_itself_in_the_frame_loop_at_its_longest_and_one_with_no_pictures_leaves_no_file() {
        let folder = std::env::temp_dir().join(format!("brickloft-gif-test-b-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let mut controls = Controls::bounded(Limits { ms: 300.0, bytes: u64::MAX });
        controls.start_in(&folder, true, 0.0).unwrap();
        controls.give(32, 32, || busy(0), 0.0);
        controls.give(32, 32, || busy(1), 150.0);
        assert_eq!(controls.poll(200.0), None);
        // past its longest: stopped, and said once the file is finished
        let mut said = controls.poll(310.0);
        assert!(!controls.recording());
        while said.is_none() {
            std::thread::yield_now();
            said = controls.poll(320.0);
        }
        let Some(Said::Done(said)) = said else { panic!() };
        assert!(said.starts_with("Recording stopped at 0 seconds, the longest it may be, and saved to") && said.contains("(0.3 seconds, 2 pictures"), "{said}");
        // started and stopped with nothing drawn between
        let said = controls.start_in(&folder, true, 1000.0).unwrap();
        let path = PathBuf::from(said.strip_prefix("Recording to ").unwrap());
        assert_eq!(controls.wait(1500.0), Some(Said::Done("Nothing was recorded: the window was not drawn while the recording ran".into())));
        assert!(!path.exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
