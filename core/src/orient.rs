//! Turn a shape pose (a world corner mask plus world extents) into the game's
//! asset, orientation and local size.

use crate::shapes::{shape_info, transform_mask, CellBrick, Kind, KINDS, ORIENT};

/// How the game's asset for each kind sits at orientation 16 (Z+, unrotated):
/// which corners of its box it fills. Indexed by `Kind as usize`.
///
/// The defaults have been confirmed in the game; `/arc calibrate` shows them
/// again if an update or a change of assets puts them in doubt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Models {
    pub local_mask: [u8; 9],
}

impl Default for Models {
    /// As confirmed in the game:
    /// - the micro wedge is solid on its local -x -y side, prism along local z
    /// - the triangle corner is the tetrahedron at local -x -y -z
    /// - the outer corner is the cube missing its +x +y +z corner
    fn default() -> Self {
        Models {
            // From the micro wedge corner (the hip) on, each is the pose
            // the game itself makes the brick in, which is also the pose
            // `shapes` calls canonical. The three before it were found by
            // looking in the game, and are the same as the game makes them.
            local_mask: [0xff, 0x77, 0x17, 0x7f, 0x1f, 0x07, 0x67, 0x47, 0x27],
        }
    }
}

/// A brick ready for the game. Positions are game units relative to the
/// centre of cell (0, 0, 0); sizes are half-extents in the brick's own axes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Placed {
    pub kind: Kind,
    pub size: [i32; 3],
    pub pos: [i32; 3],
    pub orientation: u8,
}

// upright orientations first, so symmetric shapes come out Z+ where possible
const ORDER: [usize; 24] = [
    16, 17, 18, 19, 20, 21, 22, 23, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];

/// For each world mask, the orientation that puts its kind's asset in that pose.
pub struct Solver {
    orientation: [Option<u8>; 256],
}

impl Solver {
    pub fn new(models: &Models) -> Self {
        let mut orientation = [None; 256];
        for kind in KINDS {
            // later entries of ORDER are tried first and overwritten by earlier ones
            for &o in ORDER.iter().rev() {
                let mask = transform_mask(models.local_mask[kind as usize], o);
                if shape_info(mask).is_some_and(|s| s.kind == kind) {
                    orientation[mask as usize] = Some(o as u8);
                }
            }
        }
        Solver { orientation }
    }

    /// `None` if the mask is not a shape, or the models cannot reach its pose.
    pub fn place(&self, brick: &CellBrick) -> Option<Placed> {
        let kind = shape_info(brick.mask)?.kind;
        let orientation = self.orientation[brick.mask as usize]?;
        // world extent i is the local size along perm[i]
        let perm = ORIENT[orientation as usize].perm;
        let mut size = [0; 3];
        for i in 0..3 {
            size[perm[i] as usize] = brick.ext[i];
        }
        // a cell is 2 units; cell 0 spans -1..1
        let pos = [0, 1, 2].map(|i| 2 * brick.min[i] + brick.ext[i] - 1);
        Some(Placed {
            kind,
            size,
            pos,
            orientation,
        })
    }
}

/// Integers per brick on the wire: mask, low corner x y z, extent x y z.
pub const WIRE_FIELDS: usize = 7;

/// Bricks as a flat JSON array of integers, [`WIRE_FIELDS`] per brick.
///
/// This is what the editor sends the plugin. Bricks cross as poses, not as
/// game assets and orientations, because which asset and orientation a pose
/// needs depends on the server's calibration ([`Models`]), which only the
/// plugin knows. A flat integer array needs no JSON library to write and is
/// the smallest thing to validate on the other side.
///
/// A brick's extent is in cells, as everywhere. Its low corner is in game
/// units, two to a cell: each brick is given with where its form stands in
/// the workspace, in units, and a form may stand a single unit off the cells
/// of another. The game places bricks to the unit, so forms are moved by the
/// unit; sent in cells, every form would have to sit on one shared lattice
/// of cells, and could only be moved two units at a time.
pub fn to_wire(bricks: &[(CellBrick, [i32; 3])]) -> String {
    let mut s = String::with_capacity(bricks.len() * 20 + 2);
    s.push('[');
    for (i, (b, place)) in bricks.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let low = [0, 1, 2].map(|k| 2 * b.min[k] + place[k]);
        s.push_str(&format!("{},{},{},{},{},{},{}", b.mask, low[0], low[1], low[2], b.ext[0], b.ext[1], b.ext[2]));
    }
    s.push(']');
    s
}

/// The reverse of [`to_wire`], for numbers that came from outside: every
/// field is range-checked, since the sender is a web page anyone with a
/// player's link can imitate.
///
/// Each brick comes back in cells, with the odd unit along each axis that
/// cells cannot hold (0 or 1), to be added to its position in units.
pub fn from_wire(ints: &[i64], max_ext: i32) -> Result<Vec<(CellBrick, [i32; 3])>, String> {
    if ints.len() % WIRE_FIELDS != 0 {
        return Err(format!("the brick list has {} numbers, which is not a multiple of {WIRE_FIELDS}", ints.len()));
    }
    const REACH: i64 = 400_000; // units from the anchor; far beyond any build
    ints.chunks_exact(WIRE_FIELDS)
        .enumerate()
        .map(|(i, f)| {
            let mask = u8::try_from(f[0]).ok().filter(|m| shape_info(*m).is_some());
            let mask = mask.ok_or_else(|| format!("brick {i}: {} is not a shape", f[0]))?;
            if f[1..4].iter().any(|v| v.abs() > REACH) {
                return Err(format!("brick {i}: position out of range"));
            }
            if f[4..7].iter().any(|v| *v < 1 || *v > max_ext as i64) {
                return Err(format!("brick {i}: size out of range 1..{max_ext}"));
            }
            let low = [f[1] as i32, f[2] as i32, f[3] as i32];
            // rounded down, so that the odd unit left over is never negative
            let brick = CellBrick { mask, min: low.map(|u| u.div_euclid(2)), ext: [f[4] as i32, f[5] as i32, f[6] as i32] };
            Ok((brick, low.map(|u| u.rem_euclid(2))))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::all_masks;

    #[test]
    fn every_pose_is_reachable_and_reproduces_its_mask() {
        let models = Models::default();
        let solver = Solver::new(&models);
        for mask in all_masks() {
            let brick = CellBrick {
                mask,
                min: [3, -2, 5],
                ext: [2, 3, 4],
            };
            let placed = solver
                .place(&brick)
                .unwrap_or_else(|| panic!("mask {mask:#x} unreachable"));
            let local = models.local_mask[placed.kind as usize];
            assert_eq!(transform_mask(local, placed.orientation as usize), mask);
            // the local size, carried back to the world, is the brick's extent
            let perm = ORIENT[placed.orientation as usize].perm;
            for i in 0..3 {
                assert_eq!(placed.size[perm[i] as usize], brick.ext[i]);
            }
            // cells 3..5 are units 5..9, centre 7
            assert_eq!(placed.pos, [7, -2, 13]);
        }
    }

    #[test]
    fn upright_is_preferred() {
        let solver = Solver::new(&Models::default());
        let cube = CellBrick {
            mask: 0xff,
            min: [0; 3],
            ext: [1, 2, 3],
        };
        assert_eq!(solver.place(&cube).unwrap().orientation, 16);
        assert_eq!(solver.place(&cube).unwrap().size, [1, 2, 3]);
    }

    #[test]
    fn wire_round_trips_and_rejects_bad_numbers() {
        let bricks = vec![
            CellBrick {
                mask: 0x17,
                min: [-4, 5, 6],
                ext: [1, 2, 3],
            },
            CellBrick {
                mask: 0xff,
                min: [0, 0, 0],
                ext: [80, 1, 1],
            },
        ];
        // the first form stands a unit on along x and three back along z:
        // half a cell that cells alone could not say
        let placed: Vec<(CellBrick, [i32; 3])> = vec![(bricks[0], [1, 0, -3]), (bricks[1], [0; 3])];
        let wire = to_wire(&placed);
        assert_eq!(wire, "[23,-7,10,9,1,2,3,255,0,0,0,80,1,1]");
        let ints: Vec<i64> = wire[1..wire.len() - 1].split(',').map(|v| v.parse().unwrap()).collect();
        // back in cells, rounded down, each with the odd unit left over
        let back = from_wire(&ints, 80).unwrap();
        assert_eq!(back[0], (CellBrick { min: [-4, 5, 4], ..bricks[0] }, [1, 0, 1]));
        assert_eq!(back[1], (bricks[1], [0; 3]));
        // and where a brick's middle ends up, in units, is the same either way
        let solver = Solver::new(&Models::default());
        for ((sent, place), (got, odd)) in placed.iter().zip(&back) {
            let (a, b) = (solver.place(sent).unwrap().pos, solver.place(got).unwrap().pos);
            assert_eq!([0, 1, 2].map(|k| a[k] + place[k]), [0, 1, 2].map(|k| b[k] + odd[k]));
        }

        assert!(from_wire(&ints[..13], 80).unwrap_err().contains("multiple"));
        let bad = |field: usize, value: i64| {
            let mut v = ints.clone();
            v[field] = value;
            from_wire(&v, 80).unwrap_err()
        };
        assert!(bad(0, 0x18).contains("not a shape"));
        assert!(bad(0, -1).contains("not a shape"));
        assert!(bad(0, 1 << 40).contains("not a shape"));
        assert!(bad(2, i64::MAX).contains("position"));
        assert!(bad(4, 0).contains("size"));
        assert!(bad(11, 81).contains("size"));
        assert!(from_wire(&[], 80).unwrap().is_empty());
    }
}
