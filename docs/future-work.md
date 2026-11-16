# Future work

Two additions are planned. Neither changes how forms are fitted today (see
[algorithm.md](algorithm.md)).

## More for Claude to do through MCP

Claude can already drive the editor: add, change, colour, turn, measure and
remove forms, move the camera and take screenshots, through the Model
Context Protocol ([mcp.md](mcp.md)). Three things are still to come.

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

**Finer control of shape.** Claude sets a form's points and added points
through its settings today. Tools that move one dot, or add a pull point at
a place on the form's surface, would match what the mouse does and need
less arithmetic from Claude.

**Copying to the game.** Claude cannot put the workspace on the clipboard,
so the player still presses Ctrl+C. A tool for it would let Claude finish a
build end to end, at the cost of a tool that writes outside the editor,
which the present tools deliberately do not.

## Patterns by wave function collapse

[Wave function collapse](https://github.com/mxgmn/WaveFunctionCollapse)
fills a grid with tiles so that every tile agrees with its neighbours. It
starts with every cell able to hold any tile. It then repeatedly picks the
cell with the fewest tiles left to choose from, fixes one of them, and
removes from the neighbouring cells every tile that no longer fits, which
is the propagation. The editor would use it to complete a pattern from a
small start, in two dimensions for a floor or a wall and in three for a
pattern with depth.

The patterns planned:

| Pattern | What it is | How it starts |
|---|---|---|
| Mosaic floor | small coloured tiles forming motifs and borders | a random seed and rules for which tiles may meet |
| Persian rug | a central medallion, a field and nested borders, symmetric in both directions | a random seed and the rug's rules of symmetry |
| Stepping stone path | separate flat stones along a line the player draws | the line, with the stones' size, spacing and irregularity adjustable |
| Stone slab wall | slabs of varied size in courses, with joints that do not line up | the wall's size and the range of slab sizes |

For the mosaic floor and the Persian rug, a random seed chooses the first
few tiles and the rules decide everything else, so the same seed always
gives the same pattern. A Persian rug's rules resemble a snowflake's. The
whole is fixed by one part repeated under mirror images, so only a quarter
(or an eighth) is generated, and the rest is its reflection. The rules for
the rug would also set where borders may run and which motifs may sit
inside which.

Open questions:

- **Tiles as bricks.** A tile here is a small group of bricks with a
  colour, so the pattern's output is bricks directly, without the fitting
  step a curved form needs.
- **Dead ends.** Wave function collapse can reach a cell with no tile left
  that fits. The usual answers are to step back and try another tile, or
  to start again with the next seed. Which one keeps the editor responsive
  on a large floor has to be measured.
- **Patterns on forms.** A pattern might later be laid over a fitted form
  (a mosaic on a dome) by choosing colours for the form's bricks. That
  needs the pattern mapped onto a curved surface and is a separate step.
