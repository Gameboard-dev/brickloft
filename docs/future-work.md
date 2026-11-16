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
everywhere else. A joined form can also be cut out of the others, is
settled onto the cells of the form it joins, and is joined at the forms'
own brick size where they share one. How it is done, what it costs and
where it falls short are in [algorithm.md](algorithm.md), "Forms joined on
purpose". What remains:

- **A layout for the seam.** The seam is evened out and laid with
  stretched ramps and corner pieces, which took a tenth off its
  disagreement with the joined surface and a sixth off its steps, and it is
  still a rough band against a sphere's own facets. The measurements in
  algorithm.md say why: a sphere's facets are smooth because they meet edge
  to edge, at the cost of closeness, and bricks chosen one at a time for
  closeness do not. Three ways of trading closeness for smoothness without
  a layout were tried and none lowered the steps. What is needed is the
  bricks of a patch chosen together. One way to try: over a patch where the
  surface is a height above a plane, snap the heights at the corners of a
  grid of rectangles to whole cells, and fill each rectangle with the brick
  its four corners call for (a tetrahedron or a hip for one high corner, a
  ramp for two, a cut cube or an inner corner for three). Neighbours then
  share the line between their common corners. Open questions it leaves:
  which way a patch faces where the surface turns over, how two patches of
  different grids meet, what to do with a rectangle whose high corners are
  opposite, and how the patch meets the forms' own bricks at the zone's
  edge.
- **A narrower band.** A sloped brick the join touches is refitted whole,
  so the band is as wide as the form's own bricks are large. Cutting a
  stretched brick into the shorter bricks of its slope was built, measured
  and taken out. A brick can be cut only where its slope crosses whole
  cells, which needs its lengths to share a factor: none of the sloped
  bricks of a sphere of radius 12 or of a turned form do, 45% of a sphere
  of radius 20, 68% of a ring. On two spheres of radius 20 it left 4.6%
  fewer cells to refit, with more bricks and more single wedges at the
  band's new edge. A narrower band would have to come from the fitters
  laying bricks near a seam that can be cut, or from the layout above
  reaching to the forms' own facets.
- **A hole's wall.** The wall of a hole is fitted cell by cell all the way
  through the form it is cut in, where the form that made it has a layout
  of its own for the same surface. Turning that layout inside out is not
  possible brick for brick, since the hollow side of a sloped brick is not
  a brick. A fitter run on the hole's inside as a solid of its own would
  be.
- **A blend above 6.** It needs the forms' distances to stay exact further
  than 8 microbricks from their surfaces. Widening that for every form was
  measured: curves took 1.5 to 8 times as long to build
  ([algorithm.md](algorithm.md), "The joined solid"). Two ways are left.
  A form's distance could be made a second time with a wider margin, for
  joins alone and only when a blend above 6 is asked for. And forms joined
  at a brick size above 1 already have a margin that many times as wide,
  so their blend could go to 6 bricks of that size for nothing: it needs
  the slider, the saved form and Claude's tools to take a limit that
  depends on the brick size.
- **Brick sizes out of step.** Forms of one brick size are settled to whole
  bricks apart and joined at that size. Forms of two different sizes are
  joined at single microbricks, with a band of fine bricks round the seam.
  Joining them at the smaller of the two, where it divides the larger,
  would be coarser and is not built.
- **Seeing what is cut out.** A form that is cut out is drawn as plain red
  glass, without edge lines, and its glass is not sorted by depth, so
  through several layers of it the tint is uneven.

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

- **Slivers between fitted stones.** Stepping stones with the gap at 0
  share their edges, each outline's corners moved to the grid
  ([algorithm.md](algorithm.md), 7.8). Slivers remain where an edge ends
  on the rim of the region and not on a corner of the grid, and along an
  edge that is nearly level, which is fitted as a slope from one side and
  a step from the other now and then. Ending each edge at a corner of the
  grid on the rim, and holding the fitter to one reading of a nearly level
  edge, would close them.
- **More depth.** Depth is one cell and one kind to a style: bevelled
  bricks, grooved planks from size 3, and cobblestone's sunk mortar.
  Medallions and a rug have none, and planks of the usual size are flat,
  because a board two tiles broad would be half ramp.
- **Patterns on curved surfaces.** A tile pattern is laid on the flat part
  of a form and on the sides of a box, and the curved part stays plain.
  Laying it over a sphere, a turned profile, a tube and a torus was built
  and taken out: the form's own bricks were kept and each given the
  pattern's colour at its place on a sheet laid over the form. It does not
  work, because a sloped brick is one colour and a curved form's bricks
  are large and stretched (a sphere's are 2 cells by 3 by 10 and cannot be
  cut smaller without changing the surface), so the pattern's lines fall
  on brick edges that do not follow them: saw teeth for courses of bricks
  and fragments for a rug. Cutting the surface into bricks small enough
  to carry the lines multiplies the count and gives up the smooth fit.
  Stepping stones are laid only through a flat form.

## Mazes

A maze of rings or of squares is built, with plain, patterned or hedge
walls ([algorithm.md](algorithm.md), section 7.11). What remains:

- **A texture on curved faces.** A pattern on a ring wall is as coarse
  as the long sloped bricks of its face, because a pattern may not move
  a form's outline. A pattern mapped onto curved surfaces, which the
  patterns still need for domes, would serve walls too.
- **Hedge corners and feet.** A hedge wall is the Plant form's hedge
  along each wall's line, joined to the next by running into it. A hedge
  that turns a sharp corner would make a ring wall and its spoke one
  hedge. The stems at a hedge's foot are few on a long wall.
- **More mazes.** A room larger than one ring, more than one way in,
  loops (a braid maze), cells of six sides, and a maze on several floors
  joined by stairs. Each is a change to the cells and their walls; the
  carving and the building would stand.

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
  close up. The view does not draw water translucent. A pond's patch
  hangs below the level it was laid on.
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
