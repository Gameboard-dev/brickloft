# Features

Everything the editor does, and how to do it. For how the bricks are chosen,
see [algorithm.md](algorithm.md).

Sizes are in **microbricks** throughout. One microbrick is a fifth of a stud
and two of the game's positional units, so half steps are single units.

## The window

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

Round the middle of the grid being edited are three **arrows** and three
**rings**, red for x, green for y and blue for z (see [Grids](#grids)). The
**Arrows and rings** slider at the top of the panel sets how large they are
drawn, from 0.1 to 1 of the size of what is on the grid.

## Forms

| Form | Points | Options |
|---|---|---|
| **Arch** | two feet, and a height | Kind: round, pointed, triangular, elliptical, parabolic, catenary. Shift + click adds a point that pulls it out of shape (see [Added points](#added-points)). |
| **Circle** | a centre, and a radius | **Facing**: which axis it faces (Z is lying flat). Stretch within its plane makes an ellipse. |
| **Spline** | any number, passed through | **Closed loop** joins its end to its start. |
| **Bézier** | any number, pulled towards | A weight per point: how hard it pulls. |
| **Sphere** | a centre, and a radius | Solid or a hollow shell; stretch along each axis for an ellipsoid; keep the whole, a dome, or a segment between two heights. |
| **Revolve** | a profile, spun about the upright through its first point | Solid, or a shell along the profile. |
| **Pyramid** | the middle of its base | Half its base, stretch along x and y, height; solid or a hollow tent. |

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
- **Ctrl + scroll** makes a circle, sphere or pyramid a microbrick larger or
  smaller.
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

**Shift + click** on any form adds an orange point there. Dragging it pulls
that part of the form out of shape; the form around it follows by less the
further off it is, out to **Reach of added points**.

- On a curve the point takes hold of the curve, and the reach is measured
  along it. On a circle it moves within the circle's plane; hold **Ctrl** to
  move it straight out of the plane instead.
- On a revolve it pulls the profile, so the form is wider or narrower all
  the way round at that height.
- On a sphere or pyramid it takes hold of the surface where you clicked.

The form keeps its own points and sizes, so a pulled circle is still a
circle whose radius can be changed. **Shift + right-click** an orange point
to take it off, or use **Remove the last added point**. Up to 12 per form.

### Mirror

**Mirror X / Y / Z** makes the form its own mirror image along that axis,
about the middle of its points. The low side is kept and the high side
becomes its image. An arch with one foot higher than the other gets two
feet alike. A point added on either side appears on both.

## The workspace

Several forms can be worked on together.

- **Click a form** in the view, or its line in the list, to edit it.
- **Drag a form by its bricks** to move it. It moves one unit at a time and
  stays the same bricks wherever it stands.
- Forms on the same grid never share a cell: one dragged into another stops
  beside it or comes to rest on top of it, and one reshaped until it reaches
  another pushes that one away.
- **Delete** removes the form being edited. **Ctrl+Z** and **Ctrl+Y** undo
  and redo.

### Grids

The game keeps bricks on grids. The world has one, and any other grid can
stand at any angle and pass through the others. Every form in the workspace
is on a numbered grid, shown in the list.

| Button | What it does |
|---|---|
| **Add to this grid** | another copy of this form on the same grid: kept clear of the others on it, and turned and moved with them |
| **New grid** | another copy on a grid of its own |

The arrows and rings stand at the middle of everything on the grid being
edited, sized to it.

- **Drag an arrow** to move the whole grid along that axis. A level grid
  moves a unit at a time; a grid at an angle moves freely.
- **Drag a ring** round to turn the whole grid about that axis, by any
  angle, in whole degrees.
- **Level the grid** puts a turned grid back on the world's.

A level grid is the world's own and is copied as ordinary bricks. A grid at
an angle is copied as a grid of its own, frozen in place.

### Copy and paste

| Key | What it does |
|---|---|
| **Ctrl+C** | copies the form being edited, and any others marked `+`, as a Brickadia prefab (`arcs-build.brz`) on the clipboard |
| **Ctrl + click** another form | marks it `+` to be copied too, or unmarks it |
| **Ctrl+A** | marks every form |
| **Ctrl+V** | adds the forms of a copied prefab back to the workspace, as forms that can still be reshaped, on the grid being worked on |

The prefab carries each form's bricks, colours, place and grid, and, in its
description, the settings each form was made from. That is how a paste back
into the editor gives forms and not a fixed lump of bricks. A `.brz` made
elsewhere is refused with a message saying why: bricks alone cannot be
turned back into a form.

A short notice at the top of the view confirms a copy or paste and fades.
A failure shows a message that has to be closed (Enter or Escape will do).

### Saved forms

**Save** under **Saved forms** keeps the whole workspace: every form, with
its colours, where it stands and the grid it is on. Saves are kept in
folders (`arches`, `domes`, or your own) under `Documents/omegga-arcs/forms`
in your home folder, the same on Windows, macOS and Linux.

A save is a Brickadia prefab file (`.brz`), the same file Ctrl+C makes, so
the game can load it as it stands. The file holds the bricks and, in its
description, the forms they were fitted to, which is why it opens again as
forms that can still be reshaped.

**Open** puts a save in place of the whole workspace, and Ctrl+Z brings the
workspace back. **Add** adds the save's forms beside the form being edited
and on its grid, as a paste does. Dragging a save from the list onto the
view adds it too. **Delete** removes a save from disk after asking you to
confirm (Enter deletes, Escape cancels), since Ctrl+Z cannot bring a deleted file back. Saves made by earlier versions, which were text files of
one form's settings, are still listed and opened.

## Colour

Each form has its own colouring: one colour, or up to four as a **blend**,
**stripes** or a **speckle** along a chosen axis. Each brick is one colour.

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

**Show rulers** draws the form's lengths in microbricks. **Floor below
form** lowers the floor so a form can be looked at from underneath.

## Screenshots

**Save screenshot**, or **F12**, saves a picture of the window as
`arcs-1.png`, `-2` and so on in the folder `omegga-arcs` inside your
pictures folder (`Pictures/omegga-arcs` in your home folder on Windows,
macOS and Linux). On Windows the picture is also put on the clipboard,
ready to paste, which replaces a form copied there with Ctrl+C.
**Shift+F12** leaves the panel out and takes the view alone. The browser
page cannot write files, so it has neither.

## Reports

**Save report** writes `arcs-report-1.txt`, `-2` and so on to the folder
`omegga-arcs` in the system's temporary folder, and says where. **Clear
reports** deletes every report there and nothing else. A report holds the form's settings, its bricks, the
cells where bricks and form disagree most, the share of its surface within
10, 20, 30 and 45 degrees of the true surface, and drawings of cuts through
bricks and form together. It can be rebuilt and re-measured from the command
line (see [development.md](development.md)), which is how fits are compared
before and after a change.

## Every key and mouse action

| | |
|---|---|
| Click a form | edit it |
| Drag a form by its bricks | move it within its grid |
| Drag a dot | reshape the form |
| Drag an arrow, a ring | move, turn the whole grid |
| Shift + click a form | add a point to it |
| Shift + right-click an added point | take it off |
| Ctrl + drag an added point on a circle | move it out of the circle's plane |
| Ctrl + click another form | mark it to be copied too |
| Ctrl+A | mark every form |
| Ctrl+C, Ctrl+V | copy, paste |
| Ctrl+Z, Ctrl+Y | undo, redo |
| Delete | remove the form being edited |
| Ctrl + scroll | resize a circle, sphere or pyramid |
| Right-drag, middle-click | look around, face the cursor |
| W A S D, Space, E, Q | fly |
| Shift + scroll | camera speed |
| F12, Shift+F12 | save a screenshot, with or without the panel |
| Enter or Escape | close a message |
