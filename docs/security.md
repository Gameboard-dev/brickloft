# Security and what the plugin discloses

## What it listens on

The plugin runs a small web server of its own for the editor, on `bind:port`
(default `127.0.0.1:8765`, this machine only). It serves six fixed files
from `dist/` and accepts one request, `POST /build`. No request can name a
file path.

To let other players use the editor, set `bind` to `0.0.0.0` and `url` to the
address they should open. The server speaks plain HTTP. Across the internet,
put it behind something that adds HTTPS, or a link could be read in transit.

## Who may build

`/arc` whispers the player a link ending in a 128-bit random token. The token
is the only thing that says which player a build is for.

- Anyone who has a player's link can build as that player until the player
  types `/arc` again, which retires the old link, or the plugin restarts.
- The page asks browsers not to pass its address on (`Referrer-Policy:
  no-referrer`).
- Builds are placed with the server's own authority and are owned by the
  player. The server's `max-bricks` setting caps each build.

## What is trusted

Nothing from the editor is. Every number in a build, its colours included, is range-checked before
use (`from_wire` in `core/src/orient.rs`) and again in the Lua mod, and a
request body is capped in size.

The plugin never generates code. It calls two fixed handlers in
`mods/ArcsPlace` with plain data, and the mod accepts only asset names from a
fixed list. Generating Lua from a build would have made every number in it a
way into the game process.

## Network calls

- The plugin makes none. It only answers.
- The editor makes one, to the server it was loaded from, when Send is pressed.
- Building from source downloads Rust crates from crates.io, as any Rust
  build does.

## Antivirus

Freshly built test programs open a local port and start the plugin as a child
process, which some antivirus heuristics flag. If tests fail to link or run,
exclude the `target/` folder.
