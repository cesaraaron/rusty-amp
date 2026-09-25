---
name: add-bundled-preset
description: Add a new pre-built (bundled) preset to rusty-riff. Use when asked to create, add, or contribute a bundled/artist/factory preset — a .toml tone shipped with the repo.
---

# Add a bundled preset to rusty-riff

A bundled preset is a read-only `.toml` tone shipped with the repo in `presets/`. Bundled presets cannot be deleted by users, so only add tones that are genuinely useful and well-tuned.

## Steps

### 1. Write the preset file

Create `presets/<name>.toml`. Use a short, lowercase, filesystem-safe file name (e.g. `pantera_floods.toml`); the file stem is what appears in the preset browser list.

- Study an existing preset in `presets/` (e.g. `presets/van_halen_brown_sound.toml`) as a template — match its structure and its habit of a one-line `#` comment above each section explaining *why* the values are what they are.
- **Required:** `name`, `[tube_screamer]`, `[amp]`, and `[reverb]`.
- All other sections are optional. An omitted optional effect section turns that effect **off** on apply (knob values are retained but bypassed). Two sections are exceptions that retain their current state when omitted: `[noise_gate]` and `[cabinet]`. An omitted `[chain]` resets the order to the shipped default.
- All knob values are normalised `0.0`–`1.0`.
- `name` is the display name (can be long/descriptive); `description` is the one-line blurb shown in the browser.

> `src/preset.rs` is the source of truth for what deserializes and exactly how `Preset::apply` treats omitted sections.

### 2. Test it

```bash
cargo test --package rusty-riff --lib -- preset::tests
```

Then run the app and press `P` to confirm the preset loads and sounds as intended.

## Verify

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Checklist

- [ ] `presets/<name>.toml` created, with per-section *why* comments
- [ ] Required sections present (`name`, `[tube_screamer]`, `[amp]`, `[reverb]`); values normalised `0.0`–`1.0`
- [ ] Deterministic: loading it after any other bundled preset gives the same rig (explicitly set any state the preset relies on)
- [ ] Tested via `cargo test` and by loading it in the app
- [ ] `cargo fmt` / `clippy` / `test` pass
