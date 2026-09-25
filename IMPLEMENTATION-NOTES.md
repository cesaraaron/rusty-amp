# Implementation notes — routing correctness (Phases 1–2)

This file tracks work done against [`plan.md`](plan.md). It is written so a
following agent can review the changes and continue the roadmap without
re-deriving context. Fidelity phases (0, 3, 4, 5) are intentionally **not
started** — see [Open gaps](#open-gaps-for-a-following-agent) for the exact
references and decisions each one still needs.

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

**Deviation from the roadmap's suggestion.** `plan.md` proposed an `rtrb`
command ring carrying the full order by value. A `SeqCst` seqlock was chosen
instead because it keeps the existing shared-`Arc<Params>` architecture and
needs no constructor/ring plumbing at the ~15 `DspChain::new` call sites. It is
allocation- and blocking-free and reads once per block. Revisit if profiling
ever shows seqlock spin cost (unlikely: 19 byte stores per write, one read per
block).

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

## Findings not yet actioned

### TS-808 header doc contradicts its own constructor

`src/dsp/effects/tube_screamer.rs`:
- Header (lines 8 and 13-14) says the input coupling HP is **~60 Hz**.
- Constructor (line 48) instantiates **340 Hz**, with an inline comment (line 45)
  justifying 340 Hz (`0.047 µF` into `10 kΩ`).

Only the prose is stale/inconsistent. Per the roadmap, **do not retune either
number without hardware/reference data** — the real question is which value is
circuit-correct, which is a Phase 4 fidelity task. Recommend a schematic or
measured TS-808 frequency response, then make code and both comments agree.

### Not an actual effects loop yet

Phase 2 split `Amp` and `Cab` but the amp DSP is still one block. A genuine
amp effects loop (preamp → loop send/return → power amp) is Phase 2 item 5 and
remains unimplemented. Effects between `Amp` and `Cab` model a *virtual
load-box / post-power-amp line-level* path, not the amp's internal loop.

---

## Open gaps for a following agent

The roadmap's reference-dependent phases were skipped. Each needs external
material that cannot be invented from code:

### Phase 0 — reference matrix

Need per bundled preset (`presets/*.toml`, 15 files): song/section/era, guitar,
pickups, effects **with order**, amp revision/channel, cab/speakers, mic/room,
known studio processing, and cited sources (interviews, session notes,
schematics, measured IRs). Suggested output: `docs/fidelity-references.md` or a
`docs/fidelity/` table. Label each claim *documented* / *plausible* / *unknown*;
presets without a firm source should say "inspired by".

### Phase 3 — amp/cab fidelity

Priority order from shipped presets: Hiwatt DR103 + WEM/Fane, Marshall
Super Lead/Plexi + Greenback 4×12, Fender Twin + Jensen 2×12. Needs:
schematic/revision for each amp, measured re-amp captures at matched DI level,
and compatible speaker/cab/mic IRs for magnitude/phase/decay comparison.
Specifically flagged in `plan.md`: the Plexi model's tube-rectifier assumption
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
`site/presets.md` descriptions claiming a session rig must be updated to match
the enabled signal path in the same change.

No historical/session claim in this repository should be treated as verified by
the Phase 1–2 work here; only the routing, topology, and documentation defects
above were addressed.

---

## How to verify this work

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cd site && npm run build
```

All tests pass (261 at the time of writing). The routing/topology fixes are
covered by the named tests in `src/dsp/mod.rs`, `src/preset.rs`, and
`src/ui/input.rs`. There is no listening test: Phases 1–2 change routing,
coherence, and topology, not voicing, and the default-order render is
bit-identical to before the split.
