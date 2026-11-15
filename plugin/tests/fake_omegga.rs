//! Runs the real plugin binary against a stand-in for omegga: this test
//! speaks omegga's side of JSON-RPC over the plugin's stdin and stdout, and
//! is the editor too, over HTTP. No game is involved; what it checks is
//! everything up to the request that would be handed to the Lua mod.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

struct Plugin {
    child: Child,
    stdin: ChildStdin,
    /// every line the plugin writes, parsed
    lines: Receiver<Value>,
    port: u16,
}

impl Plugin {
    fn start() -> Plugin {
        // a port nothing holds: ask the system for one, then let it go
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let mut child = Command::new(env!("CARGO_BIN_EXE_omegga-arcs")).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                let _ = tx.send(serde_json::from_str(&line).expect("the plugin wrote a line that is not JSON"));
            }
        });
        let mut plugin = Plugin { child, stdin, lines, port };
        plugin.send(json!({ "jsonrpc": "2.0", "id": 1, "method": "init", "params": { "port": port, "max-bricks": 3 } }));
        let answer = plugin.next(|m| m["id"] == 1);
        assert_eq!(answer["result"]["registeredCommands"], json!(["arc"]));
        // the server thread starts after init is answered
        for _ in 0..200 {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return plugin;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the plugin never listened on port {port}");
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
    }

    /// The next line from the plugin that `want` accepts; others are dropped.
    fn next(&self, want: impl Fn(&Value) -> bool) -> Value {
        loop {
            let line = self.lines.recv_timeout(Duration::from_secs(10)).expect("the plugin went quiet");
            if want(&line) {
                return line;
            }
        }
    }

    /// Type `/arc` as `player` and return the token from the whispered link.
    fn link(&mut self, player: &str) -> String {
        self.send(json!({ "jsonrpc": "2.0", "method": "cmd:arc", "params": [player] }));
        let whisper = self.next(|m| m["method"] == "whisper" && m["params"]["line"].as_str().is_some_and(|l| l.contains("?t=")));
        assert_eq!(whisper["params"]["target"], player);
        let line = whisper["params"]["line"].as_str().unwrap();
        assert!(line.contains(&format!("http://localhost:{}/?t=", self.port)), "{line}");
        line.rsplit("?t=").next().unwrap().to_string()
    }

    fn http(&self, method: &str, path: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        let status = answer.split(' ').nth(1).unwrap().parse().unwrap();
        (status, answer.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn a_build_goes_from_the_editor_to_the_game() {
    let mut plugin = Plugin::start();
    let token = plugin.link("Tester");
    assert_eq!(token.len(), 32);

    // a 2 x 1 x 1 cube at the anchor and a unit tetrahedron two cells along
    // the cube in the player's own colour, the tetrahedron in the palette's one
    let body = json!({ "token": token, "bricks": [255, 0, 0, 0, 2, 1, 1, 0x17, 2, 0, 0, 1, 1, 1], "palette": [12, 34, 56], "paint": [0, 1] }).to_string();
    let port = plugin.port;
    let post = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(stream, "POST /build HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    });

    // be omegga while the plugin works through the build
    let mut to_game = Value::Null;
    while to_game.is_null() {
        let call = plugin.next(|m| m.get("id").is_some() && m.get("method").is_some());
        let result = match call["method"].as_str().unwrap() {
            "player.getGhostBrick" => {
                assert_eq!(call["params"], "Tester");
                json!({ "targetGrid": "", "location": [100.0, 200.4, 299.6], "orientation": "Z_Positive_0" })
            }
            "player.getPaint" => json!({ "materialIndex": "1", "materialAlpha": "5", "material": "BMC_Glass", "color": [10, 20, 300] }),
            "plugin.emit" => {
                to_game = call["params"].clone();
                json!({ "ok": true, "result": { "sent": 2, "placed": 1, "blocked": 1 } })
            }
            other => panic!("unexpected call to omegga: {other}"),
        };
        plugin.send(json!({ "jsonrpc": "2.0", "id": call["id"], "result": result }));
    }

    assert_eq!(to_game[0], "ue4ss");
    assert_eq!(to_game[1], "call");
    assert_eq!(to_game[2], "arcs_place");
    let request = &to_game[3];
    assert_eq!(request["player"], "Tester");
    assert_eq!(request["first"], true);
    assert_eq!(request["origin"], json!([100, 200, 300]), "the ghost brick's position, rounded to whole units");
    assert_eq!(request["color"], json!([10, 20, 255]), "paint, clamped to a byte");
    assert_eq!(request["material"], "Glass");
    assert_eq!(request["assets"][2], "PB_DefaultMicroWedgeTriangleCorner");
    assert_eq!(request["palette"], json!([[12, 34, 56]]));
    // asset index (from 1), half-extents, centre in units from the anchor, orientation, colour
    assert_eq!(
        request["bricks"],
        json!([
            1, 2, 1, 1, 1, 0, 0, 16, 0, // cells 0..2 are units -1..3: centre 1
            3, 1, 1, 1, 4, 0, 0, 16, 1, // cell 2 is units 3..5: centre 4
        ])
    );

    let answer = post.join().unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let reply: Value = serde_json::from_str(answer.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(reply, json!({ "ok": true, "sent": 2, "placed": 1, "blocked": 1 }));
    let told = plugin.next(|m| m["method"] == "whisper");
    assert!(told["params"]["line"].as_str().unwrap().contains("1 were blocked"));
}

#[test]
fn requests_that_should_not_build_do_not() {
    let mut plugin = Plugin::start();
    let token = plugin.link("Tester");
    let post = |plugin: &Plugin, body: Value| plugin.http("POST", "/build", &body.to_string());
    let cube = [255, 0, 0, 0, 1, 1, 1];

    let (status, body) = post(&plugin, json!({ "token": "0".repeat(32), "bricks": cube }));
    assert_eq!(status, 401, "{body}");
    let (status, _) = post(&plugin, json!({ "bricks": cube }));
    assert_eq!(status, 401);

    // with a good token: every malformed brick list is refused before the game is asked anything
    for (bricks, why) in [
        (json!([255, 0, 0, 0, 1, 1]), "multiple"),
        (json!([24, 0, 0, 0, 1, 1, 1]), "not a shape"),
        (json!([255, 0, 0, 0, 1, 1, 81]), "size"),
        (json!([255, 0, 0, 9e9, 1, 1, 1]), "whole numbers"),
        (json!([255, 0, 0, "0; os.exit()", 1, 1, 1]), "whole numbers"),
        (json!([]), "no bricks"),
        (json!("255,0,0,0,1,1,1"), "whole numbers"),
        (json!([cube, cube, cube, cube].concat()), "more than this server allows"),
    ] {
        let (status, body) = post(&plugin, json!({ "token": token, "bricks": bricks }));
        assert!(status == 400 || status == 413, "{bricks}: {status}");
        assert!(body.contains(why), "{bricks}: expected `{why}` in {body}");
    }
    // nor a good brick list with bad colours
    for (palette, paint, why) in [(json!([1, 2]), json!([0]), "palette"), (json!([1, 2, 3]), json!([2]), "not in the palette"), (json!([1, 2, 3]), json!([1, 1]), "1 bricks and 2 colours")] {
        let (status, body) = post(&plugin, json!({ "token": token, "bricks": cube, "palette": palette, "paint": paint }));
        assert_eq!(status, 400, "{palette} {paint}: {body}");
        assert!(body.contains(why), "expected `{why}` in {body}");
    }
    let (status, _) = plugin.http("POST", "/build", "{ not json");
    assert_eq!(status, 400);

    // a new link retires the old one
    let newer = plugin.link("Tester");
    assert_ne!(newer, token);
    let (status, _) = post(&plugin, json!({ "token": token, "bricks": cube }));
    assert_eq!(status, 401);

    // only the editor's own files exist, whatever the path says
    for path in ["/../Cargo.toml", "/dist/index.html", "/..%2f..%2fCargo.toml", "/build"] {
        assert_eq!(plugin.http("GET", path, "").0, 404, "{path}");
    }
    let (status, _) = plugin.http("GET", "/", "");
    assert!(status == 200 || status == 503, "the page, or word that it is not built: {status}");
    assert_eq!(plugin.http("DELETE", "/build", "").0, 404);

    // none of that reached the game
    plugin.send(json!({ "jsonrpc": "2.0", "id": 9, "method": "stop" }));
    let mut saw_stop = false;
    while let Ok(line) = plugin.lines.recv_timeout(Duration::from_secs(5)) {
        assert!(line["method"] != "plugin.emit" && line["method"] != "player.getGhostBrick", "{line}");
        saw_stop |= line["id"] == 9;
    }
    assert!(saw_stop, "stop was not answered");
    assert!(plugin.child.wait().unwrap().success());
}
