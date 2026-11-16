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
}
