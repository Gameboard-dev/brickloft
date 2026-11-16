//! A prompt made into pictures, with Stable Diffusion 1.5 on Candle.
//!
//! The network and its sampling loop are Candle's
//! (`candle_transformers::models::stable_diffusion`); this module is the
//! loop around them that Candle keeps in an example program, cut down to the
//! one model and made to answer in this program's protocol.
//!
//! **What is asked for.** A picture is only useful here if TripoSR can make a
//! mesh of it: one whole object, on a plain ground, seen from a little above
//! level. So the prompt is wrapped in words that ask for that (`WRAPPER`),
//! and words for what goes wrong are given as the negative prompt
//! (`NEGATIVE`). Of seven wrappers tried on contact sheets in the trial this
//! one most often gave a whole object on a plain ground. It does not steer
//! the camera reliably, and about half the pictures are usable, which is why
//! several are made and ranked (`rank`).
//!
//! **The same picture from the same seed.** The noise a picture starts from
//! is made here on the processor from the seed, by a generator written out in
//! `noise`, and not by the graphics backend's own: CUDA's and Vulkan's give
//! different numbers for one seed, so a form saved with its prompt and seed
//! would come back as another picture on another backend. The sampler
//! (DDIM, https://arxiv.org/abs/2010.02502) adds no noise of its own. What is
//! left to differ is the arithmetic: two backends, or two cards, round
//! half-precision sums differently, and the pictures can differ in detail.
//! Each candidate has its own seed (`seed`, `seed + 1`, ...) and is made on
//! its own, so the third of four is the same picture as the first of one
//! asked for with that seed.

use crate::cutout;
use crate::device::Chosen;
use crate::error::{Error, Result};
use crate::files::{self, Part};
use crate::models::SD15;
use crate::protocol::Out;
use crate::rank;
use candle_core::{DType, Device, IndexOp, Module, Tensor};
use candle_transformers::models::stable_diffusion::{self, unet_2d::UNet2DConditionModel, vae::AutoEncoderKL, StableDiffusionConfig};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

/// `{}` is where the prompt goes.
pub const WRAPPER: &str = "a miniature scale model of {} on a white background, studio product photo, whole object visible, three-quarter view";
pub const NEGATIVE: &str = "cropped, cut off, out of frame, close-up, scenery, landscape, grass, sky, floor, room, text, watermark, people, multiple objects, blurry";

/// The side of a picture in pixels: what the model was trained at, and what TripoSR takes.
pub const SIDE: usize = 512;
/// How strongly the picture is pushed towards the prompt and away from the
/// negative prompt. 7.5 is the model's usual setting and the trial's.
const GUIDANCE: f64 = 7.5;
/// The scale between the autoencoder's latents and the denoiser's, a constant of the model.
const LATENT_SCALE: f64 = 0.18215;
/// The vocabulary's size, and the two word-pieces that begin and end every prompt.
const VOCABULARY: usize = 49_408;
const START: (&str, u32) = ("<|startoftext|>", 49_406);
const END: (&str, u32) = ("<|endoftext|>", 49_407);

pub fn wrapped(prompt: &str) -> String {
    WRAPPER.replace("{}", prompt)
}

fn failed(what: &'static str) -> impl Fn(candle_core::Error) -> Error {
    move |e| Error::Model { step: what, why: e.to_string() }
}

/// Starting noise for one picture: `count` numbers from the standard normal
/// distribution, the same for the same seed on every computer.
///
/// SplitMix64 (Steele, Lea and Flood 2014; the reference is at
/// https://prng.di.unimi.it/splitmix64.c) makes the uniform numbers and the
/// Box-Muller transform (https://en.wikipedia.org/wiki/Box%E2%80%93Muller_transform)
/// makes them normal. Twenty lines here and not the `rand` crates: `rand`
/// says its standard generator may change between versions, and the method
/// `rand_distr` samples a normal by is likewise its own business, so the
/// same seed could give another picture after an update of a dependency.
/// The sines and logarithms are done in 64 bits, where every platform's
/// library agrees to far more places than the 32-bit result keeps.
pub fn noise(seed: u32, count: usize) -> Vec<f32> {
    let mut state = seed as u64;
    let mut next = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    // 53 random bits as a number in (0, 1]: never 0, whose logarithm is not a number
    let mut uniform = move || ((next() >> 11) + 1) as f64 / (1u64 << 53) as f64;
    let mut out = Vec::with_capacity(count + 1);
    while out.len() < count {
        let (u, v) = (uniform(), uniform());
        let radius = (-2.0 * u.ln()).sqrt();
        let angle = std::f64::consts::TAU * v;
        out.push((radius * angle.cos()) as f32);
        out.push((radius * angle.sin()) as f32);
    }
    out.truncate(count);
    out
}

/// The tokenizer of Stable Diffusion's text encoder, put together from the
/// vocabulary and merge list in the model's own repository.
///
/// The settings are those of the `tokenizer.json` that Candle's example
/// downloads from `openai/clip-vit-base-patch32`, read from that file and
/// written out here, because that repository states no licence and so the
/// file is not fetched (see `models`). The trial used that file; its
/// vocabulary and merges were compared with these and are the same.
pub fn tokenizer(folder: &Path) -> Result<Tokenizer> {
    use tokenizers::models::bpe::{Merges, Vocab, BPE};
    use tokenizers::normalizers::{replace::Replace, unicode::NFC, utils::Lowercase, utils::Sequence as Normalizers};
    use tokenizers::pre_tokenizers::{byte_level::ByteLevel, sequence::Sequence as PreTokenizers, split::{Split, SplitPattern}};
    use tokenizers::processors::roberta::RobertaProcessing;
    use tokenizers::{AddedToken, SplitDelimiterBehavior};

    let file = |local: &str| SD15.files.iter().find(|f| f.local == local).map(|f| (folder.join(f.local), f.bytes)).ok_or(Error::Model { step: "reading the vocabulary", why: format!("{local} is not one of the model's files") });
    let (vocab_path, vocab_bytes) = file("vocab.json")?;
    let (merges_path, merges_bytes) = file("merges.txt")?;
    let bad = |path: &Path, why: &str| Error::Model { step: "reading the vocabulary", why: format!("{} is not what was fetched ({why}). Delete it and fetch the weights again", path.display()) };

    // Both files are read here, with their sizes bounded, and handed over as data: the library's own
    // reader takes a path and reads whatever is there.
    let vocab: std::collections::HashMap<String, u32> = serde_json::from_slice(&files::read_small(&vocab_path, vocab_bytes)?).map_err(|e| bad(&vocab_path, &e.to_string()))?;
    if vocab.len() != VOCABULARY || vocab.values().any(|&id| id as usize >= VOCABULARY) || vocab.get(START.0) != Some(&START.1) || vocab.get(END.0) != Some(&END.1) {
        return Err(bad(&vocab_path, "it is not the vocabulary of 49 408 word-pieces"));
    }
    let merges_text = String::from_utf8(files::read_small(&merges_path, merges_bytes)?).map_err(|e| bad(&merges_path, &e.to_string()))?;
    // the first line is a comment; each other line is a pair of pieces that join
    let merges: Merges = merges_text.lines().skip(1).filter(|l| !l.is_empty()).map(|line| line.split_once(' ').map(|(a, b)| (a.to_string(), b.to_string())).ok_or_else(|| bad(&merges_path, "a line is not a pair"))).collect::<Result<_>>()?;

    let library = |e: tokenizers::Error| Error::Model { step: "reading the vocabulary", why: e.to_string() };
    let model = BPE::builder().vocab_and_merges(vocab.into_iter().collect::<Vocab>(), merges).unk_token(END.0.into()).end_of_word_suffix("</w>".into()).build().map_err(library)?;
    let mut tokenizer = Tokenizer::new(model);
    tokenizer.with_normalizer(Some(Normalizers::new(vec![NFC.into(), Replace::new(tokenizers::normalizers::replace::ReplacePattern::Regex(r"\s+".into()), " ").map_err(library)?.into(), Lowercase.into()]))).map_err(library)?;
    let split = Split::new(SplitPattern::Regex(r"<\|startoftext\|>|<\|endoftext\|>|'s|'t|'re|'ve|'m|'ll|'d|[\p{L}]+|[\p{N}]|[^\s\p{L}\p{N}]+".into()), SplitDelimiterBehavior::Removed, true).map_err(library)?;
    tokenizer.with_pre_tokenizer(Some(PreTokenizers::new(vec![split.into(), ByteLevel::new(false, true, true).into()])));
    tokenizer.with_post_processor(Some(RobertaProcessing::new((END.0.into(), END.1), (START.0.into(), START.1)).trim_offsets(false).add_prefix_space(false)));
    tokenizer.add_special_tokens([AddedToken::from(START.0, true).normalized(true), AddedToken::from(END.0, true).normalized(false)]).map_err(library)?;
    Ok(tokenizer)
}

/// A prompt as the numbers the text encoder reads: its word-pieces between
/// the start and end marks, filled out to `length` with the end mark.
pub fn pieces(tokenizer: &Tokenizer, text: &str, length: usize) -> Result<Vec<u32>> {
    let mut ids = tokenizer.encode(text, true).map_err(|e| Error::Model { step: "reading the prompt", why: e.to_string() })?.get_ids().to_vec();
    if ids.len() > length {
        return Err(Error::PromptTooManyPieces { pieces: ids.len(), most: length });
    }
    ids.resize(length, END.1);
    Ok(ids)
}

/// The model's files, each checked to be there and the length it should be.
/// The SHA-256 was checked when they were fetched; `check --verify` checks it again.
pub fn weights_in(weights: &Path) -> Result<PathBuf> {
    let folder = SD15.folder(weights);
    for file in SD15.files {
        let path = folder.join(file.local);
        match files::length(&path) {
            None => return Err(Error::WeightsMissing { model: SD15.name, file: path }),
            Some(found) if found != file.bytes => return Err(Error::WeightsWrongSize { file: path, expected: file.bytes, found }),
            Some(_) => {}
        }
    }
    Ok(folder)
}

/// The model, loaded: what `serve` keeps between requests.
pub struct Painter {
    device: Device,
    dtype: DType,
    config: StableDiffusionConfig,
    tokenizer: Tokenizer,
    folder: PathBuf,
    vae: AutoEncoderKL,
    unet: UNet2DConditionModel,
}

impl Painter {
    pub fn load(weights: &Path, chosen: &Chosen, out: &Out) -> Result<Painter> {
        let folder = weights_in(weights)?;
        let device = chosen.device.clone();
        // Half precision: the weights are stored that way, and at full precision the denoiser alone is 3.4 GB on the card.
        let dtype = DType::F16;
        let config = StableDiffusionConfig::v1_5(None, Some(SIDE), Some(SIDE));
        out.progress("load", 0, 3, json!({ "what": "vocabulary" }));
        let tokenizer = tokenizer(&folder)?;
        out.progress("load", 1, 3, json!({ "what": "autoencoder" }));
        let vae = config.build_vae(folder.join("vae.fp16.safetensors"), &device, dtype).map_err(failed("loading the autoencoder"))?;
        out.progress("load", 2, 3, json!({ "what": "denoiser" }));
        let unet = config.build_unet(folder.join("unet.fp16.safetensors"), &device, 4, false, dtype).map_err(failed("loading the denoiser"))?;
        out.progress("load", 3, 3, json!({}));
        Ok(Painter { device, dtype, config, tokenizer, folder, vae, unet })
    }

    /// The prompt and the negative prompt as the denoiser reads them. The
    /// text encoder is loaded for this and let go again: it is half a
    /// gigabyte on the card that the denoising has better use for.
    fn embedded(&self, prompt: &str) -> Result<Tensor> {
        let step = "reading the prompt";
        let length = self.config.clip.max_position_embeddings;
        let wanted = pieces(&self.tokenizer, &wrapped(prompt), length)?;
        let unwanted = pieces(&self.tokenizer, NEGATIVE, length)?;
        let encoder = stable_diffusion::build_clip_transformer(&self.config.clip, self.folder.join("text_encoder.fp16.safetensors"), &self.device, DType::F32).map_err(failed("loading the text encoder"))?;
        let embed = |ids: &[u32]| -> Result<Tensor> {
            let ids = Tensor::new(ids, &self.device).and_then(|t| t.unsqueeze(0)).map_err(failed(step))?;
            encoder.forward(&ids).map_err(failed(step))
        };
        // the negative prompt first: the denoiser's two answers are told apart by their order
        Tensor::cat(&[embed(&unwanted)?, embed(&wanted)?], 0).and_then(|t| t.to_dtype(self.dtype)).map_err(failed(step))
    }

    /// One picture from one seed, as rows of red, green and blue bytes.
    fn painted(&self, embedded: &Tensor, seed: u32, steps: usize, each_step: &mut dyn FnMut(usize)) -> Result<image::RgbImage> {
        let step = "making the picture";
        let mut scheduler = self.config.build_scheduler(steps).map_err(failed(step))?;
        let shape = (1, 4, SIDE / 8, SIDE / 8);
        let start = Tensor::from_vec(noise(seed, 4 * (SIDE / 8) * (SIDE / 8)), shape, &Device::Cpu).and_then(|t| t.to_device(&self.device)).map_err(failed(step))?;
        let mut latents = (start * scheduler.init_noise_sigma()).and_then(|t| t.to_dtype(self.dtype)).map_err(failed(step))?;
        for (index, &timestep) in scheduler.timesteps().to_vec().iter().enumerate() {
            latents = (|| -> candle_core::Result<Tensor> {
                // Twice over: once for the negative prompt and once for the prompt. One after the other and not both in
                // one pass, as Candle's example does: together they took 5.4 GB of the card at their most and apart
                // 3.9 GB, for half a second more in ten (measured with CUDA on an RTX 4060 Ti).
                let input = scheduler.scale_model_input(latents.clone(), timestep)?;
                let without = self.unet.forward(&input, timestep as f64, &embedded.narrow(0, 0, 1)?)?;
                let with = self.unet.forward(&input, timestep as f64, &embedded.narrow(0, 1, 1)?)?;
                let guided = (&without + ((&with - &without)? * GUIDANCE)?)?;
                scheduler.step(&guided, timestep, &latents)
            })()
            .map_err(failed(step))?;
            each_step(index + 1);
        }
        let pixels = self.decoded(&latents).map_err(failed(step))?;
        image::RgbImage::from_raw(SIDE as u32, SIDE as u32, pixels).ok_or(Error::Model { step, why: "the picture came out the wrong size".into() })
    }

    /// The picture the latents stand for, as rows of red, green and blue bytes.
    ///
    /// Decoded in four overlapping tiles and not in one pass. The autoencoder's last layers work at the picture's
    /// full size with over a hundred channels, and decoding all of it at once was the most graphics memory the
    /// whole program took. A tile is 40 latents a side of the 64, so a pass holds two fifths as much. Where tiles
    /// overlap (16 latents, 128 pixels) each fades into the next, so that the little a tile gets wrong at its own
    /// edge, where it cannot see what lies beyond, is given little weight.
    fn decoded(&self, latents: &Tensor) -> candle_core::Result<Vec<u8>> {
        const LATENTS: usize = SIDE / 8;
        const TILE: usize = 40;
        const STEP: usize = LATENTS - TILE;
        // how much a tile's pixel counts along one axis: all of it away from the overlap, fading across it
        let weight = |origin: usize, at: usize| -> f32 {
            let (start, end) = (STEP * 8, TILE * 8);
            if origin == 0 {
                if at < start { 1.0 } else { (end - at) as f32 / (end - start) as f32 }
            } else if at >= end - start {
                1.0
            } else {
                (at + 1) as f32 / (end - start) as f32
            }
        };
        let mut sum = vec![0f32; SIDE * SIDE * 3];
        let mut weights = vec![0f32; SIDE * SIDE];
        for ty in [0, STEP] {
            for tx in [0, STEP] {
                let tile = latents.narrow(2, ty, TILE)?.narrow(3, tx, TILE)?;
                let decoded = self.vae.decode(&(tile / LATENT_SCALE)?)?;
                // from -1..1 to 0..1, and from channels-first to rows of pixels
                let values = ((decoded / 2.0)? + 0.5)?.to_device(&Device::Cpu)?.to_dtype(DType::F32)?.i(0)?.permute((1, 2, 0))?.flatten_all()?.to_vec1::<f32>()?;
                let side = TILE * 8;
                for y in 0..side {
                    for x in 0..side {
                        let w = weight(ty, y) * weight(tx, x);
                        let to = (ty * 8 + y) * SIDE + tx * 8 + x;
                        weights[to] += w;
                        for k in 0..3 {
                            sum[to * 3 + k] += w * values[(y * side + x) * 3 + k];
                        }
                    }
                }
            }
        }
        Ok(sum.iter().enumerate().map(|(i, v)| (v / weights[i / 3].max(1e-6) * 255.0).clamp(0.0, 255.0) as u8).collect())
    }

    /// `count` pictures of the prompt into `folder`, judged and ranked.
    pub fn candidates(&self, prompt: &str, seed: u32, count: u32, steps: u32, folder: &Path, out: &Out) -> Result<Value> {
        let embedded = self.embedded(prompt)?;
        let mut candidates = Vec::new();
        for index in 0..count {
            let seed = seed.wrapping_add(index);
            let total = (count * steps) as u64;
            let picture = self.painted(&embedded, seed, steps as usize, &mut |done| out.progress("denoise", (index * steps) as u64 + done as u64, total, json!({ "candidate": index, "seed": seed })))?;
            // Named by its seed: the same request writes the same files, and nothing of the prompt is in a file's name.
            let path = folder.join(format!("candidate-{seed}.png"));
            let part = Part::of(&path)?;
            picture.save_with_format(part.path(), image::ImageFormat::Png).map_err(|e| Error::Model { step: "writing the picture", why: e.to_string() })?;
            part.keep()?;
            let rgba = image::DynamicImage::ImageRgb8(picture).into_rgba8();
            let judged = rank::judge(cutout::object_in(&rgba).map(|(_, report)| report).as_ref());
            candidates.push((judged, json!({ "file": path, "seed": seed })));
        }
        Ok(json!({
            "prompt": prompt,
            "prompt_sent": wrapped(prompt),
            "negative_prompt": NEGATIVE,
            "steps": steps,
            "width": SIDE,
            "height": SIDE,
            "candidates": rank::ranked(candidates),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_noise_and_it_is_normal() {
        // The first numbers from seed 7, worked out once by this code and once by the same arithmetic written
        // separately: if these change, every saved seed gives another picture.
        let first = noise(7, 6);
        let expected = [1.3649923f32, 0.14452122, -0.39652398, -0.22759631, 0.004498526, 1.259433];
        for (found, expected) in first.iter().zip(expected) {
            assert!((found - expected).abs() < 1e-5, "{first:?}");
        }
        assert_eq!(noise(7, 1000), noise(7, 1000));
        assert_ne!(noise(7, 16), noise(8, 16));
        assert_eq!(noise(7, 5).len(), 5, "an odd count is honoured");
        assert_eq!(noise(7, 5), noise(7, 6)[..5]);

        let many = noise(1, 16_384);
        let mean = many.iter().sum::<f32>() / many.len() as f32;
        let spread = (many.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / many.len() as f32).sqrt();
        assert!(mean.abs() < 0.03 && (spread - 1.0).abs() < 0.03, "mean {mean}, spread {spread}");
        assert!(many.iter().all(|v| v.is_finite() && v.abs() < 9.0));
    }

    #[test]
    fn the_prompt_is_put_into_the_wrapper_once() {
        assert_eq!(wrapped("an oak barrel"), "a miniature scale model of an oak barrel on a white background, studio product photo, whole object visible, three-quarter view");
        // a prompt with the wrapper's own mark in it is text like any other
        assert_eq!(wrapped("a {} thing").matches("{}").count(), 1);
    }
}
