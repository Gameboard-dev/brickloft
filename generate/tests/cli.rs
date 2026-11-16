//! The program as the editor meets it: started with a list of arguments,
//! answering in lines of JSON, ending with a code.
//!
//! None of these needs weights or a graphics card: each is a request the
//! program has to turn down, or `serve` being asked things it can answer
//! without a model. They check the promise the editor relies on, that
//! standard output is JSON lines and nothing else, and that a run ends in
//! exactly one `result` or one `error`.

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn program() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_brickloft-generate"));
    command.stdin(Stdio::null()).stderr(Stdio::null());
    command
}

/// An empty folder to stand for the weights folder.
fn empty_weights(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("brickloft-generate-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Runs the program and gives its exit code and its lines, each of which must be a JSON object with an `event`.
fn run(args: &[&str]) -> (i32, Vec<Value>) {
    let output = program().args(args).output().unwrap();
    let text = String::from_utf8(output.stdout).expect("standard output is Unicode");
    let lines: Vec<Value> = text.lines().map(|line| {
        let value: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("a line of standard output is not JSON ({e}): {line}"));
        assert!(value["event"].is_string(), "a line has no event: {line}");
        value
    }).collect();
    (output.status.code().unwrap(), lines)
}

/// The one line a run ends with.
fn ending(lines: &[Value]) -> &Value {
    let endings: Vec<&Value> = lines.iter().filter(|l| l["event"] == "result" || l["event"] == "error").collect();
    assert_eq!(endings.len(), 1, "a run ends in exactly one result or error: {lines:?}");
    assert!(std::ptr::eq(endings[0], lines.last().unwrap()), "and it is the last line");
    endings[0]
}

#[test]
fn arguments_that_are_wrong_end_in_one_error_and_code_2() {
    for args in [&[][..], &["paint"], &["mesh", "--out", "a.glb"], &["check", "--frobnicate"], &["picture", "--prompt", "x", "--out", "p", "--count", "99"]] {
        let (code, lines) = run(args);
        assert_eq!(code, 2, "{args:?}");
        assert_eq!(lines.len(), 1);
        assert_eq!(ending(&lines)["code"], "usage");
        assert!(ending(&lines)["message"].as_str().unwrap().ends_with("Run `brickloft-generate help` for the commands"));
    }
}

#[test]
fn a_prompt_over_the_bound_is_refused_before_anything_is_loaded() {
    let weights = empty_weights("prompt");
    let long = "a".repeat(501);
    let (code, lines) = run(&["picture", "--prompt", &long, "--out", weights.join("out").to_str().unwrap(), "--weights", weights.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(ending(&lines)["code"], "prompt_too_long");
    assert_eq!(ending(&lines)["message"], "the prompt is 501 characters long and the most is 500: shorten it");
    assert!(!weights.join("out").exists(), "nothing was written");
    let (_, lines) = run(&["picture", "--prompt", "   ", "--out", "p", "--weights", weights.to_str().unwrap()]);
    assert_eq!(ending(&lines)["code"], "prompt_empty");
}

#[test]
fn fetch_shows_the_licence_and_stops_without_the_flag() {
    let weights = empty_weights("licence");
    let (code, lines) = run(&["fetch", "sd15", "--weights", weights.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["event"], "licence");
    assert_eq!(lines[0]["licence"], "CreativeML OpenRAIL-M");
    assert!(lines[0]["restrictions"].as_str().unwrap().contains("To defame, disparage or otherwise harass others;"));
    assert_eq!(ending(&lines)["code"], "licence_not_accepted");
    assert_eq!(std::fs::read_dir(&weights).unwrap().count(), 0, "nothing was fetched or made");

    // something that is not on the list is refused whether or not a licence is accepted, and no licence is shown for it
    let (code, lines) = run(&["fetch", "https://example.com/model.safetensors", "--accept-licence", "--weights", weights.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(lines.len(), 1);
    assert_eq!(ending(&lines)["code"], "not_on_list");
    assert_eq!(ending(&lines)["message"], "\"https://example.com/model.safetensors\" is not something this program fetches. It fetches: triposr, sd15");
}

#[test]
fn a_step_without_its_weights_says_how_to_get_them() {
    let weights = empty_weights("missing");
    let picture = weights.join("picture.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([200, 200, 200, 255])).save(&picture).unwrap();
    let (code, lines) = run(&["mesh", "--picture", picture.to_str().unwrap(), "--out", weights.join("m.glb").to_str().unwrap(), "--weights", weights.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(ending(&lines)["code"], "weights_missing");
    assert!(ending(&lines)["message"].as_str().unwrap().ends_with("fetch them with `brickloft-generate fetch triposr`"));
    let (_, lines) = run(&["picture", "--prompt", "an oak barrel", "--out", weights.join("p").to_str().unwrap(), "--weights", weights.to_str().unwrap()]);
    assert_eq!(ending(&lines)["code"], "weights_missing");
    assert!(ending(&lines)["message"].as_str().unwrap().ends_with("fetch them with `brickloft-generate fetch sd15`"));
    assert_eq!(std::fs::read_dir(&weights).unwrap().count(), 1, "only the picture this test made is there");
}

#[test]
fn help_is_a_result_like_any_other() {
    let (code, lines) = run(&["help"]);
    assert_eq!(code, 0);
    assert_eq!(ending(&lines)["command"], "help");
    assert_eq!(ending(&lines)["protocol"], 1);
    assert!(ending(&lines)["text"].as_str().unwrap().contains("brickloft-generate mesh --picture"));
}

#[test]
fn serve_answers_each_line_and_ends_when_its_input_does() {
    let weights = empty_weights("serve");
    let mut child = program().args(["serve", "--weights", weights.to_str().unwrap()]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let requests = [
        r#"{"command":"help"}"#.to_string(),
        "this is not a request".to_string(),
        r#"{"command":"fetch","what":"sd15"}"#.to_string(),
        serde_json::json!({ "command": "picture", "prompt": "a".repeat(501), "out": "p" }).to_string(),
        r#"{"command":"mesh","picture":"nowhere.png","out":"m.glb","url":"https://example.com"}"#.to_string(),
        "x".repeat(40_000),
        r#"{"command":"help"}"#.to_string(),
    ];
    for request in &requests {
        writeln!(stdin, "{request}").unwrap();
    }
    // the end of input, with nothing being done, is the end of the program
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let lines: Vec<Value> = String::from_utf8(output.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let said: Vec<String> = lines.iter().filter(|l| l["event"] != "licence").map(|l| format!("{}:{}", l["event"].as_str().unwrap(), l.get("command").or(l.get("code")).and_then(Value::as_str).unwrap())).collect();
    assert_eq!(said, ["result:serve", "result:help", "error:bad_request", "error:licence_not_accepted", "error:prompt_too_long", "error:bad_request", "error:bad_request", "result:help"]);
    assert_eq!(std::fs::read_dir(&weights).unwrap().count(), 0);
}
