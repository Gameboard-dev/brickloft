//! `check`: what this computer can do, said so that the editor can show it.
//!
//! Nothing is loaded and nothing is fetched. The answer has the device that
//! would be used and its memory, each model's files and licence, and for each
//! of the two steps whether it can run now and, when it cannot, why, in the
//! sentence that step would itself fail with (`Error`'s wording, so the two
//! cannot disagree).

use crate::args::Want;
use crate::device::{self, Kind, Memory, Step};
use crate::error::{Error, Result};
use crate::fetch;
use crate::files;
use crate::models::{Model, ALL, SD15, TRIPOSR};
use crate::protocol::Out;
use serde_json::{json, Value};
use std::path::Path;

/// A model's files as they are on the disk. With `verify`, each file's
/// SHA-256 is worked out too, which reads all of it.
fn weights_of(model: &Model, weights: &Path, verify: bool, out: &Out) -> Result<Value> {
    let folder = model.folder(weights);
    let (mut listed, mut present, mut bytes_present) = (Vec::new(), true, 0u64);
    for file in model.files {
        let path = folder.join(file.local);
        let found = files::length(&path);
        let right_length = found == Some(file.bytes);
        let verified = if verify && right_length {
            let mut read = 0u64;
            let hash = fetch::sha256_of(&path, |n| {
                read += n;
                if read % (256 << 20) < n {
                    out.progress("verify", read, file.bytes, json!({ "model": model.name, "file": file.local }));
                }
            })?;
            Some(hash == file.sha256)
        } else {
            None
        };
        present &= right_length && verified != Some(false);
        bytes_present += found.unwrap_or(0).min(file.bytes);
        listed.push(json!({ "file": file.local, "bytes": file.bytes, "bytes_found": found, "right_length": right_length, "verified": verified }));
    }
    let mut about = fetch::licence_of(model);
    if let Value::Object(about) = &mut about {
        about.insert("step".into(), model.step.into());
        about.insert("folder".into(), json!(folder));
        about.insert("present".into(), present.into());
        about.insert("bytes_found".into(), bytes_present.into());
        about.insert("files".into(), listed.into());
    }
    Ok(about)
}

/// Whether a step can run, and why not. `weights` is the first thing wrong with the model's files, if anything is.
fn step_of(step: Step, device: &std::result::Result<(Kind, Option<Memory>), Error>, weights: Option<Error>) -> Value {
    let (needs_mib, problem) = match device {
        Err(e) => (None, Some(e.to_string())),
        Ok((kind, memory)) => (Some(device::needs_mib(step, *kind)), weights.map(|e| e.to_string()).or_else(|| device::enough(step, *kind, memory.as_ref()).err().map(|e| e.to_string()))),
    };
    json!({ "can_run": problem.is_none(), "needs_mib": needs_mib, "reason": problem })
}

pub fn check(weights: &Path, want: Want, verify: bool, out: &Out) -> Result<Value> {
    // The device is opened and let go: opening it is the only way to know that it opens.
    let (device_json, device_problem, device_for_steps) = match device::choose(want) {
        Ok(chosen) => {
            let memory = device::memory(chosen.name.as_deref());
            let about = json!({
                "kind": chosen.kind.name(),
                "name": chosen.name.clone().or(memory.as_ref().map(|m| m.card.clone())),
                "memory_total_mib": memory.as_ref().map(|m| m.total_mib),
                "memory_free_mib": memory.as_ref().map(|m| m.free_mib),
                // said when it is not known, so that an empty field is not taken for nought
                "memory_note": memory.is_none().then_some("the graphics memory could not be measured on this card, so it is not known whether there is enough"),
            });
            (about, None, Ok((chosen.kind, memory)))
        }
        // for the steps, the device's own failure is the reason, in its own words
        Err(e) => (Value::Null, Some(e.to_string()), Err(e)),
    };

    let mut models = Vec::new();
    for model in ALL {
        models.push(weights_of(model, weights, verify, out)?);
    }
    let problem_with = |model: &Model, listed: &Value| -> Option<Error> {
        let folder = model.folder(weights);
        model.files.iter().zip(listed["files"].as_array()?).find_map(|(file, found)| match found["bytes_found"].as_u64() {
            None => Some(Error::WeightsMissing { model: model.name, file: folder.join(file.local) }),
            Some(found) if found != file.bytes => Some(Error::WeightsWrongSize { file: folder.join(file.local), expected: file.bytes, found }),
            Some(_) if found["verified"] == false => Some(Error::HashMismatch { file: file.local.to_string(), expected: file.sha256, found: "another value".into() }),
            Some(_) => None,
        })
    };
    let steps = json!({
        "picture": step_of(Step::Picture, &device_for_steps, problem_with(&SD15, &models[1])),
        "mesh": step_of(Step::Mesh, &device_for_steps, problem_with(&TRIPOSR, &models[0])),
    });
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "built_with": device::built_with().iter().map(|k| k.name()).collect::<Vec<_>>(),
        "device": device_json,
        "device_problem": device_problem,
        "weights": { "folder": weights, "models": models },
        "steps": steps,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::Scratch;
    use crate::protocol::tests::kept;

    #[test]
    fn a_step_says_why_it_cannot_run() {
        let roomy = Ok((Kind::Vulkan, Some(Memory { card: "card".into(), total_mib: 8188, free_mib: 6800 })));
        assert_eq!(step_of(Step::Mesh, &roomy, None), json!({ "can_run": true, "needs_mib": device::needs_mib(Step::Mesh, Kind::Vulkan), "reason": null }));

        let cramped = Ok((Kind::Vulkan, Some(Memory { card: "card".into(), total_mib: 8188, free_mib: 2150 })));
        let step = step_of(Step::Mesh, &cramped, None);
        assert_eq!(step["can_run"], false);
        assert!(step["reason"].as_str().unwrap().starts_with("making a mesh needs about "), "{step}");
        assert!(step["reason"].as_str().unwrap().contains("of free graphics memory and there is 2.1 GB"), "{step}");

        // weights that are missing are said before memory that is short: fetching comes first
        let missing = Error::WeightsMissing { model: "triposr", file: "w/triposr/model.safetensors".into() };
        let step = step_of(Step::Mesh, &cramped, Some(missing));
        assert!(step["reason"].as_str().unwrap().contains("fetch them with `brickloft-generate fetch triposr`"), "{step}");

        let none = Err(Error::NoDevice { asked: "any", why: "vulkan did not start (no driver)".into() });
        let step = step_of(Step::Picture, &none, None);
        assert_eq!(step, json!({ "can_run": false, "needs_mib": null, "reason": "no graphics device could be used (any): vulkan did not start (no driver)" }));

        // memory that could not be measured does not stop a step
        assert_eq!(step_of(Step::Picture, &Ok((Kind::Vulkan, None)), None)["can_run"], true);
    }

    #[test]
    fn weights_are_listed_as_they_are_on_the_disk() {
        let scratch = Scratch::new("check-weights");
        let (out, _) = kept();
        let about = weights_of(&TRIPOSR, &scratch.0, false, &out).unwrap();
        assert_eq!(about["present"], false);
        assert_eq!(about["licence"], "MIT");
        assert_eq!(about["files"].as_array().unwrap().len(), TRIPOSR.files.len());
        assert_eq!(about["files"][0], json!({ "file": "model.safetensors", "bytes": 1_677_170_936u64, "bytes_found": null, "right_length": false, "verified": null }));

        // a file of the wrong length is there and is not right
        std::fs::create_dir_all(scratch.0.join("triposr")).unwrap();
        std::fs::write(scratch.0.join("triposr").join("manifest.json"), b"{}").unwrap();
        let about = weights_of(&TRIPOSR, &scratch.0, false, &out).unwrap();
        assert_eq!(about["files"][2], json!({ "file": "manifest.json", "bytes": 667, "bytes_found": 2, "right_length": false, "verified": null }));

        // the right length and the wrong content: only `--verify` can tell
        std::fs::write(scratch.0.join("triposr").join("manifest.json"), vec![b' '; 667]).unwrap();
        assert_eq!(weights_of(&TRIPOSR, &scratch.0, false, &out).unwrap()["files"][2]["right_length"], true);
        assert_eq!(weights_of(&TRIPOSR, &scratch.0, true, &out).unwrap()["files"][2]["verified"], false);

        let sd = weights_of(&SD15, &scratch.0, false, &out).unwrap();
        assert_eq!(sd["licence"], "CreativeML OpenRAIL-M");
        assert!(sd["restrictions"].as_str().unwrap().starts_with("You agree not to use the Model or Derivatives of the Model:"));
    }
}
