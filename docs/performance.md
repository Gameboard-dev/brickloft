# Performance

A form is refitted every time a dot moves, so a fit has to take milliseconds
to feel direct. This document says what each part costs, what was done to
make it cheap, and what is still slow. The method itself is in
[how-it-fits.md](how-it-fits.md).

## Measurements

Release build, one thread, Windows. Reproduce with
`cargo run --release -p arcs-core --example timing`.

Closest fit (Simplify 0, brick size 1):

| Form | Time | Bricks | Discrepancy |
|---|---|---|---|
| Round arch, span 32, 3 thick, 4 deep | 3 ms | 32 | 5.2% |
| Round arch, span 160 | 40 ms | 135 | 4.6% |
| Circle band, radius 60 | 46 ms | 141 | 6.1% |
| Solid sphere, radius 12 | 12 ms | 217 | 4.0% |
| Solid sphere, radius 50 | 198 ms | 2,215 | 1.2% |
| Hollow sphere, radius 50, wall 2 | 264 ms | 35,191 | 9.8% |
| Hollow dome, radius 100, wall 2 | 691 ms | 71,041 | 9.5% |
| Torus, radius 40, 8 thick | 130 ms | 645 | 11.1% |

The two thin shells are the cases the sphere layout cannot handle (a wall of
2 at that radius), so they are fitted cell by cell and show what that costs
in bricks.

Most of each time is the measurement, not the fitting. Laying out a solid
sphere of radius 80 takes about 7 ms; measuring it takes about 290 ms.

## The representation does most of the work

Three choices made early decide most of the speed.

**A shape is eight bits.** A brick's shape is the set of box corners it
holds, one bit each. Rotating, mirroring, and finding what a shape leaves of
its box are bit operations and table lookups, with no geometry.

**A cell's contents are 64 bits.** Each cell is sampled at
$4 \times 4 \times 4$ points and the answers are packed into one `u64`.
Each candidate shape has its own word, computed once. How far a cell is from
a candidate is

$$\operatorname{popcount}(\text{cell} \oplus \text{candidate})$$

one exclusive-or and one instruction where the processor has it, so trying
30 candidates in a cell costs about 30 operations.

**A form is a distance function.** $f(p)$ is negative inside and never
claims the surface is further than it is. So if $\lvert f(c)\rvert$ at the
centre $c$ of a region is larger than the distance from $c$ to the region's
furthest corner, the whole region is on one side of the surface and nothing
in it needs looking at.

## What each part does

### Skipping empty and solid space

The cell-by-cell fitter and the measurement both evaluate the form once at
the centre of each $8 \times 8 \times 8$ block of cells. A block's furthest
corner is $8 \cdot \tfrac{\sqrt{3}}{2} \approx 6.93$ cells from its centre,
so a block with $\lvert f\rvert$ beyond that (plus a cell's own half
diagonal) is wholly inside or wholly outside and none of its 512 cells is
sampled. For a solid, most of the bounding box is such blocks. The same test
is then applied per cell before taking its 64 samples.

Before this, sampling every cell's centre was four fifths of the time a
large sphere took.

### Outlines

- **Triangles are scored a row at a time.** A candidate triangle is compared
  with the form using running counts of inside samples along each row
  (prefix sums), so its cost depends on its height in rows, not its area.
  Scoring cell by cell across each triangle's whole box was nine tenths of
  the time for a large arch: a span of 320 went from 1,250 ms to 156 ms.
- **What a triangle replaces is four lookups.** The one-cell pieces a
  triangle would replace are counted through a summed-area table, so the
  count costs the same whatever the triangle's size.
- **The search is a shortest path**, not a search over all polygons:
  dynamic programming over candidates in order, with the turn before each
  side as part of the state.

### Distance to a curve

The segments of a flattened curve are filed in a bucket grid. A distance
query reads the buckets near the point, not every segment.

### Spheres

- **Scoring a layout builds nothing.** Each candidate number of rings is
  scored from its cut positions alone.
- **Identical axes share their cuts.** A sphere's three axes are searched
  once.
- A torus was at first described as a small disc spun about an axis, which
  tests every sample against every edge of the disc: 313 ms. As its own
  formula it takes 21 ms.

### Stretched ramps

Each surface cell is tried with the one ramp that faces the way the form
does there, found from which half of the cell holds more of it, and cells
are looked up through a direct table. The first version tried all twelve
ramps at every cell and searched a sorted list for each lookup: 3.1 s for a
sphere of radius 50, against 0.15 s.

### Measuring

- **Only totals are kept** when building. A report keeps 16 bytes a cell;
  the build needs two numbers. Dropping the record also lifted the limit on
  how large a form can be measured, which had been sending spheres over
  radius 78 to the cell-by-cell fitter for want of a measurement (51,000
  bricks, now 3,400).
- **Samples are shared between layouts.** A revolve is built two ways and
  the closer kept. What the form holds of each surface cell is the same for
  both, so the second measurement reuses the first's samples: 311 ms down to
  192 ms.

## The editor

- **A dragged form is not measured.** While the mouse is down, forms that
  are laid out whole are built without the measurement, and measured once
  when it is let go.

  | Form | While dragging | When let go |
  |---|---|---|
  | Flat arch, span 320 | 156 ms | 156 ms |
  | Arch as a pipe, span 320 | 33 ms | 127 ms |
  | Ring as a pipe, radius 80 | 41 ms | 175 ms |
  | Solid sphere, radius 80 | 7 ms | 295 ms |
  | Revolve (a bell, radius 29) | 23 ms | 192 ms |

- **Moving and turning never refit.** A form's place, its grid's angle and
  its quarter turns are kept apart from its shape, so dragging a form or a
  ring moves bricks that already exist.
- **The best-fit search is spread over frames.** It builds the form once for
  each value it tries, a few dozen builds. It runs for about 8 ms of each
  frame, with a progress bar, so the view keeps moving. A change made by
  hand part way through ends it.
- **A frame with nothing new costs nothing.** The form is rebuilt only when
  its settings differ from those of the last build.
- **Drawing is instanced.** There is one mesh per brick shape, drawn once
  with a transform per brick, so a build is at most a few dozen draw calls
  however many bricks it has.
- **Shadows are redrawn only when bricks change**, not every frame: turning
  the camera does not move a shadow.
- **Edge lines cost no geometry.** They are drawn in the bricks' own shader
  from a per-vertex distance to the edge of the face.
- **Undo keeps settings, not bricks.** A step in the history is each form's
  settings, a few hundred bytes; bricks are rebuilt from them.

## Compile times

| Step | Time |
|---|---|
| `core`, with its tests | 11 s |
| the editor's dependencies, first time | about 50 s |
| `core`, after an edit | 3 to 5 s |

Dependencies are built optimised once and cached; the project's own crates
build unoptimised with thin debug information. The window, rendering and
panel come from one crate (`three-d`) with only the features in use switched
on.

## Still slow

- **Thin hollow spheres** that fall back to the cell-by-cell fitter: about a
  quarter of a second at radius 50, and tens of thousands of bricks.
- **A very large revolve during a best-fit search**: each build is longer
  than a frame, so the view holds for one build at a time.
- **Everything is single-threaded.** The fitters are independent per form
  and the measurement is independent per block, so both would divide across
  threads; the browser build's lack of threads is why this was not done
  first.
