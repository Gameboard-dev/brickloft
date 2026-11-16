//! A mesh as TripoSR gives it, tidied: loose bits dropped, stood level,
//! set on the ground, and turned to the way up that a .glb file means.
//! Before that, the field the mesh is drawn from is cleared of scraps
//! (`clear_scraps`).
//!
//! **Why a mesh comes out tilted.** TripoSR takes every picture to have
//! been taken from level with the object. Given one taken from above, it
//! builds what a level camera would have had to be looking at: the object
//! tipped towards the camera by the angle the camera was raised. The tip is
//! therefore always about the one axis that runs across the picture, which
//! in the model's frame (Z up, the camera out along +X) is Y, and always
//! towards the camera, since no picture is taken from under the ground.
//! That makes levelling a search over one angle, one way, and not over all
//! the ways a mesh can be turned.
//!
//! **How the angle is found.** Most things stand on something flat: a
//! plinth, a floor slab, four wheels. For each angle the mesh could be
//! tipped back by, count the vertices that would then lie within a hair of
//! the lowest point. At the angle that makes the base level, the whole of
//! the base is at the bottom and the count jumps; at any other, only an
//! edge of it is. The mesh is levelled when that jump is clear (`Tilt`),
//! and left alone and reported when it is not, as for a ball or a boat's
//! hull, where there is no base to find.
//!
//! The principal axes of the vertices were the other way considered
//! (https://en.wikipedia.org/wiki/Principal_component_analysis). They say
//! which way a thing is long, which for a tall statue is up and for a car
//! is along the ground, and for a cottage or a barrel is nearly nothing;
//! and a dragon's long axis is wherever its neck goes. What a thing rests
//! on is the same question for all of them.

/// A triangle mesh with a colour at each vertex.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub points: Vec<[f32; 3]>,
    /// Red, green and blue from 0 to 1, as they are to be shown.
    pub colours: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    /// From the flat lists the model's code gives. `None` if they do not
    /// make a mesh: a count that does not divide, a number that is not
    /// finite, a triangle naming a point that is not there. The lists come
    /// from arithmetic on a graphics card and are checked like any input.
    pub fn from_flat(points: &[f32], triangles: &[i64], colours: &[f32]) -> Option<Mesh> {
        if points.len() % 3 != 0 || triangles.len() % 3 != 0 || colours.len() != points.len() || points.len() / 3 > u32::MAX as usize {
            return None;
        }
        if !points.iter().chain(colours).all(|v| v.is_finite()) {
            return None;
        }
        let count = (points.len() / 3) as i64;
        if !triangles.iter().all(|&i| (0..count).contains(&i)) {
            return None;
        }
        let threes = |flat: &[f32]| flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect::<Vec<_>>();
        Some(Mesh { points: threes(points), colours: threes(colours), triangles: triangles.chunks_exact(3).map(|t| [t[0] as u32, t[1] as u32, t[2] as u32]).collect() })
    }

    /// The lowest and highest corner of the box around the mesh.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &self.points {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
        (lo, hi)
    }

    /// The longest side of the box around the mesh.
    pub fn size(&self) -> f32 {
        let (lo, hi) = self.bounds();
        (0..3).map(|a| hi[a] - lo[a]).fold(0.0, f32::max)
    }

    /// Drops the pieces that are small beside the largest: the crumbs the
    /// model leaves floating near an object. Returns how many were dropped.
    /// A piece is kept if it has at least `LOOSE` of the largest's
    /// triangles, so an object that really is in two parts keeps both.
    pub fn drop_loose_pieces(&mut self) -> usize {
        // which piece each point is in: union-find over the triangles' corners
        let mut parent: Vec<u32> = (0..self.points.len() as u32).collect();
        fn find(parent: &mut [u32], mut i: u32) -> u32 {
            while parent[i as usize] != i {
                parent[i as usize] = parent[parent[i as usize] as usize];
                i = parent[i as usize];
            }
            i
        }
        for t in &self.triangles {
            let a = find(&mut parent, t[0]);
            for &corner in &t[1..] {
                let b = find(&mut parent, corner);
                if a != b {
                    parent[b as usize] = a;
                }
            }
        }
        let mut triangles_in = vec![0usize; self.points.len()];
        for t in &self.triangles {
            triangles_in[find(&mut parent, t[0]) as usize] += 1;
        }
        let largest = triangles_in.iter().copied().max().unwrap_or(0);
        let least = ((largest as f32 * LOOSE).ceil() as usize).max(1);
        let dropped = triangles_in.iter().filter(|&&n| n > 0 && n < least).count();
        if dropped == 0 {
            return 0;
        }
        let mut new_index = vec![u32::MAX; self.points.len()];
        let (mut points, mut colours) = (Vec::new(), Vec::new());
        for i in 0..self.points.len() {
            let root = find(&mut parent, i as u32) as usize;
            if triangles_in[root] >= least {
                new_index[i] = points.len() as u32;
                points.push(self.points[i]);
                colours.push(self.colours[i]);
            }
        }
        self.triangles = self.triangles.iter().filter(|t| new_index[t[0] as usize] != u32::MAX).map(|t| t.map(|i| new_index[i as usize])).collect();
        self.points = points;
        self.colours = colours;
        dropped
    }

    /// Tips the mesh towards the camera (+X) by `degrees`, turning it about
    /// the Y axis: what a picture taken from that far above level does to
    /// it. A negative angle tips it back.
    pub fn tip(&mut self, degrees: f32) {
        let (s, c) = degrees.to_radians().sin_cos();
        for p in &mut self.points {
            *p = [p[0] * c + p[2] * s, p[1], p[2] * c - p[0] * s];
        }
    }

    /// Moves the mesh so that its lowest point is at height 0 and it is centred over the origin. Z is up.
    pub fn set_on_ground(&mut self) {
        let (lo, hi) = self.bounds();
        let by = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]];
        for p in &mut self.points {
            *p = [p[0] - by[0], p[1] - by[1], p[2] - by[2]];
        }
    }

    /// From the model's frame (Z up, front towards +X) to glTF's (Y up,
    /// front towards +Z). A turn and not a mirror: the three axes move round
    /// one place, so the triangles keep their facing.
    /// https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#coordinate-system-and-units
    pub fn to_y_up(&mut self) {
        for p in &mut self.points {
            *p = [p[1], p[2], p[0]];
        }
    }
}

/// A second body at least this share of the main one is part of the object and is kept.
const SECOND_BODY: f32 = 0.1;

/// Clears scraps out of the field the surface is drawn from, before it is
/// drawn: the floating bits of ground the model leaves near an object, and
/// those that hang on to it by a thread. Returns how many cells were cleared.
///
/// `density` is a cube of `side` cells a side; a cell is solid where its
/// density is above `surface`. The solid is shrunk by one cell, which
/// removes threads and leaves each body's core. The largest core is the
/// object. Every solid cell is then given to the core it is nearest to,
/// going through solid cells only, and the cells that fall to a small core,
/// or to none, are emptied. So a scrap joined to the object by a thread is
/// cut off at the middle of the thread, while a thin wall of the object
/// itself (a boat's side, a wing) stays, since no other core is nearer to it
/// than the object's own. This is why it is not a plain opening (shrink,
/// then grow back by as much), which would take the thin walls too.
///
/// Done on the field and not on the mesh because here "joined by a thread"
/// is a count of cells; on triangles it would take finding the narrow places
/// of a surface. Dropping pieces that are not joined at all is done on the
/// mesh as well (`drop_loose_pieces`), for what this leaves.
pub fn clear_scraps(density: &mut [f32], side: usize, surface: f32) -> usize {
    if side < 3 || density.len() != side * side * side {
        return 0;
    }
    let solid: Vec<bool> = density.iter().map(|&d| d > surface).collect();
    let neighbours = |i: usize| {
        let (x, y, z) = (i % side, i / side % side, i / (side * side));
        [(x > 0).then(|| i - 1), (x + 1 < side).then(|| i + 1), (y > 0).then(|| i - side), (y + 1 < side).then(|| i + side), (z > 0).then(|| i - side * side), (z + 1 < side).then(|| i + side * side)].into_iter().flatten()
    };
    // cores: solid cells all six of whose neighbours are solid (the cube's own faces count as empty)
    let core: Vec<bool> = (0..solid.len()).map(|i| solid[i] && neighbours(i).filter(|&j| solid[j]).count() == 6).collect();
    // each core's cells labelled, and its size
    let mut owner = vec![0u32; solid.len()];
    let mut sizes = vec![0usize];
    let mut queue = std::collections::VecDeque::new();
    for start in 0..core.len() {
        if !core[start] || owner[start] != 0 {
            continue;
        }
        let label = sizes.len() as u32;
        owner[start] = label;
        queue.push_back(start);
        let mut size = 0;
        while let Some(i) = queue.pop_front() {
            size += 1;
            for j in neighbours(i) {
                if core[j] && owner[j] == 0 {
                    owner[j] = label;
                    queue.push_back(j);
                }
            }
        }
        sizes.push(size);
    }
    let Some(largest) = sizes.iter().copied().max().filter(|&s| s > 0) else {
        // nothing has a core: the whole object is thin, and there is nothing to tell scraps from it by
        return 0;
    };
    let kept: Vec<bool> = sizes.iter().map(|&s| s as f32 >= largest as f32 * SECOND_BODY).collect();
    // every solid cell to its nearest core: all the cores spread at once, a cell a step
    queue.extend((0..core.len()).filter(|&i| core[i]));
    while let Some(i) = queue.pop_front() {
        for j in neighbours(i) {
            if solid[j] && owner[j] == 0 {
                owner[j] = owner[i];
                queue.push_back(j);
            }
        }
    }
    let mut cleared = 0;
    for i in 0..solid.len() {
        // owner 0 is a solid cell no core reaches: `kept[0]` is false, there being no core 0
        if solid[i] && !kept[owner[i] as usize] {
            density[i] = 0.0;
            cleared += 1;
        }
    }
    cleared
}

/// A piece with fewer than this share of the largest piece's triangles is a crumb.
const LOOSE: f32 = 0.02;
/// The furthest tip towards the camera that is looked for, in degrees. A picture taken from higher than this shows
/// mostly the object's top, and little that a mesh could be made from.
const MOST_TILT: f32 = 65.0;
/// And the furthest the other way. A camera is not below the ground, so a mesh tipped away from it is the model's
/// own error, which is small. Looking no further this way is what makes the answer single: a box tipped by 30
/// degrees has a second flat face at 60 degrees the other way, which is not its base.
const MOST_BACK: f32 = 10.0;
/// A tilt smaller than this is level: the search's own error is about a degree.
const LEVEL_WITHIN: f32 = 2.5;
/// "Within a hair of the lowest point": this share of the mesh's size.
const HAIR: f32 = 0.012;
/// The base must hold at least this share of the vertices,
const BASE_LEAST: f32 = 0.01;
/// span at least this share of the mesh's width,
const BASE_SPAN: f32 = 0.3;
/// and hold this many times what the bottom holds at any angle well away from it.
const BASE_CLEAR: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tilt {
    /// The mesh stands on a flat base that is level already.
    Level,
    /// It stands on a flat base tipped towards the camera by this many degrees (away from it, if negative).
    Tipped(f32),
    /// No flat base was found, so there is nothing to tell by.
    Unknown,
}

/// The vertices within a hair of the bottom once a tip of `degrees` towards
/// the camera has been undone: how many, and how far they span across.
fn bottom(points: &[[f32; 3]], degrees: f32, hair: f32) -> (usize, f32) {
    let (s, c) = (-degrees).to_radians().sin_cos();
    let height = |p: &[f32; 3]| p[2] * c - p[0] * s;
    let lowest = points.iter().map(height).fold(f32::MAX, f32::min);
    let (mut count, mut left, mut right) = (0, f32::MAX, f32::MIN);
    for p in points {
        if height(p) - lowest <= hair {
            let across = p[0] * c + p[2] * s;
            count += 1;
            left = left.min(across);
            right = right.max(across);
        }
    }
    (count, (right - left).max(0.0))
}

/// Whether the mesh stands level, in the model's frame (Z up, camera on +X).
pub fn tilt(mesh: &Mesh) -> Tilt {
    if mesh.points.len() < 100 {
        return Tilt::Unknown;
    }
    let size = mesh.size();
    let hair = size * HAIR;
    // every half degree; a base of a hundred vertices across is level to within about that
    let angles: Vec<f32> = (0..=(2.0 * (MOST_TILT + MOST_BACK)) as i32).map(|i| i as f32 * 0.5 - MOST_BACK).collect();
    let found: Vec<(usize, f32)> = angles.iter().map(|&a| bottom(&mesh.points, a, hair)).collect();
    let fullest = (0..angles.len()).max_by_key(|&i| found[i].0).unwrap_or(0);
    // The count is near its most over a degree or so around the true angle; the middle of that stretch is a better
    // answer than whichever of its half-degree steps happens to be highest.
    let nearly = |i: usize| found[i].0 as f32 >= 0.9 * found[fullest].0 as f32;
    let first = (0..=fullest).rev().take_while(|&i| nearly(i)).last().unwrap_or(fullest);
    let last = (fullest..angles.len()).take_while(|&i| nearly(i)).last().unwrap_or(fullest);
    let best = (first + last) / 2;
    let (count, span) = found[best];
    // the width of the mesh across the picture's depth, at that angle
    let (s, c) = (-angles[best]).to_radians().sin_cos();
    let across: Vec<f32> = mesh.points.iter().map(|p| p[0] * c + p[2] * s).collect();
    let width = across.iter().copied().fold(f32::MIN, f32::max) - across.iter().copied().fold(f32::MAX, f32::min);
    let elsewhere = (0..angles.len()).filter(|&i| (angles[i] - angles[best]).abs() >= 8.0).map(|i| found[i].0).max().unwrap_or(0);
    let clear = count as f32 >= BASE_LEAST * mesh.points.len() as f32 && span >= BASE_SPAN * width && count as f32 >= BASE_CLEAR * elsewhere as f32;
    // a base found at the very end of the search is the search's limit and not a base
    if !clear || best == 0 || best + 1 == angles.len() {
        Tilt::Unknown
    } else if angles[best].abs() < LEVEL_WITHIN {
        Tilt::Level
    } else {
        Tilt::Tipped(angles[best])
    }
}

/// How far either side of the picture's own estimate `flattest` looks, in degrees, and the furthest it looks at all.
const FLATTEST_WITHIN: f32 = 25.0;
const MOST_LAID: f32 = 80.0;

/// For a mesh with no flat base, from a picture that looks taken from about
/// `seen_from` degrees above level: the tip towards the camera that, undone,
/// leaves the mesh lying lowest. `None` if there is no such angle near the
/// estimate.
///
/// A car or a boat seen from above comes out standing on its nose. Its
/// underside is the model's guess and is not flat, so `tilt` finds nothing;
/// but such a thing is long and low, and it lies lowest when it is level.
/// That is not true of things in general (a statue lies lowest on its
/// side), so this is used only when the picture itself says it was taken
/// from high up, only within `FLATTEST_WITHIN` of the height the picture
/// suggests, and only if the lowest lie is found inside that range and not
/// at its end, where the mesh would still be getting lower as it is laid
/// further over.
pub fn flattest(mesh: &Mesh, seen_from: f32) -> Option<f32> {
    if mesh.points.len() < 100 {
        return None;
    }
    let height = |degrees: f32| {
        let (s, c) = (-degrees).to_radians().sin_cos();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for p in &mesh.points {
            let h = p[2] * c - p[0] * s;
            lo = lo.min(h);
            hi = hi.max(h);
        }
        hi - lo
    };
    let (from, to) = ((seen_from - FLATTEST_WITHIN).max(0.0), (seen_from + FLATTEST_WITHIN).min(MOST_LAID));
    let angles: Vec<f32> = (0..=((to - from) * 2.0).max(0.0) as i32).map(|i| from + i as f32 * 0.5).collect();
    let heights: Vec<f32> = angles.iter().map(|&a| height(a)).collect();
    let lowest = (0..angles.len()).min_by(|&a, &b| heights[a].total_cmp(&heights[b]))?;
    // inside the range, and a real gain on how it stands now
    (lowest > 0 && lowest + 1 < angles.len() && angles[lowest] >= LEVEL_WITHIN && heights[lowest] <= 0.9 * height(0.0)).then(|| angles[lowest])
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A box of the given size standing on Z = 0, each face a grid of points `step` apart, in the model's frame.
    pub fn box_mesh(size: [f32; 3], step: f32) -> Mesh {
        let mut mesh = Mesh::default();
        // each face: an axis that is fixed, at its low or its high side, and the two axes that run across it
        for (fixed, u, v) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
            for side in [0.0, 1.0] {
                let (nu, nv) = ((size[u] / step).round().max(1.0) as usize, (size[v] / step).round().max(1.0) as usize);
                let first = mesh.points.len() as u32;
                for j in 0..=nv {
                    for i in 0..=nu {
                        let mut p = [0f32; 3];
                        p[fixed] = side * size[fixed];
                        p[u] = size[u] * i as f32 / nu as f32;
                        p[v] = size[v] * j as f32 / nv as f32;
                        mesh.points.push([p[0] - size[0] / 2.0, p[1] - size[1] / 2.0, p[2]]);
                        mesh.colours.push([0.8, 0.4, 0.2]);
                    }
                }
                for j in 0..nv as u32 {
                    for i in 0..nu as u32 {
                        let a = first + j * (nu as u32 + 1) + i;
                        let (b, c, d) = (a + 1, a + nu as u32 + 1, a + nu as u32 + 2);
                        mesh.triangles.push([a, b, d]);
                        mesh.triangles.push([a, d, c]);
                    }
                }
            }
        }
        mesh
    }

    /// The mesh as the model would give it for a picture taken from `degrees` above level: tipped towards +X.
    pub fn tipped(mut mesh: Mesh, degrees: f32) -> Mesh {
        mesh.tip(degrees);
        mesh
    }

    fn ball(n: usize) -> Mesh {
        let mut mesh = Mesh::default();
        for j in 0..=n {
            for i in 0..2 * n {
                let (lat, long) = (std::f32::consts::PI * j as f32 / n as f32, std::f32::consts::PI * i as f32 / n as f32);
                mesh.points.push([lat.sin() * long.cos(), lat.sin() * long.sin(), lat.cos()]);
                mesh.colours.push([0.5; 3]);
            }
        }
        mesh
    }

    #[test]
    fn a_tipped_box_is_found_and_stood_level() {
        for degrees in [-6.0f32, 8.0, 20.0, 35.0, 55.0] {
            let mut mesh = tipped(box_mesh([1.0, 0.8, 0.6], 0.02), degrees);
            match tilt(&mesh) {
                Tilt::Tipped(found) => {
                    assert!((found - degrees).abs() <= 1.0, "tipped by {degrees}, found {found}");
                    mesh.tip(-found);
                }
                other => panic!("tipped by {degrees}, found {other:?}"),
            }
            assert_eq!(tilt(&mesh), Tilt::Level, "after levelling a box tipped by {degrees}");
            // level means the box's own height again, to within the search's half degree
            let (lo, hi) = mesh.bounds();
            assert!((hi[2] - lo[2] - 0.6).abs() < 0.03, "{}", hi[2] - lo[2]);
        }
    }

    #[test]
    fn a_level_box_is_left_alone_and_a_ball_is_not_guessed_at() {
        assert_eq!(tilt(&box_mesh([1.0, 0.8, 0.6], 0.02)), Tilt::Level);
        assert_eq!(tilt(&tipped(box_mesh([1.0, 0.8, 0.6], 0.02), 1.0)), Tilt::Level, "a degree is within the search's error");
        assert_eq!(tilt(&ball(60)), Tilt::Unknown);
        assert_eq!(tilt(&Mesh::default()), Tilt::Unknown);
    }

    /// A ball squashed to the given half-widths along X, Y and Z. (Its points are turned first so that the two
    /// places where a ball's points crowd together are at its ends and not underneath it.)
    fn pebble(half: [f32; 3]) -> Mesh {
        let mut mesh = ball(60);
        for p in &mut mesh.points {
            *p = [p[2] * half[0], p[1] * half[1], p[0] * half[2]];
        }
        mesh
    }

    #[test]
    fn a_long_low_thing_with_no_base_is_laid_flat_and_a_tall_one_is_not_laid_down() {
        // long and low, like a car's body, tipped as from a picture taken from 50 degrees up
        let car = tipped(pebble([1.0, 0.45, 0.3]), 50.0);
        let found = flattest(&car, 45.0).expect("it lies lowest when level");
        assert!((found - 50.0).abs() <= 1.0, "{found}");
        // the picture's estimate may be off by a good deal
        assert!((flattest(&car, 62.0).unwrap() - 50.0).abs() <= 1.0);
        // but an estimate nowhere near finds nothing, and does not make something up
        assert_eq!(flattest(&car, 10.0), None);
        // tall, like a statue: laid further over it only gets lower, which is not standing level
        assert_eq!(flattest(&tipped(pebble([0.3, 0.3, 1.0]), 40.0), 40.0), None);
        // already level: nothing to do
        assert_eq!(flattest(&pebble([1.0, 0.45, 0.3]), 15.0), None);
    }

    #[test]
    fn a_table_stands_on_its_feet() {
        // four legs under a top: the base is four small patches far apart, which is still a base
        let mut table = box_mesh([1.0, 0.6, 0.05], 0.01);
        for p in &mut table.points {
            p[2] += 0.5;
        }
        for (x, y) in [(-0.45, -0.25), (0.45, -0.25), (-0.45, 0.25), (0.45, 0.25)] {
            let leg = box_mesh([0.06, 0.06, 0.5], 0.01);
            let first = table.points.len() as u32;
            table.points.extend(leg.points.iter().map(|p| [p[0] + x, p[1] + y, p[2]]));
            table.colours.extend(leg.colours);
            table.triangles.extend(leg.triangles.iter().map(|t| t.map(|i| i + first)));
        }
        assert_eq!(tilt(&table), Tilt::Level);
        match tilt(&tipped(table, 25.0)) {
            Tilt::Tipped(found) => assert!((found - 25.0).abs() <= 1.0, "{found}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn scraps_are_cleared_from_the_field_and_thin_walls_are_not() {
        let side = 40;
        let at = |x: usize, y: usize, z: usize| (z * side + y) * side + x;
        let mut field = vec![0f32; side * side * side];
        let mut fill = |x: std::ops::Range<usize>, y: std::ops::Range<usize>, z: std::ops::Range<usize>| {
            for zz in z {
                for yy in y.clone() {
                    for xx in x.clone() {
                        field[at(xx, yy, zz)] = 100.0;
                    }
                }
            }
        };
        fill(5..20, 5..20, 5..20); // the object
        fill(20..32, 10..11, 10..11); // a thread from it, one cell thick,
        fill(32..37, 8..13, 8..13); // to a scrap with a core of its own
        fill(8..10, 30..32, 30..32); // a speck floating free, too small to have a core
        fill(5..20, 5..20, 20..30); // and on top of the object ...
        for z in 20..30 {
            for y in 6..19 {
                for x in 6..19 {
                    field[at(x, y, z)] = 0.0; // ... hollowed out: walls one cell thick, a box without a lid
                }
            }
        }
        let cleared = clear_scraps(&mut field, side, 25.0);
        let solid = |x: usize, y: usize, z: usize| field[at(x, y, z)] > 25.0;
        assert!(solid(10, 10, 10), "the object stays");
        assert!(solid(5, 12, 28) && solid(19, 12, 29) && solid(12, 5, 25), "its thin walls stay, to their top");
        assert!(!solid(34, 10, 10) && !solid(32, 8, 8), "the scrap goes");
        assert!(!solid(9, 31, 31), "the speck goes");
        assert!(solid(21, 10, 10) && !solid(31, 10, 10), "the thread is cut between them: the object's end stays and the scrap's goes");
        assert_eq!(cleared, 5 * 5 * 5 + 2 * 2 * 2 + (20..32).filter(|&x| !solid(x, 10, 10)).count());
        assert!((4..=8).contains(&(20..32).filter(|&x| !solid(x, 10, 10)).count()), "cut about half way");

        // a second body of a size to be part of the object is kept, thread and all
        let mut two = vec![0f32; side * side * side];
        for (x0, x1) in [(5, 18), (24, 34)] {
            for z in 5..15 {
                for y in 5..15 {
                    for x in x0..x1 {
                        two[at(x, y, z)] = 100.0;
                    }
                }
            }
        }
        for x in 18..24 {
            two[at(x, 10, 10)] = 100.0;
        }
        assert_eq!(clear_scraps(&mut two, side, 25.0), 0);

        // a thing with no core at all is left as it is
        let mut sheet = vec![0f32; side * side * side];
        for y in 5..35 {
            for x in 5..35 {
                sheet[at(x, y, 20)] = 100.0;
            }
        }
        assert_eq!(clear_scraps(&mut sheet, side, 25.0), 0);
        assert_eq!(clear_scraps(&mut [0.0; 7], 2, 25.0), 0, "a list that is not a cube is not touched");
    }

    #[test]
    fn crumbs_are_dropped_and_a_second_part_is_kept() {
        let mut mesh = box_mesh([1.0, 1.0, 1.0], 0.1);
        let whole = mesh.triangles.len();
        // a crumb: one small box far off; and a second part a third the size
        for (size, step, at) in [(0.05, 0.05, 3.0), (0.6, 0.1, -3.0)] {
            let part = box_mesh([size; 3], step);
            let first = mesh.points.len() as u32;
            mesh.points.extend(part.points.iter().map(|p| [p[0] + at, p[1], p[2]]));
            mesh.colours.extend(part.colours);
            mesh.triangles.extend(part.triangles.iter().map(|t| t.map(|i| i + first)));
        }
        // each face of a box is a piece of its own here (the faces share no points), so: 6 + 6 + 6 pieces
        let second = 6 * 6 * 6 * 2;
        assert_eq!(mesh.drop_loose_pieces(), 6);
        assert_eq!(mesh.triangles.len(), whole + second);
        assert!(mesh.triangles.iter().flatten().all(|&i| (i as usize) < mesh.points.len()));
        assert_eq!(mesh.points.len(), mesh.colours.len());
        assert!(mesh.points.iter().all(|p| p[0] < 2.0), "the crumb's points are gone");
        assert_eq!(mesh.drop_loose_pieces(), 0);
    }

    #[test]
    fn the_way_up_is_turned_and_the_ground_is_at_nought() {
        let mut mesh = box_mesh([1.0, 0.8, 0.6], 0.2);
        for p in &mut mesh.points {
            *p = [p[0] + 5.0, p[1] - 2.0, p[2] + 7.0];
        }
        mesh.set_on_ground();
        let (lo, hi) = mesh.bounds();
        assert!(lo[2].abs() < 1e-5 && (lo[0] + hi[0]).abs() < 1e-5 && (lo[1] + hi[1]).abs() < 1e-5);
        // a triangle on the top face, facing up (+Z) before and up (+Y) after
        let facing = |m: &Mesh, t: [u32; 3]| {
            let [a, b, c] = t.map(|i| m.points[i as usize]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
        };
        let top = *mesh.triangles.iter().find(|t| t.iter().all(|&i| (mesh.points[i as usize][2] - 0.6).abs() < 1e-5)).unwrap();
        let before = facing(&mesh, top);
        mesh.to_y_up();
        let after = facing(&mesh, top);
        assert!(before[2].abs() > 0.0 && before[0] == 0.0 && before[1] == 0.0);
        assert_eq!(after, [0.0, before[2], 0.0], "what faced up along Z faces up along Y, the same way round");
        let (lo, hi) = mesh.bounds();
        assert!(((hi[1] - lo[1]) - 0.6).abs() < 1e-5 && ((hi[2] - lo[2]) - 1.0).abs() < 1e-5, "height is along Y and the front-to-back length along Z");
    }

    #[test]
    fn lists_that_are_not_a_mesh_are_refused() {
        let points = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let colours = [0.5; 9];
        assert!(Mesh::from_flat(&points, &[0, 1, 2], &colours).is_some());
        assert!(Mesh::from_flat(&points, &[0, 1, 3], &colours).is_none(), "a triangle naming a fourth point of three");
        assert!(Mesh::from_flat(&points, &[0, 1, -1], &colours).is_none());
        assert!(Mesh::from_flat(&points, &[0, 1], &colours).is_none());
        assert!(Mesh::from_flat(&points[..8], &[0, 1, 2], &colours[..8]).is_none());
        assert!(Mesh::from_flat(&points, &[0, 1, 2], &colours[..6]).is_none());
        let mut bad = points;
        bad[4] = f32::NAN;
        assert!(Mesh::from_flat(&bad, &[0, 1, 2], &colours).is_none());
        bad[4] = f32::INFINITY;
        assert!(Mesh::from_flat(&bad, &[0, 1, 2], &colours).is_none());
    }
}
