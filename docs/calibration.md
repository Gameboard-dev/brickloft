# Calibration

The plugin has to know how each of the game's brick assets sits when it is
not rotated: which corners of its box are filled. The defaults have been
**confirmed in the game** (November 2026), so there is nothing to do here
unless a game update changes the assets or the `asset-*` settings are pointed
at different bricks. The check below is for those cases.

## The check

1. Take out a brick and aim at open ground.
2. Type `/arc calibrate`.

Five test bricks appear in a row along X, each 2 × 3 × 4 microbricks: cube,
ramp, tetrahedron, cut cube, hip. Around each are small marker cubes.

**The hip is the one that has not been checked.** It is the game's
`PB_DefaultMicroWedgeCorner`, used on a pyramid's ridges, and how it sits
unrotated is a guess: `mask-hip` is 87, and the other three ways it could
sit are 117, 55 and 115. If its markers are wrong, try those in turn.

**It is correct when every filled corner of a test brick has a marker touching
it, and no marker sits beside an empty corner.** `/arc undo` removes them.

## If it is wrong

Each asset has a `mask-*` setting: a number with one bit per corner of the
brick's box, set where the corner is filled.

| Corner (x, y, z; 0 = low side, 1 = high side) | Value |
|---|---|
| (0,0,0) | 1 |
| (1,0,0) | 2 |
| (0,1,0) | 4 |
| (1,1,0) | 8 |
| (0,0,1) | 16 |
| (1,0,1) | 32 |
| (0,1,1) | 64 |
| (1,1,1) | 128 |

Add up the values of the corners the test brick actually fills and enter the
sum. Low and high are along the world's axes as the test brick stands.

| Setting | Default | Meaning of the default |
|---|---|---|
| `mask-ramp` | 119 | every corner except the two at high X, high Y |
| `mask-tetrahedron` | 23 | the low corner and its three neighbours |
| `mask-cut-cube` | 127 | every corner except high X, high Y, high Z |

If a test brick is the wrong shape altogether (for example a pyramid where
the tetrahedron should be), the asset name is wrong: change `asset-tetrahedron`
or `asset-cut-cube`. The game's micro wedge assets are listed in
`omegga-ue4ss/types/enums.lua`.

Reload the plugin after changing settings, then run the check again.

## Other things not yet confirmed in game

- That `getGhostBrick` still reports a position on the current game build. If
  it does not, builds are placed at the player's own position instead.
- That microbrick assets accept every half-extent from 1 to 80.
- That the Lua library's function names match what `mods/ArcsPlace` expects.
  It tries both the documented names and the ones the library's own example
  uses.
