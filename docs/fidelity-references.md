# Fidelity references (Phase 0)

This document is the Phase 0 reference matrix required by [`fidelity-plan.md`](../fidelity-plan.md)
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
  [`fidelity-implement.md`](../fidelity-implement.md) and `fidelity-plan.md` Phases 3-4.

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

All 17 bundled presets use the **shipped default chain order** (none set
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
| `guns_n_roses_november_rain_solo` | marshall | marshall | gate, delay, reverb | tape, 0.40 | off |
| `led_zeppelin_stairway_solo` | plexi | marshall | gate, comp, ts, preeq, eq, delay, reverb | digital (default), 0.26 | off |
| `led_zeppelin_whole_lotta_love` | plexi | marshall | gate, fuzz, preeq, eq, delay, reverb | tape, 0.30 | Tone Bender MkII |
| `pink_floyd_another_brick_pt2` | hiwatt | wem | gate, comp, fuzz, preeq, eq, phaser, delay, reverb | tape, 0.60 | Big Muff |
| `pink_floyd_comfortably_numb_solo_1` | hiwatt | wem | gate, comp, fuzz, preeq, eq, flanger, delay, reverb | tape, 0.88 | Big Muff |
| `pink_floyd_comfortably_numb_solo_2` | hiwatt | wem | gate, comp, fuzz, preeq, eq, flanger, delay, reverb | tape, 0.90 | Big Muff |
| `pink_floyd_have_a_cigar_solo` | hiwatt | wem | gate, comp, preeq, eq, delay, reverb | tape, 0.50 | off |
| `pink_floyd_money` | hiwatt | wem | gate, wah, fuzz, eq, delay, reverb | tape, 0.58 | Fuzz Face |
| `pink_floyd_mother_solo` | hiwatt | wem | gate, comp, preeq, eq, delay, reverb | tape, 0.55 | off |
| `pink_floyd_shine_on_crazy_diamond` | hiwatt | wem | gate, comp, fuzz, ts, preeq, vibe, eq, delay, reverb | tape, 0.66 | Big Muff |
| `pink_floyd_time_chorus` | hiwatt | wem | gate, preeq, vibe, eq, delay, reverb | tape, 0.75 | off |
| `pink_floyd_time_solo` | hiwatt | wem | gate, comp, fuzz, preeq, vibe, eq, delay, reverb | tape, 0.62 | Fuzz Face |
| `van_halen_beat_it_solo` | plexi | marshall | gate, phaser, delay, reverb | tape, 0.22 | off |

### Description-vs-path flags

These are contradictions provable from the TOML alone (see commit history in
[`fidelity-implement.md`](../fidelity-implement.md) for the fixes). The **Claimed** text and **Path** are
both reproduced so the flag is auditable.

| Preset | Claimed | Actual enabled path | Flag |
| ------ | ------- | ------------------- | ---- |
| `acdc_back_in_black` | "no pedals" | pre-EQ + parametric EQ + reverb (+ gate) | Overclaim; there is no pitch/wah/drive/fuzz, but the path has EQ + reverb. |
| `acdc_highway_to_hell` | "no pedals in the way" | pre-EQ + parametric EQ + reverb (+ gate) | Same as above. |
| `led_zeppelin_stairway_solo` | "cranked small-amp crunch"; site "Echoplex slap" | Plexi + Greenback 4×12; tape-echo field **absent** so the delay resolves to **digital ping-pong** | Amp description and echo type both mismatch. |
| `pink_floyd_*` (Echorec) | "Echorec repeat/delay" | generic EP-3-style **tape** delay, no Binson model | Named hardware is an approximation. |
| `pink_floyd_another_brick_pt2` | "slow Phase 90 sweep" | generic 4-stage stereo phaser | Named hardware is an approximation. |
| `pink_floyd_comfortably_numb_*` | "Electric Mistress" | generic stereo flanger | Named hardware is an approximation. |

---

## 2. Reference matrix

> **Uncited draft.** The lines below are *widely reported* claims gathered for the
> citation pass to confirm or reject. `Supports` is the maximum tier a claim could
> reach once a source is logged; until then it is **not** evidence. `Source: _TBD_`
> means no citation yet. Do not treat any line here as verified.

### `acdc_back_in_black.toml` / `acdc_highway_to_hell.toml`

- Angus — Gibson SG into a Marshall (JTM45 / 1987 / Super Lead era), **no drive
  pedals**; Malcolm — Gretsch into a Marshall. · Supports: `plausible` · Source: _TBD_
- Marshall 4×12 with Greenbacks, close-miked, dry. · Supports: `plausible` · Source: _TBD_
- Sessions: *Highway to Hell* (1979) at Albert Studios, Sydney; *Back in Black*
  (1980) at Compass Point, Bahamas; Vanda/Young and Mutt Lange production. ·
  Supports: `unknown` · Source: _TBD_
- **Contradiction:** both presets say "no pedals" yet enable pre-EQ + parametric
  EQ + reverb — decide studio/mix processing vs model compensation. (Phase 5)

### `eagles_hotel_california_clean.toml` / `_solo.toml`

- The clean intro is commonly reported as a **12-string acoustic** plus electric,
  so the Twin + chorus + digital delay + two-reverb preset may not map to a
  recorded part. · Supports: `unknown` · Source: _TBD_
- The solo is **harmonized** (Felder + Walsh), recorded as separate parts; a
  single mono preset cannot reproduce two performances. · Supports: `plausible` · Source: _TBD_
- Leads commonly reported through **small Fender tweed combos**, not a dimed
  Plexi. · Supports: `plausible` · Source: _TBD_
- **Contradiction:** the lead enables compressor + TS + two EQs — likely model
  compensation for a mis-voiced base. (Phase 5)

### `led_zeppelin_stairway_solo.toml` / `_whole_lotta_love.toml`

- Stairway solo (1971) commonly reported as a **Telecaster into a small Supro
  combo**; the current Plexi + Greenback 4×12 + TS is a likely mismatch. ·
  Supports: `plausible` · Source: _TBD_
- Whole Lotta Love (1969): Page, Les Paul/Tele, **Sola Sound Tone Bender MkII**,
  some echo. · Supports: `plausible` · Source: _TBD_
- Sessions (Headley Grange / Olympic / Island) and per-track gear need
  verification. · Supports: `unknown` · Source: _TBD_

### `pink_floyd_*` (9 files)

- Core rig: Gilmour — Stratocaster → **Hiwatt DR103 → WEM 4×12 (Fane)**, with a
  **Colorsound Power Boost** as the boost. · Supports: `plausible` · Source: _TBD_
- Fuzz: **Fuzz Face** for early *Dark Side* material; **Big Muff (Ram's Head-era)**
  from *Wish You Were Here* onward. · Supports: `plausible` · Source: _TBD_
- Echo: **Binson Echorec** (drum echo) on *Dark Side*; **Echoplex** later. The DSP
  has no Echorec (the tape mode approximates). · Supports: `plausible` · Source: _TBD_
- Modulation: **Uni-Vibe** (*Time*, *Shine On*); **Electric Mistress** flanger on
  *The Wall*; the "Phase 90" claim on *Another Brick pt2* is unverified. ·
  Supports: `plausible` · Source: _TBD_
- `money`: a manually rocked wah is commonly claimed, but the DSP `wah` is an
  envelope auto-wah. · Supports: `unknown` · Source: _TBD_
- `mother_solo` (1979) and `have_a_cigar_solo` (1975): amp/cab/pedals unverified;
  the sparse Hiwatt/WEM starting points are placeholders. · Supports: `unknown` · Source: _TBD_
- **Contradiction:** many Floyd leads are double-tracked and carry studio
  EQ/compression; the two-EQ stack in the presets may be compensating. (Phase 5)

### `guns_n_roses_november_rain_solo.toml`

- Slash — Les Paul → a **Marshall** (JCM800 / Silver Jubilee era), possibly with a
  wah; the wide delay/reverb may be the record's mix rather than the rig. ·
  Supports: `plausible` · Source: _TBD_
- The orchestral arrangement and separately recorded solos mean one preset is an
  approximation. · Supports: `unknown` · Source: _TBD_

### `van_halen_beat_it_solo.toml`

- EVH (1982) is commonly reported on his main **Plexi (1959 Super Lead)** into a
  4×12, with an MXR Phase 90 / Echoplex; the solo may be double-tracked. ·
  Supports: `plausible` · Source: _TBD_
- The **rhythm is not EVH** (credited to the session players) — out of scope
  for this preset. · Supports: `plausible` · Source: _TBD_


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
Listen/measure:    <blind A/B or measurement notes; cite a harness run and its
                    report.toml (DI names, width, REFERENCE_VERSION)>
```

## 4. What is explicitly out of scope here

- No historical claim is considered verified by this file.
- No DSP retuning is recorded here; those belong to `fidelity-plan.md` Phases 3-4 and
  must cite a source.
- The "rescue EQ / extra pedal" question is answered only by a level-matched,
  preferably blind A/B against a reference, not by reading code.
