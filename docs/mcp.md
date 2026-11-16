# Letting Claude drive the editor

The editor can be driven by Claude. With the connection switched on, Claude
adds forms to the workspace, changes their settings, colours and grids,
measures them, moves the camera and takes screenshots, and you watch each
change happen in the editor's window.

<img src="images/editor.png" width="700" alt="The editor with the box Let Claude drive the editor ticked at the top of the panel, and a torus Claude has built">

The connection uses the [Model Context Protocol](https://modelcontextprotocol.io)
(MCP), the standard way for an AI assistant to call tools that a program
offers. The editor is the MCP server and offers fifteen tools. Claude Code, or
any other MCP client, is the client that calls them.

- [What Claude can do](#what-claude-can-do)
- [Setting it up](#setting-it-up)
- [The tools](#the-tools)
- [Coordinates](#coordinates)
- [Security and privacy](#security-and-privacy)
- [How it works](#how-it-works)
- [Limits](#limits)

## What Claude can do

Claude works on the same workspace you do, through the same code. A form
Claude adds is fitted with bricks, drawn, and added to the undo history
exactly as if you had added it, so **Ctrl+Z** takes back a step Claude
made. Each call is named at the top of the view as it happens ("Claude: add
form"), so a change you did not make is never unexplained.

Claude sets values exactly where the mouse sets them approximately. It can
give a radius of 12.5 microbricks, turn a grid by 22.5 degrees about one
axis, or set two forms 4 microbricks apart, and it can read back each
form's brick count and discrepancy to check the result and correct it.
Because it can also move the camera and take a screenshot, it can look at
what it has built before deciding what to change.

Requests that work well:

- "Build a dome 30 microbricks across with a red to yellow blend."
- "Make the arch twice as wide and put a torus round its base."
- "Turn the ring 30 degrees about X on a grid of its own."
- "Fly the camera round the build and tell me which part looks roughest."

## Setting it up

The connection is off until you switch it on, and it is off again each time
the editor starts.

1. Start the editor and tick **Let Claude drive the editor** at the top of
   the panel. The panel then shows the address it is listening at,
   `http://127.0.0.1:7878/mcp`.
2. Tell your MCP client that address. For Claude Code, run this once in a
   terminal, from any folder:

   ```sh
   claude mcp add --transport http brickloft http://127.0.0.1:7878/mcp
   ```

   The panel shows the same command in a box to copy.
3. Start Claude Code and ask it to build something. It finds the editor's
   tools by itself.

The steps are the same on Windows, macOS and Linux. Nothing in them depends
on where the editor is installed, because the client reaches the editor by
its address and never by a file path.

To have the editor listening from the moment it opens, for a script that
starts the editor and Claude together, start it with `--claude`:

```sh
brickloft --claude
```

Other MCP clients take the same address. In a client configured by a JSON
file, the entry is a server of type `http` with the URL above.

## The tools

| Tool | What it does |
|---|---|
| `get_workspace` | Lists every form: its settings, colour, grid angle, position, the box its bricks fill, its brick count and its discrepancy. |
| `add_form` | Adds a form of a given kind with given settings, colour and position. The camera turns to face it. |
| `update_form` | Changes one form's settings, colour or position. Only what is given changes. |
| `remove_form` | Takes a form out of the workspace. |
| `select_form` | Makes a form the one being edited, so its dots and settings show, and turns the camera to face it. |
| `measure_between` | Gives the offset and distance between two forms' centres and the gap between their boxes along each axis. |
| `get_camera` | Gives the camera's position and the direction it looks in. |
| `move_camera` | Sends the camera to a place, or along a path of places, looking at a chosen point at each. |
| `take_screenshot` | Returns a picture of the editor's window as a PNG image, with the panel or without it. |
| `set_view` | Switches the rulers, rings, arrows and dots on and off, and lowers the floor. |
| `move_point` | Moves one of the points a form is drawn through (a yellow dot), leaving the others where they are. |
| `add_pull_point` | Adds a point that pulls the form out of shape at one place (an orange dot), as Alt + click and a drag do. |
| `move_pull_point` | Changes how far an added point pulls the form. |
| `remove_pull_point` | Takes an added point off, so the form goes back to its own shape there. |
| `line_up_arch` | Gives an arch that stands at an angle to its grid a grid of its own, turned to line it up, so that it is fitted smoothly. Its feet stay where they are. |

### Forms and their settings

A form is named by its **index**, its place in the list `get_workspace`
returns, with the form being edited at index 0. Adding, removing or
selecting a form changes the order, so every tool that changes the
workspace returns the whole list again.

Settings are given as names and values, and they are the same settings a
saved form holds. For example, this call adds a hollow dome:

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "sphere", "radius": 15, "thick": 3, "fill": false, "cut": [0, "inf"] },
    "colour": { "mode": "blend", "colours": ["#c0392b", "#f1c40f"], "runs_along": "z" }
  }
}
```

The settings Claude can give:

| Setting | Meaning |
|---|---|
| `form` | `arch round`, `arch pointed`, `arch triangular`, `arch elliptical`, `arch parabolic`, `arch catenary`, `circle`, `spline`, `bezier`, `sphere`, `revolve`, `pyramid`, `cube` or `tree` |
| `points` | The form's own points as `[x, y, z]`: an arch's feet, a centre, a cube's middle, or the points of a curve or profile. A form added without points starts where the editor would start one. |
| `radius`, `rise`, `thick`, `depth` | Sizes in microbricks |
| `fill`, `tube`, `closed` | Solid or shell, round tube or flat band, closed loop |
| `stretch`, `cut`, `plane`, `weights` | Stretch per axis, the part of a sphere to keep, the axis a circle faces, a Bézier curve's pulls |
| `radius` and `stretch` of a `cube` | `radius` is half the cube's side, and `stretch` scales it along x, y and z. A floor 40 by 30 and 2 deep is `radius` 20 with `stretch` `[1, 0.75, 0.05]`. Each length is rounded to a whole microbrick. With `fill` false the cube is hollow, with walls `thick` on every side. `depth`, `tube` and `plane` do nothing to a cube. |
| `mirror`, `flip`, `turned` | Mirroring per axis, flipping per axis, quarter turns |
| `pulls`, `pull_reach` | Points added to pull the form out of shape, and their reach |
| `tree` | `[kind, seed, fullness, count, bend, extras]` says which plant a `tree` form is (a tree, a bush or a flower); only the kind is needed (see below) |
| `stones` | `[size, gap, uneven, seed]` lays a flat band, a filled outline or a cube as stepping stones; a `gap` of 0 fits the stones together; `none` for solid brick |
| `mosaic` | `[style, size, bevel, border, seed]`, with `mortar, stagger, upright, uneven_along, uneven_across` and then `0, depth` after it or not, lays a form as a pattern of tiles; `none` for no pattern (see below) |
| `on_grid`, `grid_degrees`, `grid_at` | Which grid the form is on, the grid's angle in degrees about x, y and z, and where the grid stands |
| `blend` | Joins the form to the forms it stands in: `0` for a sharp crease, up to `6` for a fillet that many microbricks wide, `none` for a form that stands apart (see below) |
| `grain`, `budget`, `simplify`, `max_ext` | Brick size, brick budget, simplification and the longest brick |

Forms are fitted one at a time, so two forms set in the same place
overlap, and the game refuses a brick that overlaps another. To build one
thing out of two forms, give the second a `place` partly inside the first
and a `blend`. The two are then fitted as one solid where they meet (see
[features.md](features.md), "Joining forms"). Both must stand a whole
number of microbricks apart along every axis, so a `place` ending in a half
on one form needs the same on the other. A pattern or a plant is joined by
cutting alone, whatever its `blend`. A `blend` outside 0 to 6 is refused.

A colour has a `mode` (`one`, `blend`, `stripes` or `pattern`), one to eight
`colours` written `#rrggbb`, the axis it `runs_along`, and a
`stripe_width`. The mode `pattern` gives each brick the colour its pattern
gives it: the first colour is the ground or the mortar, and the others are
shared among the pattern's parts.

`tree` names the kind first: `pine`, `palm`, `oak`, `beech`, `boxwood`,
`hedge`, `hydrangea`, `peony`, `sunflower`, `tulip`, `petunia`, `reeds`, `grass`, `birch`, `willow`, `datepalm`, `scotspine`, `cypress`, `yucca`, `daisy`, `daffodil` or `bed`
(`rose`, a kind that is gone, is read as `peony`). Whatever follows it replaces that kind's usual
settings in order: the seed, how full (0 to 100), how many (a pine's
tiers, a palm's fronds, an oak's main limbs, a boxwood's stems, the plants
along a hedge, a hydrangea's heads, the flowers in a clump, a stand's
stems), how bent (0 to 100: a pine's droop, a
palm's lean, an oak's gnarl, a hedge's looseness, how open a peony or a
tulip is, a sunflower's nod, how far petunias trail, reeds' sway) and 1 or 0 for its extras (a star and baubles, coconuts,
an oak or beech bare, berries on a boxwood or hedge, plumes on reeds). A
hedge, reeds or grass given two or more `points` lies along the curve
through them, closed into a ring with `closed`. `move_point` moves one of
them. With one point it is straight along x, `stretch` `[n, 1, 1]` times
as long as `radius`. On grass, `add_pull_point` is a touch of the brush
that shapes its ground: `at` is where, and `pulled_by` is `[range, flatten,
amount]`, with `flatten` 0 for a push (a negative amount pushes down) and
16 to flatten the ground to the height `amount`. `protrude` true gives a hedge, reeds or grass sharp corners at its
points. Grass's `count` is how far below it water stands (0 for none): a
touch with a negative amount digs, and a hollow deeper than that is a
pond. Its `depth` less 1 is how thickly reeds stand round the water, 0 to
10. Grass takes 240 touches. `fill` true freezes a grass's pond (ice, with collision), and `tube`
true on any other plant writes its greenery to be walked through. A grass's `extras` of 1
makes its blades solid in the game; at 0 they are written to be walked
through. The form may be written `tree` or `plant`. `"tree": "oak"` is the oak as the editor starts one and
`"tree": ["palm", 3]` the third palm. A tree's height is `rise` and its
spread `radius`. Added with neither, it is its usual size, a pine 140
microbricks tall, and comes in its kind's own eight colours with the mode
`pattern`. A tree is tens of thousands of bricks and takes up to a second
to build, so a call that changes one takes that long to answer.

`mosaic` takes five values, and for bricks and cobblestone five more may
follow. A form has `mosaic` or `stones`, and if both are given the stones
are laid.

| Value | Meaning |
|---|---|
| `style` | `medallions`, `rug`, `planks`, `bricks` or `cobblestone` (see [features.md](features.md#patterns)) |
| `size` | how large the boards, bricks or cobbles are: 2 is usual, the panel's slider goes from 1 to 8, and up to 16 is accepted |
| `bevel` | bricks only: 1 slopes the top microbrick of every brick down to its edge, and 0 leaves the tops flat. The form must be 2 or more deep. For every other style it is read as 0. |
| `border` | how many microbricks of plain ground are left round the edge, 0 to 8 |
| `depth` | the twelfth value, after a 0 that holds the eleventh place: 1 grooves planks of size 3 or more, or lays cobblestone's mortar one microbrick below its stones, and 0 leaves them flat. The form must be 2 or more deep. For every other style it is read as 0. |
| `seed` | any whole number, each a different pattern |
| `mortar` | bricks and cobblestone: 1 for a microbrick of mortar between pieces, 0 for none |
| `stagger` | bricks: how far each course is moved along from the one before, in hundredths of a brick's length. 50 is the usual bond, 0 stacks the bricks, and -1 moves each course by an amount of its own. |
| `upright` | bricks: 1 for courses running up the floor, 0 for across it |
| `uneven_along` | bricks and cobblestone: how unlike one another the pieces are along the courses, 0 to 100 |
| `uneven_across` | bricks and cobblestone: the same across the courses, 0 to 100 |

Left out, the last five are 1, -1, 0, 0 and 0. Cobblestone wants about 60
for both `uneven` values, because cobbles all alike are a grid of squares.
A style named by an earlier version of the editor is still read, as
bricks or cobblestone or as no pattern.

A tile pattern colours a form and does not reshape it. It is laid through
a flat form, on the flat part of a form that is not flat all over, and on
the four sides of a box 6 microbricks or more through its shortest
length. A form with a `mosaic` is built at single microbricks whatever
its `budget`, unless a `grain` is given.

For example, this call lays a floor of bevelled bricks in a half-brick
bond, 80 microbricks by 60 and 4 deep. The cube's `radius` is half its
longest side and `stretch` scales the other two, and the bevel needs a
depth of 2 or more.

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "cube", "radius": 40, "stretch": [1, 0.75, 0.05], "mosaic": ["bricks", 2, 1, 0, 3, 1, 50, 0, 0, 0] }
  }
}
```

A rug wants a rectangle 100 microbricks or more across to show its detail.

A pattern shows its colours only in the mode `pattern`. A form added with
a pattern and no `colour` comes in that pattern's own colours, as it does
from the panel. A pattern takes up to eight colours.

A call that cannot be built is refused whole, with the reason, and changes
nothing. Claude then sees why (for example "the form is too large to
build") and can correct the request.

### The camera and screenshots

`move_camera` takes a path of one or more legs. Each leg has a `position`,
a `look_at` point and, optionally, the `seconds` it takes. A leg with a
time is flown smoothly and one without is a jump, so a single leg places
the camera and several legs fly it round a build.

`take_screenshot` returns the window as you see it, or the 3D view alone
when asked for it without the panel. With `set_view` Claude can first hide
the rulers, rings, arrows and dots, for a picture of the bricks alone; the
pictures in this documentation were taken that way. A notice that Claude's
own call put up is never in the picture. A screenshot asked for
while the camera is flying is taken when the camera arrives. The picture is
at most 1280 pixels wide unless Claude asks for another width, because a
picture is costly for a model to read and a window's full width is seldom
needed.

## Coordinates

All lengths are in microbricks, the smallest brick's width, which is a
fifth of a stud. A value may end in a half. The z axis points up.

`get_workspace` gives each form's position three ways:

- **`place`** is where the form stands within its grid.
- **`box_in_grid`** is the box its bricks fill within that grid: low
  corner, high corner and size. Two forms on the same grid can be set an
  exact distance apart from these.
- **`box_in_world`** is the same box's extent and centre in the world. It
  differs from `box_in_grid` when the grid has been turned or moved, and it
  is what the camera's coordinates are measured in.

`measure_between` gives the relative position of two forms directly: the
vector and distance between their centres in the world, and the gap between
their boxes along each axis, where a negative gap is an overlap.

## Security and privacy

The connection is built on the rule that nothing arriving over it is
trusted.

**It is off by default.** The editor listens only while the box is ticked
(or after `--claude`), and the setting is not remembered between runs.

**It listens on this computer only.** The listener is bound to
`127.0.0.1`, the address that means "this computer", so no other machine on
your network or the internet can connect to it.

**It refuses web pages.** A web page open in your browser could otherwise
try to send requests to a local port. Every request must be addressed to
this computer by name, and a request that a browser sent from a page
elsewhere is refused. This is the protection the MCP specification requires
of local servers against DNS rebinding.

**It has no password.** Any program already running on your computer under
your account can connect while the box is ticked. Such a program could
already read your files, so the connection gives it nothing beyond what the
tools below allow.

**The tools are narrow.** They read and change the forms in the workspace,
move the camera, and take a picture of the editor's own window. No tool
reads or writes a file, runs a command, reaches the network, or touches the
game. Saved forms, reports and the clipboard are out of reach.

**Every value is checked.** Settings are read by the same parser that reads
a saved form and built under the same limits as any form. A request's size,
the number of forms, points and colours, the camera's range and a flight's
length are all capped.

**What leaves your computer.** The editor itself sends nothing anywhere.
What Claude reads through the tools, which is the forms' settings and any
screenshots it takes, goes to Claude as part of your conversation, under
the terms of the client you use. A screenshot shows the editor's window
only, including the panel, and never the rest of your screen.

## How it works

The editor speaks MCP's Streamable HTTP transport. The client sends each
message as an HTTP POST to `/mcp`, and the editor answers with one JSON
message. The other MCP transport has the client start the server as a
hidden child program, which suits a tool with no window. The editor is a
window you already have open and want to watch, so the client has to reach
a program that is already running, and a local address does that.

A tool call arrives on a listener thread and is handed to the editor's
frame loop, which carries it out between drawing two frames and hands the
answer back. The workspace is therefore only ever changed from one place,
and a change made by Claude is fitted and drawn in the same frame as one
made by hand.

The code is in [editor/src/mcp.rs](../editor/src/mcp.rs), and the tools'
names, descriptions and schemas are in
[editor/src/mcp_tools.json](../editor/src/mcp_tools.json). The HTTP is
written on the standard library's sockets with no web server dependency,
since one request is one small message. The code is the same on Windows,
macOS and Linux and holds no file paths.

## Limits

- The connection is part of the desktop program only. A browser page
  cannot listen for connections, so the optional browser build has no such
  box.
- One port, 7878, is used. If another program has it, the box unticks
  itself and the panel says why.
- Claude cannot yet read a point cloud or copy the workspace to the
  clipboard. Both are planned (see [future-work.md](future-work.md)).
- The live tests so far were made with a script acting as the client. The
  tools were called against a running editor on Windows; macOS
  and Linux run the same code and have not been tried.
