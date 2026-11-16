//! The models this program can fetch and run: where each comes from, what
//! its files are, and what its licence says.
//!
//! The list is the whole of what `fetch` will download. A repository, a
//! revision and every file's size and SHA-256 are written here and are never
//! taken from an argument, a file or an answer from the network: what is
//! downloaded is then exactly what was read and tried when this was written
//! (2026-11-16), and a repository that changes afterwards, or one that is
//! taken over, cannot change what a player gets. The revision is a commit, not
//! a branch, for the same reason. Hugging Face serves a file at a commit from
//! `https://huggingface.co/<repository>/resolve/<commit>/<path>`:
//! https://huggingface.co/docs/hub/en/models-downloading
//!
//! Each licence was read on the model's own page that day. Two things were
//! turned down and are not here:
//!
//! - a ready-made `tokenizer.json` for the text encoder. Candle's example
//!   takes it from `openai/clip-vit-base-patch32`, whose page states no
//!   licence. The same vocabulary is in the Stable Diffusion repository as
//!   `tokenizer/vocab.json` and `tokenizer/merges.txt`, under that
//!   repository's licence, and the tokenizer is put together from those (see
//!   `picture::tokenizer`);
//! - Lux3D's ground removers (RMBG 1.4 and 2.0, non-commercial) and Candle's
//!   Segment Anything weights (no licence stated).

use std::path::{Path, PathBuf};

pub struct File {
    /// The file's path in its repository.
    pub remote: &'static str,
    /// Its path in the model's folder under the weights folder.
    pub local: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub struct Licence {
    pub name: &'static str,
    /// Where the licence can be read.
    pub url: &'static str,
    /// What the licence does not allow, in its own words where it has a list
    /// of them. The editor shows this before it offers the download.
    pub restrictions: &'static str,
    /// The whole text, written beside the weights when they are fetched, if
    /// the repository does not carry it as a file of its own.
    pub text: Option<&'static str>,
}

pub struct Model {
    /// The name given to `fetch`, and the folder under the weights folder.
    pub name: &'static str,
    pub title: &'static str,
    /// What it is for, in the words `check` uses for a step.
    pub step: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub files: &'static [File],
    pub licence: Licence,
}

impl Model {
    pub fn folder(&self, weights: &Path) -> PathBuf {
        weights.join(self.name)
    }

    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    pub fn url(&self, file: &File) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repository, self.revision, file.remote)
    }

    /// Where the model's page is, for a person to read.
    pub fn page(&self) -> String {
        format!("https://huggingface.co/{}", self.repository)
    }
}

/// TripoSR (Tripo AI and Stability AI, 2024): one picture in, a mesh out.
/// This is Lux3D's ready-made package of it: the original checkpoint is a
/// Python pickle, which Lux3D converts with a Python script, and this is that
/// conversion published by Lux3D's author. Its page gives the licence as MIT
/// and carries the original's LICENSE file ("Copyright (c) 2024 Tripo AI &
/// Stability AI"). Not gated.
pub const TRIPOSR: Model = Model {
    name: "triposr",
    title: "TripoSR",
    step: "making a mesh from a picture",
    repository: "oxide-lab/TripoSR",
    revision: "69da98edbe5f6c97d6412a1e1e48e863e5e2dc6e",
    files: &[
        File { remote: "model.safetensors", local: "model.safetensors", bytes: 1_677_170_936, sha256: "f72bb520b8b1a5639600ac818496f22d6ccb3b42d3942412bd1e2375ef780a2b" },
        File { remote: "resolved_config.json", local: "resolved_config.json", bytes: 1_164, sha256: "480a3f742ac7f8f89e77e6a96eb8d0cb8564031d0c0f6fd225f9da9ad74a6686" },
        File { remote: "manifest.json", local: "manifest.json", bytes: 667, sha256: "33a9e558fcff782df50c60d66eb43bc29815fb5d6175b5757df76fa190774e34" },
        // Lux3D reads this file when it loads the package and checks the three above against it.
        File { remote: "checksums.json", local: "checksums.json", bytes: 626, sha256: "cc0cac9e69a9a7c1fff6ce11715b7c6b8fd64a6f4745f822522928040f01bc7a" },
        File { remote: "LICENSE", local: "LICENSE", bytes: 1_080, sha256: "ade0a66629bdd7e01e46b3296b3851cff0fd27989bca53da470ad6e96ed620fb" },
    ],
    licence: Licence {
        name: "MIT",
        url: "https://huggingface.co/oxide-lab/TripoSR/blob/69da98edbe5f6c97d6412a1e1e48e863e5e2dc6e/LICENSE",
        restrictions: "None on what it is used for. Copies must keep the copyright notice (Copyright (c) 2024 Tripo AI & Stability AI) and the licence text.",
        text: None,
    },
};

/// Stable Diffusion 1.5 (Rombach and Esser, 2022): a prompt in, a picture
/// out. The half-precision files, which are half the size and what a card
/// with 8 GB can hold. The repository is a mirror of the original
/// `runwayml/stable-diffusion-v1-5`, which its owner took down; its page
/// gives the licence as CreativeML OpenRAIL-M, which allows any use but those
/// it lists and asks that the list be passed on to whoever uses the model.
/// The page also says of the model's intended use that it "is intended for
/// research purposes only"; the licence itself has no such term. Not gated.
pub const SD15: Model = Model {
    name: "sd15",
    title: "Stable Diffusion 1.5",
    step: "making a picture from a prompt",
    repository: "stable-diffusion-v1-5/stable-diffusion-v1-5",
    revision: "451f4fe16113bff5a5d2269ed5ad43b0592e9a14",
    files: &[
        File { remote: "unet/diffusion_pytorch_model.fp16.safetensors", local: "unet.fp16.safetensors", bytes: 1_719_125_304, sha256: "c83908253f9a64d08c25fc90874c9c8aef9a329ce1ca5fb909d73b0c83d1ea21" },
        File { remote: "text_encoder/model.fp16.safetensors", local: "text_encoder.fp16.safetensors", bytes: 246_144_864, sha256: "77795e2023adcf39bc29a884661950380bd093cf0750a966d473d1718dc9ef4e" },
        File { remote: "vae/diffusion_pytorch_model.fp16.safetensors", local: "vae.fp16.safetensors", bytes: 167_335_342, sha256: "4fbcf0ebe55a0984f5a5e00d8c4521d52359af7229bb4d81890039d2aa16dd7c" },
        File { remote: "tokenizer/vocab.json", local: "vocab.json", bytes: 1_059_962, sha256: "e089ad92ba36837a0d31433e555c8f45fe601ab5c221d4f607ded32d9f7a4349" },
        File { remote: "tokenizer/merges.txt", local: "merges.txt", bytes: 524_619, sha256: "9fd691f7c8039210e0fced15865466c65820d09b63988b0174bfe25de299051a" },
    ],
    licence: Licence {
        name: "CreativeML OpenRAIL-M",
        url: "https://huggingface.co/spaces/CompVis/stable-diffusion-license",
        // The licence's Attachment A, word for word. A test checks it against the whole text.
        restrictions: include_str!("../licences/CreativeML-OpenRAIL-M-use-restrictions.txt"),
        text: Some(include_str!("../licences/CreativeML-OpenRAIL-M.txt")),
    },
};

/// The name the licence text is written under, beside the weights.
pub const LICENCE_FILE: &str = "LICENCE.txt";

pub const ALL: [&Model; 2] = [&TRIPOSR, &SD15];

pub fn named(name: &str) -> Option<&'static Model> {
    ALL.into_iter().find(|m| m.name == name)
}

/// The names `fetch` takes, for a message.
pub fn names() -> String {
    ALL.map(|m| m.name).join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_use_restrictions_are_the_licences_own_words() {
        let text = SD15.licence.text.unwrap().replace("\r\n", "\n");
        let restrictions = SD15.licence.restrictions.replace("\r\n", "\n");
        assert!(restrictions.lines().count() >= 12);
        for line in restrictions.lines() {
            assert!(text.contains(line), "not in the licence: {line}");
        }
        // and all of them: the attachment ends the licence
        assert!(text.trim_end().ends_with(restrictions.trim_end()));
    }

    #[test]
    fn every_file_is_pinned() {
        for model in ALL {
            assert_eq!(model.revision.len(), 40, "{} is pinned to a commit", model.name);
            assert!(model.revision.bytes().all(|b| b.is_ascii_hexdigit()));
            for file in model.files {
                assert_eq!(file.sha256.len(), 64, "{}", file.remote);
                assert!(file.sha256.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
                assert!(file.bytes > 0);
                // a local name is a plain file name: nothing a path could be made of
                assert!(!file.local.contains(['/', '\\', ':']) && !file.local.starts_with('.'));
                assert!(model.url(file).starts_with("https://huggingface.co/"));
            }
        }
    }
}
