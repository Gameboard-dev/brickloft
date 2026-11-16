# Brickloft

Brickloft builds curved shapes out of bricks for
[Brickadia](https://brickadia.com), a building game whose smallest pieces
are microbricks: cubes, ramps and corner wedges on a grid. The fixed shapes
and the grid are what make building approachable, since a brick cannot be
placed wrongly and everything built shares one style. The cost is curves. A
grid has none, so an arch or a dome takes many more pieces than a wall, and
the usual result is a staircase where a smooth slope should be.

<img src="docs/images/editor.png" width="700" alt="The editor: a blue and white torus in the 3D view, with the settings panel down the left side">

Building a curve by hand is hard for a reason that has little to do with
skill. Every brick is several decisions at once (which shape, how long,
turned which way, and whether it will meet its neighbours), made while
aiming the mouse, switching tools and keeping hold of the shape you set out
to make. A stone ball for the top of a gatepost, 100 microbricks across, is
2,215 such bricks, and one misplaced wedge shows as a notch. Brickloft lets
you work on the shape and leaves the bricks to the program. You pick
Sphere, drag one dot until the size is right, and press Ctrl+C, and the
ball that pastes into the game has no steps on it and differs from a true
sphere over only 1.2% of its volume. The bricks are refitted as you drag,
pull, mirror, turn and colour a form, so you see the real result at every
step and keep refining it, where a tool that converts a finished model
gives one answer at the end and no say in between.

The bricks it chooses are also few and large, which keeps a build light for
the game to store, send and draw: that ball fills about 524,000 microbrick
cells and is covered by 2,215 bricks, each stretched as far as the shape
allows. It can do this because every brick shape is a box with some of its
corners kept. For each box Brickloft asks which corners lie inside the
shape you drew, and that answer names the brick; neighbours share corners,
so slopes run on from one brick to the next
([docs/algorithm.md](docs/algorithm.md) has the method for each kind of
shape). It is a single desktop program written in Rust, with nothing to
install in the game or on a server, and Claude can drive it for you while
you watch ([docs/mcp.md](docs/mcp.md)).

## What it makes

<img src="docs/images/forms.png" width="700" alt="Six forms in one workspace: an arch, a vase, a pyramid, a torus, a dome and a winding tube, each in its own colours">

| Form | Shaped by |
|---|---|
| Arch | two feet and a height (round, pointed, triangular, elliptical, parabolic, catenary) |
| Circle | a centre and a radius: a ring, a disc, or, as a round tube, a torus |
| Spline | a smooth curve through any number of points |
| Bézier | a curve pulled towards weighted points |
| Sphere | a centre and a radius: solid or a hollow shell, stretched to an ellipsoid, cut to a dome or a segment |
| Revolve | a profile spun about an upright axis: vases, bells, bowls |
| Pyramid | a base and a height: solid, or a hollow tent |

Curves are built as a flat band or as a round tube. Every form can be
stretched, mirrored, turned, pulled out of shape at any point, coloured, and
combined with others in one workspace.

## How good the result is

Each fit is scored two ways.

- **Discrepancy:** the volume where the bricks and the true form disagree, as
  a share of the form's volume. Shown in the panel as you work.
- **Stepped faces:** the share of the outside that is a level or upright
  face where the form's surface slopes, which is what a staircase is. Shown
  in a saved report.

Typical results: a solid sphere of radius 50 is 2,215 bricks, 1.2%
discrepancy and no stepped faces. A torus of radius 40 and 8 thick is 645
bricks with about 1% of its faces stepped.

## Get it

**Windows:** download `brickloft-*-windows.zip` from the
[releases page](../../releases), unzip it, and run `brickloft.exe`. The program
is not code-signed, so an antivirus may hold it back as unknown the first
time.

**From source,** with [Rust](https://rustup.rs) installed:

```sh
git clone https://github.com/Gameboard-dev/brickloft
cd brickloft
cargo run --release -p brickloft-editor
```

The first build takes about a minute. Putting the file on the clipboard is
written for Windows; elsewhere the program still writes `brickloft-build.brz`
to the folder `brickloft` in the system's temporary folder, to be loaded
by hand.

## Use it

1. Pick a form from the menu and drag its dots.
2. Press **Ctrl+C**.
3. In Brickadia, paste.

[docs/features.md](docs/features.md) covers everything else: the workspace,
grids at an angle, added points, mirroring, colours, saved forms, the camera
and every key.

## Documentation

| | |
|---|---|
| [docs/features.md](docs/features.md) | What the editor does and how to use it |
| [docs/algorithm.md](docs/algorithm.md) | The principles and the algorithms, with the equations |
| [docs/performance.md](docs/performance.md) | What makes it fast, with measurements |
| [docs/web-page.md](docs/web-page.md) | The optional browser page and how it works |
| [docs/development.md](docs/development.md) | Building, running, testing, inspecting a fit |
| [docs/mcp.md](docs/mcp.md) | Letting Claude drive the editor: setting up, the tools, security |
| [docs/future-work.md](docs/future-work.md) | Planned additions: more for Claude to do, and generated patterns |

## Layout

| Folder | What it is |
|---|---|
| `core/` | The geometry: curves, solids as distance functions, the fitters, the measurements. No game or interface code, and one dependency (`glam`). |
| `editor/` | The program: the 3D view, the panel, the workspace, and the `.brz` file it copies. |

The project began as omegga-arcs, an omegga plugin that placed arcs of
bricks on a running server. The plugin was removed, because copy and paste
does the same with nothing to install and is the only route that can carry
forms on grids of their own. The name went with it once the program had
grown past arcs: lofting is the shipbuilder's craft of laying a hull's
smooth curves out at full size before anything is cut.

## Licence

MIT. See [LICENSE](LICENSE).
