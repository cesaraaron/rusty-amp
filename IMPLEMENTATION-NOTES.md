# Implementation notes — routing correctness (Phase 1)

This file tracks work done against [`plan.md`](plan.md). It is written so a
following agent can review the changes and continue the roadmap without
re-deriving context. Fidelity phases (0, 3, 4, 5) are intentionally **not
started** — see [Open gaps](#open-gaps-for-a-following-agent) for the exact
references and decisions each one still needs.

Scope agreed with the maintainer: **Phase 1 routing patch only**, delivered as
one commit per roadmap item, with all findings recorded here.

---

## What changed

### 1. Bypass is now wire-transparent — `fix(dsp): make bypassed stages wire-transparent`

- File: `src/dsp/mod.rs`
- Added `Params::stage_enabled(stage) -> bool` (mirrors each stage's
  `*_enabled` atomic; `AmpCab` is always live).
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

## Findings not yet actioned

### Deferred: fixed master-bus widener (Phase 1 item 4)

`master_bus` (`src/dsp/mod.rs`) always applies `widen(l, r, 1.3)` then a soft
limiter to every output, including recordings. The roadmap wants a neutral /
controllable "studio master" instead of an always-on coloration. **Not done**:
it is an output-path sound-design decision that pairs naturally with the Phase 2
amp/cab topology split, and changing it alters existing preset sound. Flagged
rather than silently changed.

### TS-808 header doc contradicts its own constructor

`src/dsp/effects/tube_screamer.rs`:
- Header (lines 8 and 13-14) says the input coupling HP is **~60 Hz**.
- Constructor (line 48) instantiates **340 Hz**, with an inline comment (line 45)
  justifying 340 Hz (`0.047 µF` into `10 kΩ`).

Only the prose is stale/inconsistent. Per the roadmap, **do not retune either
number without hardware/reference data** — the real question is which value is
circuit-correct, which is a Phase 4 fidelity task. Recommend a schematic or
measured TS-808 frequency response, then make code and both comments agree.

### Amp and cab are still one reorderable slot

`ChainStage::AmpCab` remains a single slot; `amp_cab()` calls `amp_stage` then
`cab_stage`. Phase 2 (split into `Amp` + `CabMic`, migrate `"ampcab"` preset
values, preserve AU/IR paths) is untouched.

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
the Phase 1 work here; only the routing and documentation defects above were
addressed.

---

## How to verify this work

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cd site && npm run build
```

All DSP tests pass (254 at the time of writing). The routing fixes are covered
by the named tests in `src/dsp/mod.rs`. There is no listening test for Phase 1
(the changes are wire-transparency and coherence, not voicing).
