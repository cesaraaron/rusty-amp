---
layout: page.njk
permalink: getting-started.html
title: "Get started · rusty-amp"
ogTitle: "rusty-amp · get started"
description: "Install rusty-amp from a pre-built binary or build from source, then learn the keyboard controls and startup flow."
eyebrow: "Get started"
heading: "Install, launch & play"
lead: "Everything you need to get a guitar signal running through rusty-amp — and the full keyboard map once you're in."
toc:
  - { href: "#need", label: "What you need" }
  - { href: "#install", label: "Install" }
  - { href: "#build", label: "Build from source" }
  - { href: "#startup", label: "Startup flow" }
  - { href: "#controls", label: "Controls" }
prev: { href: "index.html", label: "Home" }
next: { href: "pedals.html", label: "Pedals &amp; effects reference" }
---

## What you need {#need}

- **A guitar and an audio interface** with a high-impedance instrument input (e.g. Focusrite Scarlett).
- **Speakers or headphones** — for the full stereo image, use stereo output.

## Install {#install}

The fastest way to start is a pre-built binary — presets are baked in, so there's nothing else to download. Grab the latest from [Releases](https://github.com/danylokravchenko/rusty-amp/releases/latest), or [build from source](#build).

### macOS (Apple Silicon)

```bash
curl -L https://github.com/danylokravchenko/rusty-amp/releases/latest/download/rusty-amp-macos-aarch64 -o rusty-amp
chmod +x rusty-amp

# Remove the macOS quarantine flag (required for unsigned binaries)
xattr -d com.apple.quarantine rusty-amp

./rusty-amp
```

### Linux (x86_64)

```bash
curl -L https://github.com/danylokravchenko/rusty-amp/releases/latest/download/rusty-amp-linux-x86_64 -o rusty-amp
chmod +x rusty-amp

./rusty-amp
```

<div class="note">
<b>Windows:</b> no pre-built binary is published yet — <a href="#build">build from source</a> instead.
rusty-amp runs natively on all three platforms via <a href="https://github.com/RustAudio/cpal">cpal</a>,
which selects the OS audio backend automatically (CoreAudio, WASAPI/ASIO, or ALSA/JACK).
</div>

## Build from source {#build}

Runs on **macOS, Windows, and Linux**. Requires **Rust 1.95+** (`rustup` recommended).

```bash
cargo run --release
# or after building:
./target/release/rusty-amp
```

CLAP plugin hosting is on by default. For a minimal amp with no plugin dependencies, build with `cargo run --release --no-default-features` — see [Plugins](plugins.html).

## Startup flow {#startup}

<ol class="steps">
<li><b>Select input device</b> — your audio interface appears in the list.</li>
<li><b>Select guitar input channel</b> — a Focusrite 2i2 has 2; guitar is usually channel 2 if plugged into Input 2. On a Scarlett Solo 4th Gen, Input 1 is the instrument input and is channel 1.</li>
<li><b>Select output device</b> — pick your speakers or headphones.</li>
</ol>

The list is filtered: ALSA's `null` sink and duplicate entries are hidden. On **Linux with PipeWire**, choose the **PipeWire Sound Server** (or **Default ALSA Output**) entry rather than the raw interface — PipeWire keeps the hardware busy, so opening the raw device fails.

Your selection is remembered in `~/.config/rusty-amp/audio.conf`, so later launches skip the picker and start straight into the rig. To change devices at any time, press <kbd>O</kbd> in the rig — it reopens the picker and restarts the audio engine with the new devices. Delete that file, or launch with `RUSTY_AMP_DEVICE_PROMPT=1`, to be prompted on the next launch.

The processed signal is **true stereo**: the left channel goes to output 0, the right to output 1 (a mono output device receives the summed mix). On a stereo interface or headphones you hear the full multi-mic cab spread, ping-pong delay, and stereo reverb. Some devices only accept a large buffer; rusty-amp requests a small one for low latency and falls back automatically if the device refuses it.

<div class="note note--info">
The app launches immediately with default values. Press <kbd>P</kbd> at any time to open the preset browser,
<kbd>S</kbd> to save the current state as a new preset, and <kbd>R</kbd> to start or stop recording.
</div>

## Controls {#controls}

| Key | Action |
| --- | ------ |
| <kbd>1</kbd> / <kbd>2</kbd> / <kbd>3</kbd> / <kbd>4</kbd> | Focus the live order, amp &amp; cabinet, [practice timeline](tools.html#practice), or pedalboard — press again on the focused panel to hide it (`1` never hides) |
| <kbd>Tab</kbd> / <kbd>Shift-Tab</kbd> | Jump to the next / previous panel (chain → amp → timeline → pedals) |
| <kbd>←</kbd> / <kbd>→</kbd> | Move inside the focused panel — pick a stage on the ribbon, a knob on the amp/pedals (seeks on the timeline) |
| <kbd>↑</kbd> / <kbd>+</kbd> / <kbd>=</kbd> | Increase focused knob by 5% |
| <kbd>↓</kbd> / <kbd>-</kbd> | Decrease focused knob by 5% |
| <kbd>A</kbd> | Open the amp model browser (works from anywhere) |
| <kbd>C</kbd> | Open the cabinet model browser (works from anywhere) |
| <kbd>I</kbd> | Open the cabinet-IR browser to load/clear an external `.wav` IR ([see IRs](amps-cabs.html#irs)) |
| <kbd>X</kbd> | A/B between a loaded external IR and the built-in cab (once an IR is loaded) |
| <kbd>Space</kbd> | Bypass the ribbon's stage, the focused pedal, or the timeline transport — or open the **Add pedal** picker on the `+ ADD` tile |
| <kbd>Enter</kbd> | Open the **Add pedal** picker when the `+ ADD` tile is focused |
| <kbd>D</kbd> | Remove the focused pedal from the board (bypassed and hidden — re-add it from `+ ADD`) |
| <kbd>[</kbd> / <kbd>]</kbd> | Move the ribbon's selected stage earlier / later in the signal chain (see [signal chain](how-it-works.html#chain)) |
| <kbd>P</kbd> | Open the preset browser overlay |
| <kbd>T</kbd> | Open the [tuner](tools.html#tuner) — bypasses the rig for a clean signal |
| <kbd>M</kbd> | Open the [metronome](tools.html#metronome) — an adjustable click that plays along but stays out of recordings |
| <kbd>V</kbd> | Open the CLAP [plugin browser](plugins.html) |
| <kbd>S</kbd> | Save the current state as a new user preset |
| <kbd>R</kbd> | Start / stop [recording](tools.html#recording) — saves a WAV file to your home directory and drops the take on the [practice timeline](tools.html#practice) when stopped |
| <kbd>B</kbd> | Open the **Practice tracks** browser to load a backing track (MP3 / WAV / FLAC) or a take |
| <kbd>O</kbd> | Change the audio input/output devices — reopens the device picker and restarts the engine |
| <kbd>K</kbd> | Open the keybinding cheat-sheet (press <kbd>K</kbd> / <kbd>Esc</kbd> to close) |
| <kbd>Q</kbd> / <kbd>Ctrl-C</kbd> | Quit |

The footer only shows `K keybindings  Q quit` — the full list above lives in the <kbd>K</kbd> modal, including the preset-browser, timeline, and metronome keys.

Focus starts on the **live-order ribbon**. <kbd>Tab</kbd> walks the panels in number order — chain → amp → [practice timeline](tools.html#practice) → pedals → back to the chain. See [the pedalboard](pedals.html#board) for add/remove details, and [Presets](presets.html) for the browser and save dialog.

<div class="note note--info">
Looking for the <b>chromatic tuner</b> (<kbd>T</kbd>), the <b>metronome</b> (<kbd>M</kbd>), or <b>one-key recording</b> (<kbd>R</kbd>)? They have their own page — see <a href="tools.html">Tools</a>.
</div>
