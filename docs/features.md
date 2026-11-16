# Features

Everything the editor does, and how to do it. For how the bricks are chosen,
see [algorithm.md](algorithm.md).

Sizes are in **microbricks** throughout. One microbrick is a fifth of a stud
and two of the game's positional units, so half steps are single units.

## The window

<img src="images/window.png" width="700" alt="The editor's window: the settings panel on the left and an arch in the 3D view, with its dots, rulers, arrows and rings">

A 3D view of the bricks, with a panel down the left side. The form being
edited is drawn in its colours with its **dots**; other forms are drawn a
little darker. Each control in the panel has a small **?** beside it. Rest
the pointer on the **?**, or on the control itself, to read what the
control does.

| Dot | What it is |
|---|---|
| Yellow | one of the form's own points |
| Light blue | a size: a radius, or an arch's height |
| Orange | a point you added (see [Added points](#added-points)) |

At the form being edited are three **arrows**, and round the middle of its
grid three **rings**, red for x, green for y and blue for z (see [Grids](#grids)). They
are drawn a tenth the size of what is on the grid. The **Show** boxes at the
top of the panel switch the rulers, the rings, the arrows and the form's dots on
and off, each on its own. Anything hidden cannot be taken hold of either.

**Let Claude drive the editor**, also at the top of the panel, lets Claude
build and change forms while you watch (see [mcp.md](mcp.md)). It is off
until you tick it.

<img src="images/rulers-and-gizmo.png" width="700" alt="An arch with rulers along each axis and, at its middle, the arrows that move its grid and the rings that turn it">

## Forms

| Form | Points | Options |
|---|---|---|
| **Arch** | two feet, and a height | Kind: round, pointed, triangular, elliptical, parabolic, catenary. Alt + click adds a point that pulls it out of shape (see [Added points](#added-points)). |
| **Circle** | a centre, and a radius | **Facing**: which axis it faces (Z is lying flat). Stretch within its plane makes an ellipse. |
| **Spline** | any number, passed through | **Closed loop** joins its end to its start. |
| **Bézier** | any number, pulled towards | A weight per point: how hard it pulls. |
| **Sphere** | a centre, and a radius | Solid or a hollow shell; stretch along each axis for an ellipsoid; keep the whole, a dome, or a segment between two heights. |
| **Revolve** | a profile, spun about the upright through its first point | Solid, or a shell along the profile. |
| **Pyramid** | the middle of its base | Half its base, stretch along x and y, height; solid or a hollow tent. |
| **Cube** | its middle, and a dot on each of its six sides | A size along each of x, y and z, in whole microbricks; solid or hollow (see [The cube](#the-cube)). |
| **Plant** | the foot of its trunk or stem | Kind: a pine, palm, oak or beech tree; a boxwood bush, a hedge or a hydrangea; a peony, a sunflower, a tulip, petunias, daisies or daffodils, or a bed of them mixed; a Scots pine, a date palm, a birch or a willow; a cypress or a yucca; reeds; grass. Height, spread, three settings of its own and a seed (see [Plants](#plants)). |
| **Maze** | the middle of its floor, a dot on its rim and one at the top of its walls | Kind: theta (rings) or grid (squares). Rings, ring gap, how the rings are divided, wall thickness and height, where the way in is, a floor, the way through marked, a seed; walls plain, patterned or hedge (see [Mazes](#mazes)). |

### The cube

A cube is a box of whole bricks, so it is built with no discrepancy and
from very few bricks: one if it is solid and no longer than the longest
brick. It starts 17 microbricks each way. Drag the dot on a side to move
that side in or out by whole microbricks. The opposite side stays where it
is, so a floor already laid against a wall stays against it. The yellow dot
in the middle moves the whole cube. **Size along X / Y / Z** in the panel
sets each length about the middle instead.

Made short along one axis, a cube is a rectangular floor or wall.
A texture from the **Texture** list (see [Patterns](#patterns)) is laid through the cube's
shortest length, so it shows on the largest face: the top of a floor, or
the face of a wall. When two lengths are equally short, the pattern goes
through the height before the depth, and through the depth before the
width. A cube 6 microbricks or more through its shortest length has a
tile pattern on its four sides too.

Unticking **Solid** makes the cube hollow, with walls as thick as **Wall
thickness** on all six sides. A hollow cube that is too short to have room
inside between two walls is open through its shortest length instead. That
gives a frame, such as the border of a rug or the surround of a window.

### Curves as bricks

An arch, circle, spline or Bézier is a curve, and is built one of three ways:

- **Flat band:** as thick as **Thickness** in the curve's plane and as deep
  as **Depth** across it. The default.
- **Round tube, Rounded:** a pipe, eight-sided in section, with its edges
  sloped. A circle built this way is a torus.
- **Round tube, Protruded:** flat bands of different widths side by side,
  smooth along the curve and stepped across it.

**Fill the area the curve encloses** fills a closed flat curve: a ring
becomes a disc, and a round-tube ring a disc with a rounded rim.

## Shaping a form

- **Drag a dot** to move a point or change a size. Dots snap to the grid.
- **Dots move** (in the panel) chooses the plane a dragged point moves in:
  facing the camera, or with x, y or z held still.
- **Ctrl + scroll** makes a circle, sphere, pyramid, cube or plant a
  microbrick larger or smaller. A cube keeps its proportions as nearly as
  whole microbricks allow.
- **Stretch** scales a form along each axis, up to 1000 times.
- **Flip** mirrors the finished bricks along x, y or z: a dome becomes a bowl.
- **Quarter turn X / Y / Z** turns the form a quarter turn within its grid.
  It is the same bricks facing another way, so a pyramid can lie on its side.
- **Neighbouring dots follow** sets how much of a dragged dot's movement the
  dots near it share, from 0 (it moves alone) to 1.
- **Snap to best fit nearby** tries each dot a whole microbrick either way
  (and half a microbrick too when **Snap dots to half microbricks** is on),
  and each size half a microbrick and a whole one either way, and keeps
  whatever fits closest, with a progress bar in the top right. **Do that after every drag** does it automatically.
- **Reset this form** puts its dots, sizes, stretch, flips and mirrors back
  to how the form starts.

### Added points

**Alt + click** on any form adds an orange point there. Dragging it pulls
that part of the form out of shape; the form around it follows by less the
further off it is, out to **Reach of added points**.

- On a curve the point takes hold of the curve, and the reach is measured
  along it. On a circle it moves within the circle's plane; hold **Ctrl** to
  move it straight out of the plane instead.
- On a revolve it pulls the profile, so the form is wider or narrower all
  the way round at that height.
- On a sphere or pyramid it takes hold of the surface where you clicked.

The form keeps its own points and sizes, so a pulled circle is still a
circle whose radius can be changed. **Alt + right-click** an orange point
to take it off, or use **Remove the last added point**. Up to 12 per form.

### Mirror

**Mirror X / Y / Z** makes the form its own mirror image along that axis,
about the middle of its points. The low side is kept and the high side
becomes its image. An arch with one foot higher than the other gets two
feet alike. A point added on either side appears on both.

## The workspace

Several forms can be worked on together. The editor opens with one cube.

- **Click a form** in the view, or its line in the list, to edit it.
- **Drag a form by its bricks** to move it. It moves one unit at a time and
  stays the same bricks wherever it stands.
- Forms on the same grid are kept clear of each other: one dragged into
  another stops beside it or comes to rest on top of it, and one reshaped
  until it reaches another pushes that one away.
- **Ctrl + drag a form** to move it into another form on
  its grid and join the two into one build (see [Joining forms](#joining-forms)).
- **Delete** removes the form being edited. **Ctrl+Z** and **Ctrl+Y** undo
  and redo.

### Joining forms

The form being edited, dragged with **Ctrl** held, goes where it is
dragged, into other forms if that is where, so a tower can be sunk into a
wall or a dome set into a roof. The moved form is then joined to the others
on its grid: the forms become one build, with no two bricks overlapping,
which the game would refuse.

Where the two surfaces meet, the bricks are fitted afresh to the two forms
as one solid, so the seam has no notch in it. Away from the seam each form
keeps its own bricks. Deep inside, where blocks of one form overlap blocks
of another, the joined form's are cut back to the cells that are free.

A joined form has a **Blend** slider, from 0 to 6 microbricks.

| Blend | The seam |
|---|---|
| 0 | a sharp crease, as if the forms were carved from one piece |
| above 0 | a rounded fillet that reaches that many microbricks along each form from where they meet |

The join is fitted a microbrick at a time and then laid with stretched
ramps and corner pieces wherever one lies closer to the joined surface, so
most single wedges standing alone are gone. It is still rougher than a
sphere's own facets, because its bricks are chosen one at a time and do not
meet edge to edge as a sphere's do
([algorithm.md](algorithm.md), "Where it falls short").

A joined form stands a whole number of microbricks from the form it joins,
because the two can be fitted as one only if their bricks line up. A drag
with Ctrl therefore moves a whole microbrick at a time, and a form joined
half a microbrick off in any other way is moved by that half, with a note
over the view. Forms built at one **Brick size** above 1 are lined up to
whole bricks of that size, and are then joined in bricks of that size.

Two kinds of form are joined by cutting alone, with a notch a microbrick
or two deep where a sloped brick had to be left out, and the blend does
nothing to them: a form laid as a pattern, and a plant. The panel says so
under the slider.

#### Cutting a form out of another

A joined form has a tick box, **Cut out of what it joins**. Ticked, the
form is taken away from the forms it is joined to and none of its own
bricks are built: a cube through a wall is a window, and a bar through a
sphere is a hole. The form is drawn as red glass, fainter when it is not
the form being edited, so that it can still be seen, clicked and dragged.
**Blend** then rounds the rim of the hole. A form that is cut out cuts
every form on its grid that it reaches, joined or not, patterns and plants
included.

| Blend | The hole |
|---|---|
| 0 | sharp edges, as if cut with a knife |
| above 0 | a rim rounded for that many microbricks along the hole and along the form it is in |

The walls of a hole are fitted a microbrick at a time, so a round hole is
rougher inside than the same form is outside. A square one through a wall
is blocks and nothing else.

A joined form stays where it is when a form it sits in is reshaped, where
an unjoined one would be pushed clear. Dragging it again without Ctrl sets
it clear of the others. The join, its blend and whether it is cut out are
part of the form, so undo, a save and a copy all keep them.

Ctrl can also be pressed after the drag has begun.

### Grids

The game keeps bricks on grids. The world has one, and any other grid can
stand at any angle and pass through the others. Every form in the workspace
is on a numbered grid, shown in the list.

| Button | What it does |
|---|---|
| **Add to this grid** | a new cube on the same grid, in the plain colour: kept clear of the others on it, and turned and moved with them |
| **New grid** | a new cube on a grid of its own |
| **Duplicate** (or **Ctrl+D**) | a copy of the form being edited on its grid, in the same colours, set down beside it |

A new form always starts as a cube, whatever was being edited. Choose
another kind from **Form**, or drag the cube's sides to make a floor or a
wall.

An arch whose two feet differ along both X and Y stands at an angle to its
grid. It cannot be fitted smoothly there, and the panel says so. **Turn its
grid to line it up** gives the arch a grid of its own, turned so that the
arch runs straight along it, and leaves its feet where they are. The arch
is then fitted as smoothly as any other. An arch from one of the reports
this was written for went from 199 bricks to 31.

The rings stand at the middle of everything on the grid being edited. The
arrows stand at the middle of the forms they move: the form being edited and
any on its grid picked with it (Shift + click).

- **Drag an arrow** to move those forms along that axis, a unit at a time.
  They stop short of the forms left behind, unless Ctrl is held to join
  them. On a grid at an angle they move along the grid's own axis of that
  colour, which is not quite the way the arrow points.
- With every form on the grid picked, or one form alone on it, the arrow
  moves the whole grid. A level grid moves a unit at a time; a grid at an
  angle moves freely.
- **Drag a ring** round to turn the whole grid about that axis, by any
  angle, in whole degrees.
- **Level the grid** puts a turned grid back on the world's.

A level grid is the world's own and is copied as ordinary bricks. A grid at
an angle is copied as a grid of its own, frozen in place.

### Copy and paste

| Key | What it does |
|---|---|
| **Ctrl+C** | copies the form being edited, and any others marked `+`, as a Brickadia prefab (`brickloft-build.brz`) on the clipboard |
| **Shift + click** another form | picks it as well as the one being edited (it is marked `+`), or lets it go |
| **Ctrl+A** | marks every form |
| **Ctrl+V** | adds the forms of a copied prefab back to the workspace, as forms that can still be reshaped, on the grid being worked on |
| **Ctrl+D** | duplicates the form being edited, as **Duplicate** does, without using the clipboard |

The prefab carries each form's bricks, colours, place and grid, and, in its
description, the settings each form was made from. That is how a paste back
into the editor gives forms and not a fixed lump of bricks. A `.brz` made
elsewhere is refused with a message saying why: bricks alone cannot be
turned back into a form.

A short notice at the top of the view confirms a copy or paste and fades.
A failure shows a message that has to be closed (Enter or Escape will do).

### Saved forms

**Save** under **Saved forms** keeps the whole workspace: every form, with
its colours (up to eight), where it stands and the grid it is on. Saves are kept in
folders (`arches`, `domes`, or your own) under `Documents/brickloft/forms`
in your home folder, the same on Windows, macOS and Linux.

A save is a Brickadia prefab file (`.brz`), the same file Ctrl+C makes, so
the game can load it as it stands. The file holds the bricks and, in its
description, the forms they were fitted to, which is why it opens again as
forms that can still be reshaped.

**Open** puts a save in place of the whole workspace, and Ctrl+Z brings the
workspace back. **Add** adds the save's forms beside the form being edited
and on its grid, as a paste does. Dragging a save from the list onto the
view adds it too. **Go to folder** opens the chosen folder in your file
manager, where each save is a `.brz` file. **Delete** removes a save from disk after asking you to
confirm (Enter deletes, Escape cancels), since Ctrl+Z cannot bring a deleted file back. Saves made by earlier versions, which were text files of
one form's settings, are still listed and opened.

## Patterns

**Texture**, the tinted box under the settings every form shares, lays a
cube or any curve as a pattern in
place of plain brick. The list starts at **none**, and a form has one
pattern at most. A cube made short one way gives a rectangular floor or
wall, a filled circle a round one, and a curved band a path. How each
pattern is made, and why that way, is in [algorithm.md](algorithm.md),
sections 7.8 and 7.9.

| Texture | What it lays |
|---|---|
| **stepping stones** | separate stones of any shape with gaps between them. With **Gap** at 0 the stones are fitted together, as in a paved floor. |
| **medallions** | a few ringed figures repeated over the whole floor |
| **rug** | a Persian carpet: nested borders, a lobed medallion with pendants, corner pieces, and scrolling stems with flowers and leaves |
| **planks** | a floor of boards |
| **bricks** | courses of bricks, every one a rectangle, with or without mortar, and with their tops bevelled if you like |
| **cobblestone** | stones of any shape fitted together, with or without mortar |

<img src="images/stepping-stones.png" width="340" alt="A curved path laid as stepping stones"> <img src="images/pattern-stones-fitted.png" width="340" alt="A square floor of stepping stones fitted together with the gap at 0">

<img src="images/pattern-medallions.png" width="340" alt="A floor of medallions: ringed squares, lozenges and octagons repeated in rows"> <img src="images/pattern-planks.png" width="340" alt="A floor of boards of many lengths laid in rows">

<img src="images/pattern-rug.png" width="690" alt="A rug 240 microbricks by 160: borders with a swinging stem and flowers, a lobed medallion at the middle with a pendant at each end, corner pieces, and coiling stems with flowers over the field">

<img src="images/pattern-bricks.png" width="225" alt="A floor of bricks all alike in a half-brick bond with mortar between, seen from above"> <img src="images/pattern-bricks-bevelled.png" width="225" alt="The same floor at an angle with Bevelled ticked: the top of every brick slopes down to the mortar on all four sides"> <img src="images/pattern-cobblestone.png" width="225" alt="A floor of cobblestone seen from above: grey stones of many shapes fitted together with dark mortar between">

### Where a pattern is laid

Stepping stones need a flat form: a floor or wall made from a cube, a
flat band or a filled outline. They follow the curve, so a spline drawn across the ground
becomes a winding stone path as wide as the band's **Thickness**. Stones
fitted together with the gap at 0 still show narrow seams, because each
stone is fitted with bricks by itself.

The other five patterns are made of tiles one microbrick across. A tile is
one colour, or it is cut along a diagonal into two triangles of two
colours. A tile pattern colours a form and never reshapes it. The form is
fitted exactly as it would be with no pattern, and its bricks then take
the pattern's colours. The sloped bricks along the form's edge stay where
they are, and each takes the pattern's colour at its middle.

| The form | Where the tiles go |
|---|---|
| flat: a cube, a flat band, a filled outline | through the form, so the pattern shows on both of its faces |
| flat in part: a disc with a rounded rim, a round tube broad enough to have a flat top | on the flat part. The rest keeps its bricks, in the pattern's first colour. |
| a box 6 microbricks or more through its shortest length | on its four sides as well, one microbrick deep |

<img src="images/pattern-box-sides.png" width="340" alt="A box 60 microbricks by 44 and 24 tall laid as bricks: bevelled bricks on its top, and the same bricks flat on its sides"> <img src="images/pattern-rounded-disc.png" width="340" alt="A disc with a rounded rim laid as medallions: the pattern covers its flat top, and the sloped bricks of the rim are plain">

A side takes the pattern only where it is a flat face. The rim of a disc
is a curve built of short flat steps, so it stays plain.

Medallions and the rug are their own mirror images and are set out from
the middle of the floor.

A tile pattern is laid at single microbricks whatever the **Brick
budget** says, because a tile is one brick and a pattern's detail is its
brick count. A rug 240 by 160 is some twenty thousand bricks, and held to
a budget of 2,500 it was built three times coarser, with every flower a
blot. Make the form larger for more detail, or set a **Brick size** by
name for less.

A rug wants a large floor. Its parts are drawn afresh for the size it is
given, and a small rug has little room to show them: a medallion of one or
two layers and plain lines for its guards. Lay it 100 microbricks or more
across. The picture above is 240 by 160.

### The settings of a pattern

Each pattern shows the settings it has a use for.

| Setting | Patterns | What it does |
|---|---|---|
| **Stone size** | stepping stones | about how far apart the middles of neighbouring stones are, from 3 microbricks. A stone is a little under this across. |
| **Gap** | stepping stones | how wide the space between stones is, 0 to 8 microbricks. 0 fits them together. |
| **Uneven** | stepping stones | 0 for stones as alike as scattered stones can be, 1 for small ones crowded among large |
| **Size** | planks, bricks, cobblestone | how large the boards, bricks or cobbles are, 1 to 8. 2 is the usual size, 1 half of it and 4 twice. |
| **Uneven along** | bricks, cobblestone | how unlike one another the pieces are along the courses, 0 to 100%. For bricks it moves the joints, so the bricks differ in length and stay rectangles. For cobblestone it moves each stone from side to side. |
| **Uneven across** | bricks, cobblestone | the same across the courses. For bricks it moves the lines between courses, so the courses differ in depth. For cobblestone it moves each stone up and down. |
| **Mortar between** | bricks, cobblestone | a microbrick of mortar between one piece and the next. Without it the pieces touch and are told apart by their shades alone, so give the pattern several colours. |
| **Bevelled** | bricks | slopes the top microbrick of every brick down to its edge, all round it |
| **Uneven stagger** | bricks | moves each course along by an amount of its own, so the upright joints fall anywhere. Untick it to set one **Stagger** for every course. |
| **Stagger** | bricks | how far each course is moved along from the one before, 0 to 50% of a brick's length. 50% is the usual half-brick bond and 0% stacks the bricks in columns. |
| **Courses upright** | bricks | runs the courses up the floor instead of across it, so the bricks stand on end |
| **Border** | medallions, rug | how many microbricks of plain ground are left round the edge, 0 to 6 |
| **Seed** | all | which one of the pattern: each whole number is a different one. **Another** takes the next number and the next of the pattern's sets of colours. |

Bricks start all alike, with both **Uneven** settings at 0% and an uneven
stagger. Cobblestone starts at 60% both ways, because cobbles all alike
are a grid of squares. At the usual **Size** of 2 a course of bricks is 3
microbricks deep, which leaves the lines between courses one microbrick
to move, so **Uneven across** changes nothing below about 60%. A larger
**Size** gives it room.

**Bevelled** is real depth. The top microbrick of each brick is built of
a sloped brick along each side and a hipped one at each corner, with the
mortar at the foot of the slope. The sloped pieces along a side are joined
into one long brick, so the top of a brick is nine bricks: four slopes,
four corners and a block between them. The bevel needs a form 2 microbricks or more deep and does nothing to a
shallower one. It is cut into one face only, the top of a floor, and the
sides of a box are laid flat. Cobblestone has no bevel, because a
cobble's sides run at any angle and no brick slopes along them cleanly.

### The colours of a pattern

Choosing a pattern sets the form's colouring to **As the pattern has it**
and brings in colours the pattern starts with: a rug's reds and blues, a
floor of planks' browns. Colours you have set yourself for a pattern are
kept when you change to another pattern. **Another** always moves to the
pattern's next set of colours.

A pattern numbers its parts, and the colours are shared among the parts.
The first colour is the ground, or the mortar of bricks and cobblestone,
and it is used for nothing else. The other colours go to the pattern's parts in turn.
With two colours the whole pattern is one colour on the other. Each colour
you add with **+** picks one more part out, up to eight colours.

<img src="images/pattern-medallions-two-colours.png" width="300" alt="A floor of medallions with two colours chosen, the whole pattern in one colour on the other"> <img src="images/pattern-medallions-five-colours.png" width="300" alt="The same floor with five colours chosen, each of its four figures in a colour of its own">

The rug needs all eight of its colours. Its numbers are tones (lines, a
dark ground, a pale ground, petals, leaves, shade, hearts) that come back
in every part, so with two or three colours its border and medallion turn
into solid blocks.

Planks and stepping stones have no ground. Each board or stone takes one of
the colours, and the sets they start with are shades of one wood or one
stone.

## Plants

**Plant** grows a tree, a bush or a flower: a pine, a palm or an oak; a
beech; a boxwood bush, a hedge or a hydrangea; peonies, sunflowers,
tulips or petunias; a stand of reeds. They are one form with a
**Kind** because they are set and coloured the same way: a height, a
spread, three settings and a seed. Each kind is grown by a method of its
own, from the facts of how that plant grows: how they are made is in
[algorithm.md](algorithm.md), section 7.10.

<img src="images/tree-pine-decorated.png" width="330" alt="A pine with a gold star on its leader and red and gold baubles under the ends of its boughs"> <img src="images/tree-oak-autumn.png" width="330" alt="An oak in its early autumn colours: gold leaves in clumps, orange in their shade, on dark spreading limbs">

| Kind | What it is |
|---|---|
| **Pine** | a spruce, the Christmas tree: a straight stem with tiers of drooping boughs that shorten to one pointed leader, each bough a flat spray of side shoots |
| **Palm** | a coconut palm: a slender, ringed, eight-sided trunk, swollen at the foot, that leans and curves upright, and one crown of arching fronds with a row of leaflets down each side |
| **Oak** | an oak grown in the open: a short thick trunk, flared at the foot, dividing into heavy crooked limbs that spread wide and fork again and again, with leaves in separate masses at the ends of the branches |
| **Boxwood bush** | a dense rounded evergreen: a closed, lumpy mound of leaves on several short stems |
| **Beech tree** | a smooth grey trunk carrying on up through a tall, dense, oval crown of flat layers of leaves |
| **Hedge** | the boxwood clipped to a wall that follows a line you shape, a little narrower at the top, with stems showing at its foot |
| **Hydrangea** | a rounded bush of broad leaves carrying domed heads of small four-parted florets |
| **Peony** | a clump of leafy stems each with one bloom of many layered petals, which opens from a round bud to a full bowl |
| **Petunias** | a low mound of leaves covered in funnel-shaped flowers with darker throats, which can trail along the ground |
| **Reeds** | a stand of bulrushes with brown heads, or of plumed common reed, with long flopping leaves |
| **Sunflower** | a stout stem with heart-shaped leaves and a nodding head: a disc of seeds set by the golden angle, ringed with two rows of petals |
| **Tulip** | a cup of six petals in two rings on a bare stem, with strap-shaped leaves from the ground |

<img src="images/plant-peony.png" width="330" alt="Three pink peonies, full blown"> <img src="images/plant-sunflowers.png" width="330" alt="Three sunflowers of different heights facing the same way">

Every tree has a **Height** and a **Spread** (a palm's is its **Frond
length**), which the dot at its top and the dot at its side set too. Three
more settings mean something different for each kind.

| Kind | A number | How full, 0 to 100% | How bent, 0 to 100% | Extras |
|---|---|---|---|---|
| Pine | **Tiers of boughs**, 4 to 40 | **Fullness**: more boughs, wider sprays | **Droop** of the boughs | **A star and baubles** |
| Palm | **Fronds**, 5 to 40 | **Leaflet length** | **Lean** of the trunk | **Coconuts** |
| Oak | **Main limbs**, 2 to 7 | **Foliage**: larger masses of leaves, less sky | **Gnarl**: how crooked the limbs are | **Bare** |
| Boxwood | **Stems**, 2 to 12 | **Density** of the canopy | **Looseness** of its outline | **Berries** |
| Beech | **Main limbs**, 3 to 9 | **Foliage** | **Crookedness** | **Bare** |
| Hedge | **Plants along it**, 1 to 40 | **Density** | **Looseness**: clipped to grown out | **Berries** |
| Hydrangea | **Flower heads**, 1 to 24 | **Leaves** | **Looseness** | none |
| Peony | **Stems in the clump**, 1 to 9 | **Petals and leaves** | **How open the bloom is**: bud to full blown | none |
| Petunias | **Flowers**, 3 to 80 | **Leaves** | **How far it trails** | none |
| Reeds | **Stems**, 1 to 120 | **Leaves** | **Sway** | **Feathery plumes** |
| Grass | **Water stands this far below it**, 0 to 40 | **How thick it grows** | **Lean** of the blades | none |
| Birch, Willow, Scots pine | **Main limbs**, 3 to 9 | **Foliage** | **Crookedness** | **Bare** |
| Date palm | **Fronds**, 8 to 60 | **Leaflet length** | **Lean** | **Dates** |
| Cypress | none | **Density** | **Looseness** | none |
| Yucca | **Blades**, 8 to 80 | **Blade width** | **Bend of the blades** | **In flower** |
| Daisies, Daffodils | **Flowers in the clump**, 1 to 20 | **Leaves** | not used | none |
| Bed of mixed flowers | **Flowers**, 2 to 40 | **Leaves** | **Unevenness** | none |
| Sunflower | **Flowers in the clump**, 1 to 12 | **Petal and leaf size** | **Nod of the head** | none |
| Tulip | **Flowers in the clump**, 1 to 12 | **Leaf size** | **How open the cup is** | none |

A flower's **Spread** is half the width of its head, cup or bloom, and a
clump is as many flowers as you ask for, each its own height.

A hedge, a stand of reeds and a patch of grass lie along a line. They
start with three yellow dots in a row: drag an end to lengthen it and the
middle one aside to bend it. **Sharp corners at its dots** runs it
straight from dot to dot with a corner at each, and reeds inside a closed
line fill it. Alt + click on a hedge or reeds adds a point
that bends it further, and **Closed loop** joins a line of three or more
points into a ring. The thickness is its own setting, with a dot out to
one side of the line, and changing it leaves the length alone.

<img src="images/plant-hedge-bent.png" width="330" alt="A hedge that follows a winding line through four points, with a level top all the way"> <img src="images/plant-grass.png" width="330" alt="A lawn with two hills raised in it">

**Grass** is a thin surface: blades over a skin of soil. Open, it is a
strip **Reach to each side** wide along its line; closed, it fills its
outline. Its **Height** is the length of its blades. Its ground is shaped
with a brush:

- Choose **Push up**, **Push down** or **Flatten**, a **Range** and an
  **Intensity**, then Alt + click on the grass. Each click is one touch.
- A touch leaves an orange dot. Drag it up or down to change how far it
  pushes, or the level it flattens to. Alt + right-click takes it off.
- The panel's sliders change the touch made or dragged last. **Next**
  steps through the touches, and **New touches only** leaves them alone.
- Flatten brings the ground to the height it has where you click.
- Keep the button down after Alt + click and hold Ctrl to paint a stroke:
  the same touch is made again every 0.4 of its range as you move.
- A patch takes 240 touches.
- **Push down** goes below the lawn's own level. A hollow deeper than
  **Water stands this far below it** is a pond: the grass thins at its
  bank, then sand, then the pond's bed with a few stones, and water to
  the level. **Reeds round the water** sets how thickly reeds stand at
  its margin. Set the water to 0 for a dry hollow. A patch with a pond
  reaches below the floor it was laid on by the pond's depth, so raise it
  or set it on ground that deep.

<img src="images/plant-pond.png" width="420" alt="A lawn with a pond dug in it: blue water with a flat top, a rim of sand, and bulrushes standing round the margin">

**Frozen pond** writes the water as ice: collision on, and less
see-through. Choosing the **Winter** colours ticks it and any other set
unticks it; grass also has an **Autumn** set. On every other plant,
**Leaves can be walked through in the game** writes its greenery (leaves,
needles, fronds, petals, berries, baubles) with collision off and leaves
its wood solid; it is unticked to begin with.

**Blades can be walked through in the game** writes the blades with their
collision off, so a player walks through the grass and stands on the
soil. Untick it for solid blades. Grass that has been turned or flipped is
written solid.

**Bare** shows an oak or a beech without its leaves, with twigs carried
on from every branch end. It is the same tree as in leaf. **Berries** on
a boxwood or hedge take the seventh colour of its set.

<img src="images/tree-oak-bare.png" width="330" alt="A bare oak: a thick flared trunk and heavy crooked limbs ending in short forking twigs"> <img src="images/plant-hedge.png" width="330" alt="A clipped hedge with red berries, five times as long as it is thick">

**Seed** is which plant of its kind: any whole number, each a different
one, and the same number is always the same plant. **Another** takes the
next number. The height, spread and other settings change the tree a seed
gives without making it a different tree.

**Colours** offers each kind's sets of colours: for a pine spruce green,
under snow, blue spruce and in frost; for a palm three; for an oak summer,
early autumn, late autumn and spring; for a peony pink, deep red, white, coral and blush; for a hydrangea blue,
pink, white and blue running to pink; for a tulip six, one of them flamed. A tree is coloured **As the pattern
has it**, with a colour for each of its parts: the trunk, leaves, leaves
in shade, new growth, limbs, the top of a pine's boughs or the rib of a
frond or a flower's petals, and two for the extras, the blooms or the
seeds and heart of a flower. Change any of the eight under
[Colour](#colour) to make a set of your own. Under snow, the white is the
colour for the tops of the boughs.

A plant starts large, a pine 140 microbricks tall and a sunflower 90,
because its shoots, leaflets and petals are two microbricks thick at any
size and a small one has room for little else. At that size a tree is
17,000 to 40,000 bricks and a bush or a clump of flowers 6,000 to 9,000. A
tree builds in about a fifth of a second on a desktop's cores and in up to
a second on one core or in the browser. **Brick size**
left on **Automatic** builds a plant at 1 microbrick, whatever the
budget. A plant too large to be built at 1 microbrick is built at the
finest brick size that fits, so the **Height** slider never stops
short. A tree can be turned, flipped, moved, joined to other forms and
put on a grid of its own like any form. It cannot be mirrored and takes no
added points.

## Mazes

<img src="images/maze-theta.png" width="340" alt="A theta maze of six rings seen from above, with its way through marked by a yellow strip"> <img src="images/maze-hedge.png" width="340" alt="A theta maze with walls of clipped hedge on a brown floor">

**Maze** builds a maze to walk through. It has one way in, on its rim,
and a room in the middle. Every part of it can be reached, there are no
loops, and there is exactly one way from the way in to the room. How it
is made is in [algorithm.md](algorithm.md), section 7.11.

| Setting | What it does |
|---|---|
| **Kind** | **Theta** is rings of corridors about a round room, with walls that are arcs and spokes. **Grid** is square cells about a square room. |
| **Rings** | How many rings, the room counted as the first, from 2 to 40. Changing it keeps the corridors' width, so the maze grows. |
| **Ring gap** | A corridor and one wall, in microbricks. Changing it keeps the rings and resizes the maze. A corridor is at least 3 between its walls. |
| **Cells in the first ring** | How many cells spokes cut the first ring into (theta only). |
| **Cells double** | How the number of cells grows outwards (theta only): **to keep cells square**, or every 1 to 8 rings. Every 2 is what the older brickmaze tool did. |
| **Wall thickness** | Whole microbricks. |
| **Wall height** | How tall the walls stand above the floor. |
| **Way in, round the rim** | Where the gap in the rim is, in degrees round from the X axis. |
| **Floor thickness** | A floor under the whole maze, or 0 for none. |
| **Mark the way through** | Lays a strip one microbrick high along the ground from the way in to the room, in its own colour. |
| **Seed**, **Another** | Which maze. The same seed and settings are always the same maze. |

Two dots size it in the view. The dot on the rim drags the radius, which
widens or narrows every corridor and keeps the rings. The dot over the
middle drags the walls' height. Ctrl + scroll resizes it too.

**Texture** sets what the walls are made of:

| Texture | The walls |
|---|---|
| **none** | plain fitted walls: long sloped bricks round every ring, a few hundred to a few thousand bricks |
| **hedge** | the Plant form's hedge grown along every wall, with its **Density**, **Looseness**, **Berries** and seed. At least 6 thick and 12 tall. |
| **bricks** | courses of bricks that run level along each wall and bend round the rings with it |
| **cobblestone**, **planks**, **medallions** | the same patterns as on a floor, wrapped along each wall |

<img src="images/maze-bricks.png" width="340" alt="A theta maze with brick walls: level courses of red brick round every ring wall"> <img src="images/maze-grid.png" width="340" alt="A grid maze seen from above with its way through marked">

A texture colours the walls and never moves them. On the curved face of
a ring wall each long sloped brick is one colour to a course, so upright
joints show there only where one sloped brick ends. The straight walls
along the axes, the wall tops and a grid maze carry the pattern exactly.
Rug and stepping stones are not offered for a maze.

Choosing a texture brings its colours unless you have set your own. With
plain walls there are three colours: the floor, the walls, and the strip
that marks the way through. With a texture or hedge there are eight: the
first is the floor and the mortar or stems, and the last is the strip.

A hedge maze and a textured maze are tens of thousands of bricks. A maze
is refused, with what to change, if its corridors would be under 3
microbricks wide, its walls over 40,000 microbricks long in all, or its
hedge more than can be built in a few seconds. It can be turned,
flipped, moved and put on a grid of its own. It cannot be mirrored and
takes no added points.

## Colour

Each form has its own colouring.

| Colouring | What it does | Colours |
|---|---|---|
| **One colour** | the whole form in one colour | 1 |
| **Blend** | a gradual change from the first colour to the last along a chosen axis | up to 4 |
| **Stripes** | bands of each colour in turn along a chosen axis | up to 4 |
| **As the pattern has it** | each brick the colour its pattern gives it (see [The colours of a pattern](#the-colours-of-a-pattern)) | up to 8 |

Click a colour to change it. **+** adds a colour and **-** takes the last
one away. Right-click a colour and choose **Copy colour**, then right-click
another and choose **Paste colour**, to make the two the same. The copied
colour is kept inside the editor, so a form copied with Ctrl+C stays on the
clipboard. Each brick is one colour.

## Fit and brick count

| Setting | What it does |
|---|---|
| **Simplify** | how much closer a sloped brick must bring a flat outline to be worth a brick. 0 is the closest fit; higher is fewer bricks. |
| **Brick size** | fits the form with bricks a multiple of this size. **Automatic** picks the finest size within the **Brick budget**. |
| **Longest brick** | the longest edge one brick may have, 80 by default. |
| **Sliders go up to** | how far every size slider reaches (radius, height, thickness, depth, reach of added points), 1000 microbricks by default. Lower it for a finer slider. |
| **Use ramps and corner pieces** | off builds with cubes alone. |

The panel shows the brick count by kind, the brick size chosen, the
**discrepancy** and how long the fit took.

## The camera

The camera is flown.

| | |
|---|---|
| Right-drag | look around, as far as right over backwards |
| Middle-click | turn to face what the cursor is on |
| W A S D | move the way it looks, and sideways |
| Space or E, Q | rise, sink |
| Shift + scroll | faster or slower |

The rulers give the form's lengths in microbricks. **Floor below
form** lowers the floor so a form can be looked at from underneath.

## Screenshots

**Save screenshot**, or **F12**, saves a picture of the window as
`brickloft-1.png`, `-2` and so on in the folder `brickloft` inside your
pictures folder (`Pictures/brickloft` in your home folder on Windows,
macOS and Linux). On Windows the picture is also put on the clipboard,
ready to paste, which replaces a form copied there with Ctrl+C.
**Shift+F12** leaves the panel out and takes the view alone, and copies
it the same way. The browser
page cannot write files, so it has neither.

## Reports

**Save report** writes `brickloft-report-1.txt`, `-2` and so on to the folder
`brickloft` in the system's temporary folder, and says where. **Clear
reports** deletes every report there and nothing else. A report holds the form's settings, its bricks, the
cells where bricks and form disagree most, the share of its surface within
10, 20, 30 and 45 degrees of the true surface, and drawings of cuts through
bricks and form together. It can be rebuilt and re-measured from the command
line (see [development.md](development.md)), which is how fits are compared
before and after a change.

If the editor ever closes by itself, it first writes what went wrong to
`brickloft-crash.txt` in the same folder. Each crash adds a record: the
time, the program's version, the fault's message, the line of the program
it happened on and the calls that led there. The editor sends the file
nowhere. It is the thing to send with a report of a crash, since a window
has no console to show the message in. The file is started afresh once it
passes a megabyte.

## Every key and mouse action

| | |
|---|---|
| Click a form | edit it |
| Drag a form by its bricks | move it within its grid |
| Ctrl + drag a form | move it into another form and join the two |
| Drag a dot | reshape the form; on a cube, move one side |
| Drag an arrow | move the form being edited and any picked with it |
| Drag a ring | turn the whole grid |
| Alt + click a form | add a point to it |
| Alt + right-click an added point | take it off |
| Ctrl + drag an added point on a circle | move it out of the circle's plane |
| Shift + click another form | pick it as well as the one being edited |
| Ctrl+A | mark every form |
| Ctrl+C, Ctrl+V | copy, paste |
| Ctrl+D | duplicate the form being edited |
| Ctrl+Z, Ctrl+Y | undo, redo |
| Delete | remove the form being edited |
| Ctrl + scroll | resize a circle, sphere, pyramid, cube or plant |
| Right-drag, middle-click | look around, face the cursor |
| W A S D, Space, E, Q | fly |
| Shift + scroll | camera speed |
| F12, Shift+F12 | save a screenshot and copy it to the clipboard, with or without the panel |
| Enter or Escape | close a message |
