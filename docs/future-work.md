# Future work

Two additions are planned and not yet started. Neither changes how forms
are fitted today (see [algorithm.md](algorithm.md)).

## Claude drives the editor through MCP

The [Model Context Protocol](https://modelcontextprotocol.io) (MCP) lets an
AI assistant such as Claude call tools that a program offers. The editor
would offer its own operations as tools, so that Claude creates forms in
the running editor and reshapes them while the player watches.

The tools would take exact values where the mouse takes approximate ones:

- **Create and remove forms:** any of the editor's forms, on a named grid.
- **Lengths:** spans, radii, rises, thicknesses and depths in microbricks,
  to the half microbrick the editor already snaps to.
- **Angles:** an arch's sweep, a dome's cut, and a grid's turn about each
  axis in degrees.
- **Placement and orientation:** where a form stands in its grid, where
  the grid stands, quarter turns, flips and mirrors.
- **Added points:** where a point is added, how far it is pulled, and its
  reach.
- **Colour and shading:** the colours of a form, the kind of blend, its
  axis and its stripe width.
- **Reading back:** each form's settings, brick count and discrepancy, so
  that Claude can check a change and correct it.

Every change would pass through the same code as a change made by hand. It
would therefore be refitted, drawn and added to the undo history in the
same way, and Ctrl+Z would take back a step Claude made.

Open questions:

- **Transport.** MCP servers usually speak over standard input and output,
  which a windowed program does not have to spare. The editor would more
  likely listen on a local port, or a small separate program would relay
  between Claude and the editor.
- **Trust.** Tool calls arrive from outside the program. Every value has
  to be validated and limited in size exactly as a pasted file's values
  are, and the listener must accept connections from the same computer
  only.
- **Scope.** The plugin that once sent bricks to a running game server was
  removed as needless complexity. This connection runs the other way, into
  the editor, and the game is still reached only through copy and paste.

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
