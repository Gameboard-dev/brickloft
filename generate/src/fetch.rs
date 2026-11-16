//! The one thing this program ever asks of the network: a model's weights,
//! when a person asks for them.
//!
//! What is fetched is fixed in `models`: a file's address is made from the
//! repository, commit and path written there, and the bytes that arrive are
//! kept only if their length and SHA-256 are the ones written there. Nothing
//! about the request comes from an argument but which model. The requests
//! carry a client name (`brickloft-generate`) and, when a download is being
//! carried on, the byte to start from; no token, no cookie, nothing about the
//! computer or the person. Lux3D has a downloader of its own
//! (`lux3d_core::weights`), which this program never reaches: it takes
//! whatever is newest in a repository, sends `HF_TOKEN` from the environment
//! if there is one, and starts again from nothing when it is interrupted.
//!
//! A download is written to `<name>.part` beside where the file will be,
//! and carried on from there if it is found again: the weights are 3.8 GB
//! and a connection that drops at 3 should not cost 3. The part-file is
//! hashed from its first byte whether it was written by this run or an
//! earlier one, so a part-file that was damaged or swapped while it lay
//! there is refused like any other wrong file. A file gets its name only
//! after its hash has matched.
//!
//! Hugging Face answers a request for a large file with a redirect to the
//! server that holds it. Redirects are followed here by hand and not by the
//! HTTP library, so that each address can be checked before it is asked:
//! HTTPS, and a host of Hugging Face's.
//! https://huggingface.co/docs/hub/en/models-downloading

use crate::error::{Error, Result};
use crate::files;
use crate::models::{File, Model, LICENCE_FILE};
use crate::protocol::Out;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;

/// What a server said to a request for a file from a given byte on.
pub enum Answer {
    /// The bytes from `start` on. `start` is the byte asked for, or 0 if the
    /// server sent the whole file instead.
    Body { start: u64, bytes: Box<dyn Read> },
    /// The server will not send from that byte (the file is not that long):
    /// what is in hand is not a beginning of this file.
    CannotResume,
}

/// Where files come from. The real one is `Https`; tests have others.
pub trait Source {
    fn open(&mut self, file: &str, url: &str, from: u64) -> Result<Answer>;
}

/// Whether an address is one this program downloads from: HTTPS, at
/// huggingface.co or one of the hosts its files are served from
/// (`*.hf.co`, `*.huggingface.co`).
pub fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // a name and a password before an @, or a port after a colon, are not part of any address Hugging Face sends
    if authority.is_empty() || authority.contains(['@', ':']) {
        return false;
    }
    let host = authority.to_ascii_lowercase();
    host == "huggingface.co" || host.ends_with(".huggingface.co") || host.ends_with(".hf.co")
}

pub struct Https {
    agent: ureq::Agent,
}

impl Https {
    pub fn new() -> Https {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            // redirects are followed below, one checked address at a time
            .max_redirects(0)
            .max_redirects_will_error(false)
            // a status that is not success is this module's to put into words
            .http_status_as_error(false)
            .user_agent("brickloft-generate")
            // The operating system decides whether the server's certificate is good (see Cargo.toml for why).
            .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
            .timeout_connect(Some(std::time::Duration::from_secs(30)))
            .timeout_recv_response(Some(std::time::Duration::from_secs(60)))
            .build();
        Https { agent: config.into() }
    }
}

impl Default for Https {
    fn default() -> Self {
        Https::new()
    }
}

/// The address a redirect leads to: as given if it is whole, or on the same
/// host if it is a path.
fn redirected(from: &str, location: &str) -> String {
    if location.starts_with('/') && !location.starts_with("//") {
        let end = from["https://".len()..].find('/').map_or(from.len(), |i| i + "https://".len());
        format!("{}{location}", &from[..end])
    } else {
        location.to_string()
    }
}

impl Source for Https {
    fn open(&mut self, file: &str, url: &str, from: u64) -> Result<Answer> {
        let mut url = url.to_string();
        // Hugging Face redirects once or twice; more than a few is a loop
        for _ in 0..6 {
            if !allowed(&url) {
                return Err(Error::RedirectRefused { file: file.to_string(), to: url.chars().take(200).collect() });
            }
            let mut request = self.agent.get(&url);
            if from > 0 {
                request = request.header("Range", format!("bytes={from}-"));
            }
            let response = request.call().map_err(|e| Error::Network { file: file.to_string(), why: e.to_string() })?;
            let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
            match response.status().as_u16() {
                200 => return Ok(Answer::Body { start: 0, bytes: Box::new(response.into_body().into_reader()) }),
                206 => {
                    // "bytes 100-999/1000": only the first number matters here, and it must be the byte asked for
                    let start = header("content-range").and_then(|r| r.strip_prefix("bytes ")?.split('-').next()?.parse::<u64>().ok());
                    if start != Some(from) {
                        return Err(Error::Download { file: file.to_string(), why: "the server sent a different part of the file from the one asked for".into() });
                    }
                    return Ok(Answer::Body { start: from, bytes: Box::new(response.into_body().into_reader()) });
                }
                301 | 302 | 303 | 307 | 308 => {
                    let location = header("location").ok_or_else(|| Error::Download { file: file.to_string(), why: "the server sent this program elsewhere without saying where".into() })?;
                    url = redirected(&url, &location);
                }
                416 => return Ok(Answer::CannotResume),
                404 => return Err(Error::Download { file: file.to_string(), why: "it is no longer at its address (the server answered 404). This program needs updating".into() }),
                status => return Err(Error::Download { file: file.to_string(), why: format!("the server answered {status}") }),
            }
        }
        Err(Error::Download { file: file.to_string(), why: "the server kept sending this program elsewhere".into() })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 of a file, read in pieces, with `each` told of every piece's length.
pub fn sha256_of(path: &Path, mut each: impl FnMut(u64)) -> Result<String> {
    let io = |source| Error::Io { doing: "read", path: path.to_path_buf(), source };
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut piece = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut piece).map_err(io)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&piece[..n]);
        each(n as u64);
    }
}

/// Whether the file that is there is the one that is wanted.
pub fn is_whole(path: &Path, file: &File, each: impl FnMut(u64)) -> Result<bool> {
    Ok(files::length(path) == Some(file.bytes) && sha256_of(path, each)? == file.sha256)
}

/// Progress is said no more often than this many bytes apart, so that a
/// fast connection does not flood the editor with lines.
const SAY_EVERY: u64 = 8 << 20;

/// One file: downloaded, or carried on, into its part-file; checked; named.
fn fetch_file(model: &Model, file: &File, folder: &Path, source: &mut dyn Source, out: &Out, done_before: u64, total: u64) -> Result<()> {
    let whole = folder.join(file.local);
    // Not `files::Part`: that one is removed when a run is cancelled, and this one is what the next run carries on from.
    let part = folder.join(format!("{}.part", file.local));
    let io = |doing: &'static str| move |source| Error::Io { doing, path: folder.join(format!("{}.part", file.local)), source };
    let say = |stage: &str, done: u64| out.progress(stage, done_before + done, total, json!({ "file": file.local }));

    for attempt in 0..2 {
        let mut have = files::length(&part).unwrap_or(0);
        if have > file.bytes {
            // longer than the file can be: not a beginning of it
            std::fs::remove_file(&part).map_err(io("remove"))?;
            have = 0;
        }
        // The hash starts with what is already in hand, read back from the disk.
        let mut hasher = Sha256::new();
        if have > 0 {
            let mut said = 0;
            let mut prefix = std::fs::File::open(&part).map_err(io("read"))?;
            let mut piece = vec![0u8; 1 << 20];
            let mut read = 0u64;
            while read < have {
                let n = prefix.read(&mut piece).map_err(io("read"))?;
                if n == 0 {
                    break;
                }
                hasher.update(&piece[..n]);
                read += n as u64;
                if read - said >= SAY_EVERY * 8 {
                    said = read;
                    say("verify", read);
                }
            }
            have = read;
        }

        if have < file.bytes {
            let (start, mut bytes) = match source.open(file.local, &model.url(file), have)? {
                Answer::Body { start, bytes } => (start, bytes),
                Answer::CannotResume if attempt == 0 => {
                    std::fs::remove_file(&part).map_err(io("remove"))?;
                    continue;
                }
                Answer::CannotResume => return Err(Error::Download { file: file.local.to_string(), why: "the server would not send it".into() }),
            };
            if start != have {
                // The server sent the file from its beginning: what was in hand is dropped.
                have = 0;
                hasher = Sha256::new();
            }
            let mut to = std::fs::OpenOptions::new().create(true).write(true).append(have > 0).truncate(have == 0).open(&part).map_err(io("write"))?;
            let mut piece = vec![0u8; 1 << 16];
            let mut said = have;
            say("download", have);
            loop {
                let n = bytes.read(&mut piece).map_err(|e| Error::Network { file: file.local.to_string(), why: e.to_string() })?;
                if n == 0 {
                    break;
                }
                if have + n as u64 > file.bytes {
                    // more than the file is: stop before it is written, whatever else the server has to send
                    drop(to);
                    std::fs::remove_file(&part).map_err(io("remove"))?;
                    return Err(Error::Download { file: file.local.to_string(), why: format!("the server sent more than the {} bytes the file has", file.bytes) });
                }
                to.write_all(&piece[..n]).map_err(io("write"))?;
                hasher.update(&piece[..n]);
                have += n as u64;
                if have - said >= SAY_EVERY {
                    said = have;
                    say("download", have);
                }
            }
            to.flush().map_err(io("write"))?;
            say("download", have);
            if have < file.bytes {
                // the part-file stays: the next run carries on from it
                return Err(Error::Network { file: file.local.to_string(), why: format!("the connection ended at {have} of {} bytes", file.bytes) });
            }
        }

        let found = hex(&hasher.finalize());
        if found != file.sha256 {
            std::fs::remove_file(&part).map_err(io("remove"))?;
            return Err(Error::HashMismatch { file: file.local.to_string(), expected: file.sha256, found });
        }
        return std::fs::rename(&part, &whole).map_err(|source| Error::Io { doing: "give its name to", path: whole.clone(), source });
    }
    Err(Error::Download { file: file.local.to_string(), why: "the server would not send it".into() })
}

/// What the editor shows before it offers a download, and what `fetch`
/// says before it starts one.
pub fn licence_of(model: &Model) -> Value {
    json!({
        "model": model.name,
        "title": model.title,
        "page": model.page(),
        "bytes": model.bytes(),
        "licence": model.licence.name,
        "licence_url": model.licence.url,
        "restrictions": model.licence.restrictions.replace("\r\n", "\n").trim_end(),
    })
}

/// Fetches what is missing of a model into `weights`. The licence is said
/// first, and nothing is asked of the network unless it has been accepted.
pub fn fetch(model: &Model, weights: &Path, accept_licence: bool, source: &mut dyn Source, out: &Out) -> Result<Value> {
    out.licence(licence_of(model));
    if !accept_licence {
        return Err(Error::LicenceNotAccepted { model: model.name, licence: model.licence.name });
    }
    let folder = model.folder(weights);
    std::fs::create_dir_all(&folder).map_err(|source| Error::Io { doing: "make the folder", path: folder.clone(), source })?;

    let total = model.bytes();
    let (mut done, mut downloaded, mut listed) = (0u64, 0u64, Vec::new());
    for file in model.files {
        let whole = folder.join(file.local);
        let mut read = 0u64;
        let mut said = 0u64;
        let there = is_whole(&whole, file, |n| {
            read += n;
            if read - said >= SAY_EVERY * 8 {
                said = read;
                out.progress("verify", done + read, total, json!({ "file": file.local }));
            }
        })?;
        if !there {
            fetch_file(model, file, &folder, source, out, done, total)?;
            downloaded += file.bytes;
        }
        done += file.bytes;
        listed.push(json!({ "file": file.local, "bytes": file.bytes, "fetched": !there }));
    }
    if let Some(text) = model.licence.text {
        files::write(&folder.join(LICENCE_FILE), text.as_bytes())?;
    }
    Ok(json!({ "model": model.name, "folder": folder, "bytes": total, "bytes_fetched": downloaded, "files": listed }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::tests::Scratch;
    use crate::models::Licence;
    use crate::protocol::tests::kept;

    /// A server with one file, which can be told to misbehave.
    #[derive(Default)]
    struct Fake {
        content: Vec<u8>,
        /// stop sending after this many bytes of a body
        cut_at: Option<usize>,
        /// send the whole file whatever byte was asked for
        ignores_range: bool,
        /// bytes sent after the file's own
        extra: Vec<u8>,
        asked: Vec<(String, u64)>,
    }

    impl Source for Fake {
        fn open(&mut self, _file: &str, url: &str, from: u64) -> Result<Answer> {
            self.asked.push((url.to_string(), from));
            if from as usize > self.content.len() {
                return Ok(Answer::CannotResume);
            }
            let start = if self.ignores_range { 0 } else { from as usize };
            let mut body = self.content[start..].to_vec();
            body.extend_from_slice(&self.extra);
            if let Some(cut) = self.cut_at.take() {
                body.truncate(cut);
            }
            Ok(Answer::Body { start: start as u64, bytes: Box::new(std::io::Cursor::new(body)) })
        }
    }

    fn leaked(text: String) -> &'static str {
        Box::leak(text.into_boxed_str())
    }

    /// A model of one file with the given content, and that content.
    fn model_of(content: &[u8]) -> &'static Model {
        let file = File { remote: "folder/weights.bin", local: "weights.bin", bytes: content.len() as u64, sha256: leaked(hex(&Sha256::digest(content))) };
        Box::leak(Box::new(Model {
            name: "toy",
            title: "Toy",
            step: "testing",
            repository: "someone/toy",
            revision: "0123456789012345678901234567890123456789",
            files: Box::leak(Box::new([file])),
            licence: Licence { name: "Toy Licence", url: "https://huggingface.co/someone/toy", restrictions: "- not for eating", text: Some("the whole text") },
        }))
    }

    fn content() -> Vec<u8> {
        (0..300_000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect()
    }

    #[test]
    fn the_licence_is_said_and_nothing_is_asked_until_it_is_accepted() {
        let scratch = Scratch::new("fetch-licence");
        let model = model_of(&content());
        let mut server = Fake { content: content(), ..Default::default() };
        let (out, kept) = kept();
        let refused = fetch(model, &scratch.0, false, &mut server, &out);
        assert!(matches!(refused, Err(Error::LicenceNotAccepted { model: "toy", licence: "Toy Licence" })));
        assert!(server.asked.is_empty(), "nothing is asked of the network before the licence is accepted");
        assert!(!scratch.0.join("toy").exists(), "and nothing is written");
        let lines = kept.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], json!({ "event": "licence", "model": "toy", "title": "Toy", "page": "https://huggingface.co/someone/toy", "bytes": 300_000, "licence": "Toy Licence", "licence_url": "https://huggingface.co/someone/toy", "restrictions": "- not for eating" }));
    }

    #[test]
    fn a_file_is_fetched_from_its_pinned_address_and_named_when_whole() {
        let scratch = Scratch::new("fetch-whole");
        let model = model_of(&content());
        let mut server = Fake { content: content(), ..Default::default() };
        let (out, kept) = kept();
        let result = fetch(model, &scratch.0, true, &mut server, &out).unwrap();
        assert_eq!(server.asked, [("https://huggingface.co/someone/toy/resolve/0123456789012345678901234567890123456789/folder/weights.bin".to_string(), 0)]);
        assert_eq!(std::fs::read(scratch.0.join("toy").join("weights.bin")).unwrap(), content());
        assert_eq!(std::fs::read_to_string(scratch.0.join("toy").join(LICENCE_FILE)).unwrap(), "the whole text");
        assert!(!scratch.0.join("toy").join("weights.bin.part").exists());
        assert_eq!(result["bytes_fetched"], 300_000);
        assert_eq!(result["files"][0], json!({ "file": "weights.bin", "bytes": 300_000, "fetched": true }));
        let lines = kept.lines();
        assert_eq!(lines[0]["event"], "licence");
        assert!(lines[1..].iter().all(|l| l["event"] == "progress" && l["of"] == 300_000));
        assert_eq!(lines.last().unwrap()["done"], 300_000);

        // asked again, it finds the file whole and asks the network for nothing
        let mut server = Fake { content: content(), ..Default::default() };
        let result = fetch(model, &scratch.0, true, &mut server, &out).unwrap();
        assert!(server.asked.is_empty());
        assert_eq!(result["bytes_fetched"], 0);
    }

    #[test]
    fn a_file_with_the_wrong_hash_is_refused_and_not_kept() {
        let scratch = Scratch::new("fetch-hash");
        let model = model_of(&content());
        // the right length, one byte different
        let mut wrong = content();
        wrong[150_000] ^= 1;
        let mut server = Fake { content: wrong, ..Default::default() };
        let (out, _) = kept();
        match fetch(model, &scratch.0, true, &mut server, &out) {
            Err(Error::HashMismatch { file, expected, found }) => {
                assert_eq!(file, "weights.bin");
                assert_eq!(expected, model.files[0].sha256);
                assert_ne!(found, expected);
            }
            other => panic!("expected a hash mismatch, got {other:?}"),
        }
        assert_eq!(std::fs::read_dir(scratch.0.join("toy")).unwrap().count(), 0, "neither the file nor its part-file is kept");
    }

    #[test]
    fn a_download_that_was_cut_is_carried_on() {
        let scratch = Scratch::new("fetch-resume");
        let model = model_of(&content());
        let mut server = Fake { content: content(), cut_at: Some(100_000), ..Default::default() };
        let (out, _) = kept();
        assert!(matches!(fetch(model, &scratch.0, true, &mut server, &out), Err(Error::Network { .. })));
        let part = scratch.0.join("toy").join("weights.bin.part");
        assert_eq!(files::length(&part), Some(100_000), "what arrived is kept");
        assert!(!scratch.0.join("toy").join("weights.bin").exists(), "under a name nothing reads");

        fetch(model, &scratch.0, true, &mut server, &out).unwrap();
        assert_eq!(server.asked.last().unwrap().1, 100_000, "the second request starts where the first stopped");
        assert_eq!(std::fs::read(scratch.0.join("toy").join("weights.bin")).unwrap(), content());
    }

    #[test]
    fn a_part_file_that_was_changed_is_caught_by_the_hash() {
        let scratch = Scratch::new("fetch-tampered");
        let model = model_of(&content());
        std::fs::create_dir_all(scratch.0.join("toy")).unwrap();
        let mut beginning = content()[..100_000].to_vec();
        beginning[5] ^= 0x80;
        std::fs::write(scratch.0.join("toy").join("weights.bin.part"), beginning).unwrap();
        let mut server = Fake { content: content(), ..Default::default() };
        let (out, _) = kept();
        assert!(matches!(fetch(model, &scratch.0, true, &mut server, &out), Err(Error::HashMismatch { .. })));
        assert_eq!(std::fs::read_dir(scratch.0.join("toy")).unwrap().count(), 0);
    }

    #[test]
    fn a_server_that_ignores_the_range_or_sends_too_much() {
        let scratch = Scratch::new("fetch-range");
        let model = model_of(&content());
        std::fs::create_dir_all(scratch.0.join("toy")).unwrap();
        // a part-file in hand, and a server that sends the whole file anyway: the file must not come out with its beginning twice
        std::fs::write(scratch.0.join("toy").join("weights.bin.part"), &content()[..50_000]).unwrap();
        let mut server = Fake { content: content(), ignores_range: true, ..Default::default() };
        let (out, _) = kept();
        fetch(model, &scratch.0, true, &mut server, &out).unwrap();
        assert_eq!(std::fs::read(scratch.0.join("toy").join("weights.bin")).unwrap(), content());

        // a part-file longer than the file is no beginning of it
        let scratch = Scratch::new("fetch-long");
        std::fs::create_dir_all(scratch.0.join("toy")).unwrap();
        std::fs::write(scratch.0.join("toy").join("weights.bin.part"), vec![0u8; 400_000]).unwrap();
        let mut server = Fake { content: content(), ..Default::default() };
        fetch(model, &scratch.0, true, &mut server, &out).unwrap();
        assert_eq!(server.asked[0].1, 0);

        // a server that sends more than the file is: stopped, and nothing kept
        let scratch = Scratch::new("fetch-extra");
        let mut server = Fake { content: content(), extra: vec![7u8; 100_000], ..Default::default() };
        assert!(matches!(fetch(model, &scratch.0, true, &mut server, &out), Err(Error::Download { .. })));
        assert_eq!(std::fs::read_dir(scratch.0.join("toy")).unwrap().count(), 0);
    }

    #[test]
    fn only_hugging_faces_own_addresses_are_asked() {
        for good in ["https://huggingface.co/a/b/resolve/c/d", "https://us.aws.cdn.hf.co/x?y=1", "https://cas-bridge.xethub.hf.co/x", "https://cdn-lfs.huggingface.co/x", "https://HuggingFace.co/x"] {
            assert!(allowed(good), "{good}");
        }
        for bad in [
            "http://huggingface.co/a",
            "https://example.com/huggingface.co",
            "https://huggingface.co.example.com/a",
            "https://evilhf.co/a",
            "https://nothuggingface.co/a",
            "https://huggingface.co@example.com/a",
            "https://huggingface.co:8443/a",
            "https://example.com/?x=.hf.co",
            "https://example.com#.hf.co",
            "file:///C:/Windows/win.ini",
            "https:///a",
            "",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
        assert_eq!(redirected("https://huggingface.co/a/b", "/api/resolve-cache/x"), "https://huggingface.co/api/resolve-cache/x");
        assert_eq!(redirected("https://huggingface.co/a/b", "https://us.aws.cdn.hf.co/x"), "https://us.aws.cdn.hf.co/x");
        assert!(!allowed(&redirected("https://huggingface.co/a/b", "//example.com/x")));
    }
}
