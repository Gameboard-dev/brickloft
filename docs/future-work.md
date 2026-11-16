# Future work

What is planned and not yet built: more for Claude to do, blending forms
into one, what remains for the patterns, and trees. None of it changes how
forms are fitted today (see [algorithm.md](algorithm.md)).

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

Forms can already be joined without blending. A form dragged with Ctrl
held is moved into another, and its bricks are cut back where the two
overlap (see [algorithm.md](algorithm.md), "Forms joined on purpose").
That keeps each form its own bricks and its own settings, at the cost of a
notch a cell or two deep where a sloped brick had to be left out. Blending
would remove the notch by refitting the join.

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

## Patterns

Five patterns are built: stepping stones, medallions, a rug, planks and
masonry. They were first planned as uses of
[wave function collapse](https://github.com/mxgmn/WaveFunctionCollapse),
which fills a grid with tiles so that every tile agrees with its
neighbours. None of them uses it now.
[algorithm.md](algorithm.md), sections 7.8 and 7.9, describes each method
and gives the reasons.

| Planned as | Built as | How |
|---|---|---|
| Stepping stone path | **stepping stones** | points scattered by dart throwing, the ground divided among them as a Voronoi diagram, each stone fitted by itself |
| Stone slab wall | **masonry** | every piece the ground nearest its own point, with a measure that makes rectangles for bricks and goes over to the plain distance for stones; the tops bevelled cell by cell |
| Mosaic floor | **medallions**, **planks** | a few figures repeated, with bands of a distance measure switched on and off; boards of lengths drawn from the seed |
| Persian rug | **rug** | a medallion carpet: borders, a medallion of lobed layers, corner pieces, and a field of scrolls packed circle by circle |

Bricks and slabs were two patterns before masonry replaced both, and a
snowflake pattern was built and then removed as having no use. A file that
names bricks or slabs opens as masonry, and one that names a snowflake
opens with no pattern.

Wave function collapse was written and tried, and the solver remains in
[core/src/wfc.rs](../core/src/wfc.rs) with its tests. Its rules reach a
few tiles, so it could not give a stone a size, a rug a medallion at its
middle, or a floor its symmetry, and the floors it made from small samples
were even and aimless. A pattern made only of small repeated motifs is
the case it still suits, and no such pattern is planned yet.

In progress:

- **Parallel processing.** Work is under way to use several processor
  cores. It is not finished, and no result is recorded here yet.

What remains:

- **Seams between fitted stones.** Stepping stones with the gap at 0 keep
  seams, most of them hairline and some up to a cell wide. Each stone is
  fitted by itself, and a stone whose bricks would overlap a neighbour's is
  drawn in. Closing the seams needs the two stones either side of an edge
  to be given the same edge, which fitting each stone by itself does not
  do.
- **Bevelled corners of stones.** The bevelled top of an uneven stone in
  masonry is clean along its sides and jagged at its corners. Where two
  edges meet at an odd angle, the corners a cell reaches are no brick's
  shape, and the cell is laid whole or left out. Bricks, whose corners are
  right angles, are not affected.
- **A small rug.** The rug shows its detail on a floor 100 tiles or more
  across. Below that its medallion has one or two layers and its guards
  are plain lines.
- **A pattern with depth.** Masonry's bevel is the only depth a pattern
  has. Every other pattern is one layer of tiles pushed through the band's
  depth.
- **Patterns on forms.** A pattern might later be laid over a fitted form
  (a mosaic on a dome) by choosing colours for the form's bricks. That
  needs the pattern mapped onto a curved surface and is a separate step.

## Trees and flora

Trees, shrubs and flowers are not started. A survey of existing generators
found no dependency worth taking, so the generator would be written fresh
in the core, in Rust, like the rest.

The planned method has three parts.

- **The skeleton.** A tree is a trunk that puts out branches, each of
  which puts out its own, with every level described by a few numbers: how
  many branches, at what angle, how long, how much they bend.
  [ez-tree](https://github.com/dgreenheck/ez-tree) (MIT) is a readable
  reference for such a recursive, parametric skeleton.
- **Branch thickness.** Leonardo da Vinci's rule sets the radii: where a
  branch of radius $`r_0`$ divides into branches of radii $`r_1`$ and
  $`r_2`$, the cross-sections add up,

  ```math
  r_0^2 = r_1^2 + r_2^2
  ```

  so a tree thins towards its twigs without a thickness chosen by hand for
  each branch.
- **Foliage and flowers.** A crown is a union of rounded blobs about the
  ends of the branches, which the fitters already take as one distance
  function (see [algorithm.md](algorithm.md), section 5). A flower head
  places its seeds or petals by Vogel's model of phyllotaxis, which puts
  the $`k`$-th at the angle $`k \cdot 137.5^\circ`$ and the distance
  $`c\sqrt{k}`$ from the middle and gives the spirals of a sunflower.

Code under a licence the project cannot take is not to be ported. That
rules out the GPL generators (Veloren's `tree.rs`, friggog/tree-gen,
Blender's Sapling add-on and Arbaro) and proctree.js, which carries no
licence.
