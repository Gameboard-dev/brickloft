# Future work

What is planned and not yet built: more for Claude to do, and what
remains for joined forms, the patterns and the plants. None of it changes
how forms are fitted today (see [algorithm.md](algorithm.md)).

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

Six patterns are built: stepping stones, medallions, a rug, planks,
bricks and cobblestone. They were first planned as uses of
[wave function collapse](https://github.com/mxgmn/WaveFunctionCollapse),
which fills a grid with tiles so that every tile agrees with its
neighbours. None of them uses it now.
[algorithm.md](algorithm.md), sections 7.8 and 7.9, describes each method
and gives the reasons.

| Planned as | Built as | How |
|---|---|---|
| Stepping stone path | **stepping stones** | points scattered by dart throwing, the ground divided among them as a Voronoi diagram, each stone fitted by itself |
| Stone slab wall | **bricks**, **cobblestone** | bricks as courses and joints on the grid, each line moved by an amount of its own, with tops that can be bevelled by one cell; cobbles as a Voronoi diagram of one point to each square of a grid |
| Mosaic floor | **medallions**, **planks** | a few figures repeated, with bands of a distance measure switched on and off; boards of lengths drawn from the seed |
| Persian rug | **rug** | a medallion carpet: borders, a medallion of lobed layers, corner pieces, and a field of scrolls packed circle by circle |

A save made by an earlier version may name a pattern that is gone. It
opens as bricks or cobblestone, whichever is nearer, or with no pattern.

Wave function collapse was written and tried, and the solver remains in
[core/src/wfc.rs](../core/src/wfc.rs) with its tests. Its rules reach a
few tiles, so it could not give a stone a size, a rug a medallion at its
middle, or a floor its symmetry, and the floors it made from small samples
were even and aimless. A pattern made only of small repeated motifs is
the case it still suits, and no such pattern is planned yet.

What remains:

- **Seams between fitted stones.** Stepping stones with the gap at 0 keep
  seams, most of them hairline and some up to a cell wide. Each stone is
  fitted by itself, and a stone whose bricks would overlap a neighbour's is
  drawn in. Closing the seams needs the two stones either side of an edge
  to be given the same edge, which fitting each stone by itself does not
  do.
- **A small rug.** The rug shows its detail on a floor 100 tiles or more
  across. Below that its medallion has one or two layers and its guards
  are plain lines.
- **A pattern with depth.** The one-cell bevel of bricks is the only depth
  a pattern has. Every other pattern is one layer of tiles pushed through
  the form.
- **Patterns on curved surfaces.** A tile pattern is laid on the flat part
  of a form and on the sides of a box, and the curved part stays plain. A
  mosaic on a dome needs the pattern mapped onto a curved surface and the
  surface cut into bricks small enough to carry it. Stepping stones are
  laid only through a flat form.

## More plants

Twenty-two kinds of plant are built: eight trees, five bushes, seven
flowers and a bed of them, reeds and grass, each grown by its
own method ([algorithm.md](algorithm.md), section 7.10). What remains:

- **More plants still.** A rose was left out again: at this size its head
  is a ball. A lily, a fern, a cactus and a bamboo have rules of their
  own that are not written. A hedge still cannot fork or meet another at
  a tee.
- **Smoother limbs.** Trunks are eight-sided on the lattice and built of
  tall bricks. Limbs are still fitted cell by cell, and a slanting limb
  is steps. Building them as lifted tubes (section 7.7) would need them
  joined to the trunk and the foliage where they meet.
- **Finer bare trees.** A twig is one cell thick, as a branch end is, so a
  bare crown is coarser than a real one. A tree over 150 tall forks its
  twigs once more. Half-size bricks do not exist, and a wedge is a cell
  wide at its foot, so thinner twigs would be drawn with gaps.
- **Grass.** Its blades are one cell thick, so a lawn is coarse tufts
  close up. A pond's patch hangs below the level it was laid on. Water
  has two levels at most, every raised pond shares one height, and a
  stream is four to six cells of water across where two were asked for:
  the soil is laid on a grid 2.4 apart and a narrower channel closes
  over.
- **Speed.** A tree takes about a fifth of a second on a desktop's cores
  and up to a second on one, nearly all of it distance samples. Colouring
  is shared over threads. Growing a plant is one run of random numbers and
  stays on one thread, and filing its parts was not shared out. A graphics
  card was set aside twice. Evaluating the samples on it could give
  different bricks on different machines, because OpenGL 3.3 and WebGL2
  do not fix how arithmetic is rounded. Having it only sort cells into
  far, inside and near, with the processor deciding every near cell, was
  measured: four fifths of a tree's samples are in near cells, so the
  card could take a fifth at most (algorithm.md, section 7.10).

Code under a licence the project cannot take was not ported and is not to
be. That rules out the GPL generators (Veloren's `tree.rs`,
friggog/tree-gen, Blender's Sapling add-on and Arbaro) and proctree.js,
which carries no licence.
