# Fidelity references (Phase 0)

This document is the Phase 0 reference matrix required by [`fidelity-plan.md`](../fidelity-plan.md)
before any tone, amp, cabinet, or preset changes. It has two jobs:

1. Record, **from the code and bundled TOML only**, what each preset currently
   does (the factual baseline). Nothing here is a claim about a recording.
2. Provide the place where a human or a following agent logs *sourced* evidence
   (interviews, session notes, schematics, manuals, measured IRs) and marks each
   claim **documented**, **plausible**, or **unknown**.

> **Recording dates/credits are cited to Wikipedia (secondary).** Gear claims —
> amps, cabs, pedals, mics — are **not yet verified**: until a source is logged
> against a gear row, the preset must be described as *inspired by*, not as an
> exact session rig. Do not infer a session rig from a later touring board or an
> artist's general preferences.

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

> **Status.** Recording dates, studios, producers and guitar credits below are
> cited to **Wikipedia** (secondary; approved for date/credit facts) and marked
> `documented`. **Gear** claims (amps, cabs, pedals, mics) are **not** yet
> sourced — they stay `plausible`/`unknown` with `Source: _TBD_`.

### `acdc_back_in_black.toml` / `acdc_highway_to_hell.toml`

- *Back in Black* (1980): recorded Apr–May 1980 at Compass Point (Nassau), mixed
  at Electric Lady (NYC); produced by Mutt Lange; Angus Young lead / Malcolm
  Young rhythm. · Supports: `documented` · Source: <https://en.wikipedia.org/wiki/Back_in_Black>
- *Highway to Hell* (1979): recorded 24 Mar–14 Apr 1979 at Roundhouse (London),
  mixed at Basing Street; produced by Mutt Lange; Angus lead / Malcolm rhythm. ·
  Supports: `documented` · Source: <https://en.wikipedia.org/wiki/Highway_to_Hell_(album)>
- Gear (SG/Gretsch into Marshall, Greenback 4×12, no drive pedals): not stated by
  the above. · Supports: `plausible` · Source: _TBD_
- **Contradiction:** both presets say "no pedals" yet enable pre-EQ + parametric
  EQ + reverb — decide studio/mix processing vs model compensation. (Phase 5)

### `eagles_hotel_california_clean.toml` / `_solo.toml`

- Recorded (1976) at Record Plant (LA) and Criteria (Miami); produced by Bill
  Szymczyk; written by Felder (music), Frey/Henley (lyrics). · Supports:
  `documented` · Source: <https://en.wikipedia.org/wiki/Hotel_California>
- Intro came from Felder's demo with a **12-string guitar** (Latin/reggae feel,
  working title "Mexican Reggae"); the closing solo is **Joe Walsh (Fender
  Telecaster) trading with Don Felder (Gibson Les Paul), then harmonized**. ·
  Supports: `documented` · Source: <https://en.wikipedia.org/wiki/Hotel_California>
- Gear/amps and mic not stated. · Supports: `unknown` · Source: _TBD_
- **Contradiction:** the clean preset (Twin + chorus + digital delay + two
  reverbs) does not map to a 12-string intro; one preset cannot represent two
  separately recorded, harmonized lead players. (Phase 5)

### `led_zeppelin_stairway_solo.toml` / `_whole_lotta_love.toml`

- *Stairway to Heaven* (1971): recorded Dec 1970–Feb 1971 at Island Studios
  (London), Rolling Stones Mobile (Stargroves) and Ronnie Lane's Mobile (Headley
  Grange); produced by Jimmy Page. · Supports: `documented` · Source:
  <https://en.wikipedia.org/wiki/Stairway_to_Heaven>
- Named gear (citing Page): Harmony Sovereign H1260 acoustic, Fender Electric XII
  12-string, and a **1959 Fender Telecaster into a Supro amplifier** (Page:
  "could have been a Marshall"); Gibson EDS-1275 double-neck live. · Supports:
  `plausible` · Source: <https://en.wikipedia.org/wiki/Stairway_to_Heaven>
- *Whole Lotta Love* (1969): recorded Apr 1969 at Olympic (London) and A&M
  (Hollywood); produced by Jimmy Page; Page on guitars; a solid-state amp and
  backwards-echo production are named. · Supports: `documented` · Source:
  <https://en.wikipedia.org/wiki/Whole_Lotta_Love>
- **Contradiction:** `stairway_solo` uses Plexi + Greenback 4×12 + TS, which does
  not match the named Telecaster→Supro; the Tone Bender on *Whole Lotta Love* is
  still unverified. (Phase 5)

### `pink_floyd_*` (9 files)

- *The Dark Side of the Moon* (1973): recorded 31 May 1972–9 Feb 1973 at Abbey
  Road; produced by Pink Floyd; Gilmour on guitars. · Supports: `documented` ·
  Source: <https://en.wikipedia.org/wiki/The_Dark_Side_of_the_Moon>
- *Wish You Were Here* (1975): recorded 13 Jan–28 Jul 1975 at Abbey Road;
  produced by Pink Floyd; Gilmour on guitars. · Supports: `documented` · Source:
  <https://en.wikipedia.org/wiki/Wish_You_Were_Here_(Pink_Floyd_album)>
- *The Wall* (1979): recorded Dec 1978–Nov 1979 (Britannia Row, Super Bear,
  Miraval, CBS 30th St, Producers Workshop, Cherokee); produced by Ezrin,
  Gilmour, Guthrie, Waters; Gilmour on guitars (Lee Ritenour rhythm on one
  track). · Supports: `documented` · Source: <https://en.wikipedia.org/wiki/The_Wall>
- Gear (Hiwatt DR103 + WEM/Fane, Fuzz Face / Big Muff, Uni-Vibe, Echorec/Echoplex,
  Power Boost) and a manually rocked wah on *Money*: not stated. · Supports:
  `plausible`/`unknown` · Source: _TBD_
- **Contradiction:** many Floyd leads are double-tracked and carry studio
  EQ/compression; the two-EQ stack in the presets may be compensating. (Phase 5)

### `guns_n_roses_november_rain_solo.toml`

- From *Use Your Illusion I* (1991), single Feb 1992; written by Axl Rose;
  produced by Mike Clink and Guns N' Roses; recorded at A&M, Record Plant, Studio
  56, Image Recording, Conway, Northstar (violins), Metalworks; Slash lead
  guitar, Izzy Stradlin rhythm. · Supports: `documented` · Source:
  <https://en.wikipedia.org/wiki/November_Rain>
- Slash's solo developed from an improvisation (per his autobiography). ·
  Supports: `plausible` · Source: <https://en.wikipedia.org/wiki/November_Rain>
- Gear (Les Paul + which Marshall + wah) not stated. · Supports: `unknown` ·
  Source: _TBD_

### `van_halen_beat_it_solo.toml`

- Michael Jackson, "Beat It" (1982, *Thriller*): recorded Oct 1982 at Westlake
  (LA) and Hayvenhurst (Encino); produced by Quincy Jones; **Eddie Van Halen –
  guitar solo**; Paul Jackson Jr. – rhythm guitar; Steve Lukather – lead
  guitar/bass. · Supports: `documented` · Source: <https://en.wikipedia.org/wiki/Beat_It>
- Gear (citing sources): EVH used a **rented Marshall amplifier**, his
  Frankenstrat, and an **Echoplex**; two takes, solo played over the verse. ·
  Supports: `plausible` · Source: <https://en.wikipedia.org/wiki/Beat_It>
- The rhythm on the record is not EVH — out of scope for this preset. · Supports:
  `documented` · Source: <https://en.wikipedia.org/wiki/Beat_It>
- **Contradiction:** the preset uses `plexi`, but the source says a *rented
  Marshall* of unstated model. (Phase 3/5)


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
