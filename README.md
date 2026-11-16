# Brickloft

Design a curved form, see exactly which microbricks it becomes, and paste it
into [Brickadia](https://brickadia.com).

Brickadia's microbricks are small boxes and wedges on a grid. Building an
arch, a dome or a torus from them by hand means choosing thousands of bricks
so that a curve comes out as slopes and not as a staircase. Brickloft does the
choosing: you shape the form with a few dots and sliders, it fits the bricks,
shows them, measures how far they are from the true form, and puts the
result on the clipboard for the game to paste.

It is a desktop program written in Rust. Nothing is installed in the game or
on a server.

## What algorithm does it use?

Brickloft chooses each brick by asking which corners of its box are inside the
form being built. Bricks sit on a grid, and every brick shape is a box with
some of its eight corners kept. A box with all eight corners is a cube, one
with six is a wedge, and one with seven is a cube with a corner sliced off.
Neighbouring bricks share corners, so when the corners are right the bricks
meet face to face without gaps or steps.

The boxes have to be placed so that their corners land on the form's true
surface, and each kind of form has its own method for that. A sphere is
sliced along each direction at distances chosen so that the corners on its
surface sit on the sphere. An arch or a ring has its outline traced by a
chain of corner points. Every corner has a price, so one long slope costs
less than a staircase of short steps that fits no better. Dynamic
programming finds the cheapest chain exactly, because it works along the
outline and keeps only the cheapest way of reaching each point. A tube is
fitted as the same outline with its edges then cut back to a slope, which
gives the tube an eight-sided section.

A form that none of these methods covers is fitted cell by cell, and each
cell takes whichever brick matches it best. That result is rougher, because
no cell knows what its neighbours hold. After any of the methods, the
finished bricks are compared with the true form. The volume where the two
disagree is reported as the discrepancy, so every fit can be judged by the
same number.

The full explanation, with the equations, is in [docs/algorithm.md](docs/algorithm.md).

## What it makes

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
