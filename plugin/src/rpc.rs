//! JSON-RPC 2.0 with omegga over stdin and stdout, one message per line.
//! omegga/docs/plugins/jsonrpc.md lists the methods on both sides.
//!
//! Blocking threads and channels, not an async runtime: there is one reader
//! (the main thread) and a handful of callers that each wait for one answer.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// How long to wait for omegga to answer. Its own log-scraping calls (ghost
/// brick, paint) give up after a second or so; this only has to outlast them.
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Rpc {
    next_id: AtomicU64,
    waiting: Mutex<HashMap<u64, Sender<Result<Value, String>>>>,
}

impl Rpc {
    fn send(&self, message: Value) {
        // one locked write per message, so lines from two threads can't interleave
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{message}");
        let _ = out.flush();
    }

    /// Call one of omegga's methods and wait for its result.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = channel();
        self.waiting.lock().unwrap().insert(id, tx);
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params, "id": id }));
        let answer = rx.recv_timeout(TIMEOUT);
        self.waiting.lock().unwrap().remove(&id);
        answer.unwrap_or_else(|_| Err(format!("omegga did not answer `{method}` within {} s", TIMEOUT.as_secs())))
    }

    /// Call a method without waiting for anything back.
    pub fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Answer a request omegga made of us.
    pub fn respond(&self, id: &Value, result: Value) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    /// Refuse a request omegga made of us.
    pub fn respond_error(&self, id: &Value, code: i64, message: &str) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
    }

    /// Hand an answer from omegga to whoever is waiting for it.
    pub fn resolve(&self, message: &Value) {
        let Some(id) = message.get("id").and_then(Value::as_u64) else { return };
        if let Some(tx) = self.waiting.lock().unwrap().remove(&id) {
            let result = match message.get("error") {
                Some(e) if !e.is_null() => Err(e.get("message").and_then(Value::as_str).unwrap_or("unknown error").to_string()),
                _ => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            };
            let _ = tx.send(result);
        }
    }

    pub fn log(&self, line: impl AsRef<str>) {
        self.notify("log", json!(line.as_ref()));
    }

    /// Failures say what failed, why, and what to do about it.
    pub fn error(&self, line: impl AsRef<str>) {
        self.notify("error", json!(line.as_ref()));
    }

    pub fn whisper(&self, player: &str, line: impl AsRef<str>) {
        self.notify("whisper", json!({ "target": player, "line": line.as_ref() }));
    }
}
