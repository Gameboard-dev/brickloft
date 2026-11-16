//! The graphics device the models run on, and how much memory it has.
//!
//! Which interfaces there are is decided when the program is built (the
//! `vulkan` and `cuda` features); which is used is decided here when it
//! runs. There is no running on the processor alone: a picture would take
//! minutes, and Candle's processor backend cannot be given a seed.
//!
//! Memory is asked of `nvidia-smi`, the program NVIDIA's driver installs.
//! Neither backend of Candle tells how much memory a card has free, and the
//! system interfaces that do (DXGI on Windows, a Vulkan extension) are
//! reached through `unsafe` calls, which this program does not make. So on a
//! card that is not NVIDIA's the memory is not known, and `check` says so
//! instead of guessing. `nvidia-smi` is started from the system's own
//! folder by its full path with a fixed list of arguments, and what it
//! prints is read as untrusted text: four fields, the numbers bounded.
//! https://docs.nvidia.com/deploy/nvidia-smi/index.html

use crate::args::Want;
use crate::error::{Error, Result};
use candle_core::Device;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cuda,
    Vulkan,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Cuda => "cuda",
            Kind::Vulkan => "vulkan",
        }
    }
}

pub struct Chosen {
    pub device: Device,
    pub kind: Kind,
    /// The card's name as the device gives it, if it does.
    pub name: Option<String>,
}

/// The interfaces this program was built with, in the order they are tried.
pub fn built_with() -> Vec<Kind> {
    let mut kinds = Vec::new();
    if cfg!(feature = "cuda") {
        kinds.push(Kind::Cuda);
    }
    if cfg!(feature = "vulkan") {
        kinds.push(Kind::Vulkan);
    }
    kinds
}

/// What a panic carried, as text.
pub fn panic_text(panic: Box<dyn std::any::Any + Send>) -> String {
    match panic.downcast::<String>() {
        Ok(text) => *text,
        Err(panic) => panic.downcast::<&'static str>().map_or_else(|_| "it stopped without saying why".to_string(), |text| text.to_string()),
    }
}

fn open(kind: Kind) -> std::result::Result<Chosen, String> {
    // The CUDA backend looks for NVIDIA's libraries when it starts and panics if they are not there;
    // that is a computer without CUDA, which is an answer and not a crash.
    let opened = std::panic::catch_unwind(|| match kind {
        Kind::Cuda => Device::new_cuda(0),
        Kind::Vulkan => Device::new_vulkan(0),
    });
    let device = match opened {
        Ok(Ok(device)) => device,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(panic) => return Err(panic_text(panic)),
    };
    #[cfg(feature = "vulkan")]
    let name = device.as_vulkan_device().ok().map(|d| d.physical_device_name().to_string());
    #[cfg(not(feature = "vulkan"))]
    let name = None;
    Ok(Chosen { device, kind, name })
}

/// Opens the device asked for, or the first that opens.
pub fn choose(want: Want) -> Result<Chosen> {
    let (asked, kinds) = match want {
        Want::Auto => ("any", built_with()),
        Want::Cuda => ("cuda", vec![Kind::Cuda]),
        Want::Vulkan => ("vulkan", vec![Kind::Vulkan]),
    };
    let mut problems = Vec::new();
    for kind in kinds {
        if !built_with().contains(&kind) {
            problems.push(format!("this program was built without {}", kind.name()));
            continue;
        }
        match open(kind) {
            Ok(chosen) => return Ok(chosen),
            Err(why) => problems.push(format!("{} did not start ({})", kind.name(), why.lines().next().unwrap_or("").chars().take(300).collect::<String>())),
        }
    }
    if problems.is_empty() {
        problems.push("this program was built without a graphics backend".into());
    }
    Err(Error::NoDevice { asked, why: problems.join("; ") })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub card: String,
    pub total_mib: u64,
    pub free_mib: u64,
}

/// No card has more than this, in MiB; a larger number is not a size.
const MOST_MIB: u64 = 1 << 20;

/// Reads what `nvidia-smi --query-gpu=name,memory.total,memory.free
/// --format=csv,noheader,nounits` prints: a line for each card.
pub fn read_cards(text: &str) -> Vec<Memory> {
    text.lines().take(16).filter_map(|line| {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let [card, total, free] = fields[..] else { return None };
        let (total_mib, free_mib) = (total.parse::<u64>().ok()?, free.parse::<u64>().ok()?);
        (total_mib <= MOST_MIB && free_mib <= total_mib && !card.is_empty() && card.len() <= 100).then(|| Memory { card: card.to_string(), total_mib, free_mib })
    }).collect()
}

/// Where the driver puts `nvidia-smi`: the system folder on Windows, found
/// from the system's own variable, and `/usr/bin` elsewhere. Never looked
/// for on PATH, where a folder a person can write to may come first.
fn nvidia_smi() -> Option<PathBuf> {
    let path = if cfg!(windows) { PathBuf::from(std::env::var_os("SystemRoot")?).join("System32").join("nvidia-smi.exe") } else { PathBuf::from("/usr/bin/nvidia-smi") };
    (path.is_absolute() && path.is_file()).then_some(path)
}

/// The memory of the card named `name`, or of the first card if none has
/// that name. `None` if it cannot be asked.
pub fn memory(name: Option<&str>) -> Option<Memory> {
    let mut child = std::process::Command::new(nvidia_smi()?)
        .args(["--query-gpu=name,memory.total,memory.free", "--format=csv,noheader,nounits"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    // It answers in a tenth of a second, and has been known to hang when a driver is being replaced.
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(5) => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut text = String::new();
    use std::io::Read;
    child.stdout.take()?.take(4096).read_to_string(&mut text).ok()?;
    let cards = read_cards(&text);
    cards.iter().find(|c| Some(c.card.as_str()) == name).or(cards.first()).cloned()
}

/// A step that needs the graphics card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Picture,
    Mesh,
}

impl Step {
    pub fn words(self) -> &'static str {
        match self {
            Step::Picture => "making a picture",
            Step::Mesh => "making a mesh",
        }
    }
}

/// The graphics memory a step takes at its most, in MiB, rounded up from
/// what was measured on an RTX 4060 Ti (docs/generate.md has the figures).
pub fn needs_mib(step: Step, kind: Kind) -> u64 {
    match (step, kind) {
        (Step::Picture, Kind::Cuda) => 3600,
        (Step::Picture, Kind::Vulkan) => 3600,
        (Step::Mesh, Kind::Cuda) => 3800,
        (Step::Mesh, Kind::Vulkan) => 3800,
    }
}

/// Whether there is the memory for a step: an error that says how much is
/// short, or nothing. Memory that could not be measured is not a refusal.
pub fn enough(step: Step, kind: Kind, memory: Option<&Memory>) -> Result<()> {
    let needs_mib = needs_mib(step, kind);
    match memory {
        Some(memory) if memory.free_mib < needs_mib => Err(Error::NotEnoughMemory { step: step.words(), needs_mib, free_mib: memory.free_mib }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_nvidia_smi_prints_is_read_with_suspicion() {
        assert_eq!(read_cards("NVIDIA GeForce RTX 4060 Ti, 8188, 6828\r\n"), [Memory { card: "NVIDIA GeForce RTX 4060 Ti".into(), total_mib: 8188, free_mib: 6828 }]);
        assert_eq!(read_cards("A, 100, 50\nB, 200, 150\n").len(), 2);
        for nonsense in ["", "No devices were found", "card, many, few", "card, 8188", "card, 8188, 9000", "card, 99999999999, 5", ", 8188, 6828", "card, -1, 0", "a, 1, 1, 1"] {
            assert!(read_cards(nonsense).is_empty(), "{nonsense}");
        }
    }

    #[test]
    fn too_little_memory_is_said_in_plain_words() {
        let memory = Memory { card: "card".into(), total_mib: 8188, free_mib: 2150 };
        let needs = needs_mib(Step::Mesh, Kind::Vulkan);
        let refused = enough(Step::Mesh, Kind::Vulkan, Some(&memory)).unwrap_err();
        assert_eq!(refused.code(), "not_enough_memory");
        assert_eq!(refused.to_string(), format!("making a mesh needs about {} of free graphics memory and there is 2.1 GB: close other programs that use the graphics card", crate::error::gigabytes(needs)));
        assert!(enough(Step::Mesh, Kind::Vulkan, Some(&Memory { free_mib: needs, ..memory })).is_ok());
        assert!(enough(Step::Mesh, Kind::Vulkan, None).is_ok(), "memory that cannot be measured is not a refusal");
    }
}
