# Future work

Two additions are planned. Neither changes how forms are fitted today (see
[algorithm.md](algorithm.md)).

## More for Claude to do through MCP

Claude can already drive the editor: add, change, colour, turn, measure and
remove forms, move the camera and take screenshots, through the Model
Context Protocol ([mcp.md](mcp.md)). Two things are still to come.

**Rebuilding an object from a point cloud.** Given a point cloud of a solid
3D object, which is a list of points sampled from its surface, Claude would
recreate the object from the editor's own forms: choosing a sphere here and
an arch or a revolved profile there, then sizing, placing, turning and
colouring each until the forms together match the cloud. Each change would
appear in the editor as it is made, so the player watches the object being
assembled and can stop or correct it at any step. A cloud has to reach
Claude in a form it can reason about, and a large one will need thinning
first. Judging how well the forms match the cloud needs a measure the
editor can report, such as the share of the cloud's points lying on or near
the bricks.

**Copying to the game.** Claude cannot put the workspace on the clipboard,
so the player still presses Ctrl+C. A tool for it would let Claude finish a
build end to end, at the cost of a tool that writes outside the editor,
which the present tools deliberately do not.

## Blending forms into one

Each form is fitted with bricks by itself today. Two forms that overlap on
one grid are two sets of bricks, and the editor keeps forms on a grid clear
of each other for that reason. Blending would fit several forms as one
solid.

The description of a form already allows it. A form is a distance function
(see [algorithm.md](algorithm.md)), and two distance functions join into
one by taking the smaller of the two numbers at each place, which is the
union of the two solids. The fitters and the measurement take any distance
function.

A **smooth union** joins two solids with a rounded fillet where they meet,
where a plain union leaves a sharp crease. It replaces "the smaller of the
two numbers" with a smooth version of the smaller, which dips below both
where the two are nearly equal. The stepping stones already use the same
device to round a stone's corners. One common form, for two distances $`a`$
and $`b`$ and a blend width $`k`$, is

```math
h = \max\left(k - \lvert a - b\rvert,\ 0\right), \qquad \mathrm{smin}(a, b) = \min(a, b) - \frac{h^2}{4k}
```

Far from the join, $`h`$ is zero and the result is the plain union. Within
$`k`$ of the join it is lowered by up to a quarter of $`k`$, which fills
the crease in.

What it needs:

- **A way to say which forms blend:** a group of forms on one grid, with a
  blend width.
- **A fitter for the result.** A blended solid has no layout of its own, so
  it would be fitted cell by cell, the roughest method. Keeping each
  form's own smooth layout away from the join and refitting only near it,
  as added points do today, is the route that would keep the quality.
- **A distance function that stays an underestimate,** which the smooth
  union above does, so that skipping empty space stays correct.

## Patterns by wave function collapse

[Wave function collapse](https://github.com/mxgmn/WaveFunctionCollapse)
fills a grid with tiles so that every tile agrees with its neighbours. It
starts with every cell able to hold any tile. It then repeatedly picks the
cell with the fewest tiles left to choose from, fixes one of them, and
removes from the neighbouring cells every tile that no longer fits, which
is the propagation. The editor would use it to complete a pattern from a
small start, in two dimensions for a floor or a wall and in three for a
pattern with depth.

The stepping stone path is built (see [algorithm.md](algorithm.md), section
7.8). It scatters points and divides the ground among them, since wave
function collapse turned out not to suit stones. The patterns still planned:

| Pattern | What it is | How it starts |
|---|---|---|
| Mosaic floor | small coloured tiles forming motifs and borders | a random seed and rules for which tiles may meet |
| Persian rug | a central medallion, a field and nested borders, symmetric in both directions | a random seed and the rug's rules of symmetry |
| Stone slab wall | slabs of varied size in courses, with joints that do not line up | the wall's size and the range of slab sizes |

For the mosaic floor and the Persian rug, a random seed chooses the first
few tiles and the rules decide everything else, so the same seed always
gives the same pattern. A Persian rug's rules resemble a snowflake's. The
whole is fixed by one part repeated under mirror images, so only a quarter
(or an eighth) is generated, and the rest is its reflection. The rules for
the rug would also set where borders may run and which motifs may sit
inside which.

Open questions:

- **Tiles as bricks.** A tile here is a small group of bricks with a
  colour, so the pattern's output is bricks directly, without the fitting
  step a curved form needs.
- **Dead ends.** Wave function collapse can reach a cell with no tile left
  that fits. The usual answers are to step back and try another tile, or
  to start again with the next seed. Which one keeps the editor responsive
  on a large floor has to be measured.
- **Patterns on forms.** A pattern might later be laid over a fitted form
  (a mosaic on a dome) by choosing colours for the form's bricks. That
  needs the pattern mapped onto a curved surface and is a separate step.
