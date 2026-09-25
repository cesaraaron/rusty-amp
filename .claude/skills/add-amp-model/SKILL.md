---
name: add-amp-model
description: Add a new built-in amplifier model to rusty-amp. Use when asked to add, create, or contribute a new amp model/head (e.g. a Vox, Fender, Soldano, Peavey sim) to the amp bank. Covers the DSP code, the AmpModel enum, UI wiring, presets, tests, AND the docs site so nothing drifts.
---

# Add an amp model to rusty-amp

An amp model is a struct implementing the `Amplifier` trait, addressed through the fixed `AmpModel` enum (`Marshall`, `Mesa`, `Randall`, …) and switched live in the TUI. Adding one is **two coordinated bodies of work**: the Rust implementation (code + tests) and the documentation site. Both land in the **same PR** — a model that isn't documented is treated as incomplete.

`CONTRIBUTING.md`'s *"Adding a new amp model"* section is the short pointer for this; this skill is the full operational checklist (the `CONTRIBUTING.md` blurb says a `[C]` cycle key — that's stale copy-paste from the cabinet section, the real key is **`A`**, see step 4 below).

## Before you start

- Decide whether the model is **tube** (passive FMV-style tone stack, power-amp sag, output-transformer saturation, cathode bias, bright cap — like `Marshall`/`Mesa`) or **solid-state** (active shelf/peak tone stack, stiffer power section, no output transformer — like `Randall`). This determines which shared building blocks in `src/dsp/amp/mod.rs` you wire up.
- Reuse the shared helpers in `src/dsp/amp/mod.rs` rather than reimplementing them: `FrontEnd` (DC block + input HP), `Bloom` (preamp dynamics), `CathodeBias` (tube blocking-distortion/touch), `BrightCap` (gain-pot treble bleed, tube only), `OutputTransformer` (LF core saturation + crossover, tube only), `SpeakerLoad` (dynamic speaker-impedance bloom), `VoiceBalance` (post-tone-stack body/tilt), `Cached`/`ToneCache` (dirty-check coefficient recompute), `Oversampler8` (8× oversampling — mandatory for any nonlinear stage, see the aliasing test in step 9). For the tone stack itself, either call `crate::dsp::tonestack::ToneStack` with an existing or new `Components` preset (passive network) or build an active shelf/peak stack directly with `Biquad` (see `Randall`).
- Study `src/dsp/amp/marshall.rs` (tube template) or `src/dsp/amp/randall.rs` (solid-state template) line by line before writing the new one — the doc comments on each explain *why* each stage exists.
- Loudness-match: the new model's output trim must land within the loudness window the other models already sit in (see the `amps_are_loudness_matched` test in step 9) so switching models mid-set doesn't jump the volume.

## Part A — the code

Signal-path code must be **allocation-free and panic-free** at runtime; process samples as `f32`.

1. **`src/dsp/amp/<model>.rs`** — a `struct` implementing `Amplifier::process(sample, knobs: &[f32; AMP_MAX]) -> f32`, plus a `pub const KNOBS: &[AmpKnob]` descriptor listing the model's front-panel controls in the exact order `process` decodes them (`{ label, slug, default }`). The trait takes a fixed `[f32; AMP_MAX]` array padded with trailing slots, so a model with fewer than `AMP_MAX` controls simply ignores the tail; decode named locals at the top of `process` (`let gain = knobs[0]; …`). Use the shared `simplify` slugs — `gain`, `bass`, `mid`, `treble`, `presence`, `master` — for the controls every model shares so presets and tests can address them by role; extras (`normal`, `cut`, `reverb`, `speed`, `intensity`) get their own slugs. Mirror the stage order of your chosen template (front end → oversampled nonlinear stages → tone stack → voice balance → power amp/sag → output transformer/speaker load → presence → output trim). Document the "why" for any stage whose corner frequency or depth isn't self-evident, same style as the existing models.
2. **`src/dsp/amp/mod.rs`**:
   - `pub mod <model>;` and `pub use <model>::<Model>;` at the top.
   - Add a field to `AmpBank` and construct it in `AmpBank::new`.
   - Add a match arm in `AmpBank::process`.
3. **`src/dsp/mod.rs`**:
   - Add a variant to the `AmpModel` enum (next `#[repr(u8)]` value).
   - `AmpModel::ALL` — append the variant (the amp modal and `every_amp_model_renders_its_name` both iterate `ALL`).
   - `AmpModel::from_u8` — add the new discriminant.
   - `AmpModel::controls()` — add the arm returning the model's `KNOBS`.
   - `AmpModel::name()` and `AmpModel::short_name()` — full/short display strings.
   - `AmpModel::next()` / `AmpModel::prev()` — splice the new variant into the cycle ring (every variant must appear exactly once in each direction).
4. **`src/ui/`** — no new wiring: the amp panel (`render_amp_box` in `draw.rs`) and the `A` browser modal are both model-aware, driven by `controls()`/`ALL`, and each model gets its own knob bank automatically. If the new model exposes more controls than any current one, bump `AMP_MAX` in `src/dsp/amp/mod.rs`, add the matching `KNOBS` accessor rows in `src/ui/config.rs` (and shift `MIC_START`/pedal ranges + the `KNOBS.len()` tripwire), and re-bless the snapshot.
5. **`src/ui/input.rs`** — update the `modal_cursors_preselect_the_current_pick` count assertions (`amp_choices`) and the `AMP_KNOBS` constant if the default model's control count changes. Cycling is generic, so no match arm is needed.
6. **`src/preset.rs`**:
   - `AmpSection.model` doc comment — extend the `"marshall" | "mesa" | …` list.
   - `Preset::from_params` — add the new `AmpModel::<Model> => "<model>"` arm to the `amp_model_str` match.
   - `Preset::apply` — add `Some("<model>") => AmpModel::<Model>,` to the model-parsing match.
7. **Tests** — in `src/dsp/amp/mod.rs`'s `#[cfg(test)]` module:
   - Add the new model to `each_amp()` (it returns `(name, AmpModel, amp)`) — this feeds it through every structural test (finite/bounded output, DC-free, harmonic-not-aliased, tight low end, tone/presence controls track, loudness-matched).
   - If it's a tube model, add it to `tube_amps()` too, so the cathode-bias/bright-cap/touch-sensitivity integration tests cover it. Solid-state models are deliberately excluded from `tube_amps()` (see the comment on `Randall`).
   - `tone_and_presence_controls_track` self-exempts models without a Presence slug; if your model lacks Bass/Treble too, guard those probes the same way.
   - Tune the model's fixed output trim (or master-based trim) until `amps_are_loudness_matched` passes — the spread gate is 2.5×.
8. **`src/ui/draw.rs`** test module — `every_amp_model_renders_its_name` iterates `AmpModel::ALL`, so adding to `ALL` is enough; no literal array to edit.
9. **Snapshot** — a new `ALL` entry adds a row to the `A` browser modal, changing the golden **snapshot**. Re-render and re-bless it: run `INSTA_UPDATE=always cargo test --lib 'ui::'` (or `cargo insta review` to inspect the diff first), then **commit the updated `src/ui/snapshots/*.snap`** — CI compares against that committed file.

## Part B — the docs (required, same PR)

`site/amps-cabs.md` documents amps in a data-driven HTML block (`<div class="selector" data-tabs>`), not a new page.

1. **`site/amps-cabs.md`**, `## Amplifiers {#amp}` section:
   - Add a `<button class="tile" data-tab="<id>">` to the `.tiles` row: `.tile__name`, `.tile__sub` (one-line character blurb), `.tile__amp` with 6 decorative knob-icon spans (`--r:` rotation degrees — copy the pattern, vary the rotations).
   - Add the matching `<div class="tab-panel" data-panel="<id>">`: a `.specs` block with the 4 rows (`Gain range`, `Tone stack`, `Rectifier & power`, `Gain stages`) and a `.kv` row **per control the model exposes** (in the same order as its `KNOBS` descriptor — e.g. Plexi's `Gain`/`Normal`/`Bass`/`Mid`/`Treble`/`Presence`, Fender's `Volume`/`Treble`/`Middle`/`Bass`/`Reverb`/`Speed`/`Intensity`) — short technical sentences (frequencies, dB, component type), no marketing fluff, matching the terse style of the existing entries.
   - If the model changes the tube-vs-solid-state split described in the closing comparison paragraph, update that paragraph too.
2.  **`site/index.md`** - mention new model
3. **`site/how-it-works.md`** - new model in the chain
4. **`site/presets.md`** - new model in the template
5. **`CONTRIBUTING.md`** — while you're there, fix the stale `[C]` cycle-key reference in *"Adding a new amp model"* to `A` (see step 4 above) if it hasn't been fixed yet.
6. **`README.md`** — if it lists the built-in amp models, add the new one.

## Verify

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo nextest run                             # new amp tests + preset round-trip + UI tables
INSTA_UPDATE=always cargo test --lib 'ui::'   # re-bless the UI snapshot, then commit src/ui/snapshots/*.snap
cd site && npm run build                      # or: npm run dev  to preview
```

## Checklist

- [ ] `src/dsp/amp/<model>.rs` implementing `Amplifier` + a `KNOBS` descriptor
- [ ] `src/dsp/amp/mod.rs`: module decl/re-export, `AmpBank` field + `new`/`process` arms
- [ ] `src/dsp/mod.rs`: `AmpModel` variant, `ALL`, `from_u8`, `controls()`, `name()`, `short_name()`, `next()`/`prev()`
- [ ] `src/ui/config.rs`: only if `AMP_MAX` changed (KNOBS accessor rows + ranges + tripwire)
- [ ] `src/ui/input.rs`: `amp_choices` count assertions updated
- [ ] `src/preset.rs`: doc comment, `from_params` match, `apply` match
- [ ] `each_amp()` (and `tube_amps()` if applicable) in `dsp/amp/mod.rs` tests
- [ ] UI snapshot re-blessed and committed (`src/ui/snapshots/*.snap`)
- [ ] docs: `site/amps-cabs.md` tile + panel (with the model's own control rows), `site/index.md`, `site/how-it-works.md`, `site/presets.md`, `CONTRIBUTING.md`/`README.md` touch-ups
- [ ] all `cargo`/`npm` checks pass; manual audio pass done (load the model, sweep every knob)
- [ ] code + docs in the same PR
