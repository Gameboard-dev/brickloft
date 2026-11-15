//! The editor's web server: six static files and one POST.
//!
//! The plugin serves these itself instead of through omegga's plugin HTTP
//! routes (docs/plugins/README.md, "Plugin HTTP Routes"): those answer with
//! text or JSON only, so they cannot carry the editor's .wasm file, and they
//! require a login to omegga's admin web UI, which players do not have.
//! Players are instead recognised by a one-off token whispered to them in game.

use crate::place::{anchor, place, Colours};
use crate::Shared;
use arcs_core::orient::from_wire;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server};

/// Largest request body read. A brick is at most about 40 bytes of JSON, so
/// this covers the largest `max-bricks` setting with room to spare, and stops
/// anyone filling the server's memory with a single request.
const MAX_BODY: usize = 96 * 1024 * 1024;

/// The only files served, by exact name: a request can never name a path.
const FILES: [(&str, &str, &str); 6] = [
    ("/", "index.html", "text/html; charset=utf-8"),
    ("/style.css", "style.css", "text/css"),
    ("/loader.js", "loader.js", "text/javascript"),
    ("/icon.svg", "icon.svg", "image/svg+xml"),
    ("/arcs_editor.js", "arcs_editor.js", "text/javascript"),
    ("/arcs_editor_bg.wasm", "arcs_editor_bg.wasm", "application/wasm"),
];

/// Where the built editor is: `dist/` in the plugin's folder. omegga starts
/// the plugin from that folder; the executable's own location (three levels
/// down, in target/release) is the fallback if it was started some other way.
fn dist_dir() -> Option<PathBuf> {
    let here = std::env::current_dir().ok().map(|d| d.join("dist"));
    let by_exe = std::env::current_exe().ok().and_then(|e| e.ancestors().nth(3).map(|d| d.join("dist")));
    [here, by_exe].into_iter().flatten().find(|d| d.join("index.html").is_file())
}

pub fn serve(shared: Arc<Shared>) {
    let address = format!("{}:{}", shared.config.bind, shared.config.port);
    let server = match Server::http(&address) {
        Ok(server) => server,
        Err(e) => {
            shared.rpc.error(format!(
                "arcs: could not listen on {address}: {e}. The editor is unavailable. Another program may hold the port; change `port` in this plugin's settings and reload it."
            ));
            return;
        }
    };
    shared.rpc.log(format!("arcs: editor at http://{address}/ (players get their own link with /arc)"));
    if dist_dir().is_none() {
        shared.rpc.error("arcs: the editor has not been built, so its page will not load. Run `sh build-editor.sh` in the plugin's folder (see README.md).");
    }
    for request in server.incoming_requests() {
        let shared = shared.clone();
        // a build waits on the game for seconds; one thread each keeps the
        // page loading for everyone else meanwhile
        std::thread::spawn(move || handle(&shared, request));
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

fn handle(shared: &Shared, mut request: Request) {
    let path = request.url().split('?').next().unwrap_or("").to_string();
    let result = match (request.method(), path.as_str()) {
        (Method::Get, _) => {
            let file = FILES.iter().find(|f| f.0 == path);
            match (file, dist_dir()) {
                (None, _) => request.respond(Response::from_string("not found").with_status_code(404)),
                (Some(_), None) => request.respond(
                    Response::from_string("The editor has not been built on this server. Its owner needs to run build-editor.sh in the plugin's folder.").with_status_code(503),
                ),
                (Some(&(_, name, mime)), Some(dir)) => match std::fs::read(dir.join(name)) {
                    Ok(bytes) => request.respond(
                        Response::from_data(bytes)
                            .with_header(header("Content-Type", mime))
                            // the page holds a player's token in its address
                            .with_header(header("Referrer-Policy", "no-referrer"))
                            .with_header(header("Cache-Control", "no-cache")),
                    ),
                    Err(e) => {
                        shared.rpc.error(format!("arcs: could not read {}: {e}. Rebuild the editor with build-editor.sh.", dir.join(name).display()));
                        request.respond(Response::from_string("the editor's files are missing").with_status_code(500))
                    }
                },
            }
        }
        (Method::Post, "/build") => {
            let (status, body) = match build(shared, &mut request) {
                Ok(body) => (200, body),
                Err((status, why)) => (status, json!({ "ok": false, "error": why })),
            };
            request.respond(Response::from_string(body.to_string()).with_status_code(status).with_header(header("Content-Type", "application/json")))
        }
        _ => request.respond(Response::from_string("not found").with_status_code(404)),
    };
    // the browser going away mid-answer is not worth a log line
    let _ = result;
}

/// Handle `POST /build`: `{ "token": "...", "bricks": [integers] }`, with
/// an optional `palette` and `paint` (see `place::Colours`).
fn build(shared: &Shared, request: &mut Request) -> Result<Value, (u16, String)> {
    if request.body_length().is_some_and(|n| n > MAX_BODY) {
        return Err((413, "the build is too large to send".into()));
    }
    let mut body = Vec::new();
    // `take` enforces the limit even when the length header lies or is absent
    request.as_reader().take(MAX_BODY as u64 + 1).read_to_end(&mut body).map_err(|e| (400, format!("could not read the request: {e}")))?;
    if body.len() > MAX_BODY {
        return Err((413, "the build is too large to send".into()));
    }
    let message: Value = serde_json::from_slice(&body).map_err(|e| (400, format!("the request is not JSON: {e}")))?;

    let token = message.get("token").and_then(Value::as_str).unwrap_or("");
    let player = shared.tokens.lock().unwrap().get(token).cloned();
    let player = player.ok_or((401, "this link is no longer valid; type /arc in the game for a new one".to_string()))?;

    let ints: Option<Vec<i64>> = message.get("bricks").and_then(Value::as_array).map(|a| a.iter().map(Value::as_i64).collect()).unwrap_or(None);
    let ints = ints.ok_or((400, "the brick list must be whole numbers".to_string()))?;
    let bricks = from_wire(&ints, shared.config.max_ext).map_err(|e| (400, e))?;
    if bricks.is_empty() {
        return Err((400, "there are no bricks to place".into()));
    }
    if bricks.len() > shared.config.max_bricks {
        return Err((413, format!("{} bricks is more than this server allows ({}); make the form smaller", bricks.len(), shared.config.max_bricks)));
    }
    let colours = Colours::from_json(&message, bricks.len()).map_err(|e| (400, e))?;

    // one build at a time: two at once would interleave their transactions
    let _turn = shared.building.lock().unwrap();
    let origin = anchor(&shared.rpc, &player).map_err(|e| (409, e))?;
    let outcome = place(&shared.rpc, &shared.config, &player, origin, &bricks, &colours).map_err(|e| {
        shared.rpc.error(format!("arcs: build for {player} failed: {e}"));
        (502, e)
    })?;
    let mut line = format!("Placed {} of {} bricks.", outcome.placed, outcome.sent);
    if outcome.blocked > 0 {
        line += &format!(" {} were blocked by bricks already there.", outcome.blocked);
    }
    shared.rpc.whisper(&player, format!("{line} /arc undo removes them."));
    Ok(json!({ "ok": true, "placed": outcome.placed, "blocked": outcome.blocked, "sent": outcome.sent }))
}
