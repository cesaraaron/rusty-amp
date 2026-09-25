# Fidelity references (Phase 0)

This document is the Phase 0 reference matrix required by [`plan.md`](plan.md)
before any tone, amp, cabinet, or preset changes. It has two jobs:

1. Record, **from the code and bundled TOML only**, what each preset currently
   does (the factual baseline). Nothing here is a claim about a recording.
2. Provide the place where a human or a following agent logs *sourced* evidence
   (interviews, session notes, schematics, manuals, measured IRs) and marks each
   claim **documented**, **plausible**, or **unknown**.

> **Nothing in the "Reference needs" or "Source log" sections is verified.**
> Until a source is logged against a row, the preset must be described as
> *inspired by*, not as an exact session rig. Do not infer a session rig from a
> later touring board or an artist's general preferences.

## How to use this file

- The inventory in §1 is generated from `presets/*.toml`. If a preset changes,
  update its inventory row in the same PR.
- Fill §2's source log per preset as evidence is found. Keep *rig accuracy* and
  *audible match to the record* as separate columns/notes.
- When a preset's description contradicts its enabled path, fix the description
  (or add the missing device) — never invent history to justify the code.
- The firmware/DSP side of each named device (is the Fuzz voice actually a Big
  Muff? is the "spring" a real spring?) is tracked in
  [`IMPLEMENTATION-NOTES.md`](IMPLEMENTATION-NOTES.md) and `plan.md` Phases 3-4.

## Confidence legend

| Label | Meaning |
| ----- | ------- |
| **documented** | A contemporaneous or authoritative source names the exact gear/version. |
| **plausible** | Consistent with several sources but not directly confirmed for the session. |
| **unknown** | Default. No firm evidence; treat the preset as inspired-by. |

## Fuzz voice mapping (from `src/dsp/effects/fuzz.rs`)

`[fuzz] type` is normalized `0.0-1.0`; the DSP selects a voice by threshold:

| `type` | Voice |
| ------ | ----- |
| low (~`0.0`) | Big Muff (mid-scooped) |
| mid (~`0.5`) | Fuzz Face (rounder germanium) |
| high (~`1.0`) | Tone Bender MkII (hotter, sharper knee) |

## Echo mapping (from `src/dsp/effects/delay.rs`)

`[delay] type`: `0.0` = digital stereo ping-pong, `1.0` = EP-3-style tape echo
(single-time, mono-on-channel, wow/flutter). There is **no** Binson Echorec
model; a "Echorec" description currently selects the tape approximation. The
`time` knob maps `0..1` to `0..500 ms`.

---

## 1. Code-derived inventory

All 15 bundled presets use the **shipped default chain order** (none set
`[chain]`): gate → whammy → wah → comp → fuzz → ts → ds → metal → preeq → vibe →
**amp** → **cab** → geq → eq → flanger → chorus → phaser → trem → delay →
reverb. Only the enabled devices are listed below. Noise gate and cabinet are
always explicit (deterministic loading); `master` width defaults to `1.3`
everywhere.

| Preset | Amp | Cab | Enabled effects (in signal order) | Delay | Fuzz |
| ------ | --- | --- | --------------------------------- | ----- | ---- |
| `acdc_back_in_black` | plexi | marshall | gate, preeq, eq, reverb | off | off |
| `acdc_highway_to_hell` | plexi | marshall | gate, preeq, eq, reverb | off | off |
| `eagles_hotel_california_clean` | fender | fender | gate, comp, eq, chorus, delay, reverb | digital, 0.42 | off |
| `eagles_hotel_california_solo` | plexi | marshall | gate, comp, ts, preeq, eq, delay, reverb | digital (default), 0.40 | off |
| `led_zeppelin_stairway_solo` | plexi | marshall | gate, comp, ts, preeq, eq, delay, reverb | digital (default), 0.26 | off |
| `led_zeppelin_whole_lotta_love` | plexi | marshall | gate, fuzz, preeq, eq, delay, reverb | tape, 0.30 | Tone Bender MkII |
| `pink_floyd_another_brick_pt2` | hiwatt | wem | gate, comp, fuzz, preeq, eq, phaser, delay, reverb | tape, 0.60 | Big Muff |
| `pink_floyd_comfortably_numb_solo_1` | hiwatt | wem | gate, comp, fuzz, preeq, eq, flanger, delay, reverb | tape, 0.88 | Big Muff |
| `pink_floyd_comfortably_numb_solo_2` | hiwatt | wem | gate, comp, fuzz, preeq, eq, flanger, delay, reverb | tape, 0.90 | Big Muff |
| `pink_floyd_money` | hiwatt | wem | gate, wah, fuzz, eq, delay, reverb | tape, 0.58 | Fuzz Face |
| `pink_floyd_shine_on_crazy_diamond` | hiwatt | wem | gate, comp, fuzz, ts, preeq, vibe, eq, delay, reverb | tape, 0.66 | Big Muff |
| `pink_floyd_time_chorus` | hiwatt | wem | gate, preeq, vibe, eq, delay, reverb | tape, 0.75 | off |
| `pink_floyd_time_solo` | hiwatt | wem | gate, comp, fuzz, preeq, vibe, eq, delay, reverb | tape, 0.62 | Fuzz Face |
| `van_halen_aint_talkin_bout_love` | plexi | marshall | gate, ts, preeq, eq, flanger, phaser, delay, reverb | tape, 0.26 | off |
| `van_halen_brown_sound` | plexi | marshall | gate, ts, preeq, eq, phaser, delay, reverb | tape, 0.24 | off |

### Description-vs-path flags

These are contradictions provable from the TOML alone (see commit history in
`IMPLEMENTATION-NOTES.md` for the fixes). The **Claimed** text and **Path** are
both reproduced so the flag is auditable.

| Preset | Claimed | Actual enabled path | Flag |
| ------ | ------- | ------------------- | ---- |
| `acdc_back_in_black` | "no pedals" | pre-EQ + parametric EQ + reverb (+ gate) | Overclaim; there is no pitch/wah/drive/fuzz, but the path has EQ + reverb. |
| `acdc_highway_to_hell` | "no pedals in the way" | pre-EQ + parametric EQ + reverb (+ gate) | Same as above. |
| `led_zeppelin_stairway_solo` | "cranked small-amp crunch"; site "Echoplex slap" | Plexi + Greenback 4×12; tape-echo field **absent** so the delay resolves to **digital ping-pong** | Amp description and echo type both mismatch. |
| `pink_floyd_*` (Echorec) | "Echorec repeat/delay" | generic EP-3-style **tape** delay, no Binson model | Named hardware is an approximation. |
| `pink_floyd_another_brick_pt2` | "slow Phase 90 sweep" | generic 4-stage stereo phaser | Named hardware is an approximation. |
| `pink_floyd_comfortably_numb_*` | "Electric Mistress" | generic stereo flanger | Named hardware is an approximation. |
| `van_halen_*` | dimed Plexi + Phase/flange/echo | additionally enables **TS-808**, pre-EQ and parametric EQ | Unmentioned enabled drive/EQ. |

---

## 2. Reference matrix

For each preset, the questions to answer with sources. Record the answer and its
confidence. Leave blank until sourced.

### `acdc_back_in_black.toml` / `acdc_highway_to_hell.toml`

- Amp revision/channel: which Super Lead era did AC/DC use on each album?
- Cab & speakers: Greenback model/era, open/closed, mic position, room.
- Effects & order: were the EQs in the preset actually used, or compensating for
  the cab model? Any boost?
- Known studio processing vs the guitar-amp chain.
- Sources:

### `eagles_hotel_california_clean.toml` / `_solo.toml`

- Clean amp/guitar parts vs the multi-guitar arrangement; is one preset enough?
- Lead: TS + compressor + EQ — used, or model compensation?
- Reverb: the Twin's onboard spring (`[amp.knobs] reverb = 0.30`) plus the rack
  reverb — is the stacking intended?
- Sources:

### `led_zeppelin_stairway_solo.toml` / `_whole_lotta_love.toml`

- Session amp/guitar/cabinet **per track**; small combo vs Plexi/Greenback.
- Tone Bender use and version; TS/slap delay/compression presence.
- Sources:

### `pink_floyd_*` (7 files)

- Per-song/session fuzz (Muff version vs Fuzz Face), boost, wah, Uni-Vibe,
  Electric Mistress/phase, and echo device (Binson/Echoplex) **and position**.
- Was any claimed wah manually rocked (the DSP `wah` is an envelope auto-wah)?
- Studio EQ/compression vs the amp/cab chain; is the two-EQ stack compensating?
- Sources:

### `van_halen_aint_talkin_bout_love.toml` / `_brown_sound.toml`

- First-album Plexi revision/power supply, speaker/mic, Phase 90 / MXR flanger
  placement, tape echo, and recording-chain contribution.
- Are the enabled TS-808/pre-EQ/post-EQ compensating for model voicing?
- Sources:

---

## 3. Source log template

Copy a block per preset (or per claim) as evidence is collected.

```text
Preset:            <file>
Claim:             <amp revision / cab / effect / order / mic / studio processing>
Claimed by code:   <what the preset currently does>
Source:            <citation: interview, session notes, schematic, manual, measured IR, photo>
Recording vs live: <which recording/section; live context noted separately>
Supports:          <documented | plausible | unknown>
What it changes:   <the preset/DSP change this source would justify>
Listen/measure:    <blind A/B or measurement notes, if done>
```

## 4. What is explicitly out of scope here

- No historical claim is considered verified by this file.
- No DSP retuning is recorded here; those belong to `plan.md` Phases 3-4 and
  must cite a source.
- The "rescue EQ / extra pedal" question is answered only by a level-matched,
  preferably blind A/B against a reference, not by reading code.
