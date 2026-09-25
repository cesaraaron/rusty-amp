---
layout: page.njk
permalink: presets.html
title: "Presets · rusty-amp"
ogTitle: "rusty-amp · presets"
description: "Load, save, and write rusty-amp presets — including the bundled artist-inspired tones and the full TOML preset format."
eyebrow: "Presets"
heading: "Save, load & write tones"
lead: "Presets are plain <code>.toml</code> files. Load one while playing, save your own without restarting, or hand-write a preset from scratch."
toc:
  - { href: "#where", label: "Where they live" }
  - { href: "#browser", label: "Preset browser" }
  - { href: "#save", label: "Save dialog" }
  - { href: "#import-export", label: "Import & export" }
  - { href: "#bundled", label: "Bundled presets" }
  - { href: "#write", label: "Writing your own" }
prev: { href: "amps-cabs.html", label: "Amps, cabinets &amp; IRs" }
next: { href: "plugins.html", label: "CLAP plugins" }
---

## Where presets live {#where}

rusty-amp searches these directories, in order:

<ol class="steps">
<li><code>./presets/</code> — bundled presets (read-only, shipped with the repo).</li>
<li><code>~/.config/rusty-amp/presets/</code> — your personal presets.</li>
</ol>

Press <kbd>P</kbd> while playing to open the preset browser. Press <kbd>S</kbd> (from anywhere) to save the current state as a new user preset. The browser updates instantly — no restart required.

Bundled presets are marked as system presets and cannot be deleted from within the app. User presets show a `[user]` tag and can be deleted with <kbd>D</kbd>. Any selected preset (bundled or user) can be exported with <kbd>E</kbd>; press <kbd>I</kbd> to import a `.toml` file — see [Import &amp; export](#import-export).

## Preset browser <span class="muted">(<kbd>P</kbd>)</span> {#browser}

Press <kbd>P</kbd> while playing to open the browser overlay. Move the cursor with <kbd>↑</kbd>/<kbd>↓</kbd> and press <kbd>Enter</kbd> to apply — the change takes effect immediately, with the audio uninterrupted.

<div class="overlay">
  <div class="overlay__bar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">Presets</span></div>
  <div class="plist">
    <div class="prow"><span class="prow__name">Eagles — Hotel California (Solo)</span><span class="prow__meta">Marshall Plexi · Marshall Greenback</span></div>
    <div class="prow is-sel"><span class="prow__name">Pink Floyd — Comfortably Numb (Outro Solo)</span><span class="prow__meta">Hiwatt DR103 · WEM (Fane)</span></div>
    <div class="prow"><span class="prow__name">Pink Floyd — Time (Intro / Verse / Chorus)</span><span class="prow__meta">Hiwatt DR103 · WEM (Fane)</span></div>
    <div class="prow"><span class="prow__name">Led Zeppelin — Stairway to Heaven (Solo)</span><span class="prow__meta">Marshall Plexi · Marshall Greenback</span></div>
    <div class="prow"><span class="prow__name">my lead tone</span><span class="prow__meta">Mesa Dual Rectifier · Mesa V30</span><span class="prow__tag">user</span></div>
  </div>
  <div class="overlay__foot">
    <span><kbd>↑</kbd>/<kbd>↓</kbd> navigate</span>
    <span><kbd>Enter</kbd> apply</span>
    <span><kbd>S</kbd> save</span>
    <span><kbd>E</kbd> export</span>
    <span><kbd>I</kbd> import</span>
    <span><kbd>D</kbd> delete</span>
    <span><kbd>Esc</kbd> close</span>
  </div>
</div>

<div class="keymap">
  <div><kbd>↑</kbd> / <kbd>↓</kbd> Navigate the preset list</div>
  <div><kbd>Enter</kbd> Apply the selected preset (audio uninterrupted)</div>
  <div><kbd>S</kbd> Open the save dialog for the current state</div>
  <div><kbd>E</kbd> Export the selected preset to a file you choose</div>
  <div><kbd>I</kbd> Import a <code>.toml</code> preset file into your user presets</div>
  <div><kbd>D</kbd> Delete the selected preset (user presets only)</div>
  <div><kbd>Esc</kbd> / <kbd>P</kbd> Close without changing anything</div>
</div>

User presets are marked with a `[user]` tag in the list. The <kbd>D</kbd> hint appears in the footer only when the cursor is on a deletable preset.

## Save dialog <span class="muted">(<kbd>S</kbd>)</span> {#save}

Press <kbd>S</kbd> from anywhere to capture the current rig as a new preset. <kbd>Tab</kbd> moves between the two fields; <kbd>Enter</kbd> writes the file.

<div class="overlay overlay--narrow">
  <div class="overlay__bar"><span class="dots"><i></i><i></i><i></i></span><span class="ttl">Save preset</span></div>
  <div class="dform">
    <div class="field field--focus">
      <div class="field__label">Name</div>
      <div class="field__box">My Lead Tone<span class="caret"></span></div>
    </div>
    <div class="field">
      <div class="field__label">Description</div>
      <div class="field__box"><span class="field__ph">Sustain-focused, delay + reverb…</span></div>
    </div>
  </div>
  <div class="overlay__foot">
    <span><kbd>Tab</kbd> switch field</span>
    <span><kbd>Enter</kbd> save</span>
    <span><kbd>Esc</kbd> cancel</span>
  </div>
</div>

<div class="keymap">
  <div><kbd>Tab</kbd> Switch between Name and Description</div>
  <div><kbd>Enter</kbd> Save the preset and return to the browser</div>
  <div><kbd>Esc</kbd> Cancel without saving</div>
</div>

The preset is written to `~/.config/rusty-amp/presets/<name>.toml` and appears in the browser immediately — no restart required.

## Import &amp; export {#import-export}

Share tones as plain `.toml` files — the same format described in [Writing your own](#write).

- **Export** — select any preset in the browser (bundled or user) and press <kbd>E</kbd>. Type a destination path and press <kbd>Enter</kbd>; the path is prefilled with `./<preset-name>.toml`. A leading `~` resolves against your home directory, and missing parent folders are created. Exporting overwrites the destination if it already exists.
- **Import** — press <kbd>I</kbd> in the browser, type the path to a `.toml` file, and press <kbd>Enter</kbd>. The file is validated and copied into `~/.config/rusty-amp/presets/`, and the cursor lands on it. If a preset with the resulting file name already exists, the import fails with an error instead of overwriting — rename or delete the existing file first.

<div class="keymap">
  <div><kbd>Enter</kbd> Confirm the path</div>
  <div><kbd>Esc</kbd> Cancel without importing or exporting</div>
</div>

## Bundled presets {#bundled}

| File | Amp | Cabinet | Description |
| ---- | --- | ------- | ----------- |
| `acdc_back_in_black.toml` | Marshall Plexi | Marshall Greenback | Back in Black rhythm — guitar straight into a cranked Plexi, no pedals, tight and dry |
| `acdc_highway_to_hell.toml` | Marshall Plexi | Marshall Greenback | Highway to Hell — edge-of-breakup Plexi, jangly chords that bite when you dig in |
| `eagles_hotel_california_clean.toml` | Fender Twin Reverb | Fender 2×12 (Jensen) | Hotel California clean intro — glassy Twin, onboard spring reverb, lush chorus |
| `eagles_hotel_california_solo.toml` | Marshall Plexi | Marshall Greenback | Hotel California solo — TS boost, vocal mids, singing sustain, delay + hall |
| `led_zeppelin_stairway_solo.toml` | Marshall Plexi | Marshall Greenback | Stairway solo — cranked-amp crunch, bright mids, Echoplex slap |
| `led_zeppelin_whole_lotta_love.toml` | Marshall Plexi | Marshall Greenback | Whole Lotta Love — Tone Bender MkII fuzz into a cranked Plexi, thick and greasy |
| `pink_floyd_another_brick_pt2.toml` | Hiwatt DR103 | WEM (Fane) | Another Brick in the Wall Pt. 2 solo — Big Muff, slow Phase 90, tape echo |
| `pink_floyd_comfortably_numb_solo_1.toml` | Hiwatt DR103 | WEM (Fane) | Comfortably Numb first solo — light Big Muff, Dyna-Comp, Mistress shimmer, long Echorec |
| `pink_floyd_comfortably_numb_solo_2.toml` | Hiwatt DR103 | WEM (Fane) | Comfortably Numb outro — thick Big Muff, Electric Mistress swirl, big hall |
| `pink_floyd_money.toml` | Hiwatt DR103 | WEM (Fane) | Money — Fuzz Face into a Hiwatt with the wah rocking, Echorec-style tape echo |
| `pink_floyd_shine_on_crazy_diamond.toml` | Hiwatt DR103 | WEM (Fane) | Shine On You Crazy Diamond — Ram's-Head Big Muff, Uni-Vibe swirl, long tape echo |
| `pink_floyd_time_chorus.toml` | Hiwatt DR103 | WEM (Fane) | Dark Side "Time" intro/verse/chorus — clean, dry Hiwatt, whisper of Uni-Vibe, low dotted-eighth Echorec (no fuzz or compression) |
| `pink_floyd_time_solo.toml` | Hiwatt DR103 | WEM (Fane) | Dark Side "Time" solo — Fuzz Face bite, Uni-Vibe swirl, Echorec delay |
| `van_halen_aint_talkin_bout_love.toml` | Marshall Plexi | Marshall Greenback | Ain't Talkin' 'bout Love — dimed Plexi, Phase 90 swirl, MXR-style flange, tape slap |
| `van_halen_brown_sound.toml` | Marshall Plexi | Marshall Greenback | Brown sound lead — dimed Plexi, gentle Phase 90, Echoplex slapback |

## Writing your own preset {#write}

A preset is a TOML file. Only `[tube_screamer]`, `[amp]`, and `[reverb]` are required. For the optional effect sections, **omitting the section turns that effect off** (its stored knob values are left untouched); include the section with `enabled = false` to store values while keeping it bypassed. Two sections behave differently: an omitted `[noise_gate]` or `[cabinet]` leaves the current gate or cabinet/mic settings **unchanged**. An omitted `[chain]` resets the signal-chain order to the shipped default. All knob values are normalised `0.0–1.0`.

```toml
name        = "My Preset"
description = "Optional one-line description shown in the preset browser."

# Only [tube_screamer], [amp], and [reverb] are required.
# Omitting an optional effect section turns that effect off; add it with
# enabled = false to keep its values while bypassed. [noise_gate] and
# [cabinet] are the exceptions — omitting them leaves current settings as-is.

[noise_gate]
enabled   = true    # optional, defaults to true
threshold = 0.20    # 0.0 – 1.0  (0 = barely open, 1 = always open)
release   = 0.30    # 0.0 – 1.0  (0 = instant close, 1 = very slow)

# Omit [compressor] entirely to leave it off,
# or include it with enabled = false to store values but keep it bypassed.
[compressor]
enabled = false     # optional, defaults to true when the section is present
sustain = 0.40      # 0.0 – 1.0  (compression amount)
attack  = 0.30      # 0.0 – 1.0  (0.5 ms → 50 ms)
level   = 0.50      # 0.0 – 1.0  (output makeup, 0.5 = unity)

# Omit [pitch] entirely to leave it off (the default for the bundled presets),
# or include it with enabled = false to store values but keep it bypassed.
[pitch]
enabled = false     # optional, defaults to true when the section is present
pitch = 0.50        # 0.0 – 1.0  (0 = −12 st, 0.5 = unison, 1 = +12 st)
mix   = 0.50        # 0.0 – 1.0  (dry/wet blend)
tone  = 0.70        # 0.0 – 1.0  (wet low-pass, 0 = dark, 1 = open)

# Omit [wah] entirely to leave it off (the default for the bundled presets),
# or include it with enabled = false to store values but keep it bypassed.
[wah]
enabled = false     # optional, defaults to true when the section is present
freq = 0.40         # 0.0 – 1.0  (base peak position, ~300 Hz–1.5 kHz)
sens = 0.55         # 0.0 – 1.0  (auto-sweep amount; 0 = static cocked wah)
q    = 0.50         # 0.0 – 1.0  (resonance / sharpness of the quack)
mix  = 0.90         # 0.0 – 1.0  (dry/wet; a real wah is fully wet)

# Omit [fuzz] entirely to leave it off (the default for the bundled presets),
# or include it with enabled = false to store values but keep it bypassed.
[fuzz]
enabled = false     # optional, defaults to true when the section is present
type  = 0.0         # 0.0 = Big Muff, 0.5 = Fuzz Face, 1.0 = Tone Bender MkII
fuzz  = 0.70        # 0.0 – 1.0  (sustain/gain)
tone  = 0.50
level = 0.60

[tube_screamer]
enabled = true      # optional, defaults to true
drive = 0.40        # 0.0 – 1.0
tone  = 0.60
level = 0.70

# Omit [distortion] entirely to leave it off,
# or include it with enabled = false to store values but keep it bypassed.
[distortion]
enabled = true
drive = 0.50
tone  = 0.55
level = 0.65

# Boss ML-2 Metal Core — ultra-high-gain distortion with an active two-band EQ.
# Omit [metal_core] entirely to leave it off,
# or include it with enabled = false to store values but keep it bypassed.
[metal_core]
enabled = false       # optional, defaults to true when the section is present
dist  = 0.65          # 0.0 – 1.0  (gain into the two cascaded clip stages)
low   = 0.50          # 0.0 – 1.0  (active low shelf @ 120 Hz, ±15 dB; 0.5 = flat)
high  = 0.50          # 0.0 – 1.0  (active high shelf @ 3.2 kHz, ±15 dB; 0.5 = flat)
level = 0.60          # 0.0 – 1.0  (output volume)

# Pre-amp EQ — shapes the signal before the amp's gain stage.
# Omit [preamp_eq] entirely to leave it off, or include it with enabled = false.
[preamp_eq]
enabled = false       # optional, defaults to true when the section is present
low  = 0.50           # 0.0 = −12 dB, 0.5 = flat, 1.0 = +12 dB
mid  = 0.50
high = 0.50

# Uni-Vibe — a front-of-amp four-stage all-pass "vibe" (last pedal before the amp).
# mode 0.0 = chorus (dry + phase), 1.0 = vibrato (phase only).
[uni_vibe]
enabled = false       # optional, defaults to true when the section is present
rate  = 0.30
depth = 0.60
mix   = 0.50
mode  = 0.00

# The amp's front panel is model-specific. The fixed keys below are matched to the
# model's controls by role — a model without that control simply ignores the key
# (Vox and Fender have no Presence; the Plexi, Vox and Fender have no Master).
# For controls the fixed keys don't cover — Hiwatt's Normal, Vox's Cut, Fender's
# Reverb/Speed/Intensity — use the optional [amp.knobs] table, keyed by each
# control's slug.
[amp]
model  = "marshall"   # "marshall" | "mesa" | "randall" | "vox" | "hiwatt" | "plexi" | "fender"
gain   = 0.65         # the model's primary gain control (Gain / Volume / Top Boost / Brilliant)
bass   = 0.50
mid    = 0.45
treble = 0.65
presence = 0.50       # ignored by models with no Presence control
master = 0.55         # ignored by non-master models

# Optional per-control overrides, keyed by slug. Example for a Fender Twin:
# [amp.knobs]
# reverb    = 0.30
# speed     = 0.0
# intensity = 0.0

# Omit [cabinet] entirely and the current cabinet/mic settings are kept as-is.
[cabinet]
model     = "mesa"    # "mesa" | "marshall" | "orange" | "wem" | "vox" | "fender"
mic_pos   = 0.5       # 0.0 = edge/dark, 0.5 = neutral, 1.0 = center/bright (default 0.5)
mic_blend = 0.15      # 0.0 = SM57 dynamic, 1.0 = R121 ribbon (default 0.15)
mic_room  = 0.15      # 0.0 = dry close mic, 1.0 = full room mic (default 0.15)

# Graphic EQ (Boss GE-7): seven band faders + output level, all 0.0–1.0 with
# 0.5 = flat/unity. Sits in the effects loop, just after the cab.
[graphic_eq]
enabled = false       # optional, defaults to true when the section is present
band1 = 0.50          # 100 Hz   (0.0 = −12 dB, 0.5 = 0 dB, 1.0 = +12 dB)
band2 = 0.50          # 220 Hz
band3 = 0.50          # 470 Hz
band4 = 0.50          # 1 kHz
band5 = 0.50          # 2.2 kHz
band6 = 0.50          # 4.7 kHz
band7 = 0.50          # 10 kHz
level = 0.50          # output make-up (0.0 = −15 dB, 0.5 = unity, 1.0 = +15 dB)

[eq]
enabled = true        # optional, defaults to true
low  = 0.50           # 0.0 = −15 dB, 0.5 = 0 dB, 1.0 = +15 dB
mid  = 0.50
high = 0.50

# Omit [flanger] entirely to leave it off (the default for the bundled presets),
# or include it with enabled = false to store values but keep it bypassed.
[flanger]
enabled  = false      # optional, defaults to true when the section is present
rate     = 0.30       # 0.0 – 1.0  (LFO speed, 0.05–5 Hz, exponential)
depth    = 0.55       # 0.0 – 1.0  (sweep width; delay swings ~0.5–5 ms)
feedback = 0.35       # 0.0 – 1.0  (regeneration, capped at 90%)
mix      = 0.50       # 0.0 = dry, 0.5 = deepest flange, 1.0 = fully wet

# Omit [chorus] entirely to leave it off (the default for the bundled presets),
# or include it with enabled = false to store values but keep it bypassed.
[chorus]
enabled  = false      # optional, defaults to true when the section is present
rate     = 0.25       # 0.0 – 1.0  (LFO speed, 0.05–5 Hz, exponential)
depth    = 0.50       # 0.0 – 1.0  (sweep width; delay swings ~8–20 ms)
mix      = 0.50       # 0.0 = dry, 0.5 = classic chorus, 1.0 = fully wet

# Omit [phaser] entirely to leave it off (the default for the bundled presets),
# or include it with enabled = false to store values but keep it bypassed.
[phaser]
enabled  = false      # optional, defaults to true when the section is present
rate     = 0.30       # 0.0 – 1.0  (LFO speed, 0.05–5 Hz, exponential)
depth    = 0.70       # 0.0 – 1.0  (sweep width; break freq glides ~200 Hz–1.6 kHz)
feedback = 0.40       # 0.0 – 1.0  (regeneration, capped at 90%; higher = resonant)
mix      = 0.50       # 0.0 = dry, 0.5 = deepest phase, 1.0 = fully wet

# Tremolo / Vibrato: one LFO, blended from amplitude tremolo to pitch vibrato.
[tremolo]
enabled = false       # optional, defaults to true when the section is present
rate    = 0.35        # 0.0 – 1.0  (LFO speed, 0.5–14 Hz, exponential)
depth   = 0.55        # 0.0 – 1.0  (modulation intensity)
shape   = 0.00        # 0.0 = sine swell, 1.0 = soft-square on/off chop
mode    = 0.00        # 0.0 = pure tremolo (amplitude), 1.0 = pure vibrato (pitch)

[delay]
enabled  = true       # optional, defaults to true
type     = 0.0        # 0.0 = digital ping-pong, 1.0 = tape (Echoplex-style)
time     = 0.30       # 0.0 = 0 ms, 1.0 = 500 ms
feedback = 0.40       # 0.0 – 1.0 (internally capped at 85% digital / 70% tape)
mix      = 0.30       # 0.0 = dry, 1.0 = fully wet

[reverb]
enabled = true        # optional, defaults to true
room = 0.55
damp = 0.40
mix  = 0.25

# Signal-chain order, input to output. Optional — omit [chain] entirely and the
# preset uses the shipped order (this is how all older presets behave).
# Unknown names are ignored, duplicates collapsed, and any missing stage is
# appended in default order, so a hand-edited list can't build a half chain.
# The cab must always follow its amp (the reverse is repaired on load). Older
# files that use the pre-split "ampcab" name expand to "amp", "cab" unchanged.
# Stage names: gate whammy wah comp fuzz ts ds metal preeq vibe amp cab
#              geq eq flanger chorus phaser trem delay reverb
[chain]
order = ["gate", "whammy", "wah", "comp", "fuzz", "ts", "ds", "metal", "preeq", "vibe", "amp", "cab", "geq", "eq", "flanger", "chorus", "phaser", "trem", "delay", "reverb"]
```

<div class="note">
Drop the file in <code>~/.config/rusty-amp/presets/</code> and it will appear in the preset browser the next time you open it (or save another preset to trigger a reload).
</div>
