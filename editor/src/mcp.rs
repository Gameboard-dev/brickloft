//! Letting Claude drive the editor, through the Model Context Protocol.
//!
//! MCP is how an AI assistant calls the tools a program offers:
//! https://modelcontextprotocol.io/specification/2025-06-18
//! The editor offers its workspace as tools (see [`TOOLS`]): read the forms,
//! add one, change one's settings, colours, place and grid, remove one. A
//! change made this way goes through the same code as one made by hand, so
//! it is fitted, drawn and added to the undo history like any other, and is
//! seen in the view as it happens.
//!
//! ## How Claude reaches the editor
//!
//! Over HTTP, on this computer only (the protocol's "Streamable HTTP"
//! transport). The protocol's other transport has the assistant start the
//! program itself and talk over its standard input and output. That suits a
//! tool with no window. The editor is a window the player already has open
//! and wants to watch, so the assistant has to reach a program that is
//! already running, which is what a listening port is for.
//!
//! The HTTP is read and written here by hand, on the standard library's
//! sockets. One request is one small JSON message with a length given in
//! advance and one JSON message goes back, which is a few dozen lines; a web
//! server crate would bring an asynchronous runtime with it for the same.
//!
//! ## What is trusted
//!
//! Nothing that arrives. A request may come from any program on this
//! computer, or from a web page open in a browser on it, so:
//!
//! - the listener is bound to 127.0.0.1, and is off until the player turns
//!   it on in the panel;
//! - a request must be addressed to this computer by name (`Host`), and if
//!   a browser sent it (`Origin`) from a page of this computer, which is the
//!   protocol's defence against a web page elsewhere reaching a local
//!   server: https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#security-warning
//! - a request's head and body are limited in size and in the time they may
//!   take to arrive;
//! - every setting is read by the parser that reads a saved form
//!   (`brickloft_core::report::parse_spec`) and built by the checks any form is
//!   built by, and the number of forms, points and colours is limited.
//!
//! Desktop only: a web page cannot listen for connections.

use crate::paint::{Mode, Paint};
use crate::state::{out_of_grid, Leg, Placed, State};
use brickloft_core::DVec3;
use brickloft_core::report::{parse_spec, spec_text};
use brickloft_core::spec::Spec;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

/// The port the editor listens on. Fixed, so that the address the player
/// gives Claude once goes on working.
pub const PORT: u16 = 7878;

/// The versions of the protocol this speaks, newest first.
const VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// The most a request's head and its body may be, in bytes. A body is a
/// tool call: a few hundred bytes, or a few thousand for a form with many
/// points.
const MAX_HEAD: usize = 16 << 10;
const MAX_BODY: usize = 1 << 20;
/// How long a request may take to arrive, and its answer to be taken.
const SOCKET_WAIT: Duration = Duration::from_secs(5);
/// How long a connection is kept after its answer for the other end to close it.
const LINGER: Duration = Duration::from_secs(2);
/// How long the frame loop is given to carry out a tool call.
const CALL_WAIT: Duration = Duration::from_secs(30);

/// The most forms a workspace may hold, and the most points a form may be
/// drawn through, when they are set from outside.
const MAX_FORMS: usize = 200;
const MAX_POINTS: usize = 64;

/// What the editor offers: each tool's name, what to tell Claude about it,
/// and the JSON Schema of what it takes.
const TOOLS: &str = include_str!("mcp_tools.json");

/// What a tool gives back: words, or a picture as the bytes of a PNG file.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Text(String),
    Picture(Vec<u8>),
}

type Answered = Result<Answer, String>;

/// One tool call on its way to the frame loop, and where its answer goes.
struct Call {
    name: String,
    arguments: Value,
    answer: Sender<Answered>,
}

/// The listener, as the frame loop holds it. Dropping it stops listening.
pub struct Server {
    calls: Receiver<Call>,
    stop: Arc<AtomicBool>,
    /// Those waiting for a picture of the window: the widest each will
    /// take, whether it is to have the panel in it, and where to send it. A
    /// picture can only be taken once the frame has been drawn, which is
    /// after the calls are carried out.
    waiting: Vec<(u32, bool, Sender<Answered>)>,
    /// How many of those have waited a whole frame. A picture is of the
    /// frame after the one it was asked in: the panel of the frame it was
    /// asked in was laid out before the call was read, with whatever notice
    /// the call before it had put up, and the notice of a call is not part
    /// of what the caller asked to see.
    ripe: usize,
}

/// The widest picture given unless a narrower or wider one is asked for,
/// and the widest that may be asked for, in pixels. A picture is sent as
/// text a third again its size, and is read by a model that is charged for
/// its area: a window's full width is seldom worth either.
const USUAL_WIDTH: u32 = 1280;
const WIDEST: u32 = 4096;

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Server {
    /// Start listening on this computer. `Err` says why not, most often
    /// that another program has the port.
    pub fn start() -> Result<Server, String> {
        Server::start_on(PORT).map(|(server, _)| server)
    }

    /// [`Server::start`] on a port of the caller's choosing, or on any free
    /// one for port 0; returns the port taken.
    fn start_on(port: u16) -> Result<(Server, u16), String> {
        let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("could not listen on port {port}: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        // not blocking, so that the thread can see it has been told to stop
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (calls_in, calls) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    // each in a thread of its own: one that is slow to send, or
                    // being waited on to close, does not hold up the next
                    Ok((stream, _)) => {
                        let calls = calls_in.clone();
                        std::thread::spawn(move || serve_one(stream, &calls));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(25)),
                }
            }
        });
        Ok((Server { calls, stop, waiting: Vec::new(), ripe: 0 }, port))
    }

    /// Carry out the tool calls that have arrived, on the workspace. Called
    /// once a frame. Returns the names of the tools called, to be shown.
    pub fn serve(&mut self, state: &mut State, now_ms: impl Fn() -> f64 + Copy) -> Vec<String> {
        let mut called = Vec::new();
        self.ripe = self.waiting.len();
        while let Ok(call) = self.calls.try_recv() {
            if call.name == "take_screenshot" {
                let width = call.arguments.get("max_width").and_then(Value::as_u64).map_or(USUAL_WIDTH, |w| (w as u32).clamp(64, WIDEST));
                let panel = call.arguments.get("panel").and_then(Value::as_bool).unwrap_or(true);
                self.waiting.push((width, panel, call.answer));
            } else {
                let answer = apply(state, &call.name, &call.arguments, now_ms).map(Answer::Text);
                // (the asker may have given up waiting: nothing to do about that)
                let _ = call.answer.send(answer);
            }
            called.push(call.name);
        }
        called
    }

    /// Whether anyone is waiting for a picture of the window: with the
    /// panel in it, or of the view before the panel is drawn over it.
    pub fn wants_picture(&self, panel: bool) -> bool {
        self.waiting[..self.ripe].iter().any(|w| w.1 == panel)
    }

    /// Give those waiting for it the picture of the window as it was just
    /// drawn, with the panel or before it, as rows of pixels from the top:
    /// each one no wider than it asked for.
    pub fn give_picture(&mut self, panel: bool, width: u32, height: u32, pixels: &[[u8; 4]]) {
        let mut late = self.waiting.split_off(self.ripe);
        let (ready, mut still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting).into_iter().partition(|w| w.1 == panel);
        self.ripe = still.len();
        still.append(&mut late);
        self.waiting = still;
        for (widest, _, answer) in ready {
            let (w, h, smaller) = shrunk(width, height, pixels, widest);
            let _ = answer.send(crate::send::png_of(w, h, &smaller).map(Answer::Picture));
        }
    }
}

/// A picture made no wider than `widest` by averaging squares of pixels.
/// By whole numbers of pixels: a picture of small sharp text and thin lines
/// stays legible when each new pixel is the average of a square of old
/// ones, and is smeared by a resize to an arbitrary width.
fn shrunk(width: u32, height: u32, pixels: &[[u8; 4]], widest: u32) -> (u32, u32, Vec<[u8; 4]>) {
    let by = width.div_ceil(widest.max(1)).max(1) as usize;
    let (w, h) = (width as usize / by, height as usize / by);
    if by == 1 || w == 0 || h == 0 || pixels.len() != width as usize * height as usize {
        return (width, height, pixels.to_vec());
    }
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 4];
            for dy in 0..by {
                for dx in 0..by {
                    let p = pixels[(y * by + dy) * width as usize + x * by + dx];
                    for k in 0..4 {
                        sum[k] += p[k] as u32;
                    }
                }
            }
            out.push(sum.map(|v| (v / (by * by) as u32) as u8));
        }
    }
    (w as u32, h as u32, out)
}

/// Bytes as base64 text, which is how the protocol carries a picture.
/// https://datatracker.ietf.org/doc/html/rfc4648#section-4
fn base64(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for three in bytes.chunks(3) {
        let n = (three[0] as u32) << 16 | (*three.get(1).unwrap_or(&0) as u32) << 8 | *three.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            // a group short of three bytes is padded with `=`
            out.push(if k <= three.len() { LETTERS[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

impl Server {
    #[cfg(test)]
    fn waiting_for_a_picture(&self) -> usize {
        self.waiting.len()
    }
}

/// The address to give Claude.
pub fn address() -> String {
    format!("http://127.0.0.1:{PORT}/mcp")
}

// ---------------------------------------------------------------- HTTP

/// The parts of a request's head that matter here.
#[derive(Debug, PartialEq)]
struct Head {
    method: String,
    path: String,
    host: Option<String>,
    origin: Option<String>,
    version: Option<String>,
    length: usize,
}

/// Read a request's head from its text (everything before the blank line).
fn parse_head(text: &str) -> Result<Head, &'static str> {
    let mut lines = text.split("\r\n");
    let mut first = lines.next().ok_or("empty request")?.split(' ');
    let (Some(method), Some(path), Some(_)) = (first.next(), first.next(), first.next()) else { return Err("malformed request line") };
    let mut head = Head { method: method.to_string(), path: path.to_string(), host: None, origin: None, version: None, length: 0 };
    for line in lines.filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("malformed header")?;
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => head.host = Some(value.to_string()),
            "origin" => head.origin = Some(value.to_string()),
            "mcp-protocol-version" => head.version = Some(value.to_string()),
            "content-length" => head.length = value.parse().map_err(|_| "malformed Content-Length")?,
            // a body sent in pieces has no length given in advance: not read
            "transfer-encoding" => return Err("Transfer-Encoding is not supported; send Content-Length"),
            _ => {}
        }
    }
    Ok(head)
}

/// Whether a name with an optional port is this computer's.
fn is_local(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, port)| if port.chars().all(|c| c.is_ascii_digit()) { name } else { host });
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

/// Why a request is refused before it is read any further, if it is: as an
/// HTTP status and a word of explanation.
fn refusal(head: &Head) -> Option<(u16, &'static str)> {
    if !head.host.as_deref().is_some_and(is_local) {
        return Some((403, "this server answers only when addressed as this computer"));
    }
    // a browser says which page a request came from; anything else sends none
    if let Some(origin) = &head.origin {
        let from = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")).unwrap_or(origin);
        if !is_local(from) {
            return Some((403, "requests from web pages elsewhere are refused"));
        }
    }
    if head.path != "/mcp" {
        return Some((404, "the MCP endpoint is /mcp"));
    }
    if head.method != "POST" {
        // (the protocol's answer to a GET where no stream of events is offered)
        return Some((405, "only POST is offered"));
    }
    if head.version.as_deref().is_some_and(|v| !VERSIONS.contains(&v)) {
        return Some((400, "unsupported MCP-Protocol-Version"));
    }
    if head.length > MAX_BODY {
        return Some((413, "the request is too large"));
    }
    None
}

fn respond(stream: &mut TcpStream, status: u16, body: &str, json: bool) {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        _ => "Error",
    };
    let kind = if json { "application/json" } else { "text/plain; charset=utf-8" };
    let allow = if status == 405 { "Allow: POST\r\n" } else { "" };
    // The body is sent "chunked": its length in hexadecimal on a line of
    // its own, the body, then a chunk of no length to end it.
    // https://www.rfc-editor.org/rfc/rfc9112#section-7.1
    // The plainer way is a Content-Length header. On the machine this was
    // written on, something that reads local web traffic on its way
    // through (a virus scanner's web shield, most likely) took that header
    // out of every answer, the standard library's own test server's
    // included, and put "transfer-encoding: chunked" in its place over a
    // body it had not chunked: an answer no client can read. An answer that
    // really is chunked comes through untouched, and every HTTP/1.1 client
    // reads one.
    let whole = if body.is_empty() {
        format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nTransfer-Encoding: chunked\r\n{allow}Connection: close\r\n\r\n0\r\n\r\n")
    } else {
        format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nTransfer-Encoding: chunked\r\n{allow}Connection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n", body.len())
    };
    let _ = stream.write_all(whole.as_bytes());
    let _ = stream.flush();
    // Then the other end is left to close first, for a moment. The end of
    // a chunked answer is in the answer, so a client has no need to wait
    // for this end. Closed from here the moment the last byte was handed
    // over, a long answer (a picture) arrived cut short about one time in
    // two on the machine this was written on, behind a virus scanner and a
    // VPN client that both pass local traffic through themselves: the
    // connection was reset with the tail of the answer still on its way.
    let _ = stream.set_read_timeout(Some(LINGER));
    let mut rest = [0u8; 256];
    while matches!(stream.read(&mut rest), Ok(n) if n > 0) {}
}

/// Read one request from a connection, answer it, and close.
fn serve_one(mut stream: TcpStream, calls: &Sender<Call>) {
    // (a connection taken from a listener that does not block may not block either)
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(SOCKET_WAIT));
    let _ = stream.set_write_timeout(Some(SOCKET_WAIT));
    let mut bytes: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        if bytes.len() > MAX_HEAD {
            return respond(&mut stream, 400, "the request's head is too large", false);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
        }
    };
    let head = match std::str::from_utf8(&bytes[..end]).map_err(|_| "the request's head is not text").and_then(parse_head) {
        Ok(head) => head,
        Err(why) => return respond(&mut stream, 400, why, false),
    };
    if let Some((status, why)) = refusal(&head) {
        return respond(&mut stream, status, why, false);
    }
    let mut body = bytes.split_off(end + 4);
    while body.len() < head.length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }
    body.truncate(head.length);
    let Ok(text) = std::str::from_utf8(&body) else { return respond(&mut stream, 400, "the request's body is not text", false) };
    // a tool call is handed to the frame loop and waited for
    let call = |name: &str, arguments: &Value| -> Answered {
        let (answer, answered) = channel();
        calls.send(Call { name: name.to_string(), arguments: arguments.clone(), answer }).map_err(|_| "the editor is closing".to_string())?;
        answered.recv_timeout(CALL_WAIT).map_err(|_| "the editor did not answer in time".to_string())?
    };
    match answer(text, &call) {
        Some(reply) => respond(&mut stream, 200, &reply.to_string(), true),
        None => respond(&mut stream, 202, "", false),
    }
}

// ---------------------------------------------------------------- the protocol

/// Answer one JSON-RPC message: `None` for one that wants no answer (a
/// notification, or an answer to something this never asked).
fn answer(body: &str, call: &dyn Fn(&str, &Value) -> Answered) -> Option<Value> {
    let failed = |id: Value, code: i32, message: String| Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
    let Ok(message) = serde_json::from_str::<Value>(body) else { return failed(Value::Null, -32700, "the request is not JSON".into()) };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // several messages at once went out of the protocol in 2025-06-18
        return if message.is_object() { None } else { failed(Value::Null, -32600, "one message to a request".into()) };
    };
    let id = message.get("id").filter(|id| !id.is_null()).cloned()?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            // the version asked for if this speaks it, or else this one's newest
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = VERSIONS.iter().find(|v| **v == asked).unwrap_or(&VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "brickloft", "title": "Brickloft editor", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Tools for the Brickloft editor, which fits curved forms with Brickadia microbricks. Lengths are in microbricks (a fifth of a stud) and may end in a half. Call get_workspace first: forms are named by their place in its list, which every tool that changes the workspace returns afresh. A workspace is never empty: the editor starts with one cube. To build something of your own, change that cube into your first form with update_form, and do not leave it standing beside what you add. The player is watching the editor and sees each change as it is made."
            })
        }
        "ping" => json!({}),
        "tools/list" => serde_json::from_str(TOOLS).unwrap_or_else(|_| json!({ "tools": [] })),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else { return failed(id, -32602, "tools/call needs a name".into()) };
            if !tool_names().iter().any(|t| t == name) {
                return failed(id, -32602, format!("Unknown tool: {name}"));
            }
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call(name, &arguments) {
                Ok(Answer::Text(text)) => json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
                Ok(Answer::Picture(png)) => json!({ "content": [{ "type": "image", "data": base64(&png), "mimeType": "image/png" }], "isError": false }),
                Err(why) => json!({ "content": [{ "type": "text", "text": why }], "isError": true }),
            }
        }
        other => return failed(id, -32601, format!("Method not found: {other}")),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn tool_names() -> Vec<String> {
    let listed: Value = serde_json::from_str(TOOLS).unwrap_or(Value::Null);
    listed["tools"].as_array().map(|tools| tools.iter().filter_map(|t| t["name"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

// ---------------------------------------------------------------- the tools

/// A setting's value as the text a saved form has it in: a number or a word
/// as it is, a list of numbers joined with commas, a list of lists (points)
/// with semicolons between them.
fn setting_text(value: &Value) -> Result<String, String> {
    let plain = |v: &Value| match v {
        Value::String(s) if s.len() <= 4096 && !s.contains('\n') && !s.contains('\r') => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err("a setting is a number, true or false, a word, or a list of them".to_string()),
    };
    match value {
        Value::Array(items) if items.iter().any(Value::is_array) => {
            let rows: Result<Vec<String>, String> = items.iter().map(|row| Ok(row.as_array().ok_or("a list of points holds only points")?.iter().map(plain).collect::<Result<Vec<_>, _>>()?.join(","))).collect();
            Ok(rows?.join("; "))
        }
        Value::Array(items) => Ok(items.iter().map(plain).collect::<Result<Vec<_>, _>>()?.join(",")),
        other => plain(other),
    }
}

/// A grid's angle from turns about x, then y, then z, in degrees, as the
/// four numbers a form keeps it in (a quaternion: x, y, z, w).
/// https://en.wikipedia.org/wiki/Conversion_between_quaternions_and_Euler_angles
fn grid_from_degrees(degrees: [f64; 3]) -> [f64; 4] {
    let [(sx, cx), (sy, cy), (sz, cz)] = degrees.map(|d| (d.to_radians() / 2.0).sin_cos());
    [sx * cy * cz - cx * sy * sz, cx * sy * cz + sx * cy * sz, cx * cy * sz - sx * sy * cz, cx * cy * cz + sx * sy * sz]
}

/// The reverse, for telling Claude a grid's angle in the terms it sets it in.
fn degrees_from_grid([x, y, z, w]: [f64; 4]) -> [f64; 3] {
    let about_x = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let about_y = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0).asin();
    let about_z = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
    [about_x, about_y, about_z].map(|a| (a.to_degrees() * 1000.0).round() / 1000.0)
}

/// A form's settings with `changes` made to them. The changes are written
/// after the form's own settings in the text a saved form is kept in, where
/// a later line wins, and the whole is read by the parser that reads a
/// save: there is one reader of settings, and it has its checks already.
fn changed(spec: &Spec, changes: &Map<String, Value>) -> Result<Spec, String> {
    if changes.len() > 64 {
        return Err("too many settings at once".into());
    }
    let mut text = spec_text(spec);
    for (key, value) in changes {
        if !key.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Err(format!("`{key}` is not a setting"));
        }
        let value = match key.as_str() {
            "grid_degrees" => {
                let turns: Vec<f64> = value.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                let turns = <[f64; 3]>::try_from(turns).map_err(|_| "grid_degrees is three numbers: the turn about x, y and z")?;
                text += &format!("grid = {}\n", grid_from_degrees(turns).map(|v| v.to_string()).join(","));
                continue;
            }
            _ => setting_text(value).map_err(|e| format!("{key}: {e}"))?,
        };
        text += &format!("{key} = {value}\n");
    }
    let spec = parse_spec(&text)?;
    if spec.points.len() > MAX_POINTS || spec.pulls.len() > crate::state::most_pulls(&spec) {
        return Err(format!("a form takes at most {MAX_POINTS} points and {} added points", crate::state::most_pulls(&spec)));
    }
    Ok(spec)
}

const MODES: [(Mode, &str); 4] = [(Mode::Solid, "one"), (Mode::Blend, "blend"), (Mode::Stripes, "stripes"), (Mode::Pattern, "pattern")];

/// A form's colouring with `changes` made to it.
fn recoloured(paint: &Paint, changes: &Value) -> Result<Paint, String> {
    let mut paint = paint.clone();
    let changes = changes.as_object().ok_or("colour is an object")?;
    for (key, value) in changes {
        match key.as_str() {
            "mode" => paint.mode = MODES.iter().find(|m| Some(m.1) == value.as_str()).ok_or("mode is one, blend, stripes or pattern")?.0,
            "colours" => {
                let colours = value.as_array().filter(|c| (1..=crate::paint::MOST_COLOURS).contains(&c.len())).ok_or("colours is a list of one to 32 colours")?;
                paint.colours = colours
                    .iter()
                    .map(|c| {
                        let hex = c.as_str().and_then(|s| s.strip_prefix('#')).filter(|s| s.len() == 6 && s.is_ascii())?;
                        Some([0, 2, 4].map(|at| u8::from_str_radix(&hex[at..at + 2], 16).ok()))
                    })
                    .map(|c| match c {
                        Some([Some(r), Some(g), Some(b)]) => Ok([r, g, b]),
                        _ => Err("a colour is written #rrggbb".to_string()),
                    })
                    .collect::<Result<_, _>>()?;
            }
            "runs_along" => paint.axis = ["x", "y", "z"].iter().position(|a| Some(*a) == value.as_str()).ok_or("runs_along is x, y or z")?,
            "stripe_width" => paint.width = value.as_i64().filter(|w| (1..=1000).contains(w)).ok_or("stripe_width is a whole number from 1 to 1000")? as i32,
            other => return Err(format!("`{other}` is not part of a colour")),
        }
    }
    Ok(paint)
}

/// Where a form stands, from microbricks (which may end in a half) to the
/// game's units, of which there are two to a microbrick.
fn placed(value: &Value) -> Result<[i32; 3], String> {
    let at: Vec<f64> = value.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let at = <[f64; 3]>::try_from(at).map_err(|_| "place is three numbers: x, y and z in microbricks")?;
    if at.iter().any(|v| !v.is_finite() || v.abs() > brickloft_core::spec::MAX_SPAN) {
        return Err(format!("place is within {} microbricks of the middle", brickloft_core::spec::MAX_SPAN));
    }
    Ok(at.map(|v| (v * 2.0).round() as i32))
}

/// The most legs one flight of the camera may have, the longest one leg
/// may take in seconds, and the furthest from the middle of the world the
/// camera may be sent, in microbricks.
const MAX_LEGS: usize = 32;
const MAX_SECONDS: f64 = 30.0;
const FURTHEST: f64 = 5000.0;

/// Numbers to three places, as Claude is told them.
fn rounded(p: DVec3) -> [f64; 3] {
    p.to_array().map(|v| (v * 1000.0).round() / 1000.0)
}

/// Three numbers out of a tool's arguments, each within `limit` of zero.
fn three(value: Option<&Value>, name: &str, limit: f64) -> Result<[f64; 3], String> {
    let given: Vec<f64> = value.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let given = <[f64; 3]>::try_from(given).map_err(|_| format!("{name} is three numbers: x, y and z in microbricks"))?;
    if given.iter().any(|v| !v.is_finite() || v.abs() > limit) {
        return Err(format!("{name} is within {limit} microbricks of the middle of the world"));
    }
    Ok(given)
}

/// The box a form's bricks fill: its low and high corners within its grid,
/// in microbricks, and its eight corners' extent in the world, which for a
/// grid at an angle is not the same box.
struct Extent {
    low: DVec3,
    high: DVec3,
    world_low: DVec3,
    world_high: DVec3,
}

impl Extent {
    fn centre_in_world(&self, spec: &Spec) -> DVec3 {
        out_of_grid(spec, (self.low + self.high) / 2.0)
    }
}

/// Each form's extent, the one being edited first; `None` for one with no bricks.
fn extents(state: &State) -> Vec<Option<Extent>> {
    let forms = state.forms();
    let bounds = std::iter::once(state.bounds).chain(state.others.iter().map(|o| o.bounds));
    forms
        .iter()
        .zip(bounds)
        .map(|((spec, _, place), bounds)| {
            let (lo, hi) = bounds?;
            // where it stands is in units, of which there are two to a microbrick
            let stands = DVec3::from_array(place.map(|u| u as f64 / 2.0));
            let (low, high) = (DVec3::from_array(lo.map(|v| v as f64)) + stands, DVec3::from_array(hi.map(|v| v as f64)) + stands);
            let corners = (0..8).map(|c| out_of_grid(spec, DVec3::new(if c & 1 == 0 { low.x } else { high.x }, if c & 2 == 0 { low.y } else { high.y }, if c & 4 == 0 { low.z } else { high.z })));
            let (world_low, world_high) = corners.fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)));
            Some(Extent { low, high, world_low, world_high })
        })
        .collect()
}

/// The workspace as Claude is told it: every form, the one being edited first.
fn workspace(state: &State) -> Value {
    let forms = state.forms();
    let built = std::iter::once(&state.built).chain(state.others.iter().map(|o| &o.built));
    let extents = extents(state);
    let listed: Vec<Value> = forms
        .iter()
        .zip(built)
        .enumerate()
        .map(|(index, ((spec, paint, place), built))| {
            let settings: Map<String, Value> = spec_text(spec).lines().filter_map(|line| line.split_once(" = ").or_else(|| line.split_once(" ="))).map(|(k, v)| (k.to_string(), Value::String(v.trim().to_string()))).collect();
            let problem = if index == 0 { state.problem.clone() } else if built.bricks.is_empty() { brickloft_core::spec::build(spec).err() } else { None };
            json!({
                "index": index,
                "being_edited": index == 0,
                "settings": settings,
                "grid_degrees": degrees_from_grid(spec.grid),
                "colour": {
                    "mode": MODES.iter().find(|m| m.0 == paint.mode).map_or("one", |m| m.1),
                    "colours": paint.colours.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect::<Vec<_>>(),
                    "runs_along": (["x", "y", "z"][paint.axis.min(2)]),
                    "stripe_width": paint.width,
                },
                "place": place.map(|u| u as f64 / 2.0),
                "points": spec.points.iter().map(|p| rounded(*p)).collect::<Vec<_>>(),
                "added_points": spec.pulls.iter().map(|(at, by)| json!({ "at": rounded(*at), "pulled_by": rounded(*by) })).collect::<Vec<_>>(),
                "box_in_grid": extents[index].as_ref().map(|e| json!({ "low": rounded(e.low), "high": rounded(e.high), "size": rounded(e.high - e.low) })),
                "box_in_world": extents[index].as_ref().map(|e| json!({ "low": rounded(e.world_low), "high": rounded(e.world_high), "centre": rounded(e.centre_in_world(spec)) })),
                "bricks": built.bricks.len(),
                "discrepancy_percent": if built.solid > 0.0 { Value::from((10000.0 * built.error / built.solid).round() / 100.0) } else { Value::Null },
                "not_built_because": problem,
            })
        })
        .collect();
    json!({ "forms": listed })
}

/// Carry out one tool call on the workspace. `Ok` is what to tell Claude,
/// `Err` why it was not done; nothing is changed by a call that fails.
pub fn apply(state: &mut State, name: &str, arguments: &Value, now_ms: impl Fn() -> f64 + Copy) -> Result<String, String> {
    let index = |forms: &[Placed]| -> Result<usize, String> {
        let index = arguments.get("index").and_then(Value::as_u64).ok_or("index is the form's place in the list get_workspace gives")? as usize;
        if index < forms.len() { Ok(index) } else { Err(format!("there is no form {index}: the workspace has {}", forms.len())) }
    };
    let no_settings = Map::new();
    let settings = match arguments.get("settings") {
        None | Some(Value::Null) => &no_settings,
        Some(given) => given.as_object().ok_or("settings is an object of names and values")?,
    };
    let mut forms = state.forms();
    // what asks about the workspace or moves the camera, and changes no form
    match name {
        "get_camera" => {
            let (at, looking) = state.camera_at;
            let looking = DVec3::from_array(looking);
            return Ok(json!({
                "position": rounded(DVec3::from_array(at)),
                "looking_along": rounded(looking),
                "heading_degrees": (looking.y.atan2(looking.x).to_degrees() * 10.0).round() / 10.0,
                "tilt_degrees": (looking.z.clamp(-1.0, 1.0).asin().to_degrees() * 10.0).round() / 10.0,
                "legs_still_to_fly": state.flight.len(),
            })
            .to_string());
        }
        "move_camera" => {
            let path = arguments.get("path").and_then(Value::as_array).filter(|p| (1..=MAX_LEGS).contains(&p.len())).ok_or(format!("path is a list of one to {MAX_LEGS} places"))?;
            let mut legs = Vec::new();
            for leg in path {
                let to = three(leg.get("position"), "position", FURTHEST)?;
                let look_at = three(leg.get("look_at"), "look_at", FURTHEST)?;
                let seconds = match leg.get("seconds") {
                    None | Some(Value::Null) => 0.0,
                    Some(given) => given.as_f64().filter(|s| (0.0..=MAX_SECONDS).contains(s)).ok_or(format!("seconds is from 0 to {MAX_SECONDS}"))?,
                };
                legs.push(Leg { to, look_at, seconds });
            }
            // a new flight takes the place of one still being flown
            state.flight = legs.into();
            let whole: f64 = state.flight.iter().map(|l| l.seconds).sum();
            return Ok(format!("The camera is on its way: {} leg{}, {whole} seconds in all. A screenshot asked for now is taken once it has arrived.", state.flight.len(), if state.flight.len() == 1 { "" } else { "s" }));
        }
        "set_view" => {
            let flag = |name: &str| -> Result<Option<bool>, String> {
                match arguments.get(name) {
                    None | Some(Value::Null) => Ok(None),
                    Some(given) => given.as_bool().map(Some).ok_or(format!("{name} is true or false")),
                }
            };
            let number = |name: &str, low: f64, high: f64| -> Result<Option<f64>, String> {
                match arguments.get(name) {
                    None | Some(Value::Null) => Ok(None),
                    Some(given) => given.as_f64().filter(|v| (low..=high).contains(v)).map(Some).ok_or(format!("{name} is a number from {low} to {high}")),
                }
            };
            // all read before any is set, so that a request with a mistake in it changes nothing
            let (rulers, rings, arrows, dots) = (flag("rulers")?, flag("rings")?, flag("arrows")?, flag("dots")?);
            let floor = number("floor_below", 0.0, 40.0)?;
            state.show_ruler = rulers.unwrap_or(state.show_ruler);
            state.show_rings = rings.unwrap_or(state.show_rings);
            state.show_arrows = arrows.unwrap_or(state.show_arrows);
            state.show_dots = dots.unwrap_or(state.show_dots);
            state.floor_gap = floor.unwrap_or(state.floor_gap);
            state.scene += 1;
            return Ok(json!({ "rulers": state.show_ruler, "rings": state.show_rings, "arrows": state.show_arrows, "dots": state.show_dots, "floor_below": state.floor_gap }).to_string());
        }
        "measure_between" => {
            let which = |name: &str| -> Result<usize, String> {
                let index = arguments.get(name).and_then(Value::as_u64).ok_or(format!("{name} is a form's place in the list get_workspace gives"))? as usize;
                if index < forms.len() { Ok(index) } else { Err(format!("there is no form {index}: the workspace has {}", forms.len())) }
            };
            let (from, to) = (which("from")?, which("to")?);
            let extents = extents(state);
            let (Some(a), Some(b)) = (&extents[from], &extents[to]) else { return Err("one of the two has no bricks to measure".into()) };
            let (ca, cb) = (a.centre_in_world(&forms[from].0), b.centre_in_world(&forms[to].0));
            // how far apart the two boxes are along each axis: less than nothing where they overlap
            let gap = |lo_a: DVec3, hi_a: DVec3, lo_b: DVec3, hi_b: DVec3| rounded((lo_b - hi_a).max(lo_a - hi_b));
            let same_grid = forms[from].0.on_grid == forms[to].0.on_grid;
            return Ok(json!({
                "centre_to_centre_in_world": rounded(cb - ca),
                "distance_between_centres": ((cb - ca).length() * 1000.0).round() / 1000.0,
                "gap_between_boxes_in_world": gap(a.world_low, a.world_high, b.world_low, b.world_high),
                "on_the_same_grid": same_grid,
                "gap_between_boxes_in_their_grid": if same_grid { Value::from(gap(a.low, a.high, b.low, b.high).to_vec()) } else { Value::Null },
            })
            .to_string());
        }
        _ => {}
    }
    match name {
        "get_workspace" => {}
        "add_form" => {
            if forms.len() >= MAX_FORMS {
                return Err(format!("the workspace is full at {MAX_FORMS} forms"));
            }
            if !settings.contains_key("form") {
                return Err("settings needs at least `form`: the kind of form to add".into());
            }
            let mut spec = changed(&Spec::default(), settings)?;
            let tree = (spec.form == brickloft_core::spec::Form::Tree).then_some(spec.tree.kind);
            if let Some(kind) = tree {
                // a tree given no size is the size the editor starts one at, not a form's usual radius: a tree 8 across is a twig
                let (height, spread) = brickloft_core::tree::Tree::usual_size(kind);
                if !settings.contains_key("radius") {
                    spec.radius = spread;
                }
                if !settings.contains_key("rise") {
                    spec.rise = Some(height);
                }
                // (and a hedge, reeds or grass given no line and no length lies three times as long as it reaches to a side)
                if brickloft_core::tree::Tree::lies_along(kind) && !settings.contains_key("stretch") && spec.points.len() < 2 {
                    spec.stretch.x = brickloft_core::tree::HEDGE_LENGTH;
                }
            }
            let furniture = (spec.form == brickloft_core::spec::Form::Furniture).then_some(spec.furniture.kind);
            if let Some(kind) = furniture.filter(|_| !settings.contains_key("radius") && !settings.contains_key("stretch")) {
                // furniture given no size is the size the editor starts its kind at: a form's usual radius each way is a crate
                (spec.radius, spec.stretch) = brickloft_core::furniture::Furniture::usual_form(kind);
            }
            let maze = spec.form == brickloft_core::spec::Form::Maze;
            if maze {
                // (and a maze given no size is the size the editor starts one at: at a form's usual radius its corridors are too narrow to build)
                use brickloft_core::maze::{USUAL_HEIGHT, USUAL_SIZE};
                if !settings.contains_key("radius") {
                    spec.radius = USUAL_SIZE.0;
                }
                if !settings.contains_key("thick") {
                    spec.thick = USUAL_SIZE.1;
                }
                if !settings.contains_key("rise") {
                    spec.rise = Some(USUAL_HEIGHT);
                }
            }
            if spec.points.is_empty() {
                // set down where the editor would set one down
                let form = crate::state::form_index(spec.form);
                let (points, weights) = crate::state::usual_points(form, crate::state::half_height(form, &spec));
                spec.points = points;
                if spec.weights.is_empty() {
                    spec.weights = weights;
                }
            }
            brickloft_core::spec::build(&spec)?;
            let paint = arguments.get("colour").map(|c| recoloured(&Paint::default(), c)).transpose()?;
            let place = arguments.get("place").map(placed).transpose()?;
            state.record();
            // set down beside what is there, and edited; then as asked
            // (a pattern asked for with no colours comes in its own, as it does from the panel: in one colour it would not show at all)
            let own = spec.stones.map(|_| "stepping stones").or(spec.mosaic.map(|laid| brickloft_core::pattern::STYLES[laid.style.min(brickloft_core::pattern::STYLES.len() - 1)].name)).or(tree.map(|kind| brickloft_core::tree::KINDS[kind as usize].1)).or(furniture.map(|_| spec.furniture.colours()));
            // (a maze's colours go by what its walls are)
            let walls = maze.then(|| crate::paint::maze_palettes(crate::paint::maze_texture(&spec))[0].clone());
            state.add_form(spec);
            if let Some(paint) = paint {
                state.paint = paint;
            } else if let Some(colours) = walls {
                state.paint = Paint { mode: Mode::Pattern, colours, ..Paint::default() };
            } else if let Some(style) = own {
                state.paint = Paint { mode: Mode::Pattern, colours: crate::paint::palette(style).to_vec(), ..Paint::default() };
            }
            if let Some(place) = place {
                state.place = place;
            }
        }
        "update_form" => {
            let at = index(&forms)?;
            let (spec, paint, place) = &mut forms[at];
            let was = spec.form;
            let walls_were = crate::paint::maze_texture(spec);
            *spec = changed(spec, settings)?;
            brickloft_core::spec::build(spec)?;
            let maze = spec.form == brickloft_core::spec::Form::Maze;
            if let Some(colour) = arguments.get("colour") {
                *paint = recoloured(paint, colour)?;
            } else if maze && (was != spec.form || walls_were != crate::paint::maze_texture(spec)) {
                // (a form made into a maze, or a maze given other walls, comes in those walls' colours)
                *paint = Paint { mode: Mode::Pattern, colours: crate::paint::maze_palettes(crate::paint::maze_texture(spec))[0].clone(), ..Paint::default() };
            } else if spec.form == brickloft_core::spec::Form::Tree && was != spec.form {
                // (a form made into a tree comes in the tree's colours, as one added does)
                *paint = Paint { mode: Mode::Pattern, colours: crate::paint::palette(brickloft_core::tree::KINDS[spec.tree.kind as usize].1).to_vec(), ..Paint::default() };
            } else if spec.form == brickloft_core::spec::Form::Furniture && was != spec.form {
                // (and one made into furniture in the furniture's: its wood, and its books')
                *paint = Paint { mode: Mode::Pattern, colours: crate::paint::palette(spec.furniture.colours()).to_vec(), ..Paint::default() };
            }
            if let Some(to) = arguments.get("place") {
                *place = placed(to)?;
            }
            state.open(forms);
        }
        // One dot of a form, as the mouse moves one: a point the form is
        // drawn through, or a point added to pull it out of shape. The
        // whole list could be sent through `update_form` instead, which
        // means reading it, changing one entry and writing every entry
        // back in the text a save keeps them in: arithmetic and copying
        // that a caller gets wrong, for what is one drag in the editor.
        "move_point" => {
            let at = index(&forms)?;
            let which = arguments.get("point").and_then(Value::as_u64).ok_or("point is the number of one of the form's points, counted from 0")? as usize;
            let to = three(arguments.get("to"), "to", brickloft_core::spec::MAX_SPAN)?;
            let spec = &mut forms[at].0;
            let count = spec.points.len();
            *spec.points.get_mut(which).ok_or(format!("the form has {count} points, numbered from 0"))? = DVec3::from_array(to);
            brickloft_core::spec::build(spec)?;
            state.open(forms);
        }
        "add_pull_point" => {
            let at = index(&forms)?;
            let hold = three(arguments.get("at"), "at", brickloft_core::spec::MAX_SPAN)?;
            let by = three(arguments.get("pulled_by"), "pulled_by", brickloft_core::spec::MAX_SPAN)?;
            let spec = &mut forms[at].0;
            if spec.pulls.len() >= crate::state::most_pulls(spec) {
                return Err(format!("a form takes at most {} added points", crate::state::most_pulls(spec)));
            }
            spec.pulls.push((DVec3::from_array(hold), DVec3::from_array(by)));
            if let Some(reach) = arguments.get("reach").filter(|r| !r.is_null()) {
                spec.pull_reach = reach.as_f64().filter(|r| (1.0..=brickloft_core::spec::MAX_SPAN).contains(r)).ok_or("reach is a number of microbricks, 1 or more")?;
            }
            brickloft_core::spec::build(spec)?;
            state.open(forms);
        }
        "move_pull_point" | "remove_pull_point" => {
            let at = index(&forms)?;
            let which = arguments.get("added_point").and_then(Value::as_u64).ok_or("added_point is the number of one of the form's added points, counted from 0")? as usize;
            let spec = &mut forms[at].0;
            let count = spec.pulls.len();
            if which >= count {
                return Err(format!("the form has {count} added points, numbered from 0"));
            }
            if name == "remove_pull_point" {
                spec.pulls.remove(which);
            } else {
                spec.pulls[which].1 = DVec3::from_array(three(arguments.get("pulled_by"), "pulled_by", brickloft_core::spec::MAX_SPAN)?);
            }
            brickloft_core::spec::build(spec)?;
            state.open(forms);
        }
        "line_up_arch" => {
            let at = index(&forms)?;
            if at > 0 {
                state.activate(at - 1);
            }
            if !state.askew() {
                return Err("that form is not an arch at an angle to its grid: its two feet must differ along both x and y".into());
            }
            state.record();
            if !state.line_up() {
                return Err("an arch that has been given quarter turns cannot be lined up; set `turned` back to 16 first".into());
            }
        }
        "remove_form" => {
            let at = index(&forms)?;
            if forms.len() == 1 {
                return Err("the last form stays: a workspace is never empty. Change it instead.".into());
            }
            forms.remove(at);
            state.open(forms);
        }
        "select_form" => {
            let at = index(&forms)?;
            if at > 0 {
                state.activate(at - 1);
            }
        }
        other => return Err(format!("Unknown tool: {other}")),
    }
    state.scene += 1;
    state.refresh(now_ms);
    // A form just added, or just chosen to be edited, is brought to the
    // middle of the view: the camera flies to face it from where it now
    // looks, far enough off to take it all in. A form is set down beside
    // the others, which is often out of sight of a camera left where it
    // was, and a change the player cannot see might as well not have been
    // made.
    if matches!(name, "add_form" | "select_form") {
        if let Some(Some(extent)) = extents(state).first() {
            let (centre, size) = (extent.centre_in_world(&state.full_spec()), (extent.world_high - extent.world_low).max_element());
            let looking = DVec3::from_array(state.camera_at.1);
            let back = centre - looking * (1.8 * size + 12.0);
            state.flight = [Leg { to: back.to_array(), look_at: centre.to_array(), seconds: 0.6 }].into();
        }
    }
    Ok(workspace(state).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_tools(_: &str, _: &Value) -> Answered {
        Err("no tool was expected to be called".into())
    }

    #[test]
    fn the_handshake_and_the_list_of_tools_are_answered() {
        let hello = answer(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#, &no_tools).unwrap();
        assert_eq!(hello["result"]["protocolVersion"], "2025-03-26", "the version asked for, since this speaks it");
        assert!(hello["result"]["capabilities"]["tools"].is_object());
        let newer = answer(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}"#, &no_tools).unwrap();
        assert_eq!(newer["result"]["protocolVersion"], VERSIONS[0]);
        // a notification wants no answer
        assert!(answer(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &no_tools).is_none());
        let listed = answer(r#"{"jsonrpc":"2.0","id":"a","method":"tools/list"}"#, &no_tools).unwrap();
        let tools = listed["result"]["tools"].as_array().expect("mcp_tools.json is not a list of tools");
        assert_eq!(listed["id"], "a");
        for tool in tools {
            assert!(tool["name"].is_string() && tool["description"].is_string() && tool["inputSchema"]["type"] == "object", "a tool is missing its name, description or schema: {tool}");
        }
        // every tool listed is one `apply` knows, and the other way about
        let mut state = State::new();
        // (but for the picture, which the frame loop gives once it has drawn)
        for name in tool_names().into_iter().filter(|name| name != "take_screenshot") {
            let said = apply(&mut state, &name, &json!({}), || 0.0);
            assert!(!said.is_err_and(|e| e.starts_with("Unknown tool")), "{name} is listed and not carried out");
        }
        assert_eq!(tool_names().len(), 15);
    }

    #[test]
    fn what_is_not_understood_is_refused_in_the_protocols_own_terms() {
        assert_eq!(answer("not json", &no_tools).unwrap()["error"]["code"], -32700);
        assert_eq!(answer(r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#, &no_tools).unwrap()["error"]["code"], -32601);
        assert_eq!(answer(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"format_disk"}}"#, &no_tools).unwrap()["error"]["code"], -32602);
        assert_eq!(answer(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#, &no_tools).unwrap()["error"]["code"], -32600);
        // a tool that fails says so in its result, as the protocol has it
        let failed = answer(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_workspace"}}"#, &no_tools).unwrap();
        assert_eq!(failed["result"]["isError"], true);
    }

    #[test]
    fn a_request_is_taken_only_from_this_computer_and_only_as_a_post_to_the_endpoint() {
        let head = |text: &str| parse_head(text).unwrap();
        let good = head("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:7878\r\nContent-Length: 12\r\nMCP-Protocol-Version: 2025-06-18");
        assert_eq!((good.length, refusal(&good)), (12, None));
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1\r\nHost: localhost:7878")), None);
        // a web page elsewhere that has had its name pointed at this computer
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1\r\nHost: evil.example:7878")).unwrap().0, 403);
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:7878\r\nOrigin: https://evil.example")).unwrap().0, 403);
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:7878\r\nOrigin: http://localhost:3000")), None);
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1")).unwrap().0, 403, "no Host at all");
        assert_eq!(refusal(&head("GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:7878")).unwrap().0, 405);
        assert_eq!(refusal(&head("POST /other HTTP/1.1\r\nHost: 127.0.0.1:7878")).unwrap().0, 404);
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:7878\r\nMCP-Protocol-Version: 1999-01-01")).unwrap().0, 400);
        assert_eq!(refusal(&head("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:7878\r\nContent-Length: 99999999")).unwrap().0, 413);
        assert!(parse_head("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nTransfer-Encoding: chunked").is_err());
        assert!(parse_head("nonsense").is_err());
    }

    #[test]
    fn forms_are_added_changed_coloured_turned_and_removed() {
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "sphere", "points": [[0.5, 0.5, 10.5]], "radius": 9.5, "fill": true }, "colour": { "mode": "blend", "colours": ["#ff0000", "#0000ff"], "runs_along": "x" }, "place": [20, 0, 3.5] }), || 0.0).unwrap());
        let forms = added["forms"].as_array().unwrap();
        assert_eq!(forms.len(), 2);
        // the new form is the one being edited, and is as asked
        assert_eq!((forms[0]["settings"]["form"].as_str(), forms[0]["settings"]["radius"].as_str()), (Some("sphere"), Some("9.5")));
        assert_eq!(forms[0]["place"], json!([20.0, 0.0, 3.5]));
        assert_eq!(forms[0]["colour"]["colours"], json!(["#ff0000", "#0000ff"]));
        assert!(forms[0]["bricks"].as_u64().unwrap() > 50 && forms[0]["discrepancy_percent"].as_f64().unwrap() < 10.0);
        assert_eq!(state.spec.radius, 9.5);

        // the other form (the arch the editor starts with), turned on its grid
        let turned = said(apply(&mut state, "update_form", &json!({ "index": 1, "settings": { "thick": 3, "grid_degrees": [0, 0, 90] } }), || 0.0).unwrap());
        assert_eq!(turned["forms"][1]["settings"]["thick"], "3");
        assert_eq!(turned["forms"][1]["grid_degrees"], json!([0.0, 0.0, 90.0]));
        assert_eq!(turned["forms"][0]["settings"]["radius"], "9.5", "the form not named was changed");
        // and it is one step to undo
        assert!(state.undo());
        assert_eq!(state.others[0].spec.grid, [0.0, 0.0, 0.0, 1.0]);

        let fewer = said(apply(&mut state, "remove_form", &json!({ "index": 1 }), || 0.0).unwrap());
        assert_eq!(fewer["forms"].as_array().unwrap().len(), 1);
        assert!(apply(&mut state, "remove_form", &json!({ "index": 0 }), || 0.0).is_err(), "the last form was removed");
    }

    #[test]
    fn what_cannot_be_built_is_refused_and_changes_nothing() {
        let mut state = State::new();
        let before = state.forms();
        for bad in [
            json!({ "index": 0, "settings": { "radius": "wide" } }),
            json!({ "index": 0, "settings": { "radius": -4 } }),
            json!({ "index": 0, "settings": { "thick": 1e9 } }),
            json!({ "index": 0, "settings": { "form": "teapot" } }),
            json!({ "index": 0, "settings": { "radius\nform": 3 } }),
            json!({ "index": 0, "settings": { "points": "1,2,3\nradius = 5" } }),
            json!({ "index": 7, "settings": { "radius": 5 } }),
            json!({ "index": 0, "colour": { "colours": ["red"] } }),
            json!({ "index": 0, "colour": { "colours": [] } }),
            json!({ "index": 0, "place": [1e12, 0, 0] }),
            json!({ "index": 0, "settings": { "grid_degrees": [1, 2] } }),
            json!({ "settings": { "radius": 5 } }),
        ] {
            assert!(apply(&mut state, "update_form", &bad, || 0.0).is_err(), "{bad} was accepted");
            assert_eq!(state.forms(), before, "{bad} changed the workspace");
        }
        assert!(apply(&mut state, "add_form", &json!({ "settings": { "radius": 5 } }), || 0.0).is_err(), "a form of no kind was added");
        assert!(apply(&mut state, "add_form", &json!({ "settings": { "form": "sphere", "points": vec![[0, 0, 0]; 100] } }), || 0.0).is_err());
    }

    /// Send one request to the server as a client would, and read the answer.
    fn post(port: u16, host: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "POST /mcp HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    #[test]
    fn a_tool_called_over_the_socket_changes_the_workspace_on_the_frame_loop() {
        let (mut server, port) = Server::start_on(0).unwrap();
        let host = format!("127.0.0.1:{port}");
        // answered by the listener alone
        let hello = post(port, &host, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#);
        assert!(hello.starts_with("HTTP/1.1 200 OK") && hello.contains("application/json") && hello.contains(r#""name":"brickloft""#), "{hello}");
        // the body is one chunk: its length in hexadecimal, itself, and an empty chunk to end
        let (_, body) = hello.split_once("\r\n\r\n").unwrap();
        let (length, rest) = body.split_once("\r\n").unwrap();
        let length = usize::from_str_radix(length, 16).expect("the chunk has no length");
        assert_eq!(&rest[length..], "\r\n0\r\n\r\n", "{hello}");
        assert!(serde_json::from_str::<Value>(&rest[..length]).is_ok(), "{hello}");
        assert!(post(port, &host, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).starts_with("HTTP/1.1 202"));
        assert!(post(port, "evil.example", r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).starts_with("HTTP/1.1 403"));
        // a tool call waits for the frame loop, which is this thread
        let asking = std::thread::spawn(move || post(port, &host, r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_form","arguments":{"settings":{"form":"pyramid","radius":8}}}}"#));
        let mut state = State::new();
        let mut called = Vec::new();
        while called.is_empty() {
            called = server.serve(&mut state, || 0.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        let answer = asking.join().unwrap();
        assert_eq!(called, ["add_form"]);
        assert!(answer.starts_with("HTTP/1.1 200 OK") && answer.contains(r#""isError":false"#), "{answer}");
        assert_eq!((state.others.len(), state.spec.form, state.spec.radius), (1, brickloft_core::spec::Form::Pyramid, 8.0));
        // and the camera is sent to look at what was added, from the way it was looking
        let centre = extents(&state)[0].as_ref().unwrap().centre_in_world(&state.full_spec());
        assert_eq!((state.flight.len(), state.flight[0].look_at), (1, centre.to_array()));
        assert!(state.flight[0].to[0] < centre.x - 12.0, "the camera was not sent back along its line of sight: {:?}", state.flight[0]);
        state.flight.clear();
        // a picture is asked for, waited for until a frame has been drawn, and given
        let host = format!("127.0.0.1:{port}");
        let asking = std::thread::spawn(move || post(port, &host, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"take_screenshot","arguments":{"max_width":64}}}"#));
        while server.waiting_for_a_picture() == 0 {
            server.serve(&mut state, || 0.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!server.wants_picture(true), "a picture was due in the frame it was asked for");
        server.serve(&mut state, || 0.0);
        assert!(server.wants_picture(true) && !server.wants_picture(false));
        // the view before the panel is drawn is not what was asked for
        server.give_picture(false, 1, 1, &[[0, 0, 0, 255]]);
        assert!(server.wants_picture(true));
        // a window 200 by 100, red on the left and blue on the right
        let window: Vec<[u8; 4]> = (0..200 * 100).map(|i| if i % 200 < 100 { [255, 0, 0, 255] } else { [0, 0, 255, 255] }).collect();
        server.give_picture(true, 200, 100, &window);
        let answer = asking.join().unwrap();
        assert!(answer.contains(r#""type":"image""#) && answer.contains(r#""mimeType":"image/png""#), "{answer}");
        // (a PNG file begins with these bytes, which in base64 are these letters)
        assert!(answer.contains(r#""data":"iVBORw0KGgo"#), "{answer}");
        assert!(!server.wants_picture(true));
        // once it is dropped nothing listens there
        drop(server);
        std::thread::sleep(Duration::from_millis(120));
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err(), "still listening after being switched off");
    }

    #[test]
    fn a_picture_is_shrunk_by_whole_pixels_and_written_as_base64() {
        // the examples of the standard itself
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(plain.as_bytes()), coded);
        }
        let window: Vec<[u8; 4]> = (0..200 * 100).map(|i| if i % 200 < 100 { [255, 0, 0, 255] } else { [0, 0, 255, 255] }).collect();
        // 200 wide into at most 64: every fourth pixel, 50 by 25
        let (w, h, small) = shrunk(200, 100, &window, 64);
        assert_eq!((w, h, small.len()), (50, 25, 50 * 25));
        assert_eq!((small[0], small[49]), ([255, 0, 0, 255], [0, 0, 255, 255]));
        // already narrow enough: as it was
        assert_eq!(shrunk(200, 100, &window, 1280), (200, 100, window.clone()));
        let png = crate::send::png_of(w, h, &small).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn a_form_added_with_no_points_stands_where_the_editor_would_put_it() {
        let mut state = State::new();
        for form in ["arch pointed", "circle", "spline", "bezier", "sphere", "revolve", "pyramid", "cube"] {
            let said = apply(&mut state, "add_form", &json!({ "settings": { "form": form } }), || 0.0);
            assert!(said.is_ok(), "{form}: {said:?}");
            assert!(!state.built.bricks.is_empty(), "{form} was added with no bricks");
        }
    }

    #[test]
    fn a_maze_is_added_and_changed_and_a_hostile_one_refused() {
        use brickloft_core::maze::{Kind, Maze, USUAL_HEIGHT, USUAL_SIZE};
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        // by its form alone: the size the editor starts one at, plain walls, a maze's three colours
        let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "maze" } }), || 0.0).unwrap());
        assert_eq!((state.spec.form, state.spec.maze), (brickloft_core::spec::Form::Maze, Maze::default()));
        assert_eq!((state.spec.radius, state.spec.thick, state.spec.rise), (USUAL_SIZE.0, USUAL_SIZE.1, Some(USUAL_HEIGHT)));
        assert_eq!((state.paint.mode, &state.paint.colours), (Mode::Pattern, &crate::paint::maze_palettes("none")[0]));
        assert_eq!(added["forms"][0]["settings"]["maze"], "theta,6,4,0,1,270,0,0,0");
        assert!(added["forms"][0]["bricks"].as_u64().unwrap() > 300, "{}", added["forms"][0]);
        let plain = state.built.bricks.len();
        // changed: more rings, a floor and the way through, which is painted the third colour
        let changed = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "maze": "theta,8,6,2,9,90,2,1,0", "radius": 160 } }), || 0.0).unwrap());
        assert_eq!(state.forms()[0].0.maze, Maze { kind: Kind::Theta, rings: 8, divisions: 6, grow: 2, seed: 9, entrance: 90, floor: 2, solution: true, hedge: false });
        assert!(changed["forms"][0]["bricks"].as_u64().unwrap() as usize > plain);
        let way = crate::paint::WAY;
        assert!(state.painted().iter().filter(|b| b.1 == Some(way)).count() > 10, "the way through is not in its colour");
        // brick walls come in brick colours with the way through last; and a hedge in a hedge's
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "mosaic": ["bricks", 2, 0, 0, 1, 1, 50, 0, 0, 0] } }), || 0.0).unwrap();
        let bricks = &crate::paint::maze_palettes("bricks")[0];
        assert_eq!((&state.forms()[0].1.colours, bricks.len(), bricks[7]), (bricks, 8, way));
        assert!(state.painted().iter().filter(|b| b.1 == Some(way)).count() > 10 && state.painted().iter().filter(|b| b.1 == Some(bricks[1])).count() > 100);
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "maze": "grid,4,4,0,2,0,0,0,1", "radius": 70, "thick": 6, "rise": 14 } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].1.colours, crate::paint::maze_palettes("hedge")[0]);
        assert!(state.forms()[0].0.maze.hedge && state.built.bricks.len() > 3000, "{} bricks of hedge", state.built.bricks.len());
        // colours given are kept
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "maze": "grid,4" }, "colour": { "mode": "pattern", "colours": ["#101010", "#f0f0f0", "#ff0000"] } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].1.colours, vec![[16, 16, 16], [240, 240, 240], [255, 0, 0]]);
        // what is no maze is refused; numbers out of range are brought within it; and one too large or too narrow is refused with the workspace as it was
        for bad in [json!("hexagons"), json!("theta,minus one"), json!("theta,1,2,3,4,5,6,7,8,9"), json!(""), json!("theta\nradius = 900")] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "maze": bad } }), || 0.0).is_err(), "{bad} was accepted");
        }
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "maze": "theta,-7,9999,-2,3,725,99,5,0", "radius": 60, "thick": 2 } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].0.maze, Maze { kind: Kind::Theta, rings: 2, divisions: 64, grow: 0, seed: 3, entrance: 5, floor: 8, solution: true, hedge: false });
        let before = state.forms();
        for (hostile, why) in [(json!({ "maze": "theta,40,64,1", "radius": 1000, "thick": 2 }), "too large"), (json!({ "maze": "theta,40", "radius": 100 }), "corridors"), (json!({ "maze": "theta,10,4,0,1,0,0,0,1", "radius": 600, "thick": 30, "rise": 200 }), "hedge"), (json!({ "radius": 1e9 }), "too large")] {
            let refused = apply(&mut state, "update_form", &json!({ "index": 0, "settings": hostile }), || 0.0).unwrap_err();
            assert!(refused.contains(why), "{hostile}: {refused}");
            assert_eq!(state.forms(), before);
        }
    }

    #[test]
    fn a_single_flower_keeps_which_flower_it_is_and_how_it_lies_through_undo_copy_and_save() {
        use brickloft_core::tree::{Bloom, Flower, Kind, Tree};
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        // by its kind alone it is a red rose facing up, at its usual size
        let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "tree", "tree": "flower" } }), || 0.0).unwrap());
        assert_eq!((state.spec.tree, state.spec.radius, state.spec.rise), (Tree::of(Kind::Flower), Tree::usual_size(Kind::Flower).1, Some(Tree::usual_size(Kind::Flower).0)));
        assert_eq!((added["forms"][0]["settings"]["bloom"].as_str(), state.paint.colours.as_slice()), (Some("rose,0,0,0,0"), crate::paint::palette("flower")));
        let upright = state.built.bricks.clone();
        // which flower and how it lies, given beside the kind in either order of the two, and alone afterwards
        let lying = Bloom { flower: Flower::WaterLily, tilt: 60, turn: 45, stem: true, fallen: true };
        let changed = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "bloom": "waterlily,60,45,1,1", "tree": "flower,3,80,1,90,1" } }), || 0.0).unwrap());
        let wanted = Tree { seed: 3, fullness: 80, bend: 90, extras: true, bloom: lying, ..Tree::of(Kind::Flower) };
        assert_eq!(state.forms()[0].0.tree, wanted);
        assert_eq!(changed["forms"][0]["settings"]["bloom"], "waterlily,60,45,1,1");
        assert_ne!(state.built.bricks, upright);
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "bloom": ["rose", 20] } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].0.tree, Tree { bloom: Bloom { tilt: 20, ..Bloom::default() }, ..wanted });
        // one step back is the lily lying down, and forward again the rose
        assert!(state.undo());
        assert_eq!(state.forms()[0].0.tree, wanted);
        // copied and pasted, and saved and opened, it is the same flower lying the same way
        let (_, copied) = state.copied();
        state.paste(copied);
        assert_eq!(state.forms().iter().filter(|form| form.0.tree == wanted).count(), 2);
        let opened = crate::store::forms_in(&crate::store::describe(&state.forms())).unwrap();
        assert_eq!(opened.iter().map(|form| form.0.tree).collect::<Vec<_>>(), state.forms().iter().map(|form| form.0.tree).collect::<Vec<_>>());
        // what is no flower is refused, and what is out of range is brought within it
        let before = state.forms();
        for bad in [json!("orchid"), json!("rose,steep"), json!("rose,1,2,3,4,5"), json!(""), json!("rose\nradius = 900")] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "bloom": bad } }), || 0.0).is_err(), "{bad} was accepted");
        }
        assert_eq!(state.forms(), before);
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "bloom": "gerbera,900,-90,5,5" } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].0.tree.bloom, Bloom { flower: Flower::Gerbera, tilt: 90, turn: 270, stem: true, fallen: true });
    }

    #[test]
    fn a_tree_is_added_by_its_kind_alone_at_its_usual_size_and_in_its_own_colours() {
        use brickloft_core::tree::{Kind, Tree};
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        // by its kind and seed alone: the usual size, and not a form's usual radius of 8
        let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "tree", "tree": "palm,3" } }), || 0.0).unwrap());
        let (height, spread) = Tree::usual_size(Kind::Palm);
        assert_eq!((state.spec.form, state.spec.tree.kind, state.spec.tree.seed), (brickloft_core::spec::Form::Tree, Kind::Palm, 3));
        assert_eq!((state.spec.rise, state.spec.radius), (Some(height), spread));
        assert_eq!(state.spec.tree, Tree { seed: 3, ..Tree::of(Kind::Palm) }, "the settings not given are not the kind's own");
        assert!(state.built.bricks.len() > 5000 && state.built.shades.len() == state.built.bricks.len(), "{} bricks", state.built.bricks.len());
        // in the palm's colours, as the pattern has them, and told so
        assert_eq!((state.paint.mode, state.paint.colours.as_slice()), (Mode::Pattern, crate::paint::palette("palm")));
        assert_eq!(added["forms"][0]["colour"]["mode"], "pattern");
        assert_eq!(added["forms"][0]["settings"]["tree"], "palm,3,60,20,45,0,100,0,0,0,1,80");
        // every brick is painted the colour of its part: trunk and leaves both
        let painted = state.painted();
        let of = |n: usize| painted.iter().filter(|b| b.2 == 0 && b.1 == Some(crate::paint::palette("palm")[n])).count();
        assert!(of(0) > 100 && of(1) > 1000, "{} of trunk, {} of leaves", of(0), of(1));
        // a size and a list of settings are taken as given, and a form changed into a tree comes in the tree's colours
        let small = said(apply(&mut state, "update_form", &json!({ "index": 1, "settings": { "form": "tree", "tree": ["oak", 7, 80, 3, 20, 0], "radius": 20, "rise": 36, "points": [[0.5, 0.5, 0.5]] } }), || 0.0).unwrap());
        assert_eq!(small["forms"][1]["settings"]["tree"], "oak,7,80,3,20,0,100,0,0,0,1,80");
        assert_eq!(small["forms"][1]["colour"]["mode"], "pattern");
        assert_eq!(small["forms"][1]["box_in_grid"]["size"][2].as_f64().map(|v| v <= 37.0 && v > 28.0), Some(true), "{}", small["forms"][1]["box_in_grid"]);
        // what is not a tree is refused, and what is out of range is brought within it
        for bad in [json!("bamboo"), json!("oak,minus one"), json!("oak,1,2,3,4,5,6,7,8,9,10,11,12"), json!(""), json!("pine\nradius = 900")] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "tree": bad } }), || 0.0).is_err(), "{bad} was accepted");
        }
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "tree": "pine,1,5000,-3,999,7", "radius": 12, "rise": 30 } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].0.tree, Tree { fullness: 100, count: 4, bend: 100, extras: true, baubles: 50, tinsel: true, lights: 1, ..Tree::of(Kind::Pine) });
        // a tree too large to build is refused, and the workspace is as it was
        let before = state.forms();
        assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "radius": 990, "rise": 990 } }), || 0.0).is_err());
        assert_eq!(state.forms(), before);
        // a bush and a flower are kinds of the same form, each at its own size and in its own colours
        for (kind, name) in brickloft_core::tree::KINDS {
            apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "form": "arch round", "points": [[0.5, 0.5, 0.5], [9.5, 0.5, 0.5]], "radius": 8, "rise": "natural" } }), || 0.0).unwrap();
            apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "form": "plant", "tree": name, "radius": Tree::usual_size(kind).1 / 2.0, "rise": Tree::usual_size(kind).0 / 2.0, "points": [[0.5, 0.5, 0.5]] } }), || 0.0).unwrap_or_else(|e| panic!("{name}: {e}"));
            let (spec, paint, _) = &state.forms()[0];
            assert_eq!((spec.form, spec.tree, paint.colours.as_slice()), (brickloft_core::spec::Form::Tree, Tree::of(kind), crate::paint::palette(name)), "{name}");
        }
    }

    #[test]
    fn forms_are_told_where_they_are_and_how_far_apart() {
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        // two balls of radius 5.5, 30 apart along x, the second on a grid turned a quarter turn about z
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "form": "sphere", "points": [[0.5, 0.5, 0.5]], "radius": 5.5, "fill": true }, "place": [0, 0, 0] }), || 0.0).unwrap();
        let both = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "sphere", "points": [[0.5, 0.5, 0.5]], "radius": 5.5, "fill": true }, "place": [30, 0, 0] }), || 0.0).unwrap());
        let (near, far) = (&both["forms"][1], &both["forms"][0]);
        assert_eq!(near["box_in_grid"], json!({ "low": [-5.0, -5.0, -5.0], "high": [6.0, 6.0, 6.0], "size": [11.0, 11.0, 11.0] }));
        assert_eq!(far["box_in_world"]["centre"], json!([30.5, 0.5, 0.5]));
        let apart = said(apply(&mut state, "measure_between", &json!({ "from": 1, "to": 0 }), || 0.0).unwrap());
        assert_eq!(apart["centre_to_centre_in_world"], json!([30.0, 0.0, 0.0]));
        assert_eq!(apart["distance_between_centres"], 30.0);
        // 19 of air between them along x; along y and z they overlap by their whole width
        assert_eq!(apart["gap_between_boxes_in_their_grid"], json!([19.0, -11.0, -11.0]));
        // on a grid of its own, a quarter turn about z: x in the grid is y in the world
        let turned = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "on_grid": 5, "grid_degrees": [0, 0, 90] } }), || 0.0).unwrap());
        assert_eq!(turned["forms"][0]["box_in_world"]["centre"], json!([-0.5, 30.5, 0.5]));
        let apart = said(apply(&mut state, "measure_between", &json!({ "from": 1, "to": 0 }), || 0.0).unwrap());
        assert_eq!((apart["on_the_same_grid"].clone(), apart["gap_between_boxes_in_their_grid"].clone()), (json!(false), Value::Null));
        assert!(apply(&mut state, "measure_between", &json!({ "from": 0, "to": 9 }), || 0.0).is_err());
    }

    #[test]
    fn the_camera_is_asked_about_and_sent_on_a_flight() {
        let mut state = State::new();
        state.camera_at = ([10.0, -20.0, 5.0], [0.0, 1.0, 0.0]);
        let now: Value = serde_json::from_str(&apply(&mut state, "get_camera", &json!({}), || 0.0).unwrap()).unwrap();
        assert_eq!((now["position"].clone(), now["heading_degrees"].clone(), now["tilt_degrees"].clone()), (json!([10.0, -20.0, 5.0]), json!(90.0), json!(0.0)));
        let before = state.forms();
        apply(&mut state, "move_camera", &json!({ "path": [{ "position": [0, -40, 20], "look_at": [0, 0, 5], "seconds": 2 }, { "position": [40, 0, 20], "look_at": [0, 0, 5] }] }), || 0.0).unwrap();
        assert_eq!(state.flight.len(), 2);
        assert_eq!(state.flight[0], Leg { to: [0.0, -40.0, 20.0], look_at: [0.0, 0.0, 5.0], seconds: 2.0 });
        assert_eq!(state.flight[1].seconds, 0.0, "a leg given no time is a jump");
        assert_eq!(state.forms(), before, "moving the camera changed a form");
        for bad in [json!({}), json!({ "path": [] }), json!({ "path": [{ "position": [0, 0], "look_at": [0, 0, 0] }] }), json!({ "path": [{ "position": [1e9, 0, 0], "look_at": [0, 0, 0] }] }), json!({ "path": [{ "position": [0, 0, 0], "look_at": [0, 0, 1], "seconds": 9999 }] }), json!({ "path": vec![json!({ "position": [0, 0, 0], "look_at": [0, 0, 1] }); 40] })] {
            assert!(apply(&mut state, "move_camera", &bad, || 0.0).is_err(), "{bad} was accepted");
        }
        assert_eq!(state.flight.len(), 2, "a flight that was refused replaced the one being flown");
    }

    #[test]
    fn what_is_drawn_over_the_bricks_is_switched_on_and_off() {
        let mut state = State::new();
        let said: Value = serde_json::from_str(&apply(&mut state, "set_view", &json!({ "rulers": false, "dots": false, "floor_below": 12 }), || 0.0).unwrap()).unwrap();
        assert_eq!((state.show_ruler, state.show_dots, state.show_rings, state.show_arrows, state.floor_gap), (false, false, true, true, 12.0));
        assert_eq!(said["rings"], true);
        // a mistake anywhere in it, and none of it is done
        assert!(apply(&mut state, "set_view", &json!({ "rulers": true, "floor_below": 900 }), || 0.0).is_err());
        assert!(apply(&mut state, "set_view", &json!({ "rings": "no" }), || 0.0).is_err());
        assert_eq!((state.show_ruler, state.show_rings), (false, true));
    }

    #[test]
    fn one_dot_of_a_form_is_moved_added_and_taken_off() {
        let mut state = State::new(); // an arch: two feet
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        let before = state.spec.points.clone();
        let moved = said(apply(&mut state, "move_point", &json!({ "index": 0, "point": 1, "to": [12.5, 0.5, 0.5] }), || 0.0).unwrap());
        assert_eq!(moved["forms"][0]["points"][1], json!([12.5, 0.5, 0.5]));
        assert_eq!((state.spec.points[0], state.spec.points[1]), (before[0], DVec3::new(12.5, 0.5, 0.5)), "the other foot moved too");
        // a point added to pull the arch's crown up, moved further, and taken off
        let pulled = said(apply(&mut state, "add_pull_point", &json!({ "index": 0, "at": [2.5, 0.5, 8.5], "pulled_by": [0, 0, 3], "reach": 6 }), || 0.0).unwrap());
        assert_eq!(pulled["forms"][0]["added_points"], json!([{ "at": [2.5, 0.5, 8.5], "pulled_by": [0.0, 0.0, 3.0] }]));
        assert_eq!(state.spec.pull_reach, 6.0);
        let high = state.built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap();
        apply(&mut state, "move_pull_point", &json!({ "index": 0, "added_point": 0, "pulled_by": [0, 0, 6] }), || 0.0).unwrap();
        assert!(state.built.bricks.iter().map(|b| b.min[2] + b.ext[2]).max().unwrap() > high, "pulling the point further did not raise the arch");
        apply(&mut state, "remove_pull_point", &json!({ "index": 0, "added_point": 0 }), || 0.0).unwrap();
        assert!(state.spec.pulls.is_empty());
        // each is one step to undo
        assert!(state.undo() && state.spec.pulls.len() == 1);
        let now = state.forms();
        for (tool, bad) in [
            ("move_point", json!({ "index": 0, "point": 9, "to": [0, 0, 0] })),
            ("move_point", json!({ "index": 0, "point": 0, "to": [0, 0] })),
            ("move_point", json!({ "index": 0, "point": 0, "to": [1e9, 0, 0] })),
            ("move_pull_point", json!({ "index": 0, "added_point": 4, "pulled_by": [0, 0, 1] })),
            ("remove_pull_point", json!({ "index": 0 })),
            ("add_pull_point", json!({ "index": 0, "at": [0, 0, 0], "pulled_by": [0, 0, 1], "reach": 0 })),
        ] {
            assert!(apply(&mut state, tool, &bad, || 0.0).is_err(), "{tool} took {bad}");
            assert_eq!(state.forms(), now, "{tool} with {bad} changed the workspace");
        }
    }

    #[test]
    fn a_grids_angle_goes_to_four_numbers_and_back() {
        for turns in [[0.0, 0.0, 0.0], [30.0, 0.0, 0.0], [0.0, 45.0, 0.0], [0.0, 0.0, -120.0], [10.0, 20.0, 30.0]] {
            let grid = grid_from_degrees(turns);
            assert!((grid.iter().map(|v| v * v).sum::<f64>() - 1.0).abs() < 1e-12);
            let back = degrees_from_grid(grid);
            assert!((0..3).all(|k| (back[k] - turns[k]).abs() < 1e-3), "{turns:?} came back as {back:?}");
        }
        // a quarter turn about z takes x to y
        let [x, y, z, w] = grid_from_degrees([0.0, 0.0, 90.0]);
        assert!((x.abs() + y.abs()) < 1e-12 && (z - w).abs() < 1e-12);
    }
    #[test]
    fn a_form_is_joined_to_another_and_its_join_rounded() {
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "form": "cube", "points": [[0, 0, 8]], "radius": 8, "fill": true }, "place": [0, 0, 0] }), || 0.0).unwrap();
        let ball = json!({ "form": "sphere", "points": [[0, 0, 0]], "radius": 9, "fill": true });
        // set half in the cube's corner and not joined, it is only two forms in one place
        let apart = said(apply(&mut state, "add_form", &json!({ "settings": ball, "place": [9, 3, 15] }), || 0.0).unwrap());
        assert_eq!(apart["forms"][0]["settings"]["blend"], "none");
        state.refresh(|| 0.0);
        let own = state.built.bricks.len() + state.others[0].built.bricks.len();
        assert_eq!(state.painted().len(), own);
        // joined, the two are one build with no brick's box in another's, and a blend rounds the join
        let room = |state: &State| state.painted().iter().map(|b| b.0.volume()).sum::<f64>();
        let shares_room = |state: &State| {
            let all = state.painted();
            let unit_box = |b: &crate::state::Painted| ([0, 1, 2].map(|k| 2 * b.0.min[k] + b.3[k]), [0, 1, 2].map(|k| 2 * (b.0.min[k] + b.0.ext[k]) + b.3[k]));
            all.iter().enumerate().any(|(n, a)| all[n + 1..].iter().any(|b| (0..3).all(|k| unit_box(a).0[k] < unit_box(b).1[k] && unit_box(b).0[k] < unit_box(a).1[k])))
        };
        let joined = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "blend": 0 } }), || 0.0).unwrap());
        assert_eq!(joined["forms"][0]["settings"]["blend"], "0");
        state.refresh(|| 0.0);
        assert!(state.joined() && !shares_room(&state));
        let sharp = room(&state);
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "blend": 4 } }), || 0.0).unwrap();
        state.refresh(|| 0.0);
        assert!(!shares_room(&state) && room(&state) > sharp + 20.0, "a blend of 4 added {} microbricks", room(&state) - sharp);
        // a blend past what a join can be fitted with is refused, and changes nothing
        for blend in [json!(6.5), json!(-1), json!("wide"), json!(1e300)] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "blend": blend } }), || 0.0).is_err(), "{blend} was taken for a blend");
        }
        assert_eq!(state.spec.blend, Some(4.0));
        // and `none` sets the form apart again
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "blend": "none" } }), || 0.0).unwrap();
        assert!(!state.joined());
    }

    #[test]
    fn a_form_is_cut_out_of_another_and_one_joined_half_a_microbrick_off_is_moved() {
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "form": "cube", "points": [[0, 0, 8]], "radius": 8, "fill": true }, "place": [0, 0, 0] }), || 0.0).unwrap();
        let ball = json!({ "form": "sphere", "points": [[0, 0, 0]], "radius": 9, "fill": true, "blend": 0 });
        let room = |state: &State| state.painted().iter().map(|b| b.0.volume()).sum::<f64>();
        let cube = room(&state);
        // joined half a microbrick off along x and z, it is moved by that half, and the two are fitted as one
        let joined = said(apply(&mut state, "add_form", &json!({ "settings": ball, "place": [9.5, 3, 14.5] }), || 0.0).unwrap());
        assert_eq!((joined["forms"][0]["place"].clone(), joined["forms"][0]["settings"]["carve"].as_str()), (json!([10.0, 3.0, 15.0]), Some("false")));
        assert!(room(&state) > cube + 200.0);
        // cut out instead, none of it is built and the cube has lost a corner
        let cut = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "carve": true, "blend": 2 } }), || 0.0).unwrap());
        assert_eq!(cut["forms"][0]["settings"]["carve"], "true");
        assert!(state.painted().iter().all(|b| b.2 == 1) && room(&state) < cube - 200.0, "{} microbricks left of a cube of {cube}", room(&state));
        // and a setting that is no true or false is refused
        assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "carve": "a little" } }), || 0.0).is_err());
        assert!(state.spec.carve);
    }

    #[test]
    fn a_cube_is_added_resized_and_moved() {
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        let low_high = |state: &State| {
            let b = &state.built.bricks;
            ([0, 1, 2].map(|k| b.iter().map(|b| b.min[k]).min().unwrap()), [0, 1, 2].map(|k| b.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap()))
        };
        // named alone, it stands where the editor would put one: 16 each way, on the floor
        let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "cube", "fill": true } }), || 0.0).unwrap());
        assert_eq!(added["forms"][0]["settings"]["form"], "cube");
        assert_eq!((state.spec.form, state.others.len()), (brickloft_core::spec::Form::Cube, 1));
        assert_eq!(low_high(&state), ([-8, -8, 0], [8, 8, 16]));
        // a floor 40 by 30 and 2 deep: half a side of 20, and a stretch along each axis
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "points": [[0, 0, 1]], "radius": 20, "stretch": [1, 0.75, 0.05] } }), || 0.0).unwrap();
        assert_eq!(low_high(&state), ([-20, -15, 0], [20, 15, 2]));
        assert_eq!((state.built.bricks.len(), state.built.error), (1, 0.0));
        // hollow, it is a frame; and a pattern is laid through its 2
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "fill": false, "thick": 3 } }), || 0.0).unwrap();
        assert_eq!((state.built.bricks.len(), state.built.error), (4, 0.0));
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "fill": true, "mosaic": ["planks", 2, 0, 0, 1] } }), || 0.0).unwrap();
        assert!(state.built.bricks.len() > 20 && state.built.bricks.iter().all(|b| (b.min[2], b.ext[2]) == (0, 2)));
        assert_eq!(state.paint.mode, crate::paint::Mode::Solid, "update_form recoloured a form it was not asked to");
        // its one point is its middle
        apply(&mut state, "move_point", &json!({ "index": 0, "point": 0, "to": [10, 0, 1] }), || 0.0).unwrap();
        assert_eq!(low_high(&state), ([-10, -15, 0], [30, 15, 2]));
        // a tube left on by another form is not kept, and what cannot be built is refused
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "tube": true, "mosaic": "none" } }), || 0.0).unwrap();
        assert!(!state.spec.tube && state.built.bricks.len() == 1);
        let now = state.forms();
        for bad in [json!({ "stretch": [1, 1] }), json!({ "stretch": [1e9, 1, 1] }), json!({ "radius": 0 }), json!({ "stretch": ["wide", 1, 1] })] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": bad }), || 0.0).is_err(), "{bad} was taken");
            assert_eq!(state.forms(), now, "{bad} changed the workspace");
        }
    }

    #[test]
    fn a_bookcase_is_added_by_its_form_alone_and_resized_and_refilled() {
        use brickloft_core::furniture::{Furniture, Kind};
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        let low_high = |state: &State| {
            let b = &state.built.bricks;
            ([0, 1, 2].map(|k| b.iter().map(|b| b.min[k]).min().unwrap()), [0, 1, 2].map(|k| b.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap()))
        };
        let books = |state: &State| state.built.shades.iter().filter(|s| **s >= brickloft_core::furniture::FIRST_BOOK).count();
        // named alone, it is the bookcase the editor starts: its usual size, on the floor, in its own colours
        let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "furniture" } }), || 0.0).unwrap());
        assert_eq!((added["forms"][0]["settings"]["form"].as_str(), added["forms"][0]["settings"]["furniture"].as_str()), (Some("furniture"), Some("bookcase,1,2,0,11,34,3,1,1,85,classical,0,0,0,0,1")));
        assert_eq!((state.spec.form, state.spec.furniture, state.others.len()), (brickloft_core::spec::Form::Furniture, Furniture::of(Kind::Bookcase), 1));
        assert_eq!(low_high(&state), ([-18, -5, 0], [18, 5, 60]));
        assert_eq!((state.paint.mode, state.paint.colours.as_slice()), (Mode::Pattern, crate::paint::palette("bookcase")));
        assert!(state.built.error == 0.0 && state.built.shades.len() == state.built.bricks.len() && books(&state) > 40);
        // resized as a cube is, by half a side and a stretch, and its settings given as a list: 80 by 8 by 48, bare
        let wide = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": ["bookcase", 7, 1, 4, 9, 0, 0, 0, 0, 0], "radius": 4, "stretch": [10, 1, 6], "points": [[0, 0, 24]] } }), || 0.0).unwrap());
        assert_eq!(wide["forms"][0]["settings"]["furniture"], "bookcase,7,1,4,9,0,0,0,0,0,classical,0,0,0,0,1");
        assert_eq!(low_high(&state), ([-40, -4, 0], [40, 4, 48]));
        // two sides, a top, a bottom and four shelves, each one brick
        assert_eq!((state.built.bricks.len(), books(&state), state.built.error), (8, 0, 0.0));
        assert_eq!(state.paint.colours.as_slice(), crate::paint::palette("bookcase"), "update_form recoloured a form it was not asked to");
        // filled, by the kind and the settings up to the one wanted; the same seed is the same books, and another seed others
        let fill = json!({ "index": 0, "settings": { "furniture": "bookcase,7,1,4,9,0,0,0,0,100" } });
        apply(&mut state, "update_form", &fill, || 0.0).unwrap();
        let first = state.built.bricks.clone();
        assert!(books(&state) > 150, "{} books", books(&state));
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": "bookcase,8,1,4,9,0,0,0,0,100" } }), || 0.0).unwrap();
        assert!(state.built.bricks != first);
        apply(&mut state, "update_form", &fill, || 0.0).unwrap();
        assert_eq!(state.built.bricks, first);
        // the kind alone is the kind as it starts, and the first few settings leave the rest its own
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": "bookcase,3,4" } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].0.furniture, Furniture { seed: 3, board: 4, ..Furniture::of(Kind::Bookcase) });
        // what is out of range is brought within it, and what is no furniture is refused, leaving the workspace as it was
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": "bookcase,1,99,999,1,2,99,5,5,500" } }), || 0.0).unwrap();
        assert_eq!(state.forms()[0].0.furniture, Furniture { kind: Kind::Bookcase, seed: 1, board: 8, shelves: 60, spacing: 3, bay: 4, plinth: 20, back: true, moulding: true, books: 100, ..brickloft_core::furniture::Furniture::of(brickloft_core::furniture::Kind::Bookcase) });
        let now = state.forms();
        for bad in [json!({ "furniture": "wardrobe" }), json!({ "furniture": "bookcase,-1" }), json!({ "furniture": "bookcase,1,thick" }), json!({ "furniture": "bookcase,1,2,0,11,34,3,1,1,85,6" }), json!({ "furniture": "bookcase", "radius": 1000 }), json!({ "furniture": "bookcase,1,1,0,3,4,0,1,0,100", "radius": 500, "stretch": [2, 0.05, 2] })] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": bad }), || 0.0).is_err(), "{bad} was taken");
            assert_eq!(state.forms(), now, "{bad} changed the workspace");
        }
        // a form changed into furniture comes in the furniture's colours, and keeps the colours asked for with the change
        apply(&mut state, "update_form", &json!({ "index": 1, "settings": { "form": "furniture", "radius": 5, "stretch": [2, 1, 3], "points": [[0, 0, 15]] } }), || 0.0).unwrap();
        let (spec, paint, _) = &state.forms()[1];
        assert_eq!((spec.form, paint.mode, paint.colours.as_slice()), (brickloft_core::spec::Form::Furniture, Mode::Pattern, crate::paint::palette("bookcase")));
        apply(&mut state, "add_form", &json!({ "settings": { "form": "furniture", "furniture": "bookcase,2" }, "colour": { "mode": "pattern", "colours": ["#ffffff", "#dddddd", "#ff0000", "#00ff00", "#0000ff"] } }), || 0.0).unwrap();
        assert_eq!(state.paint.colours.len(), 5);
    }

    #[test]
    fn every_kind_of_furniture_is_added_by_its_name_and_restyled_by_its_settings() {
        use brickloft_core::furniture::{Furniture, Style, KINDS};
        let mut state = State::new();
        let said = |text: String| serde_json::from_str::<Value>(&text).unwrap();
        for (kind, name) in KINDS {
            // named alone, it is the kind as the editor starts it: its usual size, on the floor, in its style's own colours
            let added = said(apply(&mut state, "add_form", &json!({ "settings": { "form": "furniture", "furniture": name } }), || 0.0).unwrap_or_else(|e| panic!("{name}: {e}")));
            let [w, d, h] = Furniture::usual_size(kind).map(|v| v as i32);
            assert_eq!((state.spec.form, state.spec.furniture), (brickloft_core::spec::Form::Furniture, Furniture::of(kind)), "{name}");
            let b = &state.built.bricks;
            let (low, high) = ([0, 1, 2].map(|k| b.iter().map(|b| b.min[k]).min().unwrap()), [0, 1, 2].map(|k| b.iter().map(|b| b.min[k] + b.ext[k]).max().unwrap()));
            // (on the floor, or for what hangs with its top where its box's is)
            assert!((low[2] == 0 || high[2] == h) && (0..3).all(|k| high[k] - low[k] <= [w, d, h][k]) && high[2] - low[2] >= h / 2, "{name}: from {low:?} to {high:?}");
            assert_eq!((state.paint.mode, state.paint.colours.as_slice()), (Mode::Pattern, crate::paint::palette(state.spec.furniture.colours())), "{name}");
            assert!(state.built.shades.len() == state.built.bricks.len() && added["forms"][0]["settings"]["furniture"].as_str().unwrap().starts_with(name), "{name}");
            // its style, its seed and its own settings are the values after the bookcase's ten, and are read back as they were sent
            let sent = json!([name, 5, 2, 0, 11, 34, 3, 1, 1, 40, "rustic", 2, 3, 1, 1, 0]);
            let before = (state.built.bricks.clone(), state.built.shades.clone());
            let changed = said(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": sent } }), || 0.0).unwrap_or_else(|e| panic!("{name}: {e}")));
            assert_eq!(state.forms()[0].0.furniture, Furniture { seed: 5, books: 40, style: Style::Rustic, count: 2, choice: 3, second: 1, extra: true, shine: false, ..Furniture::of(kind) }, "{name}");
            assert_eq!(changed["forms"][0]["settings"]["furniture"], format!("{name},5,2,0,11,34,3,1,1,40,rustic,2,3,1,1,0"), "{name}");
            assert!((state.built.bricks.clone(), state.built.shades.clone()) != before, "{name}: the same piece after its style and seed were changed");
            apply(&mut state, "remove_form", &json!({ "index": 0 }), || 0.0).unwrap();
        }
        // a style that is none, a kind that is none and a value too many are refused, and leave the workspace as it was
        apply(&mut state, "add_form", &json!({ "settings": { "form": "furniture", "furniture": "sofa,3,2,0,11,34,3,1,1,85,random" } }), || 0.0).unwrap();
        assert_eq!(state.spec.furniture.style, Style::Random);
        assert_eq!(state.paint.colours.as_slice(), crate::paint::palette(state.spec.furniture.colours()));
        let now = state.forms();
        for bad in ["sofa,3,2,0,11,34,3,1,1,85,baroque", "hammock", "sofa,3,2,0,11,34,3,1,1,85,modern,1,1,1,1,1,1", "sofa,3,2,0,11,34,3,1,1,85,modern,many"] {
            assert!(apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": bad } }), || 0.0).is_err(), "{bad} was taken");
            assert_eq!(state.forms(), now, "{bad} changed the workspace");
        }
        // what is out of range is brought within it
        apply(&mut state, "update_form", &json!({ "index": 0, "settings": { "furniture": "sofa,3,2,0,11,34,3,1,1,85,modern,999,-4,77" } }), || 0.0).unwrap();
        let f = state.forms()[0].0.furniture;
        assert_eq!((f.count, f.choice, f.second), (12, 0, 12));
    }
}
