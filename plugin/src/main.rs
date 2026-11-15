//! omegga-arcs: an omegga plugin that puts forms from the arcs editor into
//! the world.
//!
//! It is a JSON-RPC plugin (omegga/docs/plugins/jsonrpc.md): omegga starts
//! this program and they exchange one JSON message per line. The geometry is
//! all in the editor; this side hands out links, checks what comes back, and
//! places it.

mod http;
mod place;
mod rpc;

use place::Config;
use rpc::Rpc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::BufRead;
use std::sync::{Arc, Mutex};

pub struct Shared {
    pub rpc: Arc<Rpc>,
    pub config: Config,
    /// link token -> player name. One per player; a new /arc replaces the old.
    pub tokens: Mutex<HashMap<String, String>>,
    /// held while a build is being placed
    pub building: Mutex<()>,
}

/// 128 random bits as hex: unguessable, and the only thing that says which
/// player a request from the editor is for.
fn new_token() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("the system gave no random numbers: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn command(shared: &Shared, player: &str, args: &[String]) {
    let rpc = &shared.rpc;
    match args.first().map(String::as_str) {
        None | Some("link") => match new_token() {
            Ok(token) => {
                let mut tokens = shared.tokens.lock().unwrap();
                tokens.retain(|_, p| p != player);
                tokens.insert(token.clone(), player.to_string());
                rpc.whisper(player, format!("Design a form here, then press Send: {}/?t={token}", shared.config.link_base()));
                rpc.whisper(player, "It is built at your ghost brick, so take out a brick and aim before sending.");
            }
            Err(e) => {
                rpc.error(format!("arcs: could not make a link for {player}: {e}"));
                rpc.whisper(player, "Could not make a link; the server's log says why.");
            }
        },
        Some("undo") => match place::undo(rpc, player) {
            Ok(0) => rpc.whisper(player, "There is nothing of yours to undo."),
            Ok(n) => rpc.whisper(player, format!("Removed your last build ({n} bricks).")),
            Err(e) => {
                rpc.error(format!("arcs: undo for {player} failed: {e}"));
                rpc.whisper(player, format!("Could not undo: {e}"));
            }
        },
        Some("calibrate") => {
            let bricks: Vec<_> = place::calibration(&shared.config.models).into_iter().map(|b| (b, [0; 3])).collect();
            let result = place::anchor(rpc, player).and_then(|at| place::place(rpc, &shared.config, player, at, &bricks, &place::Colours::default()));
            match result {
                Ok(o) => rpc.whisper(
                    player,
                    format!("Placed {} test bricks: cube, ramp, tetrahedron, cut cube. Each should have a small cube touching every filled corner and none at an empty one. If not, fix the mask-* settings.", o.placed),
                ),
                Err(e) => {
                    rpc.error(format!("arcs: calibrate for {player} failed: {e}"));
                    rpc.whisper(player, format!("Could not place the test bricks: {e}"));
                }
            }
        }
        _ => rpc.whisper(player, "/arc gives you a link to the editor. /arc undo removes your last build. /arc calibrate places test bricks."),
    }
}

fn main() {
    // one connection to omegga, shared by this loop and every worker thread
    let rpc = Arc::new(Rpc::default());
    let mut shared: Option<Arc<Shared>> = None;

    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
        let id = message.get("id").filter(|id| !id.is_null());
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // no method: an answer to one of our own calls
            rpc.resolve(&message);
            continue;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "init" => {
                // the config object, or a one-element list holding it
                let config = Config::from_json(params.get(0).filter(|_| params.is_array()).unwrap_or(&params));
                let state = Arc::new(Shared { rpc: rpc.clone(), config, tokens: Mutex::default(), building: Mutex::default() });
                let for_http = state.clone();
                std::thread::spawn(move || http::serve(for_http));
                if let Some(id) = id {
                    rpc.respond(id, json!({ "registeredCommands": ["arc"] }));
                }
                shared = Some(state);
            }
            "stop" => {
                if let Some(id) = id {
                    rpc.respond(id, json!({}));
                }
                // the HTTP thread has no state worth saving; ending the process closes its socket
                return;
            }
            "cmd:arc" => {
                let Some(state) = shared.clone() else { continue };
                let mut words: Vec<String> = params.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
                if words.is_empty() {
                    continue;
                }
                let player = words.remove(0);
                // commands wait on omegga's answers, which arrive on this thread
                std::thread::spawn(move || command(&state, &player, &words));
            }
            _ => {
                // omegga forwards every event; only requests need an answer
                if let Some(id) = id {
                    rpc.respond_error(id, -32601, "method not found");
                }
            }
        }
    }
}
