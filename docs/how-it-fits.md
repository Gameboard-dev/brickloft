# How a form becomes bricks

## The grid

One **cell** is the smallest microbrick: a cube 2 game units on a side. Brick
positions are whole units, so every point, radius, rise and thickness is
snapped to whole units (half cells) before anything is computed
(`Spec::snapped` in `core/src/spec.rs`). A form placed between units could
only be approximated by bricks that are all slightly off it.

## The shapes

Each shape is the convex hull of some corners of a box, so they all share
faces exactly. In a unit box:

| Shape | Solid | Corners | Volume | Poses |
|---|---|---|---|---|
| Cube | the whole box | 8 | 1 | 1 |
| Ramp | `y + z ≤ 1` | 6 | 1/2 | 12 |
| Tetrahedron | `x + y + z ≤ 1` | 4 | 1/6 | 8 |
| Cut cube | `x + y + z ≥ 1` | 7 | 5/6 | 8 |
| Hip | `x ≤ 1 − z` and `y ≤ 1 − z` | 5 | 1/3 | 24 |
| Inner corner | `min(x, y) + z ≤ 1` | 7 | 2/3 | 24 |
| Half outer corner | `x + y ≤ 1` and `z ≤ x + y` | 5 | 1/3 | 24 |
| Half inner corner | `x + y ≤ 1` and `z ≤ y` | 4 | 1/6 | 12 |
| its mirror image | `x + y ≤ 1` and `z ≤ x` | 4 | 1/6 | 12 |

The tetrahedron and the cut cube are complements: together they fill the box,
and a plane tilted on all three axes passes through the grid cutting
alternate layers of cells into one and the other. The hip is one whole face
of the box and a single corner of the opposite face: a quarter of a square
pyramid, with two sloped faces meeting along a ridge. It is used only where
a form has such a ridge, which so far is the pyramid; the fitters that work
cell by cell are not offered it. The inner corner is what a hip leaves of
its box: a valley, where two slopes meet in an inside corner. It and the
inner corner is used on the inside ridges of a hollow pyramid, where two
ramps meet in an inside corner of a sloped ring, and in a band raised to
follow its curve. That band is also where the half corners are used. Its
half squares are the one place two bricks share a box.

## Points added to a form

Shift + click on a form adds a point that takes hold of it there. Dragged,
it moves that part of the form, and the form around it follows by less the
further off it is. The form's own points and sizes are untouched.

- On a curve, and on a turned form's profile, the reach is measured along
  the curve. A flat band pulled out of its plane keeps its outline and is
  raised to follow the curve: sloped bricks above and below where one brick
  can make the slope, and a step where none can.
- On a sphere or a pyramid the reach is all round the point. The form keeps
  its own layout wherever the point does not reach; only the part it
  changes is fitted cell by cell, which is rougher. A round tube pulled out
  of its plane is treated the same way: the pipe is kept away from the pull.

## Turning

A form can be turned two ways. By quarter turns it stays on the brick grid:
it is fitted as it stands and its bricks are turned afterwards, so it is the
same bricks facing another way. By any other angle it cannot stay on the
grid, so its whole grid is turned instead: the game keeps bricks on grids,
and a grid other than the world's can stand at any angle. The form is
fitted exactly as if it were level and is copied to the clipboard on a
frozen grid of its own. Grids pass through each other, so such a form is
not kept clear of the others. Sent through the plugin, which places on the
world's grid, it is placed level.

## Mirror

Mirror, along x, y or z, makes the form its own mirror image: its low side
along that axis is kept and the high side becomes its image, about the
middle of the form's points. It is done to the form before fitting, so a
lopsided arch becomes a symmetric one and is fitted as one. A curve that
crosses the middle more than once has no one low side and is left alone.
