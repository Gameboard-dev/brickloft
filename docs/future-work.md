# Future work

What is planned and not yet built: more for Claude to do, what remains
for joined forms, the patterns, the plants and the furniture, what
remains for rule programs (MarkovJunior's language, which the editor now
runs), and a plan for forms made by other programs (meshes generated
from a prompt). None of it changes how forms are fitted today
(see [algorithm.md](algorithm.md)).

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

Twenty-three kinds of plant are built: eight trees, five bushes, seven
flowers and a bed of them, a single flower head, reeds and grass, each
grown by its own method ([algorithm.md](algorithm.md), section 7.10).
What remains:

- **More plants still.** A lily, a fern, a cactus and a bamboo have rules
  of their own that are not written. A hedge still cannot fork or meet
  another at a tee.
- **Hedge on other forms.** An arch can be built as hedge, grown over
  its distance function ([algorithm.md](algorithm.md), section 7.10). A
  spline, a Bézier curve, a circle and a sphere have distance functions
  too. The curves need one line in `Spec::hedged` and the panel's list,
  and then looking at. A circle and a sphere are laid out by other
  fitters and need a branch each.
- **An arch of hedge to walk through.** A plant's leaves can be written
  with their collision off. An arch's cannot yet: the plant's tickbox is
  kept in the setting that makes an arch a round tube.
- **A thin arch of hedge.** At the least size, 6 by 6, the tufts are as
  large as the band and are cut level on every side, so it is a ragged
  strand more than a clipped hedge.
- **More single flowers.** A rose, a water lily and a gerbera are built
  from one kind of petal. A dahlia was built from it and taken out: sixty
  petals four cells wide were confetti from above at 48 cells across. It
  would read at twice the size, so it could be offered above some spread.
  A camellia, a lily and an orchid are not written.
- **A rose from straight above.** From the side and at three quarters
  the rose shows separate cupped petals. From straight above it shows
  arcs of petal with dark gaps between them and no clear spiral in its
  heart, because the heart's petals are two cells thick round a hole one
  cell wide. A larger rose shows more.
- **A rose bush again.** The rose is a single head because it needs 40
  cells. A bush of such heads would be 300 cells tall.
- **A bloom facing down.** Tilt stops at 90 degrees. Past that the
  form's quarter turns flip it, in steps of 90.
- **Smoother limbs.** Trunks are eight-sided on the lattice and built of
  tall bricks. Limbs are still fitted cell by cell, and a slanting limb
  is steps. Building them as lifted tubes (section 7.7) would need them
  joined to the trunk and the foliage where they meet.
- **Finer bare trees.** A twig is one cell thick, as a branch end is, so a
  bare crown is coarser than a real one. A tree over 150 tall forks its
  twigs once more. Half-size bricks do not exist, and a wedge is a cell
  wide at its foot, so thinner twigs would be drawn with gaps.
- **Grass.** Its blades are one cell thick, so a lawn is coarse tufts
  close up. Water has two levels at most, every raised pond shares one
  height, and a stream is five or six cells of water across. A stream
  could now be narrower: the soil is laid cell by cell, and the 2.4 grid
  that closed a narrow channel over is used only to find where water
  stands. A corner where two bare slopes meet is a step, since the
  sloped piece for it is a different shape for each way it can face.
- **Cliffs, waterfalls and a random landscape.** The ground can be asked
  its height, its slope and its water cell by cell without the bricks
  being built (`grass::Ground`, [algorithm.md](algorithm.md), section
  7.10). A cliff would stand where the slope is steep and a waterfall
  where a stream's level steps down, and a landscape made at random
  would be touches chosen by rule. None of the three is written.
- **A touch on a very large lawn.** A build works out the whole ground
  and lays every tile each time, 16 to 22 ms for a lawn 300 by 240
  ([performance.md](performance.md#still-slow)). A touch changes the
  ground within its range and the bricks of the tiles it reaches. Two
  things kept between builds would make its cost the same on any lawn:
  the heights as they were before the last touch, to which a new touch
  is applied where it reaches, and each tile's bricks, kept under
  everything the tile is laid from (the ground of its cells and those
  round it, the lawn's settings, the reeds over it) and compared in
  full, so that a kept tile is never another tile's. They were not
  built because the lawns measured stayed within two frames a touch.
- **One draw call for every shape.** The form being edited is drawn by
  one object with a call for each brick shape in it, a dozen for a lawn
  ([performance.md](performance.md#a-touch-of-the-brush-on-a-lawn)). A
  vertex shader that moved the corners a shape lacks onto the corners it
  keeps would draw every shape from one cube, in one call. The other
  forms are still drawn through three-d's instanced mesh, one object a
  shape, which costs about 45 microseconds a shape a frame; they could go
  the same way as the form being edited if each form's grid were given
  as one matrix for its bricks.
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

## More furniture

Twenty-seven kinds of furniture are built, in five styles
([algorithm.md](algorithm.md), section 7.12). What remains:

- **Kinds not built.** A wardrobe with a mirror, a bunk bed, a rocking
  chair, a piano, shelves on a wall, a seascape and a still life are not
  written. An easel for a painting, and a painting that leans, were not
  built: a picture hangs flat.
- **Raked backs.** A wooden chair's back stands upright. A back that
  leans is a sheared piece for every slat, which was done only for an
  upholstered back's cushions.
- **Trellis shapes.** A fan and an obelisk were left out. Their slats are
  at angles other than 45 degrees, which bricks give only as steps.
- **Smoother pictures.** A picture is whole cells. The tile code can cut a
  cell in two along its diagonal, which would smooth a sun's rim and a
  hill's slope, and is not used.
- **Portraits.** A sitter looks straight ahead. There are six ways of
  wearing hair, and no long beards, scarves or earrings.
- **Plants in pots.** The plant is fitted, so a pot is hundreds of bricks
  and takes tenths of a second, and when its report is saved the
  measurement treats the plant's sloped bricks as boxes. A single
  flower in a vase on a table is not built.
- **Bookcases and styles.** A bookcase keeps its own settings and takes no
  notice of the style. Every bay has the same shelves.
- **Fronts.** A door cannot stand open, and glazed doors need a
  translucent brick, which the view does not draw.
- **Joining.** Furniture is joined to another form by cutting alone, and
  a brick a join has cut is written to the game as plain plastic, not as
  metal or light.
- **A front facing any way.** The front is on the low Y side. Another
  side is faced by a quarter turn of the whole form.

Code under a licence the project cannot take was not ported and is not to
be. That rules out the GPL generators (Veloren's `tree.rs`,
friggog/tree-gen, Blender's Sapling add-on and Arbaro) and proctree.js,
which carries no licence.

## Forms made by other programs

Two requests share one piece of work. A MarkovJunior model produces a grid
of coloured cells, and a text-to-3D model produces a triangle mesh. Neither
is one of the editor's forms, so each needs a way in, and both then need
fitting with bricks. Of the plan below, the MarkovJunior part is built
and the rest is not. It was written on
2026-11-16 from each project's public page and licence file, read that day.

### What the two need in common

A new form, **Imported**, holds what another program made: a grid of
cells, or a mesh. It has a size in microbricks, a seed where the source has
one, and the usual place, grid, colours and quarter turns. Its bricks come
from one of two fitters, chosen in the panel.

- **Blocks.** Each cell of a grid becomes a cube of a whole number of
  microbricks, and cubes of one colour are joined into the largest boxes
  ([`pattern::joined`](../core/src/pattern.rs) does this today for the
  tiles of a pattern). The result is exact and has no sloped brick. It
  suits a model whose cells are meant to be seen as cells, such as a
  dungeon or a pixel picture.
- **Smoothed.** The grid or mesh is turned into a distance function and
  given to the fitters every other form uses, so a staircase of cells that
  stands for a roof becomes ramps and a round tower becomes wedges
  ([algorithm.md](algorithm.md), sections 4 and 5). A grid's distance
  function is read from its cells, smoothed over one cell. A mesh's is its
  distance to the nearest triangle, signed by the winding number, which
  still gives an inside and an outside when the mesh has holes or
  overlapping shells, as generated meshes often have.

The second fitter is where Zhou, Chen and Xu's method applies
([doi:10.1111/cgf.13603](https://doi.org/10.1111/cgf.13603)). Their
abstract says the system keeps an architectural model's visual features,
which it names as repeating components, shape details and planarity, and
maps them to brick types, after deforming the mesh so that it agrees with
the positions bricks can take. Its brick library is 11 cuboid, 13 sloping
and 5 circular bricks. The full text could not be opened when this was
written, so the three ideas below are taken from the abstract and are to be
checked against the paper before any is built.

- **Planes snapped to the grid.** A large flat face a fraction of a cell
  off the grid is fitted as a ragged surface. Finding the mesh's large
  planes and moving each to the nearest grid plane first makes a wall one
  layer of boxes. This is the paper's deformation step, and it matters
  only for meshes: a MarkovJunior grid is on the grid already.
- **Repeats fitted once.** A model built from tiles repeats the same block
  of cells many times. Fitting each distinct block once and copying its
  bricks gives every window the same bricks and costs one fit, where
  fitting the whole grid would cost one for each copy and could lay two
  identical windows differently.
- **Features kept at least two cells thick.** A railing or a mullion
  thinner than a brick is lost by any fitter. Thickening it to the least a
  brick can show keeps it, at the cost of exactness there.

Brickadia's wedges stretch to any whole-number slope, which the paper's 13
fixed sloping bricks do not, so the existing fitters are kept and the paper
supplies the three steps before them. Its layout search for bricks that
hold together is not needed: a brick in the game does not fall.

A form has eight colour numbers today
([`MOST_COLOURS`](../editor/src/paint.rs)). MarkovJunior's palette has 16
and more, and a mesh's texture has thousands. An Imported form therefore
carries its own list of colours, reduced to 32 at most by grouping the
nearest together, and each brick takes the colour of the cell or the
triangle nearest its middle. A sloped brick is one colour, as everywhere.

### MarkovJunior

This part is built, and differently from the plan that stood here. The
plan was to start the original C# program and read the file it wrote,
and to port the language to Rust later only if that was used. The port
came first: an interpreter for the language is in
[core/src/rewrite/](../core/src/rewrite.rs), the form is **Rule
program**, and seven of the original's programs come with the editor
(see [features.md](features.md#rule-programs) and
[algorithm.md](algorithm.md), section 7.13). Starting the original was
dropped because it needs .NET installed and gives a result only when it
has finished, and the point of these programs in an editor is to watch
them grow and stop them.

What remains:

- **`observe`.** A rule node may name an end state, and the original
  then searches for a sequence of rule applications that reaches it.
  Its Sokoban and path-finding programs depend on this. It is the
  largest missing part for programs written by other people: about 400
  lines in the original (`Observation.cs` and `Search.cs`), a backward
  pass that finds how far each cell is from its goal and a best-first
  search over whole grids. A program that uses it is refused today.
- **Rules kept as `.png` pictures, `convchain`, and `wfc` from a sample
  picture.** These three serve the original's flat programs. The first
  needs only the `png` crate the editor already builds. Each is refused
  today with a message that names it.
- **A check against the original.** No run of the C# program has been
  compared with this interpreter, because .NET was not installed where
  it was written. The random numbers differ on purpose, so the check is
  of kind and of rates: whether ModernHouse fails on about one seed in
  three there too, and whether each program's results look alike.
- **Brick shapes written for a tile.** A program's cornices, railings
  and window sills are single cells. A tileset could carry, for each
  tile, the bricks to build it from in place of its cells, so that a
  cornice is a wedge. This was not tried: it needs shapes drawn by hand
  for each of the original's tilesets, and it should be shown in
  pictures to improve a building before it is kept.
- **Roofs and slopes that are not flights.** The wedge finish fills
  flights of two steps or more. A single step is left square, since it
  is also the edge of every slab, so a low parapet or a one-step roof
  edge stays stepped.
- **A file dropped on the window.** The editor has a function that
  takes a dropped `.xml` file (`State::drop_file`), and nothing calls
  it yet. The windowing library reports dropped files only to a frame
  loop written by hand, which the mesh import branch adds; when that is
  merged, its loop over dropped files needs one line to offer each to
  this function first.
- **A picker for the file.** A path is typed or pasted. A file dialog
  is a crate for each of three systems' dialogs, which the project has
  so far done without.
- **The time to open a save.** A save keeps settings and no bricks, so
  opening one runs each program again: ten seconds for a CarmaTower.
  The window keeps answering while it does. Keeping finished grids in a
  folder beside the saves would make this instant, at the price of a
  second place for a form to be read from.

### A form from a prompt

**Cube cannot be used as asked.** The licence file in
[Roblox/cube](https://github.com/Roblox/cube/blob/main/LICENSE) is titled
"CUBE3D RESEARCH-ONLY RAIL-MS LICENSE", covers "Cube3d-v0.1 and related
inference code", and allows academic or research use only. Making builds
to play with in Brickadia is not research, so wiring Cube into Brickloft
would put every player who used it outside the licence. Two things are
unclear and would need Roblox's answer: the model card for the current
weights, `cube3d-v0.5`, says only "openrail", and the licence file names
the older version. Until Roblox says otherwise the research-only text is
the one to go by. Cube would also be a poor fit on other grounds: it asks
for a graphics card with 16 GB of memory, or 24 GB for its fast mode, and
its meshes have no colour.

**The route chosen: Rust from the prompt to the bricks.** The player asked
for no Python, and a second search (2026-11-16) found the parts in Rust.
No model that turns text straight into a mesh has been ported to Rust, so
the prompt goes by way of a picture:

1. **Prompt to picture.** [Candle](https://github.com/huggingface/candle),
   Hugging Face's machine-learning library in Rust (MIT or Apache 2.0),
   runs Stable Diffusion 1.5, 2.1 and SDXL. The picture asked for is one
   object, whole, on a plain ground.
2. **Picture to mesh.** [Lux3D](https://github.com/FerrisMind/candle-3d)
   (Apache 2.0) runs TripoSR in Rust on Candle and writes the mesh as OBJ,
   GLB or PLY. TripoSR's code and weights are MIT, and it asks for about
   6 GB of graphics memory. Lux3D's page gives 1.2 s for the neural pass
   on an RTX 3060 with CUDA and 3 to 5 s on its Vulkan and WGPU backends,
   which it calls experimental.
3. **Mesh to bricks**, as described above.

The computer this was planned on has an RTX 4060 Ti with 8 GB and 32 GB of
memory, which is enough for both steps one after the other and not for
TRELLIS (16 GB).

What is not yet known, and is to be found out by trying before anything is
built into Brickloft:

- whether Lux3D builds and runs here. It is a young project (3 stars, no
  release, 135 commits) and depends on its author's own fork of Candle;
- whether the pictures Stable Diffusion makes from a short prompt give
  TripoSR a clean single object, with the ground removed (Candle has
  Segment Anything, and no model made for removing a ground);
- whether the meshes are good enough to be worth fitting: TripoSR sees
  one side of the object and guesses the rest;
- the licences of the Stable Diffusion weights as their model cards give
  them. Versions 1.5 and 2.1 are under OpenRAIL licences that restrict
  uses and not who may use them; the Turbo versions are under terms that
  sources disagree about, and are left out until read.

**How it would sit in Brickloft.** The machine-learning libraries are
large and slow to compile, and the editor's rule is few dependencies. The
generator is therefore a second program in the same workspace,
`brickloft-generate`, in Rust, built only when asked for. The editor
starts it as a program and a list of arguments, never through a shell.

1. **Settings.** The panel's Imported form has a "Generative prompt" box
   and a Generate button.
2. **Asking whether it can run.** The editor starts the generator with
   `--check`. It answers in one line of JSON: which device it would use,
   how much memory that device has, and whether the weights are in the
   editor's own folder. The panel shows the answer in plain words, such as
   "this needs 6 GB of graphics memory and this computer has 4", and
   offers Generate only when the check passes. The weights are several
   gigabytes; they are downloaded once, when the player asks, after the
   panel has shown each model's licence.
3. **Generating.** The prompt is one argument, 500 characters at most.
   The generator prints a line of progress now and then, which the panel
   shows as plain text cut to a fixed length. Cancel stops the process.
   Generation takes from seconds to minutes, so it runs beside the editor
   and never holds the window.
4. **Reading the mesh.** The generator writes one mesh file with a colour
   for each vertex. The editor reads it with a reader of its own, refusing
   more than a set number of vertices and triangles, any number that is
   not finite, and any index out of range. The mesh is scaled to the size
   asked for and fitted as described above.
5. **Keeping it.** A generated mesh cannot be relied on to come out the
   same from its prompt on another computer. The mesh is therefore saved
   beside the form in the editor's own folder, and a save names it. A
   form whose mesh is missing opens empty with a message.

Nothing leaves the computer but the one download of the weights.

**Models set aside.** Each would need Python, which is why none is used.

| Model | Licence | Why not |
|---|---|---|
| [TRELLIS](https://github.com/microsoft/TRELLIS) (Microsoft) | MIT for models and most code | Python; 16 GB NVIDIA card; Lux3D lists a TRELLIS.2 family in Rust, not yet looked at |
| [Shap-E](https://github.com/openai/shap-e) (OpenAI) | MIT for the code | Python; no port to Rust was found |
| LLaMA-Mesh (NVIDIA) | its page says non-commercial (NSCLv1) | a language model that writes a mesh as text; licence and coarse results |
| Hunyuan3D-2 (Tencent) | excludes the European Union, the United Kingdom and South Korea, as copies of its licence read | Python; not usable where the licence does not reach |
| Cube (Roblox) | research only | see above |

### Order of work

| Step | What it gives | Depends on |
|---|---|---|
| 1 | The Imported form, the `.vox` and picture readers, the Blocks fitter, its own colours | nothing; any MagicaVoxel file can be opened |
| 2 | Calling MarkovJunior: dropped, since its language is now run in Rust (see MarkovJunior above) | |
| 3 | The Smoothed fitter for grids, with repeats fitted once | step 1 |
| 4 | The OBJ reader, a mesh's distance function, the Smoothed fitter for meshes | step 3 |
| 5 | `brickloft-generate` and the panel's prompt box | step 4, and the trial above |
| 6 | Planes snapped to the grid, thin features kept | step 4, and a reading of the paper |
| 7 | MarkovJunior read in Rust: built, as the Rule program form | |

The player asked for the prompt first (2026-11-16), so the order worked
in is the trial of Candle and Lux3D, then steps 1, 4 and 5 with the Blocks
fitter left out, and MarkovJunior (steps 2, 3 and 7) afterwards.

### What is to be decided first

- Whether the trial's meshes are good enough to go on. This is decided by
  looking at them.
- Whether Brickloft may start a second program of its own, and download
  weights when asked. It starts none today except the file manager, and
  downloads nothing.
- Whether to ask Roblox about Cube's licence, or to leave Cube out.

### What could go wrong

- **A grid made for cells may not want smoothing.** A staircase in a
  MarkovJunior model may be meant as a staircase. Blocks stays the usual
  fitter for grids, and Smoothed is chosen by the player.
- **Brick counts.** A grid 100 cells a side at 4 microbricks a cell is 64
  million cells before joining. The limits a form already has apply, and
  the size offered is held to what they allow.
- **Generated meshes are rough.** They have thin sheets, floating pieces
  and no flat faces. The winding number handles the first two; a mesh may
  still fit badly, and the panel should show the mesh beside its bricks so
  the player can judge.
- **Licences change.** The generator records the licence each model's
  weights were under when it was written, and shows it before the
  download.
- **A young dependency.** Lux3D may stop being kept up. The generator is
  a program of its own so that it can be replaced without touching the
  editor.
