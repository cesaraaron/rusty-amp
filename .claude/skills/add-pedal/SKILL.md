---
name: add-pedal
description: Add a new pedal / DSP effect to rusty-riff end to end. Use when asked to add, create, or contribute a new pedal, stompbox, or rack effect (e.g. a chorus, phaser, flanger, octaver, EQ). Covers the DSP code, the UI wiring, presets, and tests.
---

# Add a pedal to rusty-riff

A pedal is a self-contained DSP effect wired into the fixed signal chain, the TUI, and the preset format. Adding one is **one coordinated body of work**: the Rust implementation plus its tests, all in the same commit.

`CONTRIBUTING.md` is the source of truth; this skill operationalizes its *"Adding a new DSP effect"* section. Read it alongside this.

## Before you start

- Decide **where in the chain** the pedal sits. The shipped order is:
  `Gate → Whammy → Wah → Comp → Fuzz → TS-808 → DS-1 → Metal → Pre-EQ → Uni-Vibe → Amp → Cab → Graphic EQ → Parametric EQ → Flanger → Chorus → Phaser → Tremolo → Delay → Reverb`.
  Its position defines its character (pre-amp shapes what the gain stage clips; post-cab colours the final mix). Insert it at the right point in every ordered list below.
- Pick a **livery colour** and a short `<id>` (used for the `PEDAL_<NAME>` colour and the UI tile).
- Reuse the shared building blocks in `src/dsp/effects/mod.rs` (`OnePoleLp`, `ThreeBandEq`, `db_to_lin`/`lin_to_db`, `param_changed`) and `Oversampler::process(x, f)` in `oversample.rs` rather than re-implementing tone LPs, EQ trios, dirty-checks, or oversample loops. Study an existing effect (`src/dsp/effects/tube_screamer.rs` or `distortion.rs`) as a template.

## Part A — the code

Signal-path code must be **allocation-free, lock-free, and panic-free** at runtime; process samples as `f32`.

1. **`src/dsp/effects/<effect>.rs`** — a `struct` holding state with a `process(sample: f32) -> f32` (or stereo) method. Reuse the shared helpers above.
2. **`src/dsp/effects/mod.rs`** — declare and re-export the module.
3. **`src/dsp/mod.rs`** — add a field to `DspChain`, a line in `DspChain::process` using the `mono_stage!` / `stereo_stage!` macro (so it bypasses cleanly and reads `Params`), and the `Params` fields + defaults: one `Arc<AtomicF32>` per knob **and** the `<fx>_enabled: Arc<AtomicBool>` bypass flag.
4. **`src/ui/draw.rs`** — update the `render_header` chain.
5. **`src/ui/config.rs`** — add the `<FX>_START` / `<FX>_END` knob-range constants and the `KNOBS` entries (in top-to-bottom on-screen order — `←`/`→` walks the array linearly), then a `PEDALS` entry (name, livery colour accessor, knob range, `enabled`-flag accessor). The tile grid, detail editor, `+ ADD` picker, and `D`-to-remove all derive from this table.
6. **`src/ui/styles.rs`** — add the `PEDAL_<NAME>` colour used by the `PEDALS` entry.
7. **`src/ui/input.rs`** — add the pedal's `enabled` flag to the `toggle_pedal` match so `Space` can bypass it on the board.
8. **`src/preset.rs`** — add the preset section/fields (including the on/off state, so a preset can place the pedal on the board — `sync_board` reads the enabled flags after load). Mirror an existing optional section like `FuzzSection`.
9. **Tests** — a `#[cfg(test)]` module in the effect file: finite/bounded output, and that each knob moves the band/level it should.
10. **UI tests** — your `config.rs` edit deliberately trips the UI table tests; update them in the same commit:
    - `src/ui/config.rs` → `table_sizes_are_stable`: bump the `PEDALS.len()` and `KNOBS.len()` expected counts. This tripwire exists so a table change is always a conscious edit — the contiguity/`pedal_of` invariant tests will confirm your new knob range tiles correctly.
    - `src/ui/draw.rs` → the golden **snapshot** (`snapshot_default_screen`) now renders your new pedal, so it no longer matches the committed `.snap`. Re-render and re-bless it: run `INSTA_UPDATE=always cargo test --lib 'ui::'` (or `cargo insta review` to inspect the diff first), then **commit the updated `src/ui/snapshots/*.snap`** — CI compares against that committed file, so an un-blessed change fails `cargo test`. The `every_pedal_renders_*` / `add_pedal_modal_*` tests pick your pedal up automatically from the `PEDALS` table; no edit needed.

## Verify

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features                     # new effect tests + preset round-trip + UI tables
INSTA_UPDATE=always cargo test --lib 'ui::'   # re-bless the UI snapshot, then commit src/ui/snapshots/*.snap
```

Then do a manual audio pass (see the end of `CONTRIBUTING.md`).

## Checklist

- [ ] `src/dsp/effects/<effect>.rs` + re-export in `effects/mod.rs`
- [ ] `DspChain` field, `process` macro stage, `Params` knobs + `_enabled` flag
- [ ] `config.rs` knob-range consts, `KNOBS` rows, `PEDALS` entry
- [ ] `styles.rs` `PEDAL_<NAME>` colour
- [ ] `toggle_pedal` match arm in `input.rs`
- [ ] preset section in `preset.rs` (with enabled state)
- [ ] `#[cfg(test)]` tests in the effect file
- [ ] UI tests updated: `table_sizes_are_stable` counts bumped + `src/ui/snapshots/*.snap` re-blessed and committed
- [ ] `cargo fmt` / `clippy` / `test` pass; manual audio pass done
