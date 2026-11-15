# arcs

Design a curved form in a browser, see exactly which microbricks it becomes,
and build it in Brickadia at your ghost brick.

Forms: arches (round, pointed, triangular, elliptical, parabolic, catenary),
arcs and circles through points, splines, weighted Bézier curves, spheres,
ellipsoids, domes, segments, hollow shells, tori, and any profile spun about
an axis. Each is fitted with cubes, ramps, tetrahedra and cut cubes so that
the volume where bricks and form disagree is as small as it can be made.

It is an [omegga](https://github.com/brickadia-community/omegga) plugin,
written in Rust, in four parts:

| Folder | What it is |
|---|---|
| `core/` | The geometry: curves, solids, and the two fitters. No game or UI code. |
| `editor/` | The editor, compiled to WebAssembly for the browser. |
| `plugin/` | The omegga plugin: serves the editor, checks what it sends, places it. |
| `mods/ArcsPlace/` | A small UE4SS Lua mod the plugin calls to place and remove bricks. |

## Requirements

- omegga 1.23 or newer, with the [omegga-ue4ss](https://github.com/Meshiest/omegga-ue4ss) plugin installed (bricks are placed through it).
- Rust, to build: <https://rustup.rs>

## Install

1. Put this folder in omegga's `plugins/` directory, named `arcs`.
2. Build the plugin: `cargo build --release -p omegga-arcs` (omegga's installer runs this for you through `setup.sh`).
3. Build the editor: `sh build-editor.sh`. The top of that file lists the two tools it needs.
4. Restart omegga, so that UE4SS loads the Lua mod.

## Use

1. Type `/arc`. You are whispered a link.
2. Open it. Pick a form, drag its dots, set thickness and the rest in the panel. The brick count and the discrepancy update as you go; **Simplify** and **Brick size** trade closeness of fit for fewer bricks.
3. In game, take out a brick and aim where the form should go.
4. Press **Send to Brickadia**. The cell at the editor's coloured axes lands on your ghost brick.
5. `/arc undo` removes your last build.

**Workspace** holds several forms at once. Add a copy of the one being
edited, add a saved form (its Add button, or drag it from the list onto the
view), and click a form in the view to edit it. Drag a form by its bricks to
move it. Forms never share a cell: one dragged into another stops beside it
or comes to rest on top of it. Everything in the workspace is sent together.

**A torus** is the Circle form with "Round tube" ticked; there is no
separate Torus form. Its Stretch sliders make it an ellipse within its plane
and a deeper or shallower tube across it, and "Fill" closes the hole, giving
a disc with a rounded rim.

**Keys and mouse in the view:**

| | |
|---|---|
| Click a form | edit it |
| Drag a form by its bricks | move it |
| Delete | remove the form being edited |
| Ctrl+Z, Ctrl+Y | undo, redo |
| Ctrl+scroll | resize a circle or sphere, a microbrick of radius a notch |
| Ctrl+C (desktop) | write the whole workspace (every form, where it stands, and its colours) to `arcs-build.brz`, a Brickadia prefab, and put that file on the clipboard |
| Ctrl+V (desktop) | add the forms of a `.brz` copied from the editor back to the workspace |

A `.brz` holding any brick other than the four microbricks is refused with
a message naming them. One made elsewhere from microbricks alone is refused
too: bricks cannot be turned back into a form, and only the editor's own
files carry their forms.

**Colour** is per form: one colour, a blend or
stripes or a speckle of up to four colours along an axis. Each brick is one
colour.

**Snap to best fit nearby** moves the dots and sizes to the closest values
at which the form lands cleanly on the brick grid. **Reset this form** puts
it back to how it starts. Under **Saved forms** you can keep forms in folders
(`arches`, `domes`, or your own) and open them again; on the desktop they are
text files under `arcs-forms/`, in the browser they are kept by the browser.

In the view: right-drag turns it, Shift + drag slides it, the wheel zooms,
W A S D move the camera over the ground, and Space and Shift raise and
lower it.

By default the editor is reachable only from the server's own machine. To let
other players use it, see [docs/security.md](docs/security.md).

## More

- [docs/how-it-fits.md](docs/how-it-fits.md): the method, its guarantees and its limits
- [docs/calibration.md](docs/calibration.md): checking the brick assets against the game
- [docs/security.md](docs/security.md): what the plugin listens on and what it trusts
- [docs/development.md](docs/development.md): building, testing, measured performance
