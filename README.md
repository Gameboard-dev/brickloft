# arcs

Design a curved form, see exactly which microbricks it becomes, and paste it
into Brickadia.

Forms: arches (round, pointed, triangular, elliptical, parabolic, catenary),
arcs and circles through points, splines, weighted Bézier curves, spheres,
ellipsoids, domes, segments, hollow shells, tori, pyramids solid or hollow,
and any profile spun about an axis. Each is fitted with the game's
microbricks (cubes, ramps and its corner wedges) so that the volume where
bricks and form disagree is as small as it can be made, and so that a
surface meant to be smooth is sloped bricks and not a staircase.

It is a desktop program, written in Rust, in two parts:

| Folder | What it is |
|---|---|
| `core/` | The geometry: curves, solids, and the fitters. No game or UI code. |
| `editor/` | The editor: a window to shape forms in, and the clipboard file it writes. |

It talks to the game through the clipboard alone: Ctrl+C in the editor puts
a Brickadia prefab (`.brz`) on the clipboard, and the game pastes it. It
once also had an omegga plugin that placed bricks on a running server; that
was more to install and keep working than copy and paste, and could not
place forms on grids of their own, so it was removed.

## Download

A Windows build is on the [releases page](../../releases): `arcs.exe`. It
needs nothing else.

The program is not code-signed, so an antivirus may hold it back as unknown
the first time it is run. Building from source, below, gives the same
program.

## Build

With Rust installed (<https://rustup.rs>):

```
cargo run --release -p arcs-editor
```

### Optional: a page for the browser

`sh build-editor.sh` builds the same editor as a web page, into `dist/`; the
top of that file lists the two tools it needs. The page shapes and measures
forms as the program does, but a browser page cannot put a file on the
clipboard, so it cannot hand a form to the game. It is there for looking and
trying, where installing a program is not wanted.

## Use

1. Pick a form, drag its dots, set thickness and the rest in the panel. The
   brick count and the discrepancy update as you go; **Simplify** and
   **Brick size** trade closeness of fit for fewer bricks.
2. Press Ctrl+C (or **Copy to clipboard**).
3. In the game, paste.

**Workspace** holds several forms at once. Add a copy of the one being
edited, add a saved form (its Add button, or drag it from the list onto the
view), and click a form in the view to edit it. Drag a form by its bricks to
move it: it moves one game unit at a time (half a microbrick), snapped to the
brick grid, and stays the same bricks wherever it stands. Forms on one grid
never share a cell: one dragged into another stops beside it or comes to
rest on top of it, and one reshaped until it reaches another pushes that one
away.

**An arch** starts on its two feet, with a kind and a height. Add point
gives it a third, and it becomes the arc through all three; more, up to
eight, and it is the smooth curve through them all.

**Added points.** Shift + click on a form adds a point that pulls it out of
shape there; the form around it follows, out to **Reach of added points**.
On a circle the point moves within the circle's plane, or out of it with
Ctrl held.

**Mirror**, along X, Y or Z, makes a form its own mirror image: its low
side along that axis is kept and the high side becomes its image. An arch
with one foot higher than the other gets two feet alike, and a point added
on either side of a circle appears on both.

**A torus** is the Circle form with "Round tube" ticked; there is no
separate Torus form. Its Stretch sliders make it an ellipse within its plane
and a deeper or shallower tube across it, and "Fill" closes the hole, giving
a disc with a rounded rim.

**Keys and mouse in the view:**

| | |
|---|---|
| Click a form | edit it |
| Drag a form by its bricks | move it |
| Shift + click a form | add a point to it |
| Delete | remove the form being edited |
| Ctrl+Z, Ctrl+Y | undo, redo |
| Ctrl+scroll | resize a circle, sphere or pyramid, a microbrick a notch |
| Ctrl + click another form | pick it to be copied along with the one being edited, or let it go |
| Ctrl+A | pick every form |
| Ctrl+C | write the form being edited and any picked with it (where each stands, its grid and its colours) to `arcs-build.brz`, a Brickadia prefab, and put that file on the clipboard |
| Ctrl+V | add the forms of a `.brz` copied from the editor back to the workspace |

A `.brz` holding any brick other than the microbricks is refused with a
message naming them. One made elsewhere from microbricks alone is refused
too: bricks cannot be turned back into a form, and only the editor's own
files carry their forms.

**Colour** is per form: one colour, a blend or stripes or a speckle of up to
four colours along an axis. Each brick is one colour.

**Snap to best fit nearby** moves the dots and sizes to the closest values
at which the form lands cleanly on the brick grid. **Reset this form** puts
it back to how it starts. Under **Saved forms** you can keep forms in folders
(`arches`, `domes`, or your own) and open them again; they are text files
under `arcs-forms/`.

In the view the camera is flown: right-drag looks around, as far as right
over backwards, a middle click turns it to face what the cursor is on,
W A S D move it the way it looks and sideways, Space or E raise it, Q lowers
it, and Shift with the scroll wheel makes it faster or slower.

**Grids.** The game keeps bricks on grids, and a grid other than the
world's can stand at any angle. Every form in the workspace is on a grid,
and the list says which. **Add to this grid** puts another form on the grid
being edited: the two are kept clear of each other, and are turned and moved
together. **New grid** starts another grid with a form of its own; grids
pass through each other.

The red, green and blue arrows and rings stand at the middle of everything
on the grid being edited, and are sized to it. Dragging an arrow moves the
whole grid along x, y or z; dragging a ring round turns the whole grid about
that axis, by any angle. A grid at an angle is copied to the clipboard as a
grid of its own, frozen where it stands. To move one form within its grid,
drag it by its bricks. Quarter turns of a single form, within its grid, have
their own buttons.

## More

- [docs/how-it-fits.md](docs/how-it-fits.md): the method, its guarantees and its limits
- [docs/development.md](docs/development.md): building, testing, measured performance
