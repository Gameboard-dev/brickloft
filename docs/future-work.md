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

Forms joined with Ctrl + drag are fitted as one solid where they meet, as a
smooth union with a blend the player sets, and keep their own bricks
everywhere else. How it is done, what it costs and where it falls short are
in [algorithm.md](algorithm.md), "Forms joined on purpose". What remains:

- **A smoother fit at the seam.** The join is fitted cell by cell, with
  single wedges and ramps stretched one way, and it shows as a rough band
  against a sphere's own facets. Fitting the seam with the stretched corner
  pieces the sphere fitter uses would need a layout for a surface that is
  no sphere, which nothing here has yet.
- **A narrower band.** A sloped brick the join touches is refitted whole,
  so the band is as wide as the form's own bricks are large. Cutting a
  stretched brick into shorter ones of the same slope, where the game has
  such a brick, would let the band stop nearer the seam.
- **Forms half a microbrick off one another.** They are joined by cutting
  alone, with a notch at the seam, because their cells do not line up. The
  editor could offer to move the joined form by half a microbrick.
- **Brick size.** A join is fitted at single microbricks even between forms
  built at a larger brick size.
- **Taking material away.** A join only adds. A form subtracted from
  another (a window cut through a wall) is the same method with the
  distance of one form negated, and is not built.
- **A blend above 6.** It needs the forms' distances to stay exact further
  than 8 microbricks from their surfaces, which costs every curve's
  distance search.

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
oak and a beech, a boxwood, a hedge and a hydrangea, peonies, sunflowers,
tulips and petunias, and reeds, each grown by its own
method ([algorithm.md](algorithm.md), section 7.10). What remains:

- **More bushes.** A hedge is the boxwood's canopy held inside a box in
  place of a dome. An upright or spiky plant (a juniper, a yucca, an
  agave) needs a rule of its own, a rosette of stiff blades, which the
  tulip's leaves are a start on. A rose bush was built and taken out: its blooms were
  balls, and a rose needs petals in a spiral, as the peony now has. A hedge
  that turns a corner or follows a curve, and a dot to drag a hedge's
  length by, are not there.
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
- **Finer bare trees.** A bare oak or beech carries twigs on from its
  branch ends, but a twig is as thick as a branch end, one cell, so the
  crown is coarser than a real one. A larger tree is finer.
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
