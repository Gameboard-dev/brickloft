# Performance

The editor refits a form every time a dot moves, so a fit has to take a few
milliseconds for dragging to feel direct. A fit that takes half a second
turns a drag into a slideshow. The method, and the terms used here (cell,
sample, corner mask, distance function), are explained in
[algorithm.md](algorithm.md).

## Measurements

These times are for a release build running on one thread on Windows. They
are for the closest fit, with Simplify at 0 and brick size 1.
`cargo run --release -p brickloft-core --example timing`, which runs
[core/examples/timing.rs](../core/examples/timing.rs), builds the same
forms. It now uses every core, so on a desktop its times are lower than
these wherever a build shares its work out (see [Threads](#threads)).

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
| Pine tree, 140 tall | 1.1 s (0.21 s on 16 threads) | 33,643 | 15.3% |
| Palm tree, 120 tall | 0.36 s (0.09 s) | 16,753 | 18.7% |
| Oak tree, 110 tall, 124 across | 1.1 s (0.22 s) | 40,092 | 7.1% |
| Beech tree, 140 tall | 1.2 s (0.25 s) | 48,667 | 6.8% |
| Boxwood bush, 40 tall | 0.23 s (0.05 s) | 8,513 | 4.9% |
| Hedge, 36 tall, 72 long | 0.17 s (0.07 s) | 11,189 | 3.2% |
| Hydrangea, 46 tall | 0.31 s (0.07 s) | 17,227 | 5.4% |
| Three peonies, 56 tall | 0.19 s (0.06 s) | 3,751 | 13.3% |
| Three sunflowers, 90 tall | 0.23 s (0.06 s) | 8,965 | 16.3% |
| Five tulips, 64 tall | 0.30 s (0.10 s) | 5,880 | 13.9% |
| Petunias, 20 tall | 0.11 s (0.03 s) | 5,756 | 14.7% |
| Reeds, 84 tall, 60 long | 0.24 s (0.07 s) | 8,368 | 21.6% |
| Lawn, 180 by 120, 24 touches of the brush | 14 ms (4 ms), and 4 ms (2.4 ms) for one more touch | 27,600 | 0% |
| Lawn, 128 by 88, a raised pond and a stream | 24 ms (8 ms) | 20,900 | 0% |
| Bookcase, 36 wide, 60 tall | 0.01 ms | 87 | 0% |
| Wall of bookcases, 400 wide, 240 tall | 0.3 ms | 3,877 | 0% |
| Dining table, sofa, cupboard, chandelier | under 0.01 ms each | 8 to 46 | 0% |
| Potted plant, 26 wide, 46 tall | about 90 ms (a window box of four plants takes about 1.2 s) | about 1,300 | fitted, as a plant is |
| Maze of 6 rings, radius 120, plain walls | 0.18 s (40 ms) | 1,122 | 3.4% |
| The same with brick walls | 0.25 s (75 ms) | 25,045 | 3.4% |
| The same with hedge walls 6 thick | 2.5 s (1.0 s) | 54,903 | 8.0% |

The ordinary forms fit in tens of milliseconds and a large solid sphere in
about a fifth of a second. The two thin shells are the exceptions. A wall
of 2 at that radius is too thin for the sphere layout, so they are fitted
cell by cell, and their brick counts show what that method costs. For
comparison, a shell of radius 70 with a wall 4 thick, which the layout can
handle, takes a tenth as many bricks laid out as it did cell by cell.

The mazes are timed by the same example with `-- maze`. Their walls are
fitted in squares of ground, each square on a thread of its own
([algorithm.md](algorithm.md), section 7.11), and a hedge maze is the
slowest form after the trees.

The plants are not in the timing example, and the trees among them are
the slowest forms. Their
times come from the test `where_the_time_goes` in
[core/src/tree.rs](../core/src/tree.rs), run by name with `--ignored
--nocapture` (see [Trees](#trees) below).

The lawns are not fitted or measured either. Their bricks are written
down from the height of their ground (see [a lawn](#a-lawn) below), and
their times come from the test `how_long_a_lawn_takes` in
[core/src/grass/tests.rs](../core/src/grass/tests.rs), run by name with
`--release -- --ignored --nocapture`.

The bookcases are not fitted or measured. Their bricks are written down
from their size ([algorithm.md](algorithm.md), section 7.12), which is why
a wall of them takes less time than the smallest arch. Their times come
from the test `how_long_a_bookcase_takes` in
[core/src/furniture/tests.rs](../core/src/furniture/tests.rs), run by name
with `--release -- --ignored --nocapture`.

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

What is left is about 100 nanoseconds a sample, nine tenths of a build on
one thread. The blocks of a fit are independent and the fitter divides
them among threads (see [Threads](#threads)), which takes a pine from 1.1
seconds to 0.21 on 16 logical cores with the same bricks. Growing the tree
and filing its parts, 50 milliseconds, and colouring its bricks, 20, are
still done on one thread and are now a third of that. A graphics card was considered for the samples and
not used, because it cannot promise the same bricks on every machine;
the reasons are in [algorithm.md](algorithm.md), section 7.10.

### A lawn

A patch of grass was built as a tree is: soil, water and every blade
were parts of one solid, and the fitter sampled that solid's distance
function over every cell of its box. On a lawn 180 cells by 120 with 24
touches of the brush, timers put in for the purpose measured this:

| Step | One thread | 16 threads |
|---|---|---|
| Growing the 27,800 parts | 7 ms | 7 ms |
| Filing them in buckets | 0.14 s | 0.13 s |
| 15.6 million distance samples | 3.4 s | 0.37 s |
| Colouring 63,500 bricks | 44 ms | 12 ms |
| All of it | 3.5 s | 0.53 s |

Every touch of the brush did all of it again, so each touch was half a
second and a stroke of ten was five.

A lawn's bricks are now written down from the height of its ground, with
no samples ([algorithm.md](algorithm.md), section 7.10, Grass). The same
lawn, and one 128 by 88 with a raised pond and a stream down from it:

| Lawn | One thread | 16 threads | Bricks |
|---|---|---|---|
| 180 by 120, 24 touches, fitted | 3.5 s | 0.53 s | 63,500 |
| The same, laid out | 14 ms | 4 ms | 27,600 |
| A raised pond and a stream, fitted | 2.0 s | 0.36 s | 37,700 |
| The same, laid out | 24 ms | 8 ms | 20,900 |

Of the 14 ms, about 5 are the ground: for every cell, whether the lawn
covers it and how high every touch leaves it. The other 9 are the tiles'
bricks. The lawn with a stream takes longer because the flood that finds
its raised ponds and the search for each stream's way are on one thread.
Those two rows were measured when every raised pond shared one level and
a stream was six cells wide. Each pond has its own level now and a
stream is three wide, which is the same flood and the same search.

Cliffs cost nothing that could be measured. A lawn 120 by 80 with a
table cut by a sharp edge, a pond on it, a fall over its side and a pond
below took 28 ms on one thread and 9.5 ms on sixteen, with **Cliffs** at
2 and at 0 alike (20,800 bricks). Those two figures are from a machine
busy with other builds, where the lawn of 24 touches took 24 ms on one
thread and not 14, so they are an upper bound and the comparison between
the two settings is what they show.

The bricks are fewer because a blade is one cell through, a column and a
sloped tip, where a fitted one was nearly two cells through with sloped
bricks round it.

A shader on the graphics card was the other candidate, for the samples.
It was not used, for the reason given under [Trees](#trees): the shading
language does not fix how its arithmetic rounds, so two cards could
build different bricks from one lawn. Laying the lawn out removed the
samples, and nothing as heavy is left for a card to do. What a build
spends now is spread over the ground's table, the joining of soil into
boxes and the blades, none of which is more than a few milliseconds.

A build from nothing grows with the lawn's area, and a touch of the
brush changes a few tiles of it. The last build of a lawn is kept, so
the next works out the heights the touch reaches and lays the tiles
whose ground changed ([algorithm.md](algorithm.md), section 7.10, What
is kept from one build to the next). The table gives one more touch, 20
in range, on a lawn that already has 24, from the test
`where_a_touch_on_a_lawn_goes` in
[core/src/grass/tests.rs](../core/src/grass/tests.rs). Times are on 16
threads, with one thread in brackets.

| Lawn | Bricks | Tiles laid, of all | From nothing | With the last build kept |
|---|---|---|---|---|
| 180 by 120 | 28,000 | 9 of 24 | 6 ms (15 ms) | 2.4 ms (4 ms) |
| 300 by 240 | 86,000 | 9 of 80 | 10 to 14 ms (56 ms) | 2.5 to 3.3 ms (5 to 6 ms) |
| 600 by 480 | 335,000 | 6 of 320 | 38 ms (207 ms) | 4 to 6 ms (5 to 8 ms) |
| 600 by 480, 120 touches | 342,000 | 6 of 320 | 44 to 55 ms (303 ms) | 4 to 8 ms (9 to 13 ms) |
| 300 by 240, raised ponds on | 86,000 | 9 of 80 | 17 ms (58 ms) | 3.6 to 4.2 ms (5 to 6 ms) |
| 600 by 480, raised ponds on | 335,000 | 6 of 320 | 52 ms (218 ms) | 7 to 9 ms (9 to 11 ms) |

A touch no longer costs by the number of touches already made: its cells
start from the heights kept. Three parts of a kept touch still grow with
the lawn. Comparing the tiles took 1.3 to 2.8 ms for the 320 tiles of
the largest lawn. Putting every tile's bricks end to end in one list is
1.5 ms for its 335,000 bricks, and handing that list to the caller is a
second copy of about the same. With raised ponds the flood is made
again at each touch, 4 to 6 ms on the largest lawn. Laying the six
tiles the touch reaches is 1 to 4 ms.

The lawns were measured while other programs were building on the same
machine, so each figure is the least of three runs of five, and the
ranges are what those runs disagreed by.

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

## Threads

A build shares its independent work out over the processor's cores, and
it builds the same bricks in the same order on any number of them. Before
this, a build ran on one thread however many cores the desktop had.

### What it saves

The table gives each form's build time in milliseconds on a desktop with 8
cores and 16 hardware threads (an AMD Ryzen 7 5800X3D, on Windows). "Let
go" is a measured build and "dragged" an unmeasured one (see
[the editor](#the-editor)). "Before" is the code as it was with everything
on one thread, and the other two columns are the code as it is, held to one
thread and given all sixteen. Each figure is the least of 27 builds or
more, because the desktop was doing other work throughout, which took
between two fifths and nine tenths of its processor time. An idle desktop
would give lower times on sixteen threads than these. That other work is
also why the first two columns of a row sometimes differ: on one thread the
code does what it did before, except that measuring does a little less.
`cargo run --release -p brickloft-core --example timing -- threads` prints
the one-thread and all-thread columns.

| Form | Bricks | Let go, before | Let go, 1 thread | Let go, 16 threads | Dragged, before | Dragged, 1 thread | Dragged, 16 threads |
|---|---|---|---|---|---|---|---|
| Solid sphere, radius 40 | 1,873 | 104 | 103 | 17 | 2.9 | 2.9 | 2.0 |
| Solid sphere, radius 80 | 3,067 | 567 | 585 | 74 | 5.8 | 5.9 | 4.8 |
| Hollow sphere, radius 50, wall 2 | 35,191 | 266 | 328 | 59 | 282 | 283 | 55 |
| Hollow sphere, radius 80, wall 4 | 8,553 | 921 | 880 | 108 | 34 | 34 | 9.4 |
| Revolve (a bell, radius 50) | 14,310 | 733 | 747 | 140 | 125 | 119 | 38 |
| Ring as a pipe, radius 60 | 737 | 188 | 184 | 45 | 26 | 25 | 16 |
| Lifted tube, radius 60 | 3,266 | 708 | 707 | 158 | 120 | 116 | 78 |
| Lifted band, radius 60 | 965 | 527 | 518 | 144 | 100 | 98 | 62 |
| Torus, radius 40, 8 thick | 645 | 117 | 117 | 30 | 17 | 16 | 8.3 |
| Spline band, 320 long, 20 wide | 243 | 61 | 64 | 36 | 66 | 67 | 38 |
| Round arch, span 320 | 200 | 145 | 143 | 136 | 145 | 149 | 132 |
| Stepping stones on that band | 1,457 | 160 | 167 | 26 | 145 | 158 | 28 |
| Stepping stones on a disc, radius 70 | 2,216 | 276 | 299 | 54 | 272 | 280 | 57 |
| Medallions on that disc | 18,434 | 55 | 46 | 25 | 48 | 49 | 27 |
| Rug on that disc | 10,226 | 52 | 53 | 28 | 57 | 58 | 36 |
| Planks on that disc | 674 | 45 | 43 | 23 | 44 | 45 | 25 |
| Bricks on that disc | 1,083 | 50 | 50 | 28 | 55 | 50 | 27 |
| Bevelled bricks on that disc | 4,897 | 54 | 58 | 29 | 61 | 52 | 29 |
| Cobblestone on that disc | 5,143 | 99 | 108 | 31 | 98 | 104 | 39 |
| Cobblestone on the band | 3,229 | 142 | 135 | 55 | 144 | 132 | 44 |
| Rug on that disc with a rounded rim, 8 thick | 10,681 | 1,854 | 1,809 | 465 | 62 | 63 | 39 |
| Cube, 120 by 84 by 48 | 4 | 7.8 | 5.8 | 4.6 | 7.8 | 5.8 | 4.7 |
| Cobblestone on that cube, its sides too | 9,402 | 101 | 117 | 49 | 121 | 130 | 46 |
| Bevelled bricks on that cube | 4,495 | 32 | 34 | 31 | 32 | 36 | 29 |

The rows from the stepping stones down were timed again after the cube and
the two patterns of bricks and cobblestone were added, as the least of 27
builds, on a desktop busier than before. Their figures are comparable with
one another and less so with the rows above them.

Measured builds gain the most, because the measurement was most of their
time and every layer of it is independent. A let-go sphere of radius 80
went from over half a second to under a tenth. Stepping stones gain as
much, because nearly all their time is the fitting of each stone by
itself. Dragged forms that were already quick gain little: a solid sphere
is laid out in a few milliseconds either way. A patterned floor gains a
half, and no more, because most of what is left is the search for the
disc's outline. Cobblestone gains the most of the patterns, since each of
its tiles looks through the stones near it, and bricks the least, a brick
being found by arithmetic.

Three rows hardly change. Bevelled bricks on a cube spend over half of
their time finding the cube's sides and the cells of each (see
[what stays on one thread](#what-stays-on-one-thread)). A long arch spends nine tenths of its time in the
search for its outline, which stays on one thread (see
[what stays on one thread](#what-stays-on-one-thread)). A lifted tube or
band spends a third or more of its dragged time building columns under
the lifted part, which also stays on one thread.

### What is shared out

Each of these is work whose items do not depend on one another.

| Work | Shared out by | Where |
|---|---|---|
| Measuring a build against its form | layers of cells | `scan` in [core/src/report.rs](../core/src/report.rs) |
| Choosing each cell's brick, cell by cell | rows of blocks | `classify` in [core/src/fit_volume.rs](../core/src/fit_volume.rs) |
| Finding longer ramps | surface cells | `stretch_ramps` in `fit_volume.rs` |
| Sampling an outline's solid | lattice points, then the cells the surface crosses | `fit_planar_towards` in [core/src/fit_planar.rs](../core/src/fit_planar.rs) |
| The circles of a turned form | circles | `fit_lathe` in [core/src/fit_lathe.rs](../core/src/fit_lathe.rs) |
| Scoring a sphere's layouts | choices of cuts | `fit_round_on` in [core/src/fit_round.rs](../core/src/fit_round.rs) |
| Stepping stones | stones | `lay_stones` in [core/src/pattern.rs](../core/src/pattern.rs) |
| The tiles of bricks, cobblestone, rugs and medallions | tiles | `drawn` in `pattern.rs` |
| The bevelled tops of bricks | cells | `lay_tiles` in `pattern.rs` |
| The walls of a maze | squares of ground, 96 cells each way | `fitted` in [core/src/maze/walls.rs](../core/src/maze/walls.rs) |
| The courses of a patterned maze's walls | pieces of the fit | `patterned` in `maze/walls.rs` |

Two of these needed their work put in a different order first. A turned
form used to fit each circle when a ring first asked for it, part way
through building the rings. The circles a form needs follow from its
section alone, so they are now listed before any ring is built and fitted
together. Stepping stones were fitted and laid one at a time, because a
stone that would touch one already laid is fitted again a little smaller.
The first fit of a stone depends on no other stone, so all the first fits
are made together, and the stones are then checked, refitted where needed
and laid one at a time in their old order.

Measuring also lost two pieces of work that only showed once the layers
ran side by side. A table of which brick holds each cell was filled on one
thread before any layer was looked at, and each thread now fills its own
layers' part of it. What the form holds of each surface cell was put into
a map for a second measurement to reuse, and it is now kept only when a
second measurement will follow, which is for a turned form. Together they
were a third of a large sphere's measuring time on sixteen threads.

A join of forms (see [algorithm.md](algorithm.md), "Forms joined on
purpose") shares out the test of each block of cells for the zone and the
sampling of each cell in it, and the measuring of each stretched brick
tried along the seam. Two spheres of radius 20 joined take 10.9 ms on one
thread and 9.6 ms on sixteen, on a machine that was busy with other
builds: the zone is a few thousand cells, about 3 ms of sampling, and
growing it to whole bricks, evening it out and running the fitter's passes
over it stay on one thread. Evening out goes by the bricks as they stand,
one colour of a chessboard of cells at a time, and could be shared among
threads within a colour. It has not been: it and the stretched bricks
together are 1 to 4 ms of a join.

### How the order is kept

All sharing out goes through one function, `map_ranges` in
[core/src/par.rs](../core/src/par.rs). It cuts the items into ranges, has
each thread take the next range not yet taken, and puts every result back
in its range's place, so the order in which threads finish never reaches a
result. Three rules for its callers make the bricks independent of where
the ranges fall:

- The work for one item does not read the work for another.
- Totals across ranges are whole numbers (counts of samples), which add up
  the same in any grouping. A sum of fractions does not, so those are
  added by the caller, item by item, in order.
- Work that depends on what came before stays on one thread, after the
  part that does not. A stone is laid only after every stone before it.

A build is given its number of threads where it starts (`build_with` in
[core/src/spec.rs](../core/src/spec.rs)), and that number is handed down to
each fitter. Tests use this to build thirty forms on one thread and on two,
three and seven, and require the same bricks, colours and figures to the
last bit. The forms in the table above and others, 71 in all with every
pattern on a disc, a band, a rounded disc and a solid and a hollow cube,
were also built by the code with everything on one thread, measured and
unmeasured, and all 142 builds were identical.

### When threads are not started

Starting a thread and waiting for it costs 50 to 100 microseconds on this
desktop, three quarters of a millisecond for sixteen. Work is therefore
shared out only where each thread gets about half a millisecond of it, and
with less than twice that it is done in place. Each caller states how many
of its items make half a millisecond, from a measurement of one item:

| Work | One item costs | Items for each thread |
|---|---|---|
| Measuring | 130 to 180 ns a cell of a solid sphere | 8,000 cells |
| Choosing each cell's brick | 200 ns a cell of a thin shell | 8,000 cells |
| Finding longer ramps | 1.2 µs a surface cell | 400 cells |
| Sampling an outline, lattice points | 40 to 210 ns | 4,000 points |
| Sampling an outline, crossed cells | 16 samples of 340 to 1,250 ns | 64 cells |
| Circles of a turned form | 1.6 µs times the radius squared | radii whose squares come to 300 |
| Scoring a sphere's layouts | 25 to 40 µs | 16 scorings |
| Stepping stones | 1.0 to 1.4 ms for a stone 10 across, by area | 1 stone from 6 across, 4 of the smallest |
| Tiles of cobblestone | 3 to 5 µs | 128 tiles |
| Tiles of bricks | 0.3 µs | 1,600 tiles |
| Tiles of a rug or medallions | 0.4 µs | 1,280 tiles |
| Bevelled tops of bricks | 0.2 to 0.4 µs a cell | 1,600 cells |

The two cell counts are set above half a millisecond's worth because a
form with less surface for its size than a sphere costs less a cell.

The threads are the standard library's
[scoped threads](https://doc.rust-lang.org/std/thread/fn.scope.html),
started for one call and joined before it returns. A pool kept alive
between builds would save the starting, at most three quarters of a
millisecond for each piece of work shared out, on builds that take tens of
milliseconds. It would also be state that outlives a build. The `rayon`
crate provides such a pool and was not added, because the fitting crate
depends on one small crate and compiles in seconds, and the part of rayon
needed here is about a hundred lines.

The browser page has no threads (see
[web-page.md](web-page.md#no-threads)). There `map_ranges` does the work in
place, and a build is the same bricks as on a desktop.

### What stays on one thread

| Work | Time | Why it stays |
|---|---|---|
| The search for an outline (`Search::solve` in `fit_planar.rs`) | 137 ms of a 320-wide arch's 150 | Its stretches are solved in rounds, each round seeing what the last took, and all of them share a table of the triangles already measured. Two mirror-image stretches could be solved at once, for at most half the time. |
| Columns under a lifted tube or band (`tube_columns` in `spec.rs`) | 20 to 40 ms | It looks through every segment of the curve for every cell. A grid of buckets, as the distance to a curve already uses, would remove most of that work, and that is the better change to make first. |
| Scattering the points of stepping stones | 3 to 5 ms | Each point is kept or dropped by the points kept before it. |
| Checking, refitting and laying stones | 1.3 ms for 200 stones, and about 1 ms more for each stone fitted again | Each stone is checked against every stone laid before it. |
| Taking longer ramps, and joining cubes into boxes | 9 ms and 4 ms of a thin hollow sphere's 55 | Both take the largest piece first and go by what is already taken. |
| The boards of planks | 0.03 ms | Each board starts where the last one ended. |
| Finding the flat faces and the sides a pattern is laid on (`pattern_flat_faces` and `pattern_side_faces` in `spec.rs`) | 20 to 30 ms for a cube 120 by 84 by 48, less the tiles | The faces are found from the finished bricks and laid one after another onto one list of them. The tiles drawn on each face are shared out, which took cobblestone on that cube from 120 ms to under 50. |

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
sphere's drag is forty times quicker. These times are on one thread. On
sixteen the let-go times are a quarter to an eighth of these (see
[Threads](#threads)), so the pause when the button is let go is shorter.
Measuring is still several times the cost of laying a form out, so a
dragged form is still not measured.

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
call per pose it uses, however many bricks it has. The form being edited
is drawn the same way by an object of its own, described under
[a touch of the brush on a lawn](#a-touch-of-the-brush-on-a-lawn) below.

Shadows are redrawn only when the bricks change, not every frame, because
turning the camera does not move a shadow. Edge lines cost no extra
geometry. They are drawn by the bricks' own shader from a number stored at
each corner of a face that says how near the face's edge it is
([editor/src/mesh.rs](../editor/src/mesh.rs),
[editor/src/material.rs](../editor/src/material.rs)).

Undo keeps settings, not bricks. A step in the history is each form's
settings, colours and place, a few hundred bytes, and the bricks are
rebuilt from them. 200 steps are kept.

### A touch of the brush on a lawn

A touch of the brush on a large lawn took about 0.7 s to show, and froze
single frames for up to 0.39 s on the way. The table follows one touch
through a frame, on the lawn 180 cells by 120 with 24 touches, before
and after the lawn was laid out and the form being edited was given a
shorter way to the graphics card. The times are from timers put in the
frame loop for the purpose and taken out again, with the same touches
made from a script (see below).

| Step of the frame | Before | After |
|---|---|---|
| Building the lawn | 500 to 650 ms, on another thread | 5 to 9 ms, in the frame |
| Every brick with its colour (`State::painted`) | 0.7 to 0.9 ms | 0.5 to 0.6 ms |
| The water's and the ghosts' sets | 10 to 20 ms | 0.1 ms |
| The bricks given to the card | 22 to 29 ms | 0.8 ms to find what changed, 1.0 to 1.5 to fill the buffers |
| The shadow map | 3 to 8 ms | 1.6 to 2.0 ms |
| The frame after | 45 to 55 ms | 16.7 ms, as any frame |
| From the touch to its bricks on screen | about 700 ms | 7 to 10 ms: the frame the touch is made in |

A stroke of ten touches, one every third frame, held 60 frames a second
throughout. Before, it drew three or four of its ten states, each half a
second late.

The form being edited goes to the card as a corner, a size and a colour
for each brick, and a vertex shader puts the brick's shape there
([editor/src/bricks.rs](../editor/src/bricks.rs),
[shaders/brick.vert](../editor/src/shaders/brick.vert)). The instanced
mesh the view used takes a matrix for each instance. Setting its
instances copied them, turned each instance's box by its matrix to find
the box round them all, split each matrix into three attributes and
converted each colour with a power function: 11 to 17 ms for the 27,600
bricks of the laid-out lawn. Filling three buffers with ten numbers a
brick takes 1.0 to 1.5 ms. The other forms are still drawn through the
instanced mesh, because each may stand on a grid turned its own way,
which is a matrix an instance, and they change only when one of them is
edited.

The water has two sets, as the solid bricks have, and a set is made
again only when its bricks change. One set was made again at every
change of the scene, by going through every brick of the workspace once
for each of the 125 shapes. That was 10 to 20 ms a touch, and as much
for every step of dragging any form that stood beside a lawn.

The bricks and their colours are asked for only when a set is to be
made again. A form that is only moved asks for none.

A form of many bricks is divided into tiles of ground, each with buffers
of its own, and only the tiles a change reaches are filled again
([editor/src/tiles.rs](../editor/src/tiles.rs)). A lawn is built in tiles
so that a touch changes few of them. A form has one tile for every
16,000 bricks, so the lawn above is one tile and is filled whole. A lawn
300 by 240, of 86,000 bricks, is four tiles, and a touch on it costs:

| Step | Lawn of 27,600 bricks | Lawn of 86,000 bricks |
|---|---|---|
| Building | 5 to 9 ms | 16 to 22 ms |
| Bricks with their colours | 0.5 ms | 1.4 to 2.8 ms |
| Finding the tiles that changed | 0.8 ms | 2.0 to 3.9 ms |
| Filling them | 1.0 to 1.5 ms | 1.3 to 5.1 ms |
| The shadow map | 1.6 to 2.0 ms | 1.6 to 1.9 ms |

The larger lawn's touch was 25 to 35 ms, so a stroke on it drew every
other frame. That was with every touch building the lawn from nothing.
With the last build kept ([a lawn](#a-lawn)), the first three steps
were measured again without a window, by the test
`timing_a_touch_on_lawns_of_three_sizes` in
[editor/src/state.rs](../editor/src/state.rs), which makes a touch and
takes it back in turn. Its building includes what the editor does with
a build besides asking for it.

| Step | 28,000 bricks | 86,000 bricks | 335,000 bricks |
|---|---|---|---|
| Building, from nothing | 4.4 ms | 11.3 ms | 36 ms |
| Building, the last build kept | 2.4 ms | 3.5 ms | 8.8 ms |
| Bricks with their colours | 0.35 ms | 1.0 ms | 4.0 ms |
| Finding the tiles that changed | 0.5 ms | 1.5 ms | 8.0 ms |
| Those three, before and after | 5.3 and 3.3 ms | 14 and 6 ms | 48 and 21 ms |

The lawn of 86,000 bricks is back within a frame for these three steps.
On the lawn of 335,000 the build is now the smaller part: colouring
every brick and comparing every brick with the one drawn before take 12
ms between them, and both go through the whole lawn at every touch.
Filling the buffers and the shadow map were not measured again, since
neither changed.

Shadows are redrawn when the bricks change and at most thirty times a
second while something is being dragged. Each redrawing draws every
brick once more, and during a drag the bricks change at every frame.

### Frames a second

With the lawn in view and nothing changing, the window draws at the
display's 60 frames a second before and after. With the wait for the
display switched off (a setting of the timers' build, not of the
editor), the same views measured:

| View | Before | After |
|---|---|---|
| Still, the whole lawn in view | 181 a second (5.5 ms a frame) | 273 (3.7 ms) |
| Flying over it, five legs of two seconds | 156 (6.4 ms) | 195 to 231 (4.3 to 5.1 ms) |

Part of the gain is the lawn's own: it is 27,600 bricks where it was
63,500. The panel takes 1.1 to 1.6 ms of every frame, and the drawing
calls for everything else 0.9 ms. A frame's drawing was 1.2 ms of the
processor's time with the lawn as 13 instanced meshes, since each mesh
is drawn as an object of its own and sets the lights again. As one
object it is 0.9 ms.

The blades are tens of thousands of thin boxes, which overlap on screen
many times over. The card draws them with the depth test on and writes a
pixel's colour only for the nearest so far. At these sizes the card
finishes a frame in under 2 ms, so nothing was done about the overlap.

Choosing a brush tool builds nothing and gives the card nothing: the
frames in which each of the three tools was chosen took 16.6 to 18.1 ms,
as the frames beside them did, before these changes and after. A tool's
settings are the brush's own and change no touch already made. The
freezes reported on choosing a tool were not reproduced as such. What
did freeze frames, before, was any build under way on another thread: it
took all sixteen of the processor's threads for half a second, and
frames in that half second took up to 0.39 s wherever the window's own
thread happened to be. A tool chosen within half a second of a touch
fell among those frames.

These figures were taken on a desktop with a Ryzen 7 5800X3D and an RTX
4060 Ti while other programs were using both, so the times of single
steps are the lower ends of what was seen, and the worst frames in a run
were often another program's doing. The mouse was stood in for by a
script in the frame loop that called what the mouse calls: `add_pull`
for an Alt + click, the same push onto the form's added points that a
stroke makes for each step, and the button held down or let go.

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
quarter of a second at radius 50 on one thread, and 55 ms on sixteen. They
still need tens of thousands of bricks.

A tree takes up to a second at its usual size on one thread, which is
what the browser has, and four times as long at twice the size, since its
time grows with its surface.

A lawn's first build grows with its area: 38 ms on sixteen threads for
one 600 cells by 480, and 207 ms on one. A touch after that is built
from what was kept of the build before, 4 to 6 ms on that lawn
([a lawn](#a-lawn)). What still grows with the lawn at every touch is in
the editor: every brick is given its colour again and compared with the
brick drawn before, 12 ms on a lawn of 335,000 bricks, so a touch on it
is about 21 ms before the graphics card is given anything
([a touch of the brush on a lawn](#a-touch-of-the-brush-on-a-lawn),
[future-work.md](future-work.md), More plants).

A very large revolve during a best-fit search is slow to respond. Each of
its builds is longer than a frame, so the view holds still for one build at
a time.

A long flat arch or band is as slow as it was, because the search for its
outline runs on one thread (see
[what stays on one thread](#what-stays-on-one-thread)). The browser page
has no threads at all (see [web-page.md](web-page.md#no-threads)), so every
build there takes its one-thread time.
