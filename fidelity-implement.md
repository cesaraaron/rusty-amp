# Fidelity — implementation notes (as-built / handover)

Companion to [`fidelity-plan.md`](fidelity-plan.md) (formerly `plan.md`). The
plan is the **design / acceptance** document; this file is the **as-built**
record: what shipped, review findings, invariants, deviations, and what is still
missing. Evidence for historical gear claims lives in
[`docs/fidelity-references.md`](docs/fidelity-references.md). Read all three
before reviewing or continuing. (This file was formerly
`IMPLEMENTATION-NOTES.md`.)

> **De-fork note (2026-09-25).** This repository is a modified fork renamed
> **rusty-riff**. The Eleventy docs site (`site/`) that earlier entries below
> reference was removed in the cleanup; current documentation is the README,
> `CONTRIBUTING.md`, and the in-app `K` help. Historical `site/*` mentions below
> describe what those past commits did at the time.

## Status

| Area | State | Where |
| --- | --- | --- |
| De-fork cleanup — docs site removed, project renamed `rusty-riff` | **Done** | increment log |
| Phase 1 — bypass transparency, order snapshot, route tests, studio-master width | **Done** (see review: the order snapshot needs A1) | below |
| Phase 2 items 1–4 — Amp/Cab split, migration, UI | **Done** | below |
| Phase 2 item 5 — real preamp/loop/power-amp split | Not started | plan Phase 2.5 |
| Phase 0 — reference matrix | **Scaffold only**: no sources logged | `docs/fidelity-references.md` |
| Phase 0 — offline harness, CPU/latency capture | Not started → **Workstream B (B1, B2)** | plan "Next increments" |
| Workstream A — routing hardening (review findings) | **Pending** | plan "Next increments" |
| Workstream B — input calibration + harness | **Pending** | plan "Next increments" |
| Phases 3–5 — amp/cab fidelity, named pedals, preset rebuild | Not started | plan Phases 3–5 |

The work below changed routing, topology, and documentation only. **No voicing
has changed yet**, so none of the bundled Pink Floyd / Eagles / Led Zeppelin
presets is closer to the records than before this roadmap started. That is
expected: the plan requires references and a measurement harness first.

Fidelity phases (0, 3, 4, 5) — see
[Open gaps](#open-gaps-for-a-following-agent) for the references and decisions
each one still needs.

Scope agreed with the maintainer: **Phase 1 routing patch (including item 4,
the configurable studio master), then Phase 2 amp/cab split**, delivered as
small commits with all findings recorded here.

---

## What changed

### 1. Bypass is now wire-transparent — `fix(dsp): make bypassed stages wire-transparent`

- File: `src/dsp/mod.rs`
- Added `Params::stage_enabled(stage) -> bool` (mirrors each stage's
  `*_enabled` atomic; `Amp`/`Cab` are always live).
- `DspChain::run_ordered_stage` (`src/dsp/mod.rs`) now returns the incoming
  `Sig` **before** any mono↔stereo bridging when the stage is disabled.
- Tests added: `bypassed_mono_stage_preserves_stereo`,
  `bypassed_stereo_stage_preserves_mono`, `stage_enabled_mirrors_bypass_flags`.

**Bug fixed:** the dispatch used to bridge domains first and consult the bypass
flag second. A disabled mono pedal on a stereo feed still evaluated
`0.5*(l+r)` and duplicated the result, collapsing L/R; a disabled stereo pedal
on mono promoted the signal early. Both are gone.

### 2. Whole-order snapshot via seqlock — `fix(dsp): publish the chain order as one coherent snapshot`

- File: `src/dsp/mod.rs`
- Added `Params::chain_seq: Arc<AtomicU64>` guarding the existing
  `chain_order: Arc<[AtomicU8; CHAIN_LEN]>`.
- `set_chain_order` publishes under an odd/even sequence; `chain_slots` reads
  the slots and retries unless the sequence is stable and even.
- `process_block` and `process` snapshot **once**; `process_core` takes
  `&order`. The old per-**sample** 19-atomic read is gone.
- Test added: `chain_order_snapshot_is_never_torn_under_concurrent_swaps`
  (200k concurrent writes, 200k reads, every read must be a whole permutation).

**Deviation from the roadmap's suggestion.** The plan proposed an `rtrb`
command ring carrying the full order by value. A `SeqCst` seqlock was chosen
instead because it keeps the existing shared-`Arc<Params>` architecture and
needs no constructor/ring plumbing at the ~15 `DspChain::new` call sites. It is
allocation-free and reads once per block.

> **Review correction (2026-09-25).** The original note called the seqlock
> "blocking-free". It is not: the reader spins **without bound** while the
> sequence is odd, so a preempted UI writer stalls the audio callback (priority
> inversion), and it is correct only with a single writer, which nothing
> enforces. The spin cost is not the problem; the unbounded wait is. Fix planned
> as **A1** (bounded `try_chain_slots` + audio-owned last-good order + a
> writer-only mutex). Also: `CHAIN_LEN` is now 20, not 19, since the amp/cab
> split.

### 3. Route-domain test matrix — `test(dsp): cover route-domain transparency and coherent reorder`

- File: `src/dsp/mod.rs`
- `bypassed_mono_stage_moved_after_live_stereo_is_identical` — end-to-end: with
  Chorus + Reverb live, moving a bypassed Wah from before the amp into the
  stereo region is bit-identical.
- `live_mono_stage_after_stereo_downsamples_to_mono` — a **live** mono stage
  still sums a stereo feed to dual mono, as documented.
- `rapid_reorder_keeps_a_complete_permutation` — 1000 adjacent swaps never
  yield a duplicate/missing stage.
- Existing `swapped_chain_order_changes_output` (noncommuting enabled effects
  differ) and `preset_chain_order_applies_and_round_trips` (save/load) already
  cover the remaining roadmap bullets and were kept.

### 4. Docs — this commit

- `site/presets.md` — replaced the false blanket claim ("omitting a section
  leaves that effect's current state unchanged"). Real `Preset::apply`
  semantics (`src/preset.rs:545`), now documented:
  - Required: `[tube_screamer]`, `[amp]`, `[reverb]`.
  - Omitted optional effect section ⇒ that effect is **turned off** (knob values
    retained but bypassed).
  - Exceptions that **retain** current state when omitted: `[noise_gate]`,
    `[cabinet]`.
  - Omitted `[chain]` ⇒ chain order resets to the shipped default.
- `site/how-it-works.md` — documented that only a **live** effect converts
  mono↔stereo; a bypassed stage is wire-transparent.
- This file.

---

## Phase 2 — amp/cab split

Commit: `feat(dsp): split the amp+cab block into separate Amp and Cab stages`

- **Enum** (`src/dsp/mod.rs`): `ChainStage::AmpCab` (one slot) became
  `Amp` + `Cab`, so `CHAIN_LEN` is now 20 (10 pre + amp + cab + 8 rack).
  `pedal_index`/`from_pedal_index` remapped; the rack pedals keep their 0–17
  indices.
- **Dispatch**: `run_pre`/`run_post`/`amp_cab`/`ampcab_index` were replaced by a
  single-pass `run_range` + `run_ordered_stage`. `Amp` always runs and folds its
  input to mono; `Cab` runs mono→stereo and is skipped only when
  `ext_amp_supplies_cab()` (a live full-rig AU) is true. Default-order rendering
  is bit-identical to Phase 1.
- **External amp path** (`process_block`): stages before `Amp` run per sample
  (mono), the hosted plugin processes the block, then stages after `Amp`
  (including `Cab` and any effects left between amp and cab) run per sample.
- **Sanitizing**: `sanitize_chain_order` now also repairs a cab placed before
  its amp (swap); `amp_precedes_cab` is the shared predicate.
- **Presets** (`src/preset.rs`): legacy `"ampcab"` in `[chain]` expands to
  consecutive `"amp"`, `"cab"` at the same position; new saves write the two
  names. Tests: `preset_legacy_ampcab_expands_to_amp_cab`,
  `preset_chain_order_applies_and_round_trips`.
- **UI**: the ribbon renders separate `AMP` / `CAB` tiles (`AU: NAME` replaces
  `AMP`; `IR: NAME` / `AU CAB` on the cab tile). `move_selected_stage` rejects a
  move that would put the cab before its amp
  (`move_selected_stage_moves_amp_and_cab_separately`). Golden snapshots
  re-blessed; the only change is the ribbon line splitting `AMP+CAB` into
  `AMP ──▶ CAB`.
- **Docs**: `site/presets.md`, `pedals.md`, `how-it-works.md`, `plugins.md`,
  `amps-cabs.md`, `index.md` updated for the two-stage model and the
  amp-before-cab constraint.

**Boundary semantics (item 2 of the roadmap).** Effects may be placed between
`Amp` and `Cab`; that region is documented/treated as line-level processing in a
virtual load box, not a pedal in the speaker cable. Ordinary pedals are never
*defaulted* there (the shipped order keeps every pedal before the amp or after
the cab). The richer "named regions" UI (labels/tooltips) and the optional real
preamp/loop/power-amp refactor (Phase 2 item 5) are **not** done.

---

## Phase 1 item 4 — configurable studio master

Commit: `feat(dsp): make the master-bus widener a configurable studio width`

- `Params::master_width` (`Arc<AtomicF32>`) drives `master_bus(l, r, width)`;
  the output soft limiter is applied independently of the width, so protection
  never depends on the setting. `DEFAULT_MASTER_WIDTH = 1.3` keeps every existing
  preset/recording sounding exactly as before.
- Read once per block, not per sample (same discipline as the chain order).
- `W` cycles neutral (`1.0`) ↔ studio wide (`1.3`) live, with a status toast;
  documented in the `K` help modal and the site key table.
- Preset `[master] width` round-trips; an omitted `[master]` resets to `1.3` for
  deterministic loading (`site/presets.md` documents it).
- Tests: `master_bus_width_is_neutral_at_one_and_limiter_is_independent`,
  `master_width_changes_the_output`,
  `preset_master_width_round_trips_and_defaults`.

**Decision (worth revisiting with the maintainer):** the roadmap prefers a
*neutral default* for a reference mode, but neutral-by-default would change the
sound of every bundled preset and the marketed "studio-grade stereo". The
default was therefore kept at the historic `1.3`, with neutral one keypress away
— the "explicit compatibility setting" the roadmap also allows. If the project
wants neutral-by-default, flip `DEFAULT_MASTER_WIDTH` to `1.0` and re-bless the
snapshots (no other change needed).

---

## Phase 0 foundation + preset accuracy cleanups

Commits: `docs: add the Phase 0 fidelity reference scaffold`,
`test(presets): assert bundled presets load deterministically`,
`docs(dsp): reconcile the TS-808 input-HP prose…`,
`docs(presets): make descriptions match the enabled signal path`.

- **Reference scaffold** — new [`docs/fidelity-references.md`](docs/fidelity-references.md):
  the code-derived inventory of all 15 presets (amp, cab, enabled effects in
  signal order, delay/fuzz mode), a description-vs-path flag table, the
  per-preset reference checklist, and a source-log template. It makes no
  historical claim; it is the baseline a following agent annotates.
- **Determinism regression** — `bundled_presets_load_deterministically` applies
  each preset onto a hostile prior state and compares a sound-determining
  fingerprint to a fresh load. Verified it fails when a preset omits
  `[noise_gate]`. All 15 already set `[noise_gate]`/`[cabinet]` and omit
  `[chain]`, so this only locks behavior.
- **TS-808 prose** — the module header now says 340 Hz (matching the constructor
  and `site/how-it-works.md`), flagged as an RC-derived estimate pending a
  measured response. **No DSP change.**
- **Preset descriptions** — fixed contradictions provable from the TOML, no
  parameter/tone changes:
  - AC/DC ×2 said "no pedals" while enabling pre-EQ + parametric EQ + reverb →
    "no drive pedals" + names the shaping.
  - Stairway solo said "small-amp" while selecting Plexi + Greenback 4×12, and
    its site blurb said "Echoplex slap" while the preset's `[delay]` omitted
    `type`, resolving to **digital ping-pong**. Description fixed; `type = 0.0`
    pinned explicitly (behavior-identical). The intended echo device is now a
    flagged reference question.
  - Pink Floyd `Echorec` / `Electric Mistress` / `Phase 90` are hedged as
    `-style` approximations (the DSP implements a generic tape echo / flanger /
    phaser), and Money's wah is described as what it is (auto-wah).
  - Van Halen presets now name the enabled Tube Screamer.
  - Mirrored in the `site/presets.md` bundled table.

---

## Findings not yet actioned

### TS-808 input-HP value still unverified

`src/dsp/effects/tube_screamer.rs` now documents the input coupling HP as
**340 Hz** (`0.047 µF` into `10 kΩ`). That is an RC estimate, not a measured
value; the code, the 720 Hz feedback-network peak, and the two EQ stages in the
Floyd presets may still be compensating for each other. Per the roadmap, do
**not** retune without a schematic or a measured TS-808 frequency response
(Phase 4). The prose contradiction itself is resolved.

### Not an actual effects loop yet

Phase 2 split `Amp` and `Cab` but the amp DSP is still one block. A genuine
amp effects loop (preamp → loop send/return → power amp) is Phase 2 item 5 and
remains unimplemented. Effects between `Amp` and `Cab` model a *virtual
load-box / post-power-amp line-level* path, not the amp's internal loop.

### No crossfade on the built-in ↔ AU toggle

A2 guarantees the built-in/AU switch lands on a **block boundary** (one coherent
`BlockRoute` per block), but it does not crossfade: a mid-block toggle is simply
deferred to the next block. A clickless crossfade remains out of scope.

---

## Review 2026-09-25

A code review of everything above. Each finding lists evidence and the plan
item that resolves it (see `fidelity-plan.md` → *Next increments*). Mark a
finding **resolved** here, with the commit, when its item ships.

### Code findings

| # | Severity | Finding | Evidence | Resolved by | Status |
| --- | --- | --- | --- | --- | --- |
| R1 | **High** | The chain-order seqlock reader spins without bound on the audio thread while a writer holds the sequence odd; a preempted UI writer stalls the callback. Single-writer is assumed, not enforced. | `Params::chain_slots` / `set_chain_order`, `src/dsp/mod.rs` ~1110–1140; called from `process` / `process_block` | A1 | **resolved** (A1) |
| R2 | Medium | Routing state is read at inconsistent rates: `use_ext_amp` per block, but the Cab stage's `ext_amp_supplies_cab()` per sample. A mid-block AU toggle can run the built-in amp with no cab for the rest of the block. `process()` never runs the AU yet skips the cab when a full-rig AU is flagged active. | `process_block` ~1697 vs `run_ordered_stage` ~1605; `ext_amp_supplies_cab` ~1379 | A2 | **resolved** (A2) |
| R3 | Low | With a **full-rig** AU, stages placed between AMP and CAB process the AU's already-miked output, not a line-level signal; the docs describe that region only as "virtual load box". | `run_ordered_stage` Cab arm; README and in-app `K` help | A4 | open |
| R4 | Low | `amp_stage` doc says it is bypassed when an external amp is active (only `process_block` does that); comments still say "19 byte stores" although `CHAIN_LEN = 20`. | `src/dsp/mod.rs` ~1107, ~1386 | A1, A2 | **resolved** (A1, A2) |
| R5 | Low | Relative links in `docs/fidelity-references.md` pointed at `plan.md` / `IMPLEMENTATION-NOTES.md` inside `docs/` (files that do not exist there). | `docs/fidelity-references.md` lines 3, 27, 86, 164 | Doc reorganization (this commit) | **resolved** |
| R6 | Low | The determinism test fingerprint omits amp knobs, fuzz/delay `type`, and knob values of enabled stages, so it would not catch a regression there (loading is currently correct: `apply` resets amp knobs to model defaults and serde defaults the types). | `rig_fingerprint`, `src/preset.rs` ~967 | A3 | open |
| R7 | Low | `on_input` resizes/extends buffers when a callback exceeds `MAX_BLOCK = 4096` frames — an allocation on the audio thread (rare). | `src/audio/mod.rs` ~835–849, `MAX_BLOCK` line 80 | A5 | open |
| R8 | Decision | `DEFAULT_MASTER_WIDTH = 1.3` kept for compatibility. For period-accurate presets, set `[master] width = 1.0` per preset (guitar on these records is a mono track) rather than flipping the global default. | `src/dsp/mod.rs` ~475 | Preset phase (deferred) | open |
| R9 | Gap | There is **no input-level calibration** anywhere: amp breakup depends on the user's interface gain, so presets tuned on one interface are under/over-driven on another. Likely cause of the "rescue" TS + two EQs in several presets. | `rg -i "input_gain\|trim\|calibrat" src/` finds nothing relevant | B3–B5, B8 | open |
| R10 | Gap | Phase 0 step 3 (offline harness) was not built. Analysis helpers (`db`, `rms`, `goertzel`, `ltas`, `envelope`, `percentile`) are duplicated across `examples/`. The timeline's raw takes + offline export (`src/export.rs`) already provide most of the rendering machinery. | `examples/di_compare.rs`, `drive_analysis.rs`, `amp_analysis.rs`, `knob_match.rs` | B1, B2 | open |

Verified correct during the review (no action): bypass wire-transparency;
Amp/Cab split and `"ampcab"` migration; `sanitize_chain_order` repair;
`Preset::apply` determinism for amp knobs and types; the take chain and live
chain each snapshot the order once per block; audio buffers are preallocated at
`MAX_BLOCK`; dry captures are 32-bit float.

### Deferred findings for the fidelity phases

Historical claims below are **commonly reported, not verified** — log sources
in `docs/fidelity-references.md` before changing any preset on their basis.

- **Anachronisms (objective, cheap to test).** The TS-808 (1979) is enabled in
  presets for earlier recordings: `led_zeppelin_stairway_solo` (1971),
  `pink_floyd_shine_on_crazy_diamond` (1975), `eagles_hotel_california_solo`
  (1976), both `van_halen_*` (1978). Proposal: add `year` metadata per preset and
  a test that fails when an enabled named device postdates the recording.
- **Stairway solo.** The cleanup changed the description *toward the code*
  (Plexi + Greenback 4×12 + TS). The commonly reported session rig is a
  Telecaster into a small Supro combo — the earlier "small-amp" wording was
  probably closer. Flag the code, not the description; no Supro-like model
  exists yet.
- **Hotel California.** The solo preset still says Plexi is "the cranked
  non-master head the Eagles actually used" — an unhedged claim the description
  pass missed. Commonly reported: Les Paul (Felder) into small Fender tweed
  amps. The intro is commonly reported as a 12-string acoustic, so the clean
  Twin + chorus + digital delay + two reverbs preset may not correspond to a
  recorded part.
- **Plexi rectifier.** The GZ34 tube-rectifier assumption fits a JTM45; 1959
  Super Leads from roughly 1967 onward are generally silicon-rectified, which
  covers the Zeppelin/AC/DC/EVH era. Check a schematic, then retune the sag
  (Phase 3), using the Workstream B harness for before/after.
- **Hiwatt DR103** silicon-rectified supply claim is consistent with the
  hardware; the Hiwatt/WEM pair dominates every Floyd preset, so it stays
  first in Phase 3.
- **Missing components with the largest expected impact** for Pink Floyd /
  Eagles / Led Zeppelin: Binson Echorec (multi-head drum echo; currently the
  EP-3 tape mode), a spring reverb for the Twin (currently Freeverb), small
  Supro-style and tweed-Deluxe-style combos (`add-amp-model` skill), a
  pickup/guitar-volume input model (single-coil vs humbucker loading; Fuzz Face
  cleanup), and a Colorsound Power Boost-style boost as the period-correct
  alternative to the TS.

---

## Increment log

Append one row per commit from Workstreams A/B onward.

| Commit | Item | Summary | Tests | Deviations |
| --- | --- | --- | --- | --- |
| `e77b04d` | docs | Renamed `plan.md` → `fidelity-plan.md`, `IMPLEMENTATION-NOTES.md` → `fidelity-implement.md`; added Workstreams A/B to the plan and this review; fixed links (R5). | n/a | — |
| `a100885` | chore | Removed the Eleventy docs site (`site/`), `.eleventy.js`, `package.json`, and the Pages workflow; kept `demo.gif`/`screenshot.png` as `assets/`. | n/a | site docs retired |
| `d6b8b53` | refactor | Renamed the crate/binary, config dir, CLAP host id, workspace file, release assets, and UI snapshots from `rusty-amp` to `rusty-riff`; added a one-time `~/.config/rusty-amp` → `~/.config/rusty-riff` migration. | `cargo test --all-features` (snapshot re-blessed) | — |
| `5c0fa6f` | docs | Replaced the README with a rusty-riff version; added `NOTICE` crediting the original project. | n/a | — |
| `26359e2` | docs | Rewrote `CONTRIBUTING.md` as local development notes (no fork/PR/site flow). | n/a | — |
| `3a13c5f` | docs | Updated and renamed `Agents.md` → `AGENTS.md`; dropped the docs-site section and old config paths. | n/a | — |
| `52d3195` | docs | Rewrote `.claude/skills/add-*` without the docs-site steps or PR flow. | n/a | — |
| A1 | routing | Bounded audio-thread chain-order read: `try_chain_slots` (≤`CHAIN_READ_ATTEMPTS` tries) plus an audio-owned `last_order` fallback; a writer-only mutex serializes `set_chain_order`; `chain_slots()` is now control-thread-only. `process`/`process_block` use `snapshot_order`. R1 resolved. | `audio_read_never_waits_for_a_stalled_writer`, `concurrent_writers_never_tear_the_order`; existing torn/rapid order tests | `debug_assert` checks a pure permutation, not `sanitize_chain_order(order) == *order`: amp-before-cab is a UI-level constraint and `rapid_reorder` legitimately swaps across that boundary |
| A2 | routing | One `BlockRoute` snapshot (order, `use_ext_amp`, `skip_cab`, `width`) taken per block/call and threaded through `process_core`/`run_full`/`run_range`/`run_ordered_stage`; the Cab decision is no longer re-read per sample, and `process()` never runs an AU or skips the cab. Deleted `ext_amp_supplies_cab`; fixed the `amp_stage` doc. R2 and R4 resolved. | `route_truth_table`, `per_sample_process_keeps_the_cab_with_a_full_rig_au_flagged`; existing `process_block_matches_per_sample` and AU full-rig/amp-only tests | built-in↔AU switch lands on a block boundary only (no crossfade; documented under "Findings not yet actioned") |

---

## Open gaps for a following agent

The roadmap's reference-dependent phases were skipped. Each needs external
material that cannot be invented from code:

### Phase 0 — reference matrix

Scaffold created: [`docs/fidelity-references.md`](docs/fidelity-references.md)
holds the code-derived inventory, the per-preset checklist, and the source-log
template. It is **unfilled on the evidence side**: a human or following agent
must log the sources per claim (song/section/era, guitar, pickups, effects with
order, amp revision/channel, cab/speakers, mic/room, studio processing) and mark
each *documented* / *plausible* / *unknown*; presets without a firm source stay
"inspired by".

### Phase 3 — amp/cab fidelity

Priority order from shipped presets: Hiwatt DR103 + WEM/Fane, Marshall
Super Lead/Plexi + Greenback 4×12, Fender Twin + Jensen 2×12. Needs:
schematic/revision for each amp, measured re-amp captures at matched DI level,
and compatible speaker/cab/mic IRs for magnitude/phase/decay comparison.
Specifically flagged in `fidelity-plan.md`: the Plexi model's tube-rectifier assumption
(`src/dsp/amp/plexi.rs`) must be checked against the chosen 1959 revision before
the sag/ripple tuning is trusted.

### Phase 4 — named pedal/echo/reverb circuits

Needs circuit data + measured sweeps for: Muff vs Fuzz Face vs Tone Bender
(`src/dsp/effects/fuzz.rs`), TS-808 high-pass (above), Dyna Comp vs the generic
compressor, manual-wah vs `wah.rs` auto-wah (needs an expression input), Phase
90 / MXR flanger / Electric Mistress / Uni-Vibe modulation trajectories, a real
Binson Echorec model if evidence calls for it, and a spring-tank model to
replace the Twin's generic Freeverb "spring".

### Phase 5 — rebuild bundled presets

Use the Phase 0 matrix; start sparse and add only source-backed effects.
Preset descriptions claiming a session rig must be updated to match
the enabled signal path in the same change (now in the README and the preset
`description` fields).

No historical/session claim in this repository should be treated as verified by
the Phase 1–2 work here; only the routing, topology, and documentation defects
above were addressed.

---

## How to verify this work

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

All tests pass (262 at the time of writing). The routing/topology fixes are
covered by the named tests in `src/dsp/mod.rs`, `src/preset.rs`, and
`src/ui/input.rs`. There is no listening test: Phases 1–2 change routing,
coherence, and topology, not voicing, and the default-order render is
bit-identical to before the split.
