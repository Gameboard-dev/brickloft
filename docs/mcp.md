# Letting Claude drive the editor

The editor can be driven by Claude. With the connection switched on, Claude
adds forms to the workspace, changes their settings, colours and grids,
measures them, moves the camera and takes screenshots, and you watch each
change happen in the editor's window.

<img src="images/editor.png" width="700" alt="The editor with the box Let Claude drive the editor ticked at the top of the panel, and a torus Claude has built">

The connection uses the [Model Context Protocol](https://modelcontextprotocol.io)
(MCP), the standard way for an AI assistant to call tools that a program
offers. The editor is the MCP server and offers twenty tools. Claude Code, or
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
| `run_program` | Plays, pauses, steps, restarts or finishes the run of the rule program being edited. |
| `remove_form` | Takes a form out of the workspace. |
| `select_form` | Makes a form the one being edited, so its dots and settings show, and turns the camera to face it. |
| `measure_between` | Gives the offset and distance between two forms' centres and the gap between their boxes along each axis. |
| `get_camera` | Gives the camera's position and the direction it looks in. |
| `move_camera` | Sends the camera to a place, or along a path of places, looking at a chosen point at each. |
| `take_screenshot` | Returns a picture of the editor's window as a PNG image, with the panel or without it. |
| `start_gif` | Starts recording the window as an animated GIF in the editor's pictures folder, with the panel or without it. |
| `stop_gif` | Stops the recording and says where the file is, how long it plays and how large it is. |
| `set_view` | Switches the rulers, rings, arrows and dots on and off, and lowers the floor. |
| `move_point` | Moves one of the points a form is drawn through (a yellow dot), leaving the others where they are. |
| `add_pull_point` | Adds a point that pulls the form out of shape at one place (an orange dot), as Alt + click and a drag do. |
| `move_pull_point` | Changes how far an added point pulls the form. |
| `remove_pull_point` | Takes an added point off, so the form goes back to its own shape there. |
| `line_up_arch` | Gives an arch that stands at an angle to its grid a grid of its own, turned to line it up, so that it is fitted smoothly. Its feet stay where they are. |
| `list_programs` | Lists the rule programs a `program` form can name: the seven that come with the editor and those you have imported. |
| `list_meshes` | Lists the mesh files in the editor's imports folder by name and size. These are the only files an `imported` form can be built from. |

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
| `form` | `arch round`, `arch pointed`, `arch triangular`, `arch elliptical`, `arch parabolic`, `arch catenary`, `circle`, `spline`, `bezier`, `sphere`, `revolve`, `pyramid`, `cube`, `tree`, `furniture`, `maze` or `imported` |
| `points` | The form's own points as `[x, y, z]`: an arch's feet, a centre, a cube's middle, or the points of a curve or profile. A form added without points starts where the editor would start one. |
| `radius`, `rise`, `thick`, `depth` | Sizes in microbricks |
| `fill`, `tube`, `closed` | Solid or shell, round tube or flat band, closed loop |
| `stretch`, `cut`, `plane`, `weights` | Stretch per axis, the part of a sphere to keep, the axis a circle faces, a Bézier curve's pulls |
| `radius` and `stretch` of a `cube` | `radius` is half the cube's side, and `stretch` scales it along x, y and z. A floor 40 by 30 and 2 deep is `radius` 20 with `stretch` `[1, 0.75, 0.05]`. Each length is rounded to a whole microbrick. With `fill` false the cube is hollow, with walls `thick` on every side. `depth`, `tube` and `plane` do nothing to a cube. |
| `mirror`, `flip`, `turned` | Mirroring per axis, flipping per axis, quarter turns |
| `pulls`, `pull_reach` | Points added to pull the form out of shape, and their reach |
| `tree` | `[kind, seed, fullness, count, bend, extras]` says which plant a `tree` form is (a tree, a bush or a flower); only the kind is needed (see below) |
| `hedge` | `true` builds an arch as clipped hedge, grown as the `tree` setting says a hedge is (`hedge,seed,fullness,count,bend,extras`); `false` for plain brick |
| `bloom` | `[flower, tilt, turn, stem, fallen]` says which flower a `tree` of the kind `flower` is and how it lies; only the flower is needed (see below) |
| `furniture` | `[kind, seed, board, shelves, spacing, bay, plinth, back, moulding, books, style, count, choice, second, extra, shine]` says which piece a `furniture` form is; only the kind is needed (see below) |
| `stones` | `[size, gap, uneven, seed]` lays a flat band, a filled outline or a cube as stepping stones; a `gap` of 0 fits the stones together; `none` for solid brick |
| `mosaic` | `[style, size, bevel, border, seed]`, with `mortar, stagger, upright, uneven_along, uneven_across` and then `0, depth` after it or not, lays a form as a pattern of tiles; `none` for no pattern (see below) |
| `on_grid`, `grid_degrees`, `grid_at` | Which grid the form is on, the grid's angle in degrees about x, y and z, and where the grid stands |
| `blend` | Joins the form to the forms it stands in: `0` for a sharp crease, up to `6` for a fillet that many microbricks wide, `none` for a form that stands apart (see below) |
| `carve` | `true` takes a joined form away from the forms it joins and builds none of its own bricks; `false`, the usual, adds it (see below) |
| `grain`, `budget`, `simplify`, `max_ext` | Brick size, brick budget, simplification and the longest brick |

Forms are fitted one at a time, so two forms set in the same place
overlap, and the game refuses a brick that overlaps another. To build one
thing out of two forms, give the second a `place` partly inside the first
and a `blend`. The two are then fitted as one solid where they meet (see
[features.md](features.md), "Joining forms"). A joined form must stand a
whole number of microbricks from the form it joins along every axis, so one
given a `place` half a microbrick off is moved by that half, and the
workspace that comes back says where it stands. A pattern, a plant or a piece of
furniture is joined by cutting alone, whatever its `blend`. A `blend` outside 0 to 6 is
refused.

To cut a window or a hole, give the cutting form a `place` inside what it
is to cut, a `blend` and `carve: true`. It is taken away from every form on
its grid that it reaches, and `blend` is then how far the rim of the hole
is rounded. The form stays in the workspace, listed with the bricks it
would have had, and can be moved or reshaped like any other.

```json
{ "name": "add_form", "arguments": {
    "settings": { "form": "cube", "points": [[0, 0, 0]], "radius": 5, "fill": true, "blend": 0, "carve": true },
    "place": [2, 0, 18] } }
```

A colour has a `mode` (`one`, `blend`, `stripes` or `pattern`), one to eight
`colours` written `#rrggbb`, the axis it `runs_along`, and a
`stripe_width`. The mode `pattern` gives each brick the colour its pattern
gives it: the first colour is the ground or the mortar, and the others are
shared among the pattern's parts.

`tree` names the kind first: `pine`, `palm`, `oak`, `beech`, `boxwood`,
`hedge`, `hydrangea`, `peony`, `sunflower`, `tulip`, `petunia`, `reeds`, `grass`, `birch`, `willow`, `datepalm`, `scotspine`, `cypress`, `yucca`, `daisy`, `daffodil`, `bed`, `cherry` or `flower`
(`rose`, a kind that is gone, is read as `peony`). Whatever follows it replaces that kind's usual
settings in order: the seed, how full (0 to 100), how many (a pine's
tiers, a palm's fronds, an oak's main limbs, a boxwood's stems, the plants
along a hedge, a hydrangea's heads, the flowers in a clump, a stand's
stems), how bent (0 to 100: a pine's droop, a
palm's lean, an oak's gnarl, a hedge's looseness, how open a peony or a
tulip is, a sunflower's nod, how far petunias trail, reeds' sway) and 1 or 0 for its extras (a star and baubles, coconuts,
an oak or beech bare, berries on a boxwood or hedge, plumes on reeds). A seventh number is a tree's trunk width, 50 to 200 as a percentage of the usual, 100 if not given. A pine takes four more for its decorations: baubles (0 to 100), tinsel (1 or 0), lights (0 for none, 1 for a string, 2 for candles) and whether the lights shine in the game (1 or 0), then how far out the tinsel hangs (40 to 110, a percentage of the way from the stem to the ends of the boughs, 80 if not given). Its `extras` is the star. A
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
10. Grass takes 240 touches. `fill` true freezes a grass's pond (ice, with collision). `tube` true on grass gives it raised ponds, each at its own height, with a
stream from each down to the nearest water that stands lower and a waterfall wherever that stream goes over a cliff, and `tube`
true on any other plant writes its greenery to be walked through. The eighth number of grass's `tree` is how readily steep ground is a cliff, 0 (none, if not given) to 3. A touch's middle number is 0 to push, 16 to flatten, and 32 more for a sharp edge. `closed` true on a cherry scatters fallen petals under it. A grass's `extras` of 1
makes its blades solid in the game; at 0 they are written to be walked
through. The form may be written `tree` or `plant`. `"tree": "oak"` is the oak as the editor starts one and
`"tree": ["palm", 3]` the third palm. A tree's height is `rise` and its
spread `radius`. Added with neither, it is its usual size, a pine 140
microbricks tall, and comes in its kind's own eight colours with the mode
`pattern`. A tree is tens of thousands of bricks and takes up to a second
to build, so a call that changes one takes that long to answer.

The kind `flower` is a single flower head with no plant under it, and
`bloom` says the rest about it. `bloom` names the flower first: `rose`,
`waterlily` or `gerbera`. Whatever follows replaces the usual settings
in order: the tilt (0 to 90 degrees from facing straight up), the turn
(0 to 359 degrees about the upright, 0 towards +x), and 1 or 0 for a
short stem and for fallen petals beside it. For this kind the `tree`
setting's fullness is how many petals, its bend is how open the bloom is
(0 a closed bud, 100 full blown) and its extras are the sepals.
`"tree": "flower"` alone is a red rose 60% open, facing up, 48
microbricks across when full blown. Adding `"bloom": "rose,60,45,1,1"`
tips it 60 degrees over towards the diagonal, on a stem, with petals
beside it. The two settings are kept apart so that a tilt is the second
value of `bloom` and not the fourteenth of `tree`, after a pine's
decorations. A bloom is 3,000 to 7,000 bricks.

`furniture` names the kind first. Six values follow the bookcase's ten:

| Value | Meaning |
|---|---|
| `style` | `modern`, `farmhouse`, `classical`, `rustic`, `industrial`, or `random` for whichever the seed picks |
| `count` | a number of the kind's own, 0 for as many as its size has room for: a cupboard's doors, a chest's drawers, a kitchen's units, a sofa's seats, a desk's drawers; a pot's height in tenths; the microbricks between a trellis's slats |
| `choice` | a choice of the kind's own, 0 for the style's: a table's legs (1 square, 2 tapered, 3 turned, 4 trestle, 5 pedestal), a chair's or bench's back (1 slats, 2 ladder, 3 panel, 4 spindles), a stool's seat (1 square, 2 round), a sofa's arms (1 square, 2 rolled), a cupboard's handles (1 knobs, 2 bars, 3 none), a lamp's shade (1 drum, 2 cone, 3 globe), a bed's headboard (1 panel, 2 slats, 3 rails), a pot (1 terracotta, 2 glazed, 3 square, 4 tall), a lattice (1 square, 2 diamond, 3 arched), a painting's subject (1 sunset, 2 landscape, 3 abstract), what a sitter wears (1 nothing, 2 glasses, 3 a hat, 4 a beard) |
| `second` | a second choice, 0 for the style's or the seed's: a table's top (1 rectangular, 2 round), a seat (1 bare, 2 cushion), what a sofa stands on (1 legs, 2 a skirt), what a cupboard stands on (1 a plinth, 2 feet), a chandelier's arms (1 two, 2 four, 3 eight candles), a candlestick's candles (1 one, 2 three), a bed's pillows (1 or 2), the plant in a pot (1 yucca, 2 boxwood, 3 hydrangea, 4 peonies, 5 tulips, 6 daffodils, 7 daisies, 8 petunias, 9 cypress, 10 palm, 11 cherry), a climber (1 ivy, 2 clematis, 3 rose, 4 wisteria, 5 none) |
| `extra` | 1 or 0: a dining table's stretchers, a coffee table's shelf, a side table's drawer, a desk's second pedestal, a chair's arms, a bench's back, a cupboard's drawer row, a chest's two small top drawers, a sideboard's middle drawers, a kitchen unit's drawer, a bed's footboard, a picture's mount, and for a plant in a pot whether its leaves are walked through |
| `shine` | 1 or 0: whether a lamp's lights carry a real light in the game as well as glowing |

The kinds are `bookcase`, `cupboard`, `drawers`, `sideboard`, `kitchen`,
`dining`, `coffee`, `side`, `desk`, `chair`, `armchair`, `stool`, `bench`,
`sofa`, `tablelamp`, `floorlamp`, `pendant`, `chandelier`, `sconce`,
`candlestick`, `bed`, `potted`, `basket`, `windowbox`, `trellis`,
`painting` and `portrait`. A bookcase takes no notice of the six new
values, and a bookcase saved with ten values is read as it was. The other
kinds take no notice of the bookcase's values but `seed`, and a trellis
reads `books` as how much of it its climber covers.

Pieces of one `style` and one `seed` are a set: they share their legs,
their weight and their woods. To furnish a room, give every piece the
same two. A piece added with no `radius` or `stretch` is its kind's usual
size (a dining table 44 by 24 by 19, a chair 11 by 11 by 23), and with no
`colour` it comes in its style's colours. A pendant and a chandelier hang
from the top of their box, and a sconce, a trellis and a picture are
against its high y side. For example, this call adds the fourth
farmhouse chair, with arms:

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "furniture", "furniture": ["chair", 4, 2, 0, 11, 34, 3, 1, 1, 85, "farmhouse", 0, 0, 0, 1, 1] }
  }
}
```

For a bookcase, whatever follows the kind replaces its usual settings in order:

| Value | Meaning |
|---|---|
| `seed` | any whole number, each a different arrangement of books |
| `board` | how thick the boards are, 1 to 8 (usually 2) |
| `shelves` | how many shelves stand between the bottom and the top, 1 to 60, or 0 for as many as the height has room for at `spacing` |
| `spacing` | the clear height wanted between shelves, 3 to 200 (usually 11) |
| `bay` | the widest an opening may be before an upright divides it, 4 to 400 (usually 34), or 0 for no uprights |
| `plinth` | how far the bottom board is lifted off the floor, 0 to 20 (usually 3) |
| `back` | 1 for a back panel, 0 for none |
| `moulding` | 1 for a cornice and a skirting of sloped bricks, 0 for none |
| `books` | how full the shelves are, 0 to 100 (usually 85) |

A bookcase fills the box that a cube of the same `radius` and `stretch`
would fill, and its one point is the middle of that box. Its front is on
the low y side, and `turned` faces it another way. Added with no `radius`
or `stretch` it is 36 microbricks wide, 10 deep and 60 tall, and added
with no `points` it stands on the floor. It comes in its own eight colours
with the mode `pattern`: the wood, the back panel, and six that the books
share. A value out of range is brought within it, and a name that is no
kind of furniture is refused. For example, this call adds a bookcase 80
wide, 10 deep and 60 tall with five shelves in one span, packed with
books:

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "furniture", "radius": 5, "stretch": [8, 1, 6], "points": [[0, 0, 30]], "furniture": ["bookcase", 4, 2, 5, 11, 0, 3, 1, 1, 100] }
  }
}
```

A bookcase has no discrepancy and answers at once. `thick`, `depth`,
`fill`, `tube`, `rise`, `mirror`, `pulls`, `grain`, `mosaic` and `stones`
do nothing to it, and it is joined to other forms by cutting alone.

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
from the panel. A paint takes up to 32 colours; a pattern has eight numbered parts at most and shares them among the colours given.

A maze is the form `maze`, and its own settings are one line of text,
`maze`, as a plant's are `tree`: `kind,rings,divisions,grow,seed,entrance,floor,solution,hedge`,
of which only the kind (`theta` or `grid`) is needed. The form's `radius`
is from its middle to the middle of its outside wall, `thick` is the
walls' thickness and `rise` their height; left out they are 120, 4 and
30. A corridor with one wall is `radius` divided by `rings`. For example,
this call adds a maze of eight rings with a floor 2 thick, its way
through marked, and walls of brick in level courses:

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "maze", "maze": "theta,8,4,0,3,270,2,1,0", "radius": 160, "mosaic": ["bricks", 2, 0, 0, 1, 1, 50, 0, 0, 0] }
  }
}
```

The last number of `maze` set to 1 makes the walls hedge, grown with the
`tree` setting's fullness, bend and berries. A `mosaic` of `bricks`,
`cobblestone`, `planks` or `medallions` is wrapped along every wall; a
`rug` and `stones` are not laid on a maze. A maze added with no `colour`
comes in its walls' own colours: three for plain walls (floor, walls,
way through) and eight otherwise, the first being the floor and the last
the way through. A maze whose corridors would be under 3 microbricks
wide, or whose walls or hedge are more than may be built, is refused
with the reason and what to change.

A model from a file is the form `imported`, and its own settings are one
line of text, `import`: `name,up,fitter,loose,colours,smooth`, of which
only the name is needed. The name is one that `list_meshes` gives. A mesh
file gets into that list when you open it in the editor, by typing its
path in the panel or dropping it on the window, which copies it into the
editor's imports folder under a name made from its content. Claude
cannot open a file, and cannot name one anywhere else: a name is sixteen
hexadecimal digits and `.glb`, `.obj` or `.ply`, and anything else is
refused before a file is looked for. For example, this call builds a
mesh 80 microbricks along its longest side, with z as its up, in eight
of its own colours:

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "imported", "import": "07d13976ad23fe22.glb,z,smoothed,0,8", "radius": 40 }
  }
}
```

`up` is `y`, `z` or `x`, and left out it is `y` for a `.glb` or `.obj`
file and `z` for a `.ply`. `fitter` is `smoothed` or `blocks`. `loose`
leaves out pieces of the build smaller than that many microbricks that
touch no other piece, and 0 keeps everything. `colours` is how many
colours the mesh's own are reduced to, 12 if left out, and 0 leaves them
out. `smooth` is how many times over the surface is smoothed before it
is fitted, 0 to 60, 0 if left out. The form's `radius` is half the length of the mesh's longest side
(24 if left out), its one point is its foot, `fill` false leaves a shell
`thick` through, and `simplify` above 0 trades closeness for fewer
bricks. A mesh added with no `colour` comes in its own colours.

A rule program is the form `program`, and its own settings are one line
of text, `program`: `name,x,y,z,cell,seed,finish,limit,stop`, of which
only the name is needed. The name is one of the programs that come with
the editor (`Apartemazements`, `CarmaTower`, `ModernHouse`, `SeaVilla`,
`PillarsOfEternity`, `Escher`, `StrangeDungeon`) or `imported:` and the
name of a program you have brought in through the panel, and
`list_programs` gives both lists. `x,y,z` is the
grid in cells, `cell` the microbricks to a cell, `finish` is `wedges` or
`plain`, `limit` the most turns a run may take, and `stop` the number of
turns the run has taken, or `end`. This call adds a ModernHouse of seed 7
and runs it to its end before it answers:

```json
{
  "name": "add_form",
  "arguments": {
    "settings": { "form": "program", "program": "ModernHouse,9,9,4,3,7" }
  }
}
```

A form's bricks are those of its grid at the turn `stop` says. A `stop`
of `end`, the usual, runs the whole program before the call answers,
which takes from a tenth of a second for Apartemazements to ten seconds
for CarmaTower, and the window waits for it. A `stop` of `0` adds the
form at once with nothing built. `run_program` with `"action": "play"`
then runs it on a second thread while the editor goes on answering, and
`get_workspace` reports the run under `program_run`: the turn, whether
there are more turns to take, whether it is playing, the node whose turn
is next, the seed shown, any seeds that were passed over, and whether it
was stopped early at its step limit. To record a building growing, call
`start_gif`, then `run_program` with `restart`, ask `get_workspace`
until `more_turns_to_take` is false, and call `stop_gif`.

A program is named and never given as a path. Claude can name a program
that comes with the editor or one already in
`Documents/brickloft/programs/`, and there is no tool that imports a
file: a program is brought in by the person at the editor (see
[features.md](features.md#importing-a-program)). A name that is not
plain letters, digits, spaces and `_ -` is refused, as is the name of a
program that is not there.

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

`start_gif` and `stop_gif` record the window as an animated GIF, as the
**Record GIF** button does ([features.md](features.md#recording-a-gif)):
for a flight of the camera, or a form changing as Claude changes it.
`start_gif` takes the same `panel` choice as a screenshot, a `size`
(`full`, `half` or `third` of the window) and a `rate` (10, 20 or 25
pictures a second). The file goes to `Pictures/brickloft` under a name
with the date and time, and nowhere else: neither tool takes a path. The
recording is not sent to Claude. `stop_gif` returns where the file is, how
long it plays, how many pictures it holds, its size, and how long each
picture took the editor. A recording left running stops by itself at 60
seconds or 64 MB.

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
writes a file, runs a command, reaches the network, or touches the game.
Saved forms, reports and the clipboard are out of reach.

**One folder of files can be read, and only by name.** `list_meshes`
gives the names and sizes of the mesh files in the editor's imports
folder, and an `imported` form is built from one of them. Those are
files you opened in the editor yourself. A name must be sixteen
hexadecimal digits and one of three endings, so it has no slash, no
`..` and no drive letter in it, and a name of any other shape is refused
by the parser before any file is looked for. No tool takes a path, and
no tool returns a file's contents: what comes back from a mesh is its
bricks.

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
