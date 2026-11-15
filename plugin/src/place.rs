//! From validated bricks to bricks in the world: find where the player wants
//! them, give each its game asset and orientation, and hand them to the Lua
//! mod (mods/ArcsPlace) through the ue4ss plugin.

use crate::rpc::Rpc;
use arcs_core::orient::{Models, Solver};
use arcs_core::shapes::{CellBrick, CUBE, KINDS};
use serde_json::{json, Value};

/// The plugin's settings, from doc.json's `config` as omegga passes it to `init`.
pub struct Config {
    pub port: u16,
    pub bind: String,
    /// what players are told to open; empty means http://localhost:<port>
    pub url: String,
    pub max_bricks: usize,
    pub max_ext: i32,
    /// game asset per kind, in `Kind` order
    pub assets: [String; 9],
    pub models: Models,
}

impl Config {
    pub fn from_json(v: &Value) -> Config {
        let num = |key: &str, default: i64, lo: i64, hi: i64| v.get(key).and_then(Value::as_i64).unwrap_or(default).clamp(lo, hi);
        let text = |key: &str, default: &str| {
            v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(default).to_string()
        };
        let defaults = Models::default().local_mask;
        Config {
            port: num("port", 8765, 1, 65535) as u16,
            bind: text("bind", "127.0.0.1"),
            url: text("url", ""),
            max_bricks: num("max-bricks", 60_000, 1, 2_000_000) as usize,
            // the game refused a half-extent of 90 and took 80
            // (omegga-ue4ss/docs/brickadia-internals.md)
            max_ext: num("max-extent", 80, 1, 80) as i32,
            assets: [
                text("asset-cube", "PB_DefaultMicroBrick"),
                text("asset-ramp", "PB_DefaultMicroWedge"),
                text("asset-tetrahedron", "PB_DefaultMicroWedgeTriangleCorner"),
                text("asset-cut-cube", "PB_DefaultMicroWedgeOuterCorner"),
                text("asset-hip", "PB_DefaultMicroWedgeCorner"),
                text("asset-inner-corner", "PB_DefaultMicroWedgeInnerCorner"),
                text("asset-half-outer-corner", "PB_DefaultMicroWedgeHalfOuterCorner"),
                text("asset-half-inner-corner", "PB_DefaultMicroWedgeHalfInnerCorner"),
                text("asset-half-inner-corner-inverted", "PB_DefaultMicroWedgeHalfInnerCornerInverted"),
            ],
            models: Models {
                local_mask: [
                    0xff,
                    num("mask-ramp", defaults[1] as i64, 0, 255) as u8,
                    num("mask-tetrahedron", defaults[2] as i64, 0, 255) as u8,
                    num("mask-cut-cube", defaults[3] as i64, 0, 255) as u8,
                    num("mask-hip", defaults[4] as i64, 0, 255) as u8,
                    num("mask-inner-corner", defaults[5] as i64, 0, 255) as u8,
                    num("mask-half-outer-corner", defaults[6] as i64, 0, 255) as u8,
                    num("mask-half-inner-corner", defaults[7] as i64, 0, 255) as u8,
                    num("mask-half-inner-corner-inverted", defaults[8] as i64, 0, 255) as u8,
                ],
            },
        }
    }

    pub fn link_base(&self) -> String {
        if self.url.is_empty() {
            format!("http://localhost:{}", self.port)
        } else {
            self.url.trim_end_matches('/').to_string()
        }
    }
}

#[derive(Default)]
pub struct Outcome {
    pub sent: u64,
    pub placed: u64,
    pub blocked: u64,
}

/// Bricks per call to the Lua mod. Its own limit is 4096; smaller calls keep
/// each transaction to a few milliseconds of game time, so a large build
/// appears in steps instead of stalling the server once.
const CHUNK: usize = 2000;

/// The world position, in game units, of the centre of the player's ghost
/// brick, or of the player if they have no ghost brick out.
pub fn anchor(rpc: &Rpc, player: &str) -> Result<[i64; 3], String> {
    let round3 = |v: &Value| -> Option<[i64; 3]> {
        let a = v.as_array().filter(|a| a.len() == 3)?;
        let n: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|n| n.is_finite() && n.abs() < 1e6).collect();
        (n.len() == 3).then(|| [n[0].round() as i64, n[1].round() as i64, n[2].round() as i64])
    };
    if let Ok(ghost) = rpc.call("player.getGhostBrick", json!(player)) {
        if let Some(at) = ghost.get("location").and_then(round3) {
            return Ok(at);
        }
    }
    let position = rpc.call("player.getPosition", json!(player))?;
    round3(&position).ok_or_else(|| "could not find you in the world; take out a brick and aim where the build should go".to_string())
}

/// Integers per brick sent to the Lua mod: asset, size x y z, position
/// x y z, orientation, colour. The mod's own `FIELDS` must match.
const FIELDS: usize = 9;

/// The colours of a build: a palette, and for each brick 0 for the player's
/// own colour in the game or 1 and up for an entry of the palette.
#[derive(Default, Debug)]
pub struct Colours {
    pub palette: Vec<[u8; 3]>,
    pub paint: Vec<usize>,
}

impl Colours {
    /// Read and check the `palette` and `paint` of a request. Both may be
    /// left out, which leaves every brick to the player's colour.
    pub fn from_json(message: &Value, bricks: usize) -> Result<Colours, String> {
        let ints = |key: &str| -> Result<Vec<i64>, String> {
            match message.get(key) {
                None | Some(Value::Null) => Ok(Vec::new()),
                Some(v) => v.as_array().and_then(|a| a.iter().map(Value::as_i64).collect()).ok_or_else(|| format!("`{key}` must be a list of whole numbers")),
            }
        };
        let flat = ints("palette")?;
        if flat.len() % 3 != 0 || flat.len() > 3 * 256 || flat.iter().any(|v| !(0..=255).contains(v)) {
            return Err("the palette must be at most 256 colours, each three numbers from 0 to 255".into());
        }
        let palette: Vec<[u8; 3]> = flat.chunks_exact(3).map(|c| [c[0] as u8, c[1] as u8, c[2] as u8]).collect();
        let paint = ints("paint")?;
        if !paint.is_empty() && paint.len() != bricks {
            return Err(format!("there are {bricks} bricks and {} colours for them", paint.len()));
        }
        if paint.iter().any(|i| *i < 0 || *i as usize > palette.len()) {
            return Err("a brick's colour is not in the palette".into());
        }
        Ok(Colours { palette, paint: paint.into_iter().map(|i| i as usize).collect() })
    }
}

/// Place `bricks` for `player`: each in cells relative to the anchor, with
/// the odd unit along each axis that cells cannot hold (see `orient::from_wire`).
pub fn place(rpc: &Rpc, config: &Config, player: &str, origin: [i64; 3], bricks: &[(CellBrick, [i32; 3])], colours: &Colours) -> Result<Outcome, String> {
    let solver = Solver::new(&config.models);
    let mut flat = Vec::with_capacity(bricks.len() * FIELDS);
    for (i, (b, odd)) in bricks.iter().enumerate() {
        let p = solver.place(b).ok_or_else(|| {
            format!("no orientation of the configured assets makes shape {:#04x}; check the mask-* settings against /arc calibrate", b.mask)
        })?;
        // 1-based asset index, as Lua counts
        let colour = colours.paint.get(i).copied().unwrap_or(0) as i64;
        flat.extend([p.kind as i64 + 1, p.size[0] as i64, p.size[1] as i64, p.size[2] as i64, (p.pos[0] + odd[0]) as i64, (p.pos[1] + odd[1]) as i64, (p.pos[2] + odd[2]) as i64, p.orientation as i64, colour]);
    }

    // the player's current paint, or white plastic if it can't be read
    let paint = rpc.call("player.getPaint", json!(player)).unwrap_or(Value::Null);
    let color: Vec<i64> = paint
        .get("color")
        .and_then(Value::as_array)
        .map(|c| c.iter().filter_map(Value::as_i64).map(|v| v.clamp(0, 255)).collect())
        .filter(|c: &Vec<i64>| c.len() >= 3)
        .unwrap_or_else(|| vec![255, 255, 255]);
    let material = paint.get("material").and_then(Value::as_str).unwrap_or("Plastic").trim_start_matches("BMC_").to_string();

    let mut outcome = Outcome::default();
    for (i, chunk) in flat.chunks(CHUNK * FIELDS).enumerate() {
        let request = json!({
            "player": player,
            "first": i == 0,
            "origin": origin,
            // the player's own colour, for bricks with colour 0
            "color": &color[..3],
            "palette": colours.palette,
            "material": material,
            "assets": config.assets,
            "bricks": chunk,
        });
        let result = lua(rpc, "arcs_place", request)?;
        let count = |key: &str| result.get(key).and_then(Value::as_u64).unwrap_or(0);
        outcome.sent += count("sent");
        outcome.placed += count("placed");
        outcome.blocked += count("blocked");
    }
    Ok(outcome)
}

pub fn undo(rpc: &Rpc, player: &str) -> Result<u64, String> {
    Ok(lua(rpc, "arcs_undo", json!(player))?.get("removed").and_then(Value::as_u64).unwrap_or(0))
}

/// Call a handler of our Lua mod through the ue4ss plugin.
/// omegga-ue4ss/docs/interop.md, "From an omegga plugin"
fn lua(rpc: &Rpc, handler: &str, arg: Value) -> Result<Value, String> {
    let reply = rpc.call("plugin.emit", json!(["ue4ss", "call", handler, arg]))?;
    if reply.is_null() {
        return Err("the ue4ss plugin is not loaded; install omegga-ue4ss next to this plugin and restart omegga".into());
    }
    if reply.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = reply.get("error").and_then(Value::as_str).unwrap_or("no reason given");
        return Err(format!("the game refused `{handler}`: {why}. If it says the handler is unknown, the ArcsPlace mod did not load: check that the server was restarted after installing this plugin"));
    }
    Ok(reply.get("result").cloned().unwrap_or(Value::Null))
}

/// One brick of each kind as its asset sits at orientation 16, with a unit
/// cube touching each corner the settings say is filled.
///
/// Standing in front of them shows at a glance whether a `mask-*` setting is
/// wrong: a marker cube floats beside an empty corner, or a filled corner has
/// none. The bricks are 2 x 3 x 4 cells so that no two axes can be confused.
pub fn calibration(models: &Models) -> Vec<CellBrick> {
    let ext = [2, 3, 4];
    let mut bricks = Vec::new();
    for kind in KINDS {
        let mask = models.local_mask[kind as usize];
        let min = [kind as i32 * 8, 0, 1];
        bricks.push(CellBrick { mask, min, ext });
        // (an inner corner's mask is not the corners it holds: see `shapes`)
        let held = arcs_core::shapes::corners(mask);
        for corner in (0..8u8).filter(|c| held >> c & 1 == 1) {
            // diagonally outside the corner, touching it at a point
            let at = [0, 1, 2].map(|i| if corner >> i & 1 == 1 { min[i] + ext[i] } else { min[i] - 1 });
            bricks.push(CellBrick { mask: CUBE, min: at, ext: [1; 3] });
        }
    }
    bricks
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcs_core::shapes::shape_info;

    #[test]
    fn config_takes_defaults_and_clamps() {
        let c = Config::from_json(&json!({}));
        assert_eq!((c.port, c.bind.as_str(), c.max_ext), (8765, "127.0.0.1", 80));
        assert_eq!(c.link_base(), "http://localhost:8765");
        assert_eq!(c.models, Models::default());

        let c = Config::from_json(&json!({ "port": 99999, "max-extent": 500, "url": " https://example.org/arcs/ ", "mask-cut-cube": 254, "bind": "" }));
        assert_eq!((c.port, c.max_ext, c.bind.as_str()), (65535, 80, "127.0.0.1"));
        assert_eq!(c.link_base(), "https://example.org/arcs");
        assert_eq!(c.models.local_mask[3], 254);
    }

    #[test]
    fn colours_are_checked() {
        let ok = Colours::from_json(&json!({ "palette": [200, 0, 0, 0, 0, 200], "paint": [1, 0, 2] }), 3).unwrap();
        assert_eq!((ok.palette, ok.paint), (vec![[200, 0, 0], [0, 0, 200]], vec![1, 0, 2]));
        // left out altogether: every brick the player's own colour
        let none = Colours::from_json(&json!({}), 3).unwrap();
        assert!(none.palette.is_empty() && none.paint.is_empty());
        for (bad, why) in [
            (json!({ "palette": [200, 0], "paint": [] }), "palette"),
            (json!({ "palette": [200, 0, 256] }), "palette"),
            (json!({ "palette": [200, 0, -1] }), "palette"),
            (json!({ "palette": "red" }), "whole numbers"),
            (json!({ "palette": [1, 2, 3], "paint": [1, 1] }), "3 bricks and 2 colours"),
            (json!({ "palette": [1, 2, 3], "paint": [1, 2, 0] }), "not in the palette"),
            (json!({ "palette": [1, 2, 3], "paint": [1, -1, 0] }), "not in the palette"),
            (json!({ "paint": [0, 0, 1.5] }), "whole numbers"),
            (json!({ "palette": vec![0; 3 * 257] }), "palette"),
        ] {
            let error = Colours::from_json(&bad, 3).unwrap_err();
            assert!(error.contains(why), "{bad}: {error}");
        }
    }

    #[test]
    fn calibration_is_placeable_and_marks_every_filled_corner() {
        let models = Models::default();
        let bricks = calibration(&models);
        let solver = Solver::new(&models);
        // 9 bricks and a marker per filled corner: 8 + 6 + 4 + 7 + 5, and
        // 7 + 5 + 4 + 4 for the inner corner and the half corners
        assert_eq!(bricks.len(), 9 + 30 + 20);
        let mut cells = std::collections::HashSet::new();
        for b in &bricks {
            assert!(shape_info(b.mask).is_some());
            // the test bricks must come out unrotated, or they would show
            // nothing about how the asset sits
            let placed = solver.place(b).unwrap();
            if b.ext != [1; 3] {
                assert_eq!(placed.orientation, 16, "{b:?}");
                assert_eq!(placed.size, [2, 3, 4]);
            }
            for z in 0..b.ext[2] {
                for y in 0..b.ext[1] {
                    for x in 0..b.ext[0] {
                        assert!(cells.insert([b.min[0] + x, b.min[1] + y, b.min[2] + z]), "overlap at {b:?}");
                    }
                }
            }
        }
    }
}
