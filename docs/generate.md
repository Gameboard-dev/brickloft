# The generator: a form from a picture or a prompt

`brickloft-generate` is a second program beside the editor. Given a
picture of one object, it makes a mesh of that object with a colour at
each point. Given a prompt such as "an oak barrel", it first makes
pictures of it and then a mesh from the one that is chosen. Both steps
run on the computer's own graphics card with models whose weights are
downloaded once. Nothing is sent to a service.

The editor does not use it yet. This page describes the program as it
stands on the command line: how it is kept apart from the editor, how to
build it, which models it runs and under what licences, what it asks of
the network, the lines it answers in, how well it does, and what the
editor will need in order to drive it.

The code is in [`generate/`](../generate), a module to a file, and each
file's opening comment says what it is for and why it is written as it
is ([`generate/src/lib.rs`](../generate/src/lib.rs) lists them).

## The route from a prompt to a mesh

No model that turns text straight into a mesh could be used: each one
found needs Python, and the editor's rule is that what ships is Rust
([future-work.md](future-work.md#a-form-from-a-prompt) has the models
set aside and why). The prompt therefore goes by way of a picture.

1. **Prompt to picture.** Stable Diffusion 1.5 makes a 512 by 512
   picture from the prompt in 25 steps. The network is run by
   [Candle](https://github.com/huggingface/candle), Hugging Face's
   machine-learning library in Rust.
2. **Picture to object.** The ground behind the object is cut away, by
   arithmetic on the colours and without a model, and the object is
   centred on a see-through square.
3. **Object to mesh.** TripoSR turns that square into a field of density
   and colour, from which a surface is drawn as triangles. It is run by
   [Lux3D](https://github.com/FerrisMind/Lux3D), a port of TripoSR and
   other models to Rust on Candle.
4. **Tidying.** Scraps are cleared, the mesh is stood level and set on
   the ground, and it is written as a `.glb` file.

A person with a photograph starts at step 2.

## How it is kept apart from the editor

The editor builds in under a minute from a handful of crates. The
generator builds on Candle and Lux3D, which bring about 370 crates,
take ten minutes to compile, and need a C++ compiler and a shader
compiler to do it. If the two shared a workspace, every build of the
editor would have to resolve the generator's dependencies, two of which
are git repositories pinned by commit: a clone without network access,
or a commit that had gone from either repository, would stop the editor
building.

So `generate/` is a workspace of its own. It has its own
[`Cargo.toml`](../generate/Cargo.toml) with an empty `[workspace]`
table and its own `Cargo.lock`, and the root
[`Cargo.toml`](../Cargo.toml) names it under `exclude`. `cargo build`
and `cargo test` run at the root never read the folder. The generator
is built only by running cargo inside `generate/`.

The alternative was to list it under `members` and leave it out of
`default-members`. Cargo skips such a member when building, but still
resolves it, so its 370 packages would have been written into the
editor's lock file and fetched on every fresh clone.

## Building it

These steps are for Windows, where it was built. Nothing here is
installed by the program or by cargo; each tool is installed by the
person building.

| Tool | Why | Needed for |
|---|---|---|
| Rust 1.85 or later | Lux3D uses the 2024 edition | both backends |
| Visual Studio Build Tools, with the C++ workload | several dependencies compile C++ | both |
| `libclang.dll` (from LLVM), with `LIBCLANG_PATH` set to its folder | one dependency reads a C++ header to write its Rust bindings | both |
| `glslc` on `PATH` (in the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home)) | compiles the Vulkan backend's shaders | Vulkan |
| The [CUDA Toolkit](https://developer.nvidia.com/cuda-toolkit), with `nvcc` on `PATH` | compiles the CUDA backend's kernels | CUDA |

For the Vulkan backend, which is the default:

```powershell
cd generate
cargo build --release --locked
```

For CUDA, the build has to run in a Visual Studio developer prompt
(`vcvars64.bat`), because `nvcc` calls Microsoft's compiler. With CUDA
13 the variable `CL` must also be set to `/Zc:preprocessor`: its headers
refuse the compiler's older preprocessor, and the compiler reads extra
options from that variable.

```powershell
$env:CL = '/Zc:preprocessor'
cargo build --release --locked --no-default-features --features cuda
```

The program is `generate/target/release/brickloft-generate.exe`. With
both features it tries CUDA first and falls back to Vulkan.

**What the build runs.** Lux3D's own crates have no build scripts. Its
dependencies' build scripts run these programs, and none of them
downloads anything:

- `candle-vulkan-kernels` runs `glslc` on its shaders. It also compiles
  a small C++ program from its own source with the system's compiler
  and runs it, to generate variants of each shader.
- `candle-kernels` (CUDA only) runs `nvcc` on its kernels and
  `nvidia-smi` to ask which generation the card is. `cudarc` runs
  `nvcc --version`.
- `xatlas-rs-v2` compiles the C++ library it wraps and runs `bindgen`
  on its header, which is what needs `libclang`. `meshopt`, `esaxx-rs`
  and `ring` compile C or C++ sources they carry.

**The C runtime.** [`generate/.cargo/config.toml`](../generate/.cargo/config.toml)
makes the whole program link Microsoft's C runtime statically. The CUDA
kernels are compiled that way and the linker will not mix the two
kinds; it also means the program runs on a computer without the Visual
C++ runtime installed.

**Tests.** `cargo test --release --locked` runs the tests that need
neither weights nor a graphics card. `--release` is deliberate: it
reuses the dependencies already compiled for the program, where an
unoptimised build would compile all of them again. The tests that run
the models are ignored unless asked for:

```powershell
$env:BRICKLOFT_WEIGHTS = '<the weights folder>'
cargo test --release --locked --test models -- --ignored --test-threads 1
```

**Pictures of a mesh.** [`generate/examples/look.rs`](../generate/examples/look.rs)
draws a `.glb` from five sides and shows a picture's cut-out. It is how
the meshes described under [results](#what-five-prompts-gave) were
judged.

## The models and their licences

Two models are used. Each licence was read on the model's own page on
2026-11-16, and the program shows it before it will download anything.

| | TripoSR | Stable Diffusion 1.5 |
|---|---|---|
| Does | picture to mesh | prompt to picture |
| Made by | Tripo AI and Stability AI, 2024 | Rombach, Esser and others, 2022 |
| Fetched from | [`oxide-lab/TripoSR`](https://huggingface.co/oxide-lab/TripoSR) | [`stable-diffusion-v1-5/stable-diffusion-v1-5`](https://huggingface.co/stable-diffusion-v1-5/stable-diffusion-v1-5) |
| Size | 1.68 GB | 2.13 GB (the half-precision files) |
| Licence | MIT | CreativeML OpenRAIL-M |
| Restrictions on use | none | the licence's list, below |
| Gated | no | no |

**TripoSR.** The original checkpoint is a Python pickle, which Rust
code cannot read safely. `oxide-lab/TripoSR` is the same weights
converted to the safetensors format by Lux3D's author. Its page gives
the licence as MIT and carries the original's licence file, "Copyright
(c) 2024 Tripo AI & Stability AI". MIT asks only that copies keep that
notice and the licence text.

**Stable Diffusion 1.5.** The repository is a mirror of the original
`runwayml/stable-diffusion-v1-5`, which its owner took down. Its page
gives the licence as
[CreativeML OpenRAIL-M](https://huggingface.co/spaces/CompVis/stable-diffusion-license).
That licence allows any use, commercial use included, except the uses
listed in its Attachment A, and it requires whoever passes the model on
to pass that list on too and to require it of their users. The
generator does not pass the model on (each player downloads it from
Hugging Face), but it shows the list for the same reason. The list in
the licence's words:

> You agree not to use the Model or Derivatives of the Model:
> - In any way that violates any applicable national, federal, state, local or international law or regulation;
> - For the purpose of exploiting, harming or attempting to exploit or harm minors in any way;
> - To generate or disseminate verifiably false information and/or content with the purpose of harming others;
> - To generate or disseminate personal identifiable information that can be used to harm an individual;
> - To defame, disparage or otherwise harass others;
> - For fully automated decision making that adversely impacts an individual's legal rights or otherwise creates or modifies a binding, enforceable obligation;
> - For any use intended to or which has the effect of discriminating against or harming individuals or groups based on online or offline social behavior or known or predicted personal or personality characteristics;
> - To exploit any of the vulnerabilities of a specific group of persons based on their age, social, physical or mental characteristics, in order to materially distort the behavior of a person pertaining to that group in a manner that causes or is likely to cause that person or another person physical or psychological harm;
> - For any use intended to or which has the effect of discriminating against individuals or groups based on legally protected characteristics or categories;
> - To provide medical advice and medical results interpretation;
> - To generate or disseminate information for the purpose to be used for administration of justice, law enforcement, immigration or asylum processes, such as predicting an individual will commit fraud/crime commitment (e.g. by text profiling, drawing causal relationships between assertions made in documents, indiscriminate and arbitrarily-targeted use).

One sentence on the model's page reads differently from its licence.
Under "Direct Use" the page says the model "is intended for research
purposes only". That is the authors' statement of what they intended
it for; the licence itself has no such term and its grant is not
limited to research. It is recorded here so that whoever decides to
ship this has read both.

**How a player is shown them.** `check` returns each model's licence
name, a link to its text and its restrictions, so the editor can show
them before it offers a download. `fetch` prints the same as its first
line and stops there unless it was started with `--accept-licence`.
The whole text of the OpenRAIL-M licence is compiled into the program
from [`generate/licences/`](../generate/licences) and written beside
the weights as `LICENCE.txt`; TripoSR's licence file is fetched with
its weights.

**What was turned down.**

- The ready-made tokenizer file that Candle's example downloads comes
  from `openai/clip-vit-base-patch32`, whose page states no licence.
  The same vocabulary is in the Stable Diffusion repository under that
  repository's licence, and the tokenizer is built from it
  ([`picture.rs`](../generate/src/picture.rs), `tokenizer`). The two
  were compared and give the same numbers.
- Lux3D's ground removers, RMBG 1.4 and 2.0, are licensed for
  non-commercial use. They are never loaded.
- Candle's copy of the Segment Anything weights states no licence.

## What it sends, and to where

The program uses the network for one thing: `fetch`, when a person
runs it with `--accept-licence`. No other command opens a connection.
Lux3D has a downloader of its own that would fetch missing weights
unasked; the generator always hands Lux3D the folder to read, which
turns that downloader off.

A fetch makes HTTPS requests to `huggingface.co` and to the hosts it
redirects to, which are Hugging Face's file servers under `hf.co`.
Each request is a `GET` for one file at a fixed address:

```text
https://huggingface.co/<repository>/resolve/<commit>/<path>
```

The repository, the commit and the path are written in
[`models.rs`](../generate/src/models.rs) and cannot be given as
arguments. A request carries the client name `brickloft-generate`
and, when a download is being carried on, a `Range` header with the
byte to start from. It carries no token, no cookie, and nothing about
the computer or the person. As with any request, the server sees the
address it came from.

Every file's length and SHA-256 are also written in `models.rs`. A
file is kept only if both match; otherwise it is deleted and the
command fails with `hash_mismatch`. A redirect to any address that is
not HTTPS on one of Hugging Face's hosts is refused before it is
followed.

The server's certificate is checked by the operating system, as a
browser's is. The computer this was written on runs an antivirus that
reads HTTPS traffic by way of a certificate authority it installs in
Windows, and a first version that trusted only its own compiled-in
list of authorities could not download at all there. Trusting the
system costs nothing in safety here, because a file is accepted for
its hash and not for who sent it.

## The commands

Options that every command takes: `--weights <folder>` says where the
weights are kept, `--device auto|cuda|vulkan` chooses the backend, and
`--watch-stdin` makes the program stop when its standard input closes.

Without `--weights` the folder is `%LOCALAPPDATA%\brickloft\weights`
on Windows (`~/Library/Application Support/brickloft/weights` on
macOS, `$XDG_DATA_HOME/brickloft/weights` elsewhere). It is worked out
from the environment when the program runs. The weights are not kept
beside the program, which may sit in a folder that cannot be written
to, nor under Documents, which is often synchronised to a cloud drive.

```text
brickloft-generate check [--verify]
```

Reports the graphics device and its memory, each model's files and
licence, and whether each step can run. `--verify` also works out each
file's SHA-256, which reads 3.8 GB.

```text
brickloft-generate fetch <triposr|sd15> [--accept-licence]
```

Prints the model's licence. With `--accept-licence` it downloads the
files that are missing into the weights folder. A download that is
interrupted leaves a `.part` file and is carried on from it the next
time. Any other name is refused.

```text
brickloft-generate picture --prompt "an oak barrel" --out <folder> [--seed N] [--count K] [--steps S]
```

Makes K pictures (1 to 8) and ranks them. The prompt is one argument
of at most 500 characters. Picture number i is made from seed N + i
and written as `candidate-<seed>.png`, so the same request writes the
same files.

```text
brickloft-generate mesh --picture <file> --out <file.glb> [--no-cutout] [--no-level] [--resolution N]
```

Makes a mesh of the object in a PNG, JPEG or WebP picture.
`--resolution` (32 to 256, 128 if not given) is how many cells a side
the surface is drawn from. A picture that is already see-through
around its object is used as it is, whether or not `--no-cutout` is
given; `--no-cutout` insists on it and fails if nothing is
see-through. `--no-level` leaves a tilted mesh tilted.

```text
brickloft-generate serve
```

Takes the same commands as lines of JSON on standard input and keeps
the models loaded between them (see [keeping it running](#keeping-it-running)).

## The protocol

The program writes one JSON object to a line on standard output and
nothing else there. What the libraries have to say goes to standard
error. Every line has an `event`:

| `event` | When | Fields |
|---|---|---|
| `progress` | while a command runs | `stage`, `done`, `of`, and sometimes `what`, `file`, `candidate`, `seed` |
| `licence` | from `fetch`, first | `model`, `title`, `page`, `bytes`, `licence`, `licence_url`, `restrictions` |
| `result` | once, when a command has worked | `command`, `protocol` (now 1), and the command's own fields |
| `error` | once, when it has not | `code` and `message` |

A run ends with exactly one `result` or one `error`, as its last line.
The exit code is 0 after a result, 2 for arguments that could not be
understood, 3 for a run that was cancelled, and 1 for any other error.

The `stage` of a progress line is `load`, `denoise`, `cutout`,
`reconstruct`, `download` or `verify`. `done` and `of` count steps, or
bytes for the last two.

### Errors

`message` is a sentence for a person, which the editor can show as it
is. It says what failed, why, and what to do where there is something
to do. `code` is for the editor to act on, and stays the same if a
sentence is reworded. All the wording is in one file,
[`error.rs`](../generate/src/error.rs).

| `code` | Meaning | What the editor might do |
|---|---|---|
| `usage` | the arguments were wrong | a bug in the editor |
| `prompt_empty`, `prompt_too_long` | the prompt is empty, over 500 characters, or over the 77 word-pieces the model reads | show the message by the prompt box |
| `weights_missing`, `weights_damaged` | a weights file is absent or the wrong length | offer the download |
| `licence_not_accepted` | `fetch` without `--accept-licence` | show the licence and ask |
| `not_on_list` | `fetch` was asked for something else | a bug in the editor |
| `network`, `download_failed`, `redirect_refused`, `hash_mismatch` | the download failed | show the message; `network` can be tried again and carries on |
| `picture_unreadable`, `picture_too_large` | the file is not a picture, or is over 64 MB or 8192 pixels a side | show the message |
| `no_transparency` | `--no-cutout` on a picture with nothing see-through | |
| `nothing_to_cut_out` | no object was found on a plain ground | suggest another picture |
| `empty_mesh` | the model made nothing | suggest another picture |
| `no_device` | no graphics backend started | show the message |
| `not_enough_memory` | the card has less free memory than the step needs | show the message |
| `model_failed` | a library failed; `message` has its words | show the message |
| `io` | a file could not be read or written | show the message |
| `bad_request` | a line sent to `serve` was not a request | a bug in the editor |
| `cancelled` | standard input closed | nothing |

For example, on a card with too little free memory:

```json
{"code":"not_enough_memory","event":"error","message":"making a mesh needs about 3.7 GB of free graphics memory and there is 2.1 GB: close other programs that use the graphics card"}
```

### `check`

```json
{"event":"result","command":"check","protocol":1,"version":"0.1.0",
 "built_with":["vulkan"],
 "device":{"kind":"vulkan","name":"NVIDIA GeForce RTX 4060 Ti","memory_total_mib":8188,"memory_free_mib":6710,"memory_note":null},
 "device_problem":null,
 "weights":{"folder":"...","models":[{"model":"triposr","title":"TripoSR","step":"making a mesh from a picture","present":true,
   "bytes":1677174473,"bytes_found":1677174473,"folder":"...","page":"https://huggingface.co/oxide-lab/TripoSR",
   "licence":"MIT","licence_url":"...","restrictions":"...",
   "files":[{"file":"model.safetensors","bytes":1677170936,"bytes_found":1677170936,"right_length":true,"verified":null}]}]},
 "steps":{"picture":{"can_run":true,"needs_mib":3600,"reason":null},
          "mesh":{"can_run":false,"needs_mib":3800,"reason":"the weights for triposr are not there (... is missing): fetch them with `brickloft-generate fetch triposr`"}}}
```

(Shown over several lines here; the program writes it as one.)
`device` is null and `device_problem` says why when no backend
starts. `reason` is the sentence the step itself would fail with.
`verified` is null unless `--verify` was given.

The card's memory is read from `nvidia-smi`, the program NVIDIA's
driver installs, because neither backend of Candle reports it and the
system calls that do are `unsafe` code, which the generator does not
contain. On a card from another maker the two memory fields are null,
`memory_note` says that it could not be measured, and the steps are
allowed to run.

### `fetch`

A `licence` line, then `progress` lines with `stage` `verify` (a file
already there is being checked) or `download`, then:

```json
{"event":"result","command":"fetch","protocol":1,"model":"sd15","folder":"...","bytes":2134190091,"bytes_fetched":1584581,
 "files":[{"file":"unet.fp16.safetensors","bytes":1719125304,"fetched":false}, {"file":"vocab.json","bytes":1059962,"fetched":true}]}
```

### `picture`

```json
{"event":"result","command":"picture","protocol":1,"prompt":"an oak barrel",
 "prompt_sent":"a miniature scale model of an oak barrel on a white background, studio product photo, whole object visible, three-quarter view",
 "negative_prompt":"cropped, cut off, ...","steps":25,"width":512,"height":512,
 "candidates":[
  {"rank":1,"file":"...\\candidate-8.png","seed":8,"score":1.0,"usable":true,"problems":[],
   "cutout":{"coverage":0.31,"on_frame":0.0,"pieces":1,"plain_frame":1.0,"shadow_removed":true,"holes":0,"rise":0.21,"view":"level"}},
  {"rank":2,"file":"...\\candidate-7.png","seed":7,"score":0.3,"usable":false,
   "problems":["the object is cut off by the edge of the picture"],"cutout":{"...":"..."}}]}
```

`candidates` is in order of rank, best first. `problems` are sentences
for a person. `cutout` is null when no object was found.

### `mesh`

```json
{"event":"result","command":"mesh","protocol":1,"file":"...\\barrel.glb","bytes":1174372,
 "vertices":24460,"triangles":48916,"resolution":128,
 "up":"y","front":"+z","colours":"srgb","size":[0.97,0.93,0.98],
 "picture":{"file":"...","width":512,"height":512},
 "cut_by":"ground","cutout":{"...":"..."},
 "pieces_dropped":1,"scrap_cells_cleared":0,
 "tilt":{"state":"levelled","degrees":12.5},
 "notes":["the mesh came out tipped by 12 degrees, as from a picture taken from above, and was stood level"]}
```

`cut_by` is `ground` when the ground was cut away by its colour and
`transparency` when the picture's own see-through pixels were used.
`tilt.state` is `level`, `levelled`, `tilted` (found and left, with
`--no-level`) or `unknown`. `notes` are sentences for a person: what
is wrong with the picture, and what was done about a tilt.

**The file.** Binary glTF with one mesh: positions, a colour for each
vertex, 32-bit indices, no normals. Its axes are the format's own, Y
up with the object's front towards +Z, and its lowest point is at
height 0 with the object centred over the origin. TripoSR and Lux3D
write Z up, which the format's text does not allow, so the generator
turns the mesh before it writes it. `size` is the mesh's width, height
and depth in the model's units, in which an object is about 1 across;
it carries no real size.

The colours are written as they are to be shown on a screen (`"colours":
"srgb"`). The glTF text asks for linear light in `COLOR_0`. The
editor's own reader takes the numbers as shown, and so do the programs
TripoSR's output is made for, so a conversion here would make every
such reader show the mesh too dark.

### Stopping a run

Started with `--watch-stdin`, the program ends when its standard
input closes: it removes the files it was writing, prints an `error`
with code `cancelled` and exits with 3. The editor holds the input
open and closes it to cancel. It is an option and not the default
because a program started with no input at all, as a script does,
reads the end of input at once.

Killing the program is also safe. Every file is written under a name
with `.part` in it and renamed when whole, so a picture or a mesh
under its final name is always complete. A killed run can leave a
`.part` file, which the next run writes over. A download's `.part`
file is kept on purpose, to be carried on from.

### Keeping it running

`serve` reads requests from standard input, one JSON object to a line,
with the command line's names without their dashes:

```json
{"command":"picture","prompt":"an oak barrel","seed":7,"count":4,"out":"C:\\...\\pictures"}
{"command":"mesh","picture":"C:\\...\\candidate-8.png","out":"C:\\...\\barrel.glb","resolution":128}
{"command":"quit"}
```

It answers `{"event":"result","command":"serve","ready":true}` when it
starts, and then answers each request as the command line would. A
name the command does not take is refused (`bad_request`), as is a
line over 16 KB. Requests are done one at a time. The end of input
ends the program, and cancels the request being done if there is one.

## How a picture is made ready, and a mesh tidied

TripoSR was trained on pictures of single objects rendered alone, from
about level with them. Three things done around it decide whether a
mesh is usable: cutting the object out of its picture, choosing which
picture to use, and tidying the mesh afterwards. None of the three
uses a model.

### Cutting the object out

TripoSR needs the object alone on a see-through ground. The models
made for removing a ground were turned down on their licences, so
[`cutout.rs`](../generate/src/cutout.rs) does it from the colours. It
relies on what the prompt asks Stable Diffusion for, which is one
object on a plain studio ground.

1. The picture's corners are taken to be ground, and each is followed
   along the edge of the picture for as long as the colour changes
   gently. A corner counts only if another corner is nearly its colour,
   so that an object filling one corner is not taken for ground. The
   colours found are the ground's palette. A studio ground often has a
   floor and a wall of different tones, and the palette holds both.
2. The ground is flooded inwards from the edge, over pixels that are a
   palette colour or that differ little from their neighbours. The
   second rule follows a ground's slow change of brightness. The flood
   may not stray far from the palette, so a smooth red car is not
   ground however gently its own colour changes. It also runs on a
   mask shrunk by two pixels, so a small gap in the object's outline
   does not let it in.
3. Shadows are followed as ground: a shadow is a palette colour made
   darker by the same factor in each channel. If following shadows
   would take more than a third of what is left, the object was
   probably grey on a white ground and had been mistaken for shadow,
   and the step is undone.
4. Ground seen through the object, such as between a statue's legs, is
   cleared where it is a palette colour and smooth.
5. The mask is opened, which removes lines and specks thinner than
   five pixels, and the largest piece is kept.

The trial's cut-out was the flood alone, started from every pixel of
the edge. On its ten pictures the new one removes the thin lines that
the hard edge of a shadow left around four objects, and keeps a rock
base that the old one cut ragged. It still leaves a dark contact
shadow under an object when the shadow's edge is sharp, and it cannot
separate an object from a ground of its own colour. A picture that is
already see-through around its object skips all of this.

A model of the ground as one smooth surface, fitted to the picture's
edge, was tried first and was worse: it left the floor standing
wherever the ground had a floor and a wall.

### Choosing among candidates

About half the pictures Stable Diffusion makes for a prompt are no use
to TripoSR, and wording does not fix that, so `picture` makes several
and ranks them. Each is cut out, which takes half a second, and judged
on what the cut-out found ([`rank.rs`](../generate/src/rank.rs)):

| What is checked | How | If it fails |
|---|---|---|
| The object is whole | no part of it lies on the picture's edge | not usable |
| There is one object | one piece of a size to be an object | not usable |
| The ground is plain | most of the picture's edge is ground | not usable when little of it is |
| The object is a sensible size | it covers 5% to 75% of the picture | not usable |
| The view is near level | the slope of the object's lower outline | lower score, still usable |

The last check estimates how high the camera was. A box seen from a
corner shows its base as a V, and a round base shows as half an
ellipse. In both, the outline rises from its lowest point at a slope
set by the camera's height, and the two arms' slopes multiply to the
square of that height's sine however the box is turned. The score
drops when the estimate is high, because the mesh then comes out
tilted. The estimate is wrong for objects with no base to see, such as
a boat, whose hull curves upward whatever the camera does.

The candidates come back best first, each with a score from 0 to 1,
whether it is usable, and its problems as sentences. The score judges
whether a picture suits TripoSR. It does not judge whether the picture
shows what was asked for, which only a person looking at it can.

### Tidying the mesh

[`level.rs`](../generate/src/level.rs) does four things to a mesh.

**Scraps are cleared before the surface is drawn.** TripoSR leaves
bits of ground floating near an object, some joined to it by a thread,
and each would be fitted with bricks. The field the surface is drawn
from is a cube of cells. It is shrunk by one cell, which removes
threads and leaves the core of each body. Every solid cell is then
given to the core nearest to it through solid cells, and the cells
that fall to a small core, or to none, are emptied. A scrap on a
thread is cut off at the thread's middle. A thin wall of the object
itself, such as a boat's side, stays, because no other core is nearer
to it than the object's own. A second body at least a tenth the size
of the first is kept as part of the object. Pieces that are not joined
at all and are under 2% of the largest are also dropped from the mesh.

**A tilted mesh is stood level.** TripoSR takes every picture to have
been taken from level with the object. From a picture taken from above
it builds the object tipped towards the camera by the camera's height,
always about the one axis that runs across the picture. Levelling is
therefore a search over one angle. For each angle from 10 degrees back
to 65 degrees forward, the mesh is tipped back by it and the vertices
within a hair of its lowest point are counted. At the angle that makes
the object's base level, the whole base is at the bottom and the count
jumps. When the jump is clear the mesh is tipped back by that angle,
and the result says by how much. A plinth, a floor slab or four wheels
are found this way.

**A mesh with no flat base is laid flat, if the picture was taken from
high up.** A car seen from above comes out standing on its nose, and
its underside, which TripoSR had to guess, is not flat enough to find.
For such a mesh, and only when the picture's own estimate says the
camera was high, the mesh is laid at the angle near that estimate at
which it is lowest. A long, low thing lies lowest when level. This is
rougher than finding a base, and the result's note says so. When
neither way finds an angle, the mesh is left as it is and the note
says it may be tilted.

**It is set on the ground and turned Y up**, as described under
[`mesh`](#mesh).

## What the editor will need

The editor does not start the generator yet. The step that makes it do
so is small because of how the pieces already fit:

- **The Imported form opens the result as it is.** It reads `.glb`
  with vertex colours, takes Y as up for that format, and copies an
  opened mesh into `Documents/brickloft/imports` under a name made
  from its content. A generated mesh is opened the same way, so a
  saved form keeps working when the generator is not installed.
- **Where things go.** The editor passes `--weights` with a folder of
  its own. Its `places` module has no folder for a program's data yet;
  the generator's own default (`%LOCALAPPDATA%\brickloft\weights`) is
  the suggestion, because the weights should not be in a folder that a
  cloud drive synchronises. Pictures and the mesh in the making go to
  the editor's folder for passing files, `brickloft` in the system's
  temporary folder.
- **Finding the program.** The editor should look for
  `brickloft-generate` beside its own executable and nowhere else, and
  show a link to this page when it is not there. It should not be
  found by a search of `PATH`.
- **The panel.** A "Generative prompt" box and a Generate button on
  the Imported form. On first use: `check`, then for each model that
  is missing its licence and restrictions from `check`'s answer, a
  question, and `fetch <model> --accept-licence` with a progress bar
  fed by the `download` lines. Then `picture` with a count of 4, the
  candidates shown in the order returned with the `problems` of each,
  and `mesh` on the one picked.
- **Running it.** A thread reads the generator's lines and hands them
  to the window, which never waits on it. Cancel closes the
  generator's input. An `error` line is shown as its `message`; the
  table of codes above says which ones the panel can act on.
- **One program or two runs.** `serve` saves the loading time when a
  player asks for more pictures or tries a second candidate (see the
  measurements). The simplest first version starts one command at a
  time and can move to `serve` without any change to the generator.
- **What a save records.** The mesh by its name in `imports`, as for
  any Imported form. The prompt and seed can be kept beside it for
  the player's own reference; they are not needed to open the form.

## Limits and risks

- **The pictures decide everything.** TripoSR makes a good mesh from a
  good picture, and Stable Diffusion 1.5 gives a good picture for a
  short prompt less than half the time. Ranking moves the better
  pictures to the front, and it cannot make one that is not there.
  What a picture shows is for the player to judge.
- **The back is a guess.** TripoSR sees one side. The other side of
  the mesh is plausible and smooth, and wrong in its details.
- **The cut-out needs a plain ground.** A photograph of an object on
  a patterned cloth or in a room will not be cut out well. A picture
  that is already see-through is the way round it.
- **Graphics memory is read only on NVIDIA cards.** On others the
  generator cannot tell whether a step will fit, and lets it try.
- **Two projects it depends on are young.** Lux3D is one author's
  work, eight months old, with no release. It depends on that author's
  fork of Candle, which is several hundred commits ahead of the
  original. Both are pinned by commit, so what is built does not
  change unless the pins are changed, and the price is that fixes do
  not arrive unless someone moves them. If either repository
  disappeared, the generator could not be built from a clean checkout
  without a copy of it.
- **The CUDA build is large to ship.** NVIDIA's runtime libraries that
  must sit beside it come to about 650 MB.
- **Model licences can change at their source.** The weights are
  pinned by commit and hash, so what is downloaded is what was read.
  If a repository is taken down, `fetch` fails with a message and the
  pins need moving to another copy.
