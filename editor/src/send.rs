//! Sending a build to the plugin: one POST to the server the page came from,
//! made only when the player presses Send.

use crate::state::Painted;
use arcs_core::orient::to_wire;
use arcs_core::shapes::CellBrick;
use std::cell::RefCell;
use std::rc::Rc;

/// The request body:
/// `{ "token": "...", "bricks": [integers], "palette": [r, g, b, ...], "paint": [integers] }`.
///
/// `paint` has one number per brick: 0 for the player's own colour in the
/// game, or 1 and up for an entry of `palette`. Colours go as a palette and
/// an index per brick because a form has far more bricks than colours.
///
/// The token is copied only if it is the plain hex the plugin issues, so
/// nothing from the page's address can break out of the JSON string.
pub fn body(token: &str, bricks: &[Painted]) -> Option<String> {
    let hex = !token.is_empty() && token.len() <= 64 && token.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex {
        return None;
    }
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let paint: Vec<String> = bricks
        .iter()
        .map(|(_, colour, _, _)| match colour {
            None => 0,
            Some(c) => 1 + palette.iter().position(|p| p == c).unwrap_or_else(|| {
                palette.push(*c);
                palette.len() - 1
            }),
        })
        .map(|i| i.to_string())
        .collect();
    let cells: Vec<(CellBrick, [i32; 3])> = bricks.iter().map(|b| (b.0, b.3)).collect();
    let flat: Vec<String> = palette.iter().flatten().map(|v| v.to_string()).collect();
    Some(format!("{{\"token\":\"{token}\",\"bricks\":{},\"palette\":[{}],\"paint\":[{}]}}", to_wire(&cells), flat.join(","), paint.join(",")))
}

/// The value of `key` in a reply like `{"ok":true,"placed":12}`: just enough
/// reading for the three numbers and one message the plugin sends back,
/// without bringing a JSON parser into the page.
pub fn field<'a>(reply: &'a str, key: &str) -> Option<&'a str> {
    let rest = &reply[reply.find(&format!("\"{key}\":"))? + key.len() + 3..];
    if let Some(text) = rest.strip_prefix('"') {
        return text.split('"').next();
    }
    Some(rest.split([',', '}']).next()?.trim())
}

pub fn describe(reply: &str) -> String {
    if field(reply, "ok") == Some("true") {
        let (placed, sent) = (field(reply, "placed").unwrap_or("?"), field(reply, "sent").unwrap_or("?"));
        match field(reply, "blocked") {
            Some("0") | None => format!("Placed {placed} of {sent} bricks."),
            Some(blocked) => format!("Placed {placed} of {sent} bricks; {blocked} were blocked by bricks already there."),
        }
    } else {
        format!("Not placed: {}", field(reply, "error").unwrap_or("the server gave no reason"))
    }
}

#[cfg(target_arch = "wasm32")]
pub fn send(bricks: &[Painted], status: Rc<RefCell<String>>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let Some(window) = web_sys::window() else { return };
    // the link from /arc ends in ?t=<token>
    let search = window.location().search().unwrap_or_default();
    let token = search.trim_start_matches('?').split('&').find_map(|pair| pair.strip_prefix("t=")).unwrap_or("");
    let Some(body) = body(token, bricks) else {
        *status.borrow_mut() = "This page was not opened from a link. Type /arc in the game and open the link it gives you.".into();
        return;
    };
    *status.borrow_mut() = "Sending…".into();

    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(&body));
    // a relative address: the request can only go to the server this page came from
    let request = window.fetch_with_str_and_init("build", &init);

    // Three callbacks: the request failed, the response arrived, its text
    // arrived. Each is a `Closure::once`, which frees itself when called, and
    // is then handed over to the browser with `forget`, since it must outlive
    // this function. Of `failed` and `got_response` only one ever runs, so the
    // other's few bytes stay allocated: one small leak per press of Send, the
    // price of not pulling in an async runtime (wasm-bindgen-futures) for a
    // single request.
    // https://docs.rs/wasm-bindgen/latest/wasm_bindgen/closure/struct.Closure.html#method.once
    let failed: Closure<dyn FnMut(JsValue)> = {
        let status = status.clone();
        Closure::once(move |_: JsValue| {
            *status.borrow_mut() = "Could not reach the server. Is it still running?".into();
        })
    };
    let got_response: Closure<dyn FnMut(JsValue)> = Closure::once(move |response: JsValue| {
        let got_text: Closure<dyn FnMut(JsValue)> = {
            let status = status.clone();
            Closure::once(move |text: JsValue| {
                *status.borrow_mut() = describe(&text.as_string().unwrap_or_default());
            })
        };
        match response.dyn_into::<web_sys::Response>().ok().and_then(|r| r.text().ok()) {
            Some(text) => {
                let _ = text.then(&got_text);
                got_text.forget();
            }
            None => *status.borrow_mut() = "The server's answer could not be read.".into(),
        }
    });
    let _ = request.then(&got_response).catch(&failed);
    got_response.forget();
    failed.forget();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn send(bricks: &[Painted], status: Rc<RefCell<String>>) {
    // the desktop window is for trying the editor out; it has no server to talk to
    *status.borrow_mut() = format!("{} bricks ready. Sending works from the browser page the plugin serves.", bricks.len());
}

/// Put a report where it can be read: a file beside where the program was
/// started on the desktop, the clipboard in the browser (a page cannot write
/// files). Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_report(text: &str) -> String {
    // arcs-report-1.txt, -2, ...: the first number not yet used, so that every
    // save is kept. (One fixed name lost every report but the last.)
    let folder = std::env::current_dir().unwrap_or_default();
    let path = (1..10_000).map(|n| folder.join(format!("arcs-report-{n}.txt"))).find(|p| !p.exists()).unwrap_or_else(|| folder.join("arcs-report.txt"));
    match std::fs::write(&path, text) {
        Ok(()) => format!("Report saved to {}", path.display()),
        Err(e) => format!("Could not save the report to {}: {e}. Start the editor from a folder you can write to.", path.display()),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn save_report(text: &str) -> String {
    match web_sys::window() {
        // the browser allows this only on a page served from localhost or over https
        Some(window) => {
            let _ = window.navigator().clipboard().write_text(text);
            "Report copied to the clipboard. Paste it into a file or a message.".into()
        }
        None => "Could not reach the clipboard.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_takes_only_hex_tokens() {
        let brick = CellBrick { mask: 0xff, min: [0, 0, 0], ext: [1, 1, 1] };
        let plain: Painted = (brick, None, 0, [0; 3]);
        assert_eq!(body("0a1B", &[plain]).unwrap(), r#"{"token":"0a1B","bricks":[255,0,0,0,1,1,1],"palette":[],"paint":[0]}"#);
        for bad in ["", "abc\"", "ab cd", "x", "a\",\"bricks\":[]"] {
            assert_eq!(body(bad, &[plain]), None, "{bad}");
        }
        // a form standing an odd unit off is sent a unit on from its cells
        let moved: Painted = (CellBrick { mask: 0xff, min: [3, -2, 0], ext: [1, 2, 1] }, None, 0, [1, 0, -3]);
        assert!(body("ab", &[moved]).unwrap().contains(r#""bricks":[255,7,-4,-3,1,2,1]"#));
        // each colour is in the palette once, however many bricks use it
        let (red, blue) = (Some([200, 0, 0]), Some([0, 0, 200]));
        let coloured: Vec<Painted> = [red, None, blue, red].into_iter().map(|c| (brick, c, 0, [0; 3])).collect();
        let text = body("ab", &coloured).unwrap();
        assert!(text.ends_with(r#""palette":[200,0,0,0,0,200],"paint":[1,0,2,1]}"#), "{text}");
    }

    #[test]
    fn replies_are_described() {
        assert_eq!(describe(r#"{"ok":true,"placed":12,"blocked":0,"sent":12}"#), "Placed 12 of 12 bricks.");
        assert_eq!(describe(r#"{"blocked":3,"ok":true,"placed":9,"sent":12}"#), "Placed 9 of 12 bricks; 3 were blocked by bricks already there.");
        assert_eq!(describe(r#"{"ok":false,"error":"this link is no longer valid"}"#), "Not placed: this link is no longer valid");
        assert_eq!(describe("gibberish"), "Not placed: the server gave no reason");
    }
}
