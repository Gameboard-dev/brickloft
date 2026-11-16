# Performance

The editor refits a form every time a dot moves, so a fit has to take a few
milliseconds for dragging to feel direct. A fit that takes half a second
turns a drag into a slideshow. The method, and the terms used here (cell,
sample, corner mask, distance function), are explained in
[algorithm.md](algorithm.md).

## Measurements

These times are for a release build running on one thread on Windows. You
can reproduce them with
`cargo run --release -p brickloft-core --example timing`, which runs
[core/examples/timing.rs](../core/examples/timing.rs). They are for the
closest fit, with Simplify at 0 and brick size 1.

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
| Pine tree, 140 tall | 1.0 s | 33,643 | 15.3% |
| Palm tree, 120 tall | 0.4 s | 16,753 | 18.7% |
| Oak tree, 110 tall, 124 across | 0.9 s | 40,092 | 7.1% |

The ordinary forms fit in tens of milliseconds and a large solid sphere in
about a fifth of a second. The two thin shells are the exceptions. A wall
of 2 at that radius is too thin for the sphere layout, so they are fitted
cell by cell, and their brick counts show what that method costs. For
comparison, a shell of radius 70 with a wall 4 thick, which the layout can
handle, takes a tenth as many bricks laid out as it did cell by cell.

The trees are the slowest forms and are not in the timing example. Their
times come from the test `where_the_time_goes` in
[core/src/tree.rs](../core/src/tree.rs), run by name with `--ignored
--nocapture` (see [Trees](#trees) below).

Most of each time is the measurement of the result, not the fitting. Laying
out a solid sphere of radius 80 takes about 7 ms, and measuring it takes
about 290 ms. That is why the editor skips the measurement while a form is
being dragged (see [the editor](#the-editor) below).

## How shapes, cells and forms are represented

Most of the speed comes from how shapes, cells and forms are represented.
Each representation turns a geometric question into a few operations on
whole numbers, each of which a processor does in one instruction.

### A shape is eight bits

A brick's shape is the set of its box's corners that it holds, one bit each,
in one 8-bit corner mask ([core/src/shapes.rs](../core/src/shapes.rs)).
Turning a shape, mirroring it, and finding the brick that fills the rest of
its box are therefore operations on bits and lookups in a table of 256
entries, with no geometry at all. For example, flipping a brick along one
axis swaps each corner bit with the bit of the corner opposite it along that
axis.

### A cell's contents are 64 bits

The cell-by-cell fitter tests each cell at $`4 \times 4 \times 4`$ points
and packs the 64 answers, inside or outside, into one 64-bit number, a
`u64`. Each candidate shape has its own such number, computed once at
start-up (`patterns` in [core/src/fit_volume.rs](../core/src/fit_volume.rs)).
How far a cell is from a candidate is the number of samples where the two
disagree:

```math
\mathrm{popcount}(\text{cell} \oplus \text{candidate})
```

Here $`\oplus`$ is the exclusive-or, which gives a 1 in every place where
the two numbers differ, and $`\mathrm{popcount}`$ counts the 1s. Both are
single instructions on a modern processor. Comparing a cell with one
candidate's samples therefore takes two instructions, where comparing them
one at a time would take 64 comparisons, and this is repeated for each of
the 30 candidates in every cell the surface crosses.

### A form is a distance function

A form is described by a distance function $`f(p)`$, which is negative
inside the form and never claims the surface is further away than it is
([core/src/sdf.rs](../core/src/sdf.rs)). That one guarantee allows whole
regions to be skipped. If $`\lvert f(c)\rvert`$ at the centre $`c`$ of a
region is larger than the distance from $`c`$ to the region's furthest
corner, the surface cannot pass through the region. The whole region is on
one side of the surface, so one evaluation stands in for the hundreds of
cells in the region.

## The savings in each part

### Skipping empty and solid space

Most of a solid's bounding box is far from its surface. A sphere 100 cells
across has about a million cells, of which only about 60,000 touch the
surface. Both the cell-by-cell fitter and the measurement therefore look at
the form in blocks of $`8 \times 8 \times 8`$ cells first.

The cell-by-cell fitter (`classify` in `fit_volume.rs`) evaluates the form
once at the centre of each block. A full block's furthest corner is
$`8 \cdot \tfrac{\sqrt{3}}{2} \approx 6.93`$ cells from its centre, so a
block whose $`\lvert f\rvert`$ is beyond that is wholly outside the form,
and skipped, or wholly inside, and filled with cubes at once:

```rust
                let centre = f(base + half);
                if centre > half.length() {
                    continue;
                }
                // ...
                if centre < -half.length() {
                    for z in 0..size[2] {
                        for y in 0..size[1] {
                            shape[index(0, y, z)..index(size[0], y, z)].fill(CUBE);
                        }
                    }
                    solid += (size[0] * size[1] * size[2]) as u64 * per_cell;
                    continue;
                }
```

`half` is half the block's size along each axis, so `half.length()` is the
6.93 cells. In a block the surface may cross, the form is evaluated at the
block's 729 cell corners, and a cell whose eight corners are all more than
0.87 of a cell (a cell's half diagonal) on the same side is settled without
taking its 64 samples:

```rust
                            if c.iter().all(|&v| v > CELL_REACH) {
                                continue;
                            }
                            if c.iter().all(|&v| v < -CELL_REACH) {
                                shape[index(x, y, z)] = CUBE;
                                solid += per_cell;
                                continue;
                            }
```

The measurement (`scan` in [core/src/report.rs](../core/src/report.rs))
does the same with a slightly wider margin. It skips a block when
$`\lvert f\rvert`$ at its centre is beyond the block's half diagonal plus a
cell's own half diagonal, and then tests each remaining cell at its centre
before taking its 64 samples. Before this, sampling every cell's centre was
four fifths of the time a large sphere took.

### Outlines

The outline fitter ([core/src/fit_planar.rs](../core/src/fit_planar.rs))
spends its time measuring candidate triangles: for every pair of candidate
corners, how much does the triangle between them get wrong?

Triangles are scored a row at a time. A triangle covers one unbroken run of
samples in each row of samples. If you know, for each row, the running
count of inside samples from the left edge (a list of **prefix sums**), the
number of inside samples in any run is one subtraction: the count at its
right end less the count at its left end. This is the inner loop of
`triangle_error_by_rows`:

```rust
    for sy in 0..ny {
        let (lo, hi) = row_span(corner, sy, nx, ny);
        let at = (p.y as usize * K + sy) * across + p.x as usize * K;
        let in_run = rows[at + hi.max(lo)] - rows[at + lo];
        let in_box = rows[at + nx] - rows[at];
        error += (hi.saturating_sub(lo) as u32 - in_run) + (in_box - in_run);
    }
```

For each row it finds the triangle's run (`lo` to `hi`), counts the inside
samples in the run and in the whole row of the box, and adds the samples
the triangle wrongly covers plus those it wrongly leaves out. The cost
depends on the triangle's height in rows, not its area. Scoring cell by
cell across each triangle's whole box was nine tenths of the time for a
large arch, and replacing it took a span of 320 from 1,250 ms to 156 ms.

What a triangle replaces is found in four lookups. The search compares each
triangle with the one-cell pieces it would replace, and those pieces' total
error over any rectangle comes from a **summed-area table**. This is the
two-dimensional version of a prefix sum: each entry holds the total of
everything above and to the left of it, so the total over any rectangle is
four entries added and subtracted. The count costs the same whatever the
triangle's size.

The search is a shortest path, not a search over all polygons. It is
dynamic programming over the candidate corners in order, keeping the way
the outline last turned as part of each step. The number of polygons grows
explosively with the number of candidates, while the shortest-path search
grows only with the number of pairs it has to try. Pairs whose midpoint is
two cells or more from the surface are rejected before any measuring, since
such a side cuts across the form or the air and cannot be the better fit.

### Distance to a curve

A curve is a polyline of many short segments, and the distance from a point
to the curve is the distance to the nearest segment. Checking every segment
for every query would be slow for a long curve. So the segments are filed
in a **bucket grid**, a coarse grid of boxes at least 4 cells on a side,
each listing the segments near it, like a filing cabinet sorted by
neighbourhood (`PolyDist` in `sdf.rs`). A query reads only the list for the
box it falls in:

```rust
        let b = c[0] + self.dims[0] * (c[1] + self.dims[1] * c[2]);
        let mut best = far;
        let mut best2 = f64::INFINITY;
        for &s in &self.items[self.start[b] as usize..self.start[b + 1] as usize] {
```

`b` is the number of the bucket the point is in, and the loop runs over
that bucket's segments only. All the lists are stored end to end in one
array, so a query's segments sit together in memory. A point more than 8
cells beyond the surface is answered as "far" without searching. The fitters only need exact distances near the surface, and this
margin is set above the block size used for skipping, so the skipping above
stays correct.

### Spheres

Scoring a sphere layout builds nothing. Each candidate number of rings is
scored from its cut positions alone, by comparing the height of the
faceted surface with the true one over a 40 by 40 grid of columns
(`mismatch` in [core/src/fit_round.rs](../core/src/fit_round.rs)). Many
candidates can therefore be tried, and only the winner is built. Axes with
the same radius keep the same cuts and are improved together. For a sphere,
the improving step tries the moves of one axis instead of three, and the
sphere stays the same along each axis.

The torus is a circle built as a round tube, so it costs what a pipe
costs: one outline fit, the chamfer, and the measurement against the tube's
distance function, which the 130 ms in the table above includes.

### Trees

A tree is the union of thousands of parts, and its time is the time to
find the distance to the nearest part at about ten million samples (see
[algorithm.md](algorithm.md), section 7.10). Four choices set it.

| Choice | Before | After |
|---|---|---|
| Parts filed in buckets 1 cell wide in place of 4 | pine 2.9 s | 1.0 s |
| An oak's leaves as a union of small balls in place of smooth noise on large ones | oak 2.2 s | 0.9 s |
| A sample's bucket found without rounding down | palm 0.47 s | 0.39 s |
| How far the empty buckets are from the tree worked out on a grid 4 times coarser | 100 ms a tree | under 5 ms |

Buckets four cells wide suit a curve, whose segments are long. A pine's
side shoots are two cells apart, and a bucket that wide listed about fifty
of them, so a sample was compared with fifty parts to find the one or two
near it. A bucket of one cell lists six.

Smooth noise added to a distance function makes it steeper, and a
distance function may never overstate (see
[skipping empty and solid space](#skipping-empty-and-solid-space)). The
noisy leaves had to be scaled to under half their value to stay honest.
The fitter then took every cell within two cells of a leaf for a cell on
the surface and sampled it 64 times, with the noise worked out at each
sample. A union of balls is an exact distance and needs none of that.

Rounding a number down is a call into the maths library on a processor
the compiler may not assume has the instruction for it. A sample's bucket
needs three, and they were a fifth of a palm's time. Inside the grid the
numbers are never negative, so dropping the fraction is the same answer.

What is left is about 100 nanoseconds a sample, nine tenths of a build.
The blocks of a fit are independent, so the remaining gain is in dividing
them among threads. A graphics card was considered for the samples and
not used, because it cannot promise the same bricks on every machine;
the reasons are in [algorithm.md](algorithm.md), section 7.10.

### Stretched ramps

When the cell-by-cell fitter looks for longer ramps
(`stretch_ramps` in `fit_volume.rs`), each surface cell is tried only with
the one ramp that faces the way the form does there. The facing is found
from which half of the cell holds more of the form along each axis:

```rust
        let lean: [i32; 3] = [0, 1, 2].map(|k| {
            (bits & upper[k]).count_ones() as i32 - (bits & !upper[k]).count_ones() as i32
        });
        let axis = (0..3).min_by_key(|k| lean[*k].abs()).unwrap();
```

`upper[k]` marks the samples in the upper half of a cell along axis `k`, so
`lean[k]` is how many more inside samples are in the upper half than the
lower. The axis the form changes least along is the one the ramp runs
along, and the signs of the other two leans say which way it faces. Cells
are also found through a direct table instead of by searching a sorted
list. The first version tried all twelve ramps at every cell and searched a
sorted list for each lookup. It took 3.1 s for a sphere of radius 50,
against 0.15 s now.

### Measuring

Only totals are kept when building. A report keeps 16 bytes for each cell,
to show where the bricks and the form disagree, but building needs only two
numbers: the form's volume and the discrepancy. Dropping the record also
lifted the limit on how large a form can be measured. That limit had been
sending spheres over radius 78 to the cell-by-cell fitter for want of a
measurement (51,000 bricks, now 3,400).

Samples are shared between layouts. A revolve is built two ways and the
closer one is kept (see [algorithm.md](algorithm.md#76-profiles-spun-about-an-axis)).
What the form holds of each surface cell is the same for both, so the second
measurement reuses the first's samples (`measure_with` in `report.rs`). That
took one revolve from 311 ms to 192 ms.

## The editor

A dragged form is not measured. While the mouse button is down, forms that
are laid out whole are built without the measurement, and measured once
when the button is let go. This is the start of `refresh` in
[editor/src/state.rs](../editor/src/state.rs), where `busy` is true while
something is being dragged:

```rust
        let owed = self.built.unmeasured && !self.busy;
        if self.built_from.as_ref() == Some(&spec) && !owed {
            return false;
        }
        let start = now_ms();
        match brickloft_core::spec::build_as(&spec, !self.busy) {
```

The second argument of `build_as` says whether to measure. `owed` notes
that the last build was not measured, so one more build is due once the
drag ends. The difference is large for the laid-out forms:

| Form | While dragging | When let go |
|---|---|---|
| Flat arch, span 320 | 156 ms | 156 ms |
| Arch as a pipe, span 320 | 33 ms | 127 ms |
| Ring as a pipe, radius 80 | 41 ms | 175 ms |
| Solid sphere, radius 80 | 7 ms | 295 ms |
| Revolve (a bell, radius 29) | 23 ms | 192 ms |

The flat arch gains nothing because its fitter measures as it goes; the
sphere's drag is forty times quicker.

A frame with nothing new costs nothing, because `refresh` rebuilds the form
only when its settings differ from those of the last build
(`self.built_from`). Moving or turning does not refit either, because a
form's place, its grid's angle and its quarter turns are kept apart from
its shape, so dragging a form or a ring moves bricks that already exist.

The best-fit search is spread over frames. It builds the form once for each
value it tries, a few dozen builds in all, which for a large form would
freeze the window for seconds. So `search_for` in `state.rs` runs it for
about 8 ms of each frame and then hands control back, with a progress bar:

```rust
        let start = now_ms();
        while self.search_step() && now_ms() - start < budget_ms {}
```

Each `search_step` is one build. The view keeps moving between slices, and
a change made by hand part way through ends the search, since what it has
measured no longer applies.

Drawing is instanced. Telling the graphics card to draw something, a
**draw call**, has a fixed cost, and in a browser that cost limits a scene
long before the number of triangles does. So there is one mesh for each of
the 125 poses of the nine shapes, and each is drawn once with a list of
positions and sizes, one per brick in that pose
([editor/src/lib.rs](../editor/src/lib.rs)). A build takes at most one draw
call per pose it uses, however many bricks it has.

Shadows are redrawn only when the bricks change, not every frame, because
turning the camera does not move a shadow. Edge lines cost no extra
geometry. They are drawn by the bricks' own shader from a number stored at
each corner of a face that says how near the face's edge it is
([editor/src/mesh.rs](../editor/src/mesh.rs),
[editor/src/material.rs](../editor/src/material.rs)).

Undo keeps settings, not bricks. A step in the history is each form's
settings, colours and place, a few hundred bytes, and the bricks are
rebuilt from them. 200 steps are kept.

## Compile times

| Step | Time |
|---|---|
| `core`, with its tests | 11 s |
| the editor's dependencies, first time | about 50 s |
| `core`, after an edit | 3 to 5 s |

In a development build, the dependencies and the `core` crate are compiled
with optimisation, and the editor crate is compiled without it, with only
line-number debug information ([Cargo.toml](../Cargo.toml), `[profile.dev]`).
The dependencies are compiled once and cached, so an edit to the editor
rebuilds only the editor, quickly, while the fitters still run at nearly
full speed in tests. The window, the rendering and the panel come from one
crate, `three-d`, with only the features in use switched on.

## Still slow

Thin hollow spheres that fall back to the cell-by-cell fitter take about a
quarter of a second at radius 50 and need tens of thousands of bricks.

A tree takes up to a second at its usual size, and four times as long at
twice the size, since its time grows with its surface. The view holds
still for each build, so a tree's sliders move it in steps.

A very large revolve during a best-fit search is slow to respond. Each of
its builds is longer than a frame, so the view holds still for one build at
a time.

Everything runs on one thread. The fitters are independent for each form,
and the measurement is independent for each block, so both could be divided
across threads. That was not done first because the browser build has no
threads (see [web-page.md](web-page.md#no-threads)), and the fitters had to
be fast on one thread there anyway.
