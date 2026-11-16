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

## More plants

Three trees, two bushes and two flowers are built: a pine, a palm and an
oak, a boxwood and a rose, sunflowers and tulips, each grown by its own
method ([algorithm.md](algorithm.md), section 7.10). What remains:

- **More bushes.** A hedge is the boxwood's canopy held inside a box in
  place of a dome. An upright or spiky plant (a juniper, a yucca, an
  agave) needs a rule of its own, a rosette of stiff blades, which the
  tulip's leaves are a start on. A hydrangea or rhododendron is the rose
  with a rounder habit and its blooms in large heads at the ends of the
  shoots.
- **More flowers.** A daisy is a small sunflower. A lily or a daffodil
  needs a trumpet, and a rose seen close needs petals in a spiral, which
  the golden angle would place. A bed of mixed flowers would be several
  kinds from one seed.
- **More kinds.** A birch or a willow is the oak's rule with other
  numbers: a leading stem that carries on, thin limbs, and for the willow
  a pull downwards on the ends of the branches. A date palm is the palm
  with a thicker trunk and stiffer fronds. A Scots pine needs a rule the
  pine does not have, for a crown that loses its lower boughs and flattens
  with age.
- **A bare tree.** The oak's skeleton is complete without its leaves, and
  a winter oak is a setting away. Its twigs stop at one cell thick, so a
  bare crown would be coarser than a real one.
- **Smoother limbs.** A tree is fitted cell by cell. Its trunk and limbs
  are round tubes along curves, which the pipe and lifted-tube layouts
  build with far fewer steps (sections 7.5 and 7.7). Building the limbs
  that way and only the foliage cell by cell would need the two joined
  where they meet, as added points are joined to a sphere today.
- **Speed.** A tree takes about a fifth of a second on a desktop's cores
  and up to a second on one, nearly all of it distance samples. Growing
  the tree and filing its parts is still done on one thread. Evaluating
  the samples on a graphics card is
  possible in OpenGL 3.3 and WebGL2, by drawing slices of the field into
  a texture, and would be several times faster again. It was set aside
  because those versions do not fix how arithmetic is rounded, so two
  machines could build different bricks from one seed. It could be used
  if the card only sorted cells into far, inside and near the surface,
  and the processor still decided every cell near the surface.

Code under a licence the project cannot take was not ported and is not to
be. That rules out the GPL generators (Veloren's `tree.rs`,
friggog/tree-gen, Blender's Sapling add-on and Arbaro) and proctree.js,
which carries no licence.
