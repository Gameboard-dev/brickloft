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
inner corner is used on the inside ridges of a hollow pyramid, and where two
ramps meet in an inside corner of a sloped ring. The three half corners can
be placed, drawn, measured and saved, but no fitter lays them out yet.

A pyramid that is not filled is a tent, open underneath: the same pyramid
less a smaller one inside it. Its wall is measured straight up, and comes
out a little thicker or thinner than asked where that lets its slopes land
exactly on the grid. All five can be stretched by whole cells
along each axis.

## Pyramids

A pyramid is built exactly, with no fitting (`core/src/fit_pyramid.rs`): a
ramp for each slope and a hip on each ridge. Centred on a lattice point it is
four hips and nothing else; centred in a cell it has a top one cell square,
so nine bricks. One too large for single bricks is cut into as few levels as
keep every brick within the game's limit.

## Curves

Every curve is flattened to a polyline within 0.02 cell of the true curve
(`core/src/curves.rs`), so the rest of the code never needs to know what kind
of curve it was.

- **Arc through 3 points:** the unique circle through them.
- **Arches:** closed-form curves between two feet, with a chosen rise.
- **Spline:** centripetal Catmull-Rom, which passes through every point and
  cannot form cusps or loops within a span.
- **Bézier:** rational, with a weight per point. A weight pulls the curve
  towards its point; with three points it yields exact conics, including the
  circular arc that a plain Bézier cannot represent.

## Solids

A form is a function that is negative inside and gives the distance to the
surface near it (`core/src/sdf.rs`): a band or tube along a curve, a filled
outline, an ellipsoid, or an outline spun about an axis. Hollow shells, domes
and segments are the same functions combined with `max`, not separate cases.

## Fitting: minimise the disagreement

The measure is the volume (area, in 2D) where the bricks and the form
disagree: material that should be there and is not, plus material that is
there and should not be.

Every brick owns the cells of its bounding box outright, because the game
refuses bricks whose boxes overlap. So the total disagreement is a sum over
boxes, and each box can be judged on its own.

**Unit bricks are solved exactly.** A cell can hold one of 30 things (nothing,
a cube, 12 ramps, 8 tetrahedra, 8 cut cubes). Each cell is sampled 64 times
and given whichever of the 30 disagrees least. Since cells do not affect one
another, that is the true minimum for unit bricks.

**Flat forms are fitted as one continuous outline** (`core/src/fit_planar.rs`).
When a curve lies flat across the grid, its outline is fitted in 2D and
pushed through. The outline is a polygon whose corners are lattice points
along the form's surface. Each sloped side of it is the hypotenuse of exactly
one ramp; each level or upright side is a plain brick edge. Consecutive sides
share a corner by construction, so slopes meet end to end.

The lattice points on offer are the corners of the unit-brick result, a fine
staircase that hugs the surface. Each outline is cut where it truly turns
back (ignoring one-cell wobbles in the staircase), and along each stretch
between cuts the corners to keep are chosen by an exact shortest-path search
that minimises disagreement plus a cost per corner. A step is two corners
where a slope is one, so a slope wins unless the form really steps there.
Where the two faces of a thin wall want the same cell, the larger ramp has it
and the other face is fitted again round it.

An earlier version chose each ramp separately on its own merit. It measured
slightly closer and left a step wherever two ramps' ends did not happen to
meet; bridging those afterwards never fully worked.

**Everything else is fitted in 3D** (`core/src/fit_volume.rs`), first with
unit bricks as above. Unit ramps are all at 45°, so a gentler slope, such as
the top of a dome, would come out as a row of teeth. A second pass replaces
runs of cells with one stretched ramp, two to six cells long and one high (or
the reverse), wherever that disagrees with the form less than what is there.
Then cubes are merged into boxes and ramps into longer ramps.

The editor shows the result of the measure for the current form as
"Discrepancy".

## Fit against brick count

The closest fit uses many small bricks. Two settings trade some of it away.

**Simplify** applies to flat forms. It adds to the cost of each corner of the
outline, in microbricks of area, so several short sides give way to one long
one wherever that costs less disagreement than the corners saved. Even at 0
a corner costs a tenth of a microbrick, which is what makes a slope beat a
step of equal fit.

Volumes have no such price. One was tried and removed: a tetrahedron or cut
cube corrects at most a sixth of a cell, so any useful price stripped every
corner piece and left a pit wherever three slopes meet. In the plane a price
swaps short pieces for a long one and the outline stays whole; on a surface
there is nothing longer to swap a corner for.

The price is judged in the outline alone, not multiplied by the form's
depth: depth only pushes the outline through the grid, so making a form
deeper never changes how its outline is fitted.

A cube that merging leaves on its own is exempt: it is one brick whatever its
shape, so it is given the shape that fits its cell best. Without that, the
price kept full cubes at steps in an outline where a ramp fitted better for
the same count.

**Brick size** fits the form on a coarser lattice, so every brick is a
multiple of that size. Bricks two across cover a surface with about a quarter
as many pieces and follow it half as finely. **Automatic** picks the finest
size that keeps within the brick budget, with limits: a wall stays at least
two bricks thick, a round solid at least four bricks to its radius, and a
flat form's depth a whole number of bricks. If the budget cannot be met
within those limits the editor says so instead of building something that is
no longer the form.

The editor starts at Simplify 0.25, automatic size, budget 2500. The core
library's own defaults are the closest fit.

## Where a form fits cleanly

A form fits cleanly only where its surfaces land on the grid. A round arch
whose rise makes its radius 8.3 has a 2-thick wall from 7.3 to 9.3 cells
out; bricks can only put it at 7 to 9, a third of a cell off all the way
round, and the discrepancy is about 16%. At radius 8 the same wall sits on
lattice lines and the discrepancy halves. A 2-thick ring is clean at a whole
radius and not at a half one; a 3-thick ring is the other way about.

Which values are good depends on thickness and proportions, so the editor
searches for them: **Snap to best fit nearby** tries each dot and size a half
and a whole microbrick either way, keeps whatever fits closest, and repeats
until nothing improves. It finds the nearest good fit, not the best possible
one somewhere else.

## Spheres, ellipsoids and domes

These are laid out whole, not cell by cell (`core/src/fit_round.rs`). A
sphere is `x² + y² + z² <= r²`, one term per axis. Each axis is cut at chosen
distances from the centre, numbered 0 to K outwards; a corner of the
resulting boxes is inside when the numbers of its three coordinates sum to K
or less, and each box holds the convex hull of its inside corners. That hull
is always nothing, a whole box, or one of the four microbricks stretched to
the box, and neighbouring boxes share corners, so every brick meets its
neighbours face to face. The surface has no steps and no gaps by
construction, and with the cuts' gaps never growing outwards it is convex.

With cuts at `r * sqrt(i / K)` every surface corner lies exactly on the
sphere. The cuts must be on the lattice, so they are the nearest lattice
values that keep the gaps in order, chosen together by an exact search and
then scored against the true solid for every K the lattice has room for.
About one ring per 2 to 3 cells of radius is typical.

A hollow sphere is the same solid less a smaller one from the same cuts, and
needs a wall of at least 2 cells. A dome or segment is cut at one of the cuts
across z; a cut asked for through the middle of a row of cells moves up to
the next lattice plane. Where the method does not apply (a wall under 2
cells, bricks limited to cubes) the solid is fitted cell by cell as before.

What it costs: the corners are within half a cell of the sphere instead of
each cell being as full as it can be, so the volume disagreement is higher,
about 3 to 4% for a radius of 12 against 2 to 3% cell by cell. In exchange
a radius-13.5 sphere went from 1311 bricks with a third of its surface
stepped to 321 bricks with none, every face within 20° of the true surface.

## Round tubes and tori

A round tube along a curve that lies flat across the grid, and a torus, are
built as a pipe (`core/src/fit_stack.rs`): an octagon in section. The band
of its full width is fitted as an outline, exactly as a flat arch of that
width would be, and the chamfer is cut back from that outline: a ramp in
from each level or upright side, a tetrahedron on the triangle of each
sloped side, and a cube cut on the slant at each corner between them. Those
pieces share whole faces and start exactly on the wall, so the chamfer is
one continuous surface.

The wall's outline is fitted knowing the top hangs off it. A sloped side is
charged for landing the top's outline off the true surface; a level or
upright side shorter than the chamfer is charged for the pieces it has no
room for; and no sloped side may be deeper than the chamfer, because every
brick owns its whole box and deeper pieces from the two faces of a band
reach into each other's boxes across the narrow top.

The chamfer was first built the other way, outwards from the narrow top
band. Where that band ran diagonally it could only be fitted as a
staircase, its chamfer pieces collided and were dropped, and the wall was
left with holes: a ring of radius 19 was 29% steps and is now 1%.

A torus is not a form of its own: it is a circle as a round tube. Stretched
within its plane the circle is an ellipse; stretched across it, the tube is
that many times as deep as it is wide, and its chamfers that many times as
tall. Filled, every band takes in what the circle encloses, which gives a
disc with a rounded rim.

"Protruded" instead builds flat bands of different widths side by side,
which is smooth along the curve and stepped across it.

Under 3 microbricks across there is no room for a chamfer, so a thin tube is
a square bar, and its discrepancy against the true round tube reads about
30%. A tube that does not lie flat is still fitted as a volume.

## Turned forms

A profile spun about an upright axis is laid out from its section
(`core/src/fit_lathe.rs`). The section, in the half-plane of distance from
the axis and height, is fitted as an outline. Spun, each upright side of it
is a wall standing on a circle, each level side a flat face, and each sloped
side a ring whose circle shrinks as it rises: the chamfer of a pipe, cut back
from the outline of the ring's wide end. Sloped sides are kept to one cell
across or one cell up, so that the pieces of a wall's two faces keep to
their own cells. That is done two ways, in the fitting or after it, and the
closer of the two results is kept: a straight cone suits one and a thin
curved wall the other.

Each sloped ring's outline is fitted for that ring, so where one ring ends
and the next begins two outlines of the same circle meet, and can differ by
a sliver. Where two faces of the form run into each other, at the bottom of
a groove or the root of a lip, the pieces that will not fit are built as
whole cells, which rounds the groove off with a little too much. On your
bell-shaped shell (report 24) this took stepped faces from 25% to 12%, with
the volume disagreement up from 7% to 11%. An axis on a lattice line one way
and in the middle of a cell the other is still fitted cell by cell.

## Reading a surface

Volume cannot tell a slope from a staircase. The report's `[surface]`
section samples every exposed face and compares the way it points with the
way the form's surface points there: the share of the outside within 10, 20,
30 and 45 degrees, the stepped share (level and upright faces 30° or more
off), the same by layer, and the worst faces. `[section]` drawings cut
through bricks and form together, finer than the cells.

## Samples on a sloped face

A regular sample grid lines up with a 45° face, so some samples lie exactly
on it. Counting them all inside or all outside would make every ramp measure
well over or under half its box. In the volume fitter the middle half of
those samples count as inside, which makes a ramp measure exactly half and
keeps mirror images identical. The planar fitter counts them outside for both
the form and the triangle, which is exact for lattice-aligned slopes and
reads a triangle about 6% small otherwise.

## Turning one way

Round a curve an outline should turn one way throughout. Chosen on fit
alone, a slope the lattice has no single side for is made by alternating the
two nearest (1 in 1, 1 in 2, 1 in 1, 1 in 2), which is a little closer by
area and turns left, right, left, right: from above, a curve with a ripple
in it. So the outline search charges three quarters of a cell for each
change in the way the outline turns, and the same sides come out in order of
slope. A ring is also set on the lattice whatever its radius: left half a
cell off, the straight run where it crosses an axis fits best, by area, as a
row of unit triangles, which looks like the teeth of a saw.

## Symmetry

Where two candidates tie exactly, they are almost always mirror images on a
plane of symmetry. Picking "the first" would give the two halves of a
symmetric form different bricks, so a tie goes to the best candidate that
nothing ties with. Tests check that a ring and a sphere come out mirror
symmetric.

## Limits

- **Tilted tubes and tilted bands are still fitted cell
  by cell**, with ramps stretched one way where that helps, so their
  surfaces can show stray pieces and steps. Spheres, ellipsoids, pyramids,
  flat round tubes, tori and turned forms are laid out whole.
- **A thin pipe's outline has few directions.** No sloped side of a pipe's
  outline may be deeper than its chamfer, so a tube under 6 across, whose
  chamfer is one cell, can only use slopes of 1 in n: round a quarter of a
  ring, a few straight runs (at radius 38: slopes of 9, 14, 27, 45, 63, 76
  and 81 degrees), each turning the same way into the next. Letting sides
  go a cell deeper was tried and brought steps back, because the deeper
  pieces collide across the narrow top.
- **A pipe has one chamfer.** Its section is an octagon whatever its size; a
  very thick tube would look rounder with two or three slopes.
- **A pipe's chamfer is weaker on a tight curve.** Where the inside of a
  curve has a radius under about three times the chamfer, its outline is
  mostly unit triangles, which carry only a shallow chamfer, and where
  pieces from two sides would cross, one is left out and the solid stands a
  little proud there. A tube 6.5 across on a radius of 8 is about 3% steps,
  where on a radius of 19 it is under 1%.
- **A sphere's radius should end on the lattice.** Centred in a cell, a
  radius of 12.5 or 13.5 ends on a lattice plane and 13 does not; the poles
  are then half a cell out.
- **The outline may only use corners of the unit-brick result.** A lattice
  point slightly off that staircase is sometimes the better corner and is
  not considered.
- **Large thin shells stay brick-hungry.** A hollow sphere of radius 50 with
  a wall 2 thick is about 36,000 bricks, because its wall is too thin for a
  larger brick size. A thicker wall lets automatic sizing coarsen it.
- **Thin forms read as a high percentage.** A band or shell a few cells thick
  is nearly all surface, and the disagreement is all at the surface. Per unit
  of surface it is the same as for a thick form.
- **Tilted flat curves cannot be filled.** Only a curve lying flat across the
  grid has an outline the 2D fitter can fill.

## Machine learning

Not used, and not needed: the objective is exact and cheap to compute, the
unit case has a provable optimum by trying 30 options per cell, and the
result is deterministic and symmetric. A model could at best imitate that.
