# Timeline, raw takes, export, and sessions — review plan

**Status:** design proposal for review. No feature implementation is implied by this document. This work belongs on a **new branch created from the intended base when implementation starts**; do not fold it into the existing amp/cab fidelity plan in [`plan.md`](plan.md). File names, bindings, limits, and the sample schema below are proposals until reviewed.

## 1. What the user should be able to do

1. Import **multiple** MP3/WAV/FLAC files, each on its own scrollable timeline row, starting at the current playhead. Imported files can be repositioned and have independent monitor gain and mute. They are backing/reference audio, not guitar input to the rig.
2. Seek with `←`/`→`; change the seek increment using simple timeline-only `+`/`-` controls. Proposed increments: **1, 5, 10, 30 seconds**, initially **5 seconds**.
3. Press `R` to immediately create/arm a **new take row**, start transport if paused, and record the **dry, selected input channel** from the current timeline position. Press `R` again to finalize the take. Never automatically write a *processed* WAV to `~/` just because recording stopped. Each subsequent take adds another row rather than replacing the previous one.
4. Hear finished raw takes through the **current** amp/pedals/cab/chain. Retuning the rig changes take playback without destructively changing raw audio. Import playback, clicks, other takes, and existing FX returns must never feed the recording source.
5. Export the **processed guitar-take timeline only** as a stereo WAV at a chosen path. Include all unmuted takes according to their placement and gain, and include the rig's effect tails. Exclude imported backing/reference audio, metronome, and live, unrecorded guitar. Export must use the currently active AU amp, external IR, and CLAP insert when present, or explain clearly why an exact render is unavailable.
6. Save and load **whole sessions**: project assets plus rig, tracks, positions, gains, mutes, seek step, loop/playhead, metronome, and relevant external-rig state. Preserve unsaved raw takes in app recovery storage until incorporated into a session. Continue supporting standalone tone presets independently.

### Confirmed behavior vs proposals needing review

| Confirmed from discussion | Proposed implementation detail |
| --- | --- |
| Raw takes respond to live rig changes; imported audio remains monitor-only. | Sum unmuted raw takes into **one playback guitar bus** and process it through its own `DspChain`; retain a separate `DspChain` for live guitar. This makes two simultaneously sounding takes drive one shared amp. If independently processed takes are wanted later, that is a different, more CPU-intensive feature. |
| Export guitar takes only, with the live rig including external plugins. | Default export range is frame zero through the last unmuted take's end **plus its effect tail**, regardless of the selected loop. Export one stereo 32-bit float WAV, at the project sample rate. |
| Session is a portable project folder. | Copy imported audio and loaded IR into `audio/` and `irs/` on save; store plugin **identity/state references**, because third-party binaries cannot be made portable by copying ordinary audio assets. |
| Record auto-plays from the playhead and stops at loop end. | When `R` auto-started transport from pause, return to pause at stop; if playback was already running, keep it running after a manual stop. At loop end, stop capture *before wrap* and pause at the loop out-point. These transport details should be confirmed during UI review. |
| Imports begin at playhead and may be repositioned. | Use a stable project time base so changing devices/sample rates cannot drift offsets. Provide a separate move-clip control rather than overloading seek arrows. |
| Unsaved takes use recovery storage. | Raw captures go to a recoverable, session-scoped cache, **not** to the user's requested export path. A save moves/copies them into the session folder; failed saves retain the recoverable source. |

## 2. Existing behavior and constraints to preserve

- [`src/practice.rs`](src/practice.rs) owns a shared transport with playhead, one-shot seek, loop markers, **one backing slot and one take slot**, and gain/mute for those two slots. [`src/dsp/player.rs`](src/dsp/player.rs) sums those two decoded stereo tracks against one cursor.
- [`src/ui/practice.rs`](src/ui/practice.rs) has a fixed transport + two 3-row waveform layout. `B` opens a file browser with a Backing/Take destination toggle. A new take replaces the old take. Its `seek_by` uses engine frames and clamps to the two-slot timeline length. [`src/ui/mod.rs`](src/ui/mod.rs) binds `←`/`→` to ±5 seconds when timeline focused.
- `R` currently starts recording and, on the next `R`, calls `stop_take`, installs the processed stereo result as the sole take, **and calls `save_wav`**. Switching audio devices while recording calls `stop_and_save`; starting a new engine resets the timeline/track state. [`src/recording.rs`](src/recording.rs) buffers interleaved processed stereo samples under `Mutex<Vec<f32>>`.
- In [`src/audio/mod.rs`](src/audio/mod.rs), `in_buf` holds the selected mono guitar input. The rig processes that block into `out_l`/`out_r`; the current recording tap stores **processed** L/R samples; metronome and practice playback are then added to monitor output. Track swaps use `rtrb` and displaced buffers return to the control thread.
- [`src/preset.rs`](src/preset.rs) already serializes built-in rig settings, including the newer split amp/cab chain and master width. Presets are **rig snapshots**, not timeline sessions. Active IR path, plugin identities, and plugin parameter state live outside those preset values, in their browser/host UI handles.
- `R`, `B`, `P`, `S`, `V`, `U`, `I`, `E` (in the preset browser), and panel-specific `[`/`]` already have meanings. New bindings should be scoped to the timeline or a session modal, leaving preset import/export intact.
- `AGENTS.md` real-time rules apply: **no callback allocations, blocking, filesystem access, or dropping large audio/plugin buffers**. Audio-rate effects retain state across blocks. Decode, resample, peaks, session IO, and WAV writing belong on control/worker threads.

## 3. Proposed data model and clock

### Session-owned track records

Keep a control-thread `SessionState` as the canonical editable project, independent of a particular `AudioEngine`. Each track has a stable ID that survives reordering, a name, `kind` (`import` or `raw_take`), source asset reference, source sample rate/channel count, **project-tick start and length**, gain, mute, display peaks, and a lifecycle (`loading`, `recording`, `ready`, `error`). UI row selection uses the ID, not a vector index. Playback may keep optimized decoded/resampled buffers in bounded audio-thread slots indexed by ID/generation; those are caches, not the session source of truth.

Choose and document a **project time base** when a session starts (for example, the initial engine sample rate, stored explicitly in the manifest). Capture/asset files also record their own actual source rate. All saved clip starts, cursor, and loop markers are in project ticks; on device changes, convert ticks to output frames with a defined rounding rule, and resample source assets off-thread. Do not mutate the saved timing or repeatedly resample an already resampled buffer when devices change. Keep sub-frame rounding consistent in live playback and export. For an empty timeline, allow seeking/recording from zero and extend the visible extent with the armed recording cursor rather than clamping to an old file length.

Suggested first release: a bounded maximum count (e.g. **32** concurrent tracks) with an explicit UI error when full. Establish a memory/duration budget before choosing the exact count: a 10-minute 48 kHz stereo `f32` decode is roughly 230 MB **per track**, so unlimited eager decoding is not viable. An initial bounded in-memory version is acceptable if limits are visible; long-track streaming/chunk caching is a separate design if larger sessions are expected.

### Audio-thread representation

- Preallocate track slots, mix scratch, per-track gain/mute state, and control rings. Install/remove whole prepared tracks by command at a callback boundary; do not allocate or resize in that callback.
- Use stable IDs/generations to prevent a delayed decode/install from populating a row that the user deleted or replaced. Capture pending decode **target ID/path** in each worker result; the current browser's mutable `slot` must not decide which row receives an earlier request.
- Return displaced decoded buffers to a worker/control disposal queue. A full return queue must **not** silently free a large `Vec` on the callback: reserve capacity, defer the swap, or use another explicit ownership/backpressure strategy.
- Make track-control updates (add/remove/gain/mute/seek) coherent per callback. Never rely on reading an arbitrarily half-updated collection from UI atomics; define acknowledgement so the UI knows whether an install actually succeeded.
- When a new engine starts after a device change, reconstruct its playback caches from session-owned assets at the new rate; retain session metadata, tracks, and recovery paths. Rebuild external plugin instances/IR as applicable or report failure to restore instead of showing an active-but-missing rig.

## 4. Playback and recording signal flow

```text
Interface selected mono guitar input ─┬─> live DspChain ───────────────────┐
                                     └─> dry capture ring (when armed)     │
                                                                               ├─> monitor only
Unmuted raw takes, positioned/gained ───> take-bus DspChain ──────────────┤
Imported files, positioned/gained ───────> monitor mix only ───────────────┤
Metronome click ──────────────────────────> monitor mix only ───────────────┘

OFFLINE EXPORT (separate worker, no live input):
Unmuted raw takes, positioned/gained ───> fresh matching DspChain ──> WAV
```

- Capture is **one mono sample per selected input frame**, before gate, pedals, amp, cab, plug-ins, limiter, player and click. The guitar still sounds through the live rig while recording. Muted/imported tracks are not input to recording. The actively recording row does not feed its own playback until finalized; older takes remain monitorable while overdubbing.
- The take bus uses the same control settings as the live rig but **separate mutable DSP state**, so reverb/delay/amp sag from one bus does not leak into the other. An active AU amp/CLAP insert needs a second live processing instance as well; instantiation and swapping happen off the callback. The extra DSP/plugin CPU load and plugin-instance restrictions must be benchmarked rather than assumed cheap.
- Gain on an imported track is its **monitor output volume**. Gain on a raw take is **pre-rig level**, affecting distortion/compression as intended; the UI should distinguish these. If a separate post-rig guitar volume is desired, add a named bus master rather than silently changing what the existing gain means.
- The take bus may reuse the project's transport cursor for positioning, but it must produce silence when transport is stopped without resetting its effect tails unexpectedly. Decide and test pause, seek, mute/unmute and loop behavior for delay/reverb state (e.g. flush vs allow tails), making the export path consistent for its selected range.
- Mixing multiple raw takes *before* one nonlinear amp can change the tone versus processing each separately. Expose/document this behavior; test both gain staging and clipping and don't describe the resulting bus as multiple independently mic'd amps.

### Frame-accurate recording lifecycle

1. On `R`: allocate/prepare the capture buffers and recovery destination on a worker; create a **pending/armed row immediately**. Request an audio-thread start that atomically captures the actual transport frame used for the first guitar sample and starts transport if paused. Acknowledgements populate the precise start timestamp. Present a recording indicator and growing timeline end/peaks without blocking redraw. If arming fails, mark/remove the row and explain the error.
2. On each callback: push dry selected-channel samples into a **preallocated SPSC ring** in blocks, with a worker draining/writing them to a mono float WAV or an explicitly versioned raw format. No `Vec::push`, `Mutex::try_lock` plus per-sample locking, or file IO on the callback. Capture input/sample-rate identity and source frame count. An overflow is reported and the take is flagged **incomplete**, never silently shortened.
3. On manual stop or reaching a valid active loop's out-point: stop **before** advancing/wrapping the capture position, let the writer flush and finalize on a worker, then mark the row ready. One loop pass yields one contiguous take. Keep the prior loop settings. Do not create a second row or duplicate first/last frames at the boundary. Decide cancellation/zero-length-take behavior explicitly.
4. On device switch, quit, engine failure, or recovery: stop/flush in the same way where possible, keep the source in recovery storage, and report any incomplete capture. Do not quietly call the old `stop_and_save()` processed-WAV path. Persist enough metadata to discover orphaned takes on the next launch and offer recovery/discard actions.

## 5. Timeline UI and keyboard map (proposal)

| Context | Control | Behavior |
| --- | --- | --- |
| Anywhere outside a modal | `R` | Arm a new raw-take row / stop current recording; focus/show the timeline when arming. |
| Timeline focus, transport row | `Space` | Play/pause. |
| Timeline focus, track row | `Space`, `Delete` | Toggle row mute; remove selected row from session after stopping/handling any active writer. Removal must not delete source assets still referenced elsewhere. |
| Timeline focus | `↑`/`↓`, `←`/`→` | Select transport/rows (scroll when needed); seek backward/forward by configured step regardless of selected row. |
| Timeline focus | `+`/`-` | Advance/reverse the seek-step choice (`1 → 5 → 10 → 30` seconds), shown beside the playhead. This overrides the current inert timeline use of these keys, not amp/pedal knob nudging elsewhere. |
| Timeline focus, row selected | `G` | Edit selected track output/pre-rig gain with a small numeric/value modal; propose 0–200% or calibrated dB display, plus reset to unity. Show the value on the row. |
| Timeline focus, row selected | Proposed move action (`Shift+←/→` or a small position dialog) | Move the clip start in project-time steps; do not move transport playhead. Pick keys only after checking terminal support for modified arrows. |
| Timeline focus | `[`, `]`, `L` | Set loop start/end and toggle loop, preserving existing controls. |
| Anywhere outside a modal | `B` | Browser appends a new imported track at current playhead; remove its old Backing/Take target toggle. Multiple concurrent decodes must retain the correct target ID. |
| Timeline focus | `E` | Open **guitar-only final WAV export** path/options and show worker progress. `E` inside the preset browser remains *preset export*. |
| Anywhere outside a modal | Proposed `J` | Session browser: New / Save / Save As / Load / recovery. Existing `S` and `P` remain preset save/browser. |

Replace the fixed 10-row, two-track pane in `src/ui/practice.rs` and its `Constraint::Min(10)` allocation in `src/ui/draw.rs` with a scrollable/virtualized view. Show compact row name, type, mute, gain, start/end, waveform, playhead, and active recording progress; scale waveform display to the shared timeline extent. Keep small-terminal behavior intentional (minimum readable height, selected row visible, no panel overlap) and preserve accessible status/errors. Build min/max peaks off the audio thread from decoded/imported or recovering-take data; redraw from cached peaks.

## 6. Final-audio export contract

**Range:** by default from project tick zero to the end of the latest **unmuted raw take**, then enough extra silence to render the current rig's delay/reverb tails, with a documented maximum and a silence threshold to avoid infinite feedback tails. Muted takes and imported tracks do not extend export range. No takes → explain why there is nothing to export. The loop region affects monitoring/recording but does not silently truncate the export. If an explicit selection/loop export is added later, label it separately.

**Snapshot:** at export start, freeze all raw take IDs/assets, starts, gains/mutes, project rate, amp/pedal/chain/master values, selected built-in or external cab, AU amp mode, loaded plugins and their settings. Live UI changes after export starts apply only to **subsequent** exports. A fresh render graph starts with known DSP state; process in deterministic blocks and keep filter/plugin state across blocks. Resample each source from its stored original rate off-thread. Output stereo 32-bit float WAV to a temporary file beside the destination, then finalize/rename on success; allow progress and cancellation without presenting a partial final WAV.

**External-rig parity:** the existing `DspChain::process_block` has a post-rack CLAP insert and an AU amp override with amp-only/full-rig and latency handling. Reusing the live plugin instance concurrently from an export worker is unsafe; create separate instances on a suitable control/worker context. Persist stable plugin identifiers, loaded IR asset, AU amp-only/full-rig setting, AU/CLAP parameter IDs and values, and (where supported) opaque plugin state/preset data. Restore those to both live take bus and offline renderer. Account for plugin processing latency when aligning dry clips and defining the export start/end, and preserve tail flushing. Some plugins cannot be faithfully cloned from exposed parameters alone; **do not silently produce a built-in-only export**. Report the missing capability/instance and keep the raw sources intact. A session may still load partially, with an explicit missing-plugin state and inability to claim a matching export until resolved.

**Comparison:** for a single raw take with no backing/click/live input, compare rendered output with an appropriately captured monitored take-bus segment at matched start/rate/initial effect state. Test gain changes, staggered takes, mute, loop, output limiter, multiple callback/block sizes, and external-plugin latency. Do not require sample-exact equivalence for nondeterministic plug-ins; document such exceptions and audible parity expectations.

## 7. Session storage and restore contract

Session means a **project**, not a preset or an already rendered WAV. Proposed default location: `~/.config/rusty-amp/sessions/<session-name>/`, with a path picker for Save As/Open. A portable folder may have this shape (names illustrative):

```text
my-session/
  session.toml        # versioned manifest, project timing, transport, rig, track list
  audio/
    imported-<stable-id>.flac  # copied original import bytes, format may be MP3/WAV/FLAC
    take-<stable-id>.wav       # original-rate dry mono float capture
  irs/
    cabinet.wav        # selected external IR, when applicable
```

Manifest shape to review **before coding** (illustrative, not a frozen schema):

```toml
version = 1
name = "Practice session"
project_sample_rate = 48000

[transport]
playhead = 960000          # project ticks
seek_seconds = 5           # 1 / 5 / 10 / 30
loop_enabled = true
loop_start = 480000
loop_end = 1440000

[metronome]
enabled = false
bpm = 120

# Rig section: versioned, complete snapshot reused from the built-in preset schema,
# plus explicit external IR/plugin identities, active flags and available state.
[rig]
# ...

[[tracks]]
id = "stable-id-1"
kind = "import"
name = "Backing"
asset = "audio/imported-stable-id-1.flac"
start = 960000
gain = 0.8
muted = false

[[tracks]]
id = "stable-id-2"
kind = "raw_take"
name = "Take 1"
asset = "audio/take-stable-id-2.wav"
start = 960000
gain = 1.0
muted = false
```

- On save, snapshot a **coherent** rig/transport/track list, copy required original assets into a staging directory, write/validate the versioned manifest, then commit without destroying the previous valid session if any copy/write fails. Resolve asset paths relative to the project folder and reject paths that escape it; don't make a supposedly portable session depend on absolute local imports. Leave originals and recovery captures untouched until save completes.
- On load, parse and validate version, project rate, loop/order, IDs, asset paths and existence; decode/resample/assets and prepare plugin/IR instances off-thread; then replace the active session coherently. A failure should keep the previous working session. Reset effect playback state at the new session boundary, seek to the saved playhead, and **load paused** even if transport was playing when saved, unless users explicitly opt into auto-play. Restore track selection/panel UI if useful, but treat appearance as secondary to audio/project data.
- If a required AU/CLAP binary or plugin state is unavailable, report which one is missing and what was not restored; never display an external rig as active if its processor is absent. The raw project audio still loads, so the user can resolve it or switch to a built-in rig deliberately.
- Sessions should restore the selected loaded IR, whether it was active, full-rig versus amp-only AU routing, plugin insert, and all available control values. A standalone preset load may replace rig settings within the session but must not delete/move its tracks. Changing audio devices must not erase the session, including a partly recovered one.
- Recovery location proposal: `~/.config/rusty-amp/recovery/<temporary-session-id>/`. Store dry audio plus minimal metadata (track ID, start tick, source rate, frame count, capture health). Offer a recovery prompt or browser entry on next launch; a user may choose restore, save into a session, or discard. Only garbage-collect after explicit discard or verified incorporation into a durable project.

## 8. Work packages and file map

1. **Session types/time model:** add `src/session.rs` (manifest, IDs, project-clock conversions, asset loading, atomic save/load, recovery metadata) and keep `src/preset.rs` responsible for reusable rig snapshots rather than extending preset files into project files.
2. **Multitrack playback:** refactor `src/dsp/player.rs`, `src/practice.rs`, and `src/audio/mod.rs` from two replacement slots to bounded, ID-addressed tracks with a transport shared between imported and raw take lanes; implement safe install/remove/disposal and track gain/mute.
3. **Raw capture:** refactor `src/recording.rs` and the input callback in `src/audio/mod.rs` to use dry `in_buf`, worker-backed SPSC capture and recovery assets; implement start/stop/loop/device-change acknowledgement and lifecycle.
4. **Take/live render split:** use separate per-bus `DspChain` instances, including external amp/IR/insert when active, and a common control snapshot. Profile the cost, synchronization and exact mixing order.
5. **Timeline UI:** refactor `src/ui/practice.rs`, `src/ui/mod.rs`, `src/ui/draw.rs` (and relevant navigation helpers) for unbounded-looking, bounded/scrollable rows, per-track controls, seek-step choices and export/session modals. Keep existing preset keys and plugin browsers functional.
6. **Offline export + plugin restore:** add an exporter module/worker, refactor `src/host/{clap_host,au}.rs` and `src/ui/{plugins,amp_plugins,ir_browser}.rs` to expose stable identities and restorable state. Use the same routing rules as live take playback; fail visibly if equivalence cannot be guaranteed.
7. **Docs:** update `site/tools.md`, `site/getting-started.md`, `site/presets.md` (preset vs session meaning), `site/plugins.md` (plugin/session/export behavior), and the recording widget in `site/assets/site.js`; update help text and UI snapshots with the new bindings.

Suggested delivery order: establish the session/clock model and playback track IDs; add safe raw capture; integrate the take bus and dynamic UI; add session/recovery persistence; finish offline export and plugin-state parity; update docs/tests with each increment. The branch is feature work, so intermediate patches may be reviewed separately, but the final user flow is incomplete until export and save/load round-trip together.

## 9. Acceptance tests and operational checks

1. Import three differently sized stereo/mono files at different playheads; adjust each gain/mute/start, seek and loop, and verify sample-accurate relative playback and displayed waveform/extent.
2. With transport initially paused, press `R`: one new row appears, transport moves, dry input is captured; press `R` to stop, change from clean to distorted rig, and hear the **same raw take** change. Record again while old take and imported audio play: the second take contains **only new dry input**.
3. With a loop `[A, B)`, start at A and record; verify exactly `B-A` aligned project-time frames (allowing explicitly defined sample-rate conversion rounding), no wrap-overwrite or duplicate frame, and transport behavior matches the UI description.
4. Under a deliberately slow worker/full capture ring, surface overflow and mark the take incomplete; verify the callback neither blocks nor allocates. Remove/replace tracks rapidly while rendering/playing and ensure no stale track appears, no audio-thread `Vec` free, and no queue-full data loss.
5. Muting/importing/moving a backing track changes the monitor but has **zero influence** on raw capture or final exported samples. Muting/leveling a raw take affects monitor and export as documented. Export includes reverb/delay tail and excludes live guitar, click, and imported audio.
6. Save a project, quit, reopen on a different audio device/sample rate, then compare track times, gains, loop, metronome, seek step, rig settings and export. Test a missing import, unavailable plug-in, interrupted save, corrupt manifest, and recoverable abandoned take without destroying a previously loaded session.
7. Validate export against built-in, external IR, amp-only/full-rig AU, and CLAP setups with saved parameter settings; show an actionable error if a plugin cannot be faithfully restored. Check latency and tails on repeated exports and when changing knobs during a render.
8. Test large-file/many-track memory bounds, audio CPU/latency at 44.1/48/96 kHz in a release build, playback/record startup timing, UI at small terminal sizes, and preset/session keyboard-modal interactions.
9. Run targeted Rust tests, then `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `npm run build` in `site/` after code/docs changes. Update existing ratatui snapshots for intentional UI changes. Hardware monitoring tests belong on `cargo run --release`.

## 10. Review questions before implementation

1. Confirm **single shared raw-take amp bus** versus one independent rig instance per recorded take. The first is faster and makes overdubs interact through shared distortion; the second more closely resembles separately re-amped tracks.
2. Confirm recording transport behavior after manual stop and loop-end stop, and whether arming `R` should immediately place a provisional row as proposed.
3. Confirm default export range, project sample rate/output format, tail cap, and whether a later option for guitar-only stems or including the backing is desirable. The agreed initial export remains **guitar takes only**.
4. Confirm timeline keyboard labels (`G` for gain, `J` for sessions, timeline-only `E` for export, clip-move binding) after a quick terminal usability check.
5. Confirm policy for non-restorable third-party plugins: this plan prioritizes honest session status and **refuses an inaccurately labeled final export** over silently substituting a built-in rig.

These questions are **review points, not blockers to understanding the requested features**. The agreed behaviors in section 1 remain the basis for implementation once the document is approved.

## 11. Implementation status (increment 1 — playback + dry capture)

Branch `timeline-sessions`, cut from `main`. The first increment is the
"playback + dry capture vertical slice"; session save/load, recovery indexing,
offline export and plugin-state parity remain follow-ups (sections 6–8).

**Done — D2 (external-rig mirroring):**

- `src/audio/mod.rs` — parallel take-bus rings and `set_plugin_insert_take` /
  `set_external_cab_take` / `set_external_amp_take`; the callback applies the same
  swaps to the take `DspChain` with displaced instances returned for off-thread
  disposal.
- `src/ui/plugins.rs`, `src/ui/amp_plugins.rs` — each load instantiates **two**
  processors (live + take bus) and keeps their parameters in sync; a failed
  second instantiation falls back to the built-in rig with a visible message.
- `src/ui/ir_browser.rs` + `LoadedIr::duplicate` — the active IR is installed on
  both chains.

**Done — D1 (built-in take bus):**

- `src/session.rs` — canonical `Session`/`Track` model, project-tick time base
  (`ticks_to_frames` / `frames_to_ticks`, single half-away-from-zero rounding
  rule), stable ids, seek-step cycle.
- `src/dsp/player.rs` — `PlayerVoice` refactored from two replacement slots to a
  bounded `MAX_TRACKS` table of id-addressed slots, split into a rig-processed
  **take bus** and a monitor-only **import bus**; gain/mute per row.
- `src/audio/mod.rs` — ID-addressed `TrackCommand` ring (install/remove/gain/
  mute) with `TrackAck` acknowledgements and displaced-buffer disposal; a second
  `DspChain` (`take_chain`) sharing `Params` but with independent DSP state; dry
  capture ring + loop-end auto-stop; device changes no longer erase the project.
- `src/recording.rs` — dry mono capture: `CaptureState` atomics + SPSC ring +
  writer worker producing a recoverable mono float WAV, peaks and a ready
  `PlayerTrack`.
- `src/ui/practice.rs`, `src/ui/draw.rs`, `src/ui/mod.rs` — scrollable multitrack
  timeline (transport + per-track rows, shared-extent waveform), `B` imports at
  the playhead, `R` arms/stops a raw take, `+`/`-` seek step, `G` gain modal,
  `Del` remove. Help text and the docs site updated.

**Done — increment 3 (session persistence):**

- `src/project.rs` — versioned `session.toml` manifest, atomic
  `write_session`, validated `read_manifest`, asset path-escape rejection,
  `list_sessions`, and `Manifest::into_session`.
- `src/session.rs` — session name + saved-folder tracking, `restore_tracks`.
- `src/ui/sessions.rs` — `J` session browser (New / Save / Save As / Load /
  Delete) for portable folders under `~/.config/rusty-amp/sessions/`.
- Save bundles tracks, positions/gains/mutes, loop/playhead/seek step,
  metronome and the built-in rig; imported originals and dry takes are copied
  into `audio/`, the active IR into `irs/`. AU/CLAP identity is recorded but not
  restored (reported on load).

**Done — increment 4 (recovery):**

- Each finalized dry capture writes `recovery/<session-temp-id>/take-<id>.toml`
  metadata (start tick, rates, frames, overflow).
- `project::list_recovery` / `discard_recovery_file` / `is_recovery_asset`.
- The `J` browser lists recoverable takes; `Enter` restores one into the current
  session, `D` discards. A save that incorporates a take deletes its recovery
  copy. The prompt opens automatically on first launch when takes are found.

**Done — increment 5 (offline export, §6):**

- `src/export.rs` — worker render of the unmuted raw takes through a fresh
  `DspChain` built from a frozen `Preset` snapshot, at the project sample rate,
  to a stereo 32-bit float WAV (temp + rename, progress + cancel). Includes the
  delay/reverb tail (capped); excludes imports/metronome/live guitar. External
  IR is re-loaded at the export rate; the UI **refuses** export while an AU amp
  or CLAP insert is loaded (no state capture yet) with an actionable message.
- Timeline `E` opens a path dialog; a progress modal runs during the render.
- **Fixed a latent decode bug:** the `wav` symphonia feature is only the RIFF
  reader; added the `pcm` codec feature, without which every WAV (including our
  own captures) failed to decode.

**Done — increment 6 (plugin-state export parity):**

- CLAP: registered the host `state` extension; `LoadedPlugin::save_state` +
  `host::load_with_state` restore opaque plugin state before activation.
- AU: `LoadedAu::param_snapshot` / `apply_param_snapshot` and a reconstructed
  `DiscoveredAu`, applied to a fresh instance for export.
- `export.rs`: `BuildExternal` closures re-instantiate the live AU/CLAP on the
  worker (keeping the CLAP main-thread handle alive for the render); the export
  chain installs the insert or the amp override (with amp-only routing and
  latency). Export no longer refuses a loaded plugin unless AU state cannot be
  captured.

**Done — increment 7 (session plugin restore):**

- Manifest gains `[clap_insert]` / `[au_amp]` sections; state is written to
  `plugins/insert.state` (opaque CLAP state) and `plugins/amp.params` (AU
  parameter snapshot). `Manifest::load_external` returns `SessionExternal`.
- On load, the UI re-instantiates the CLAP insert and AU amp on both chains from
  their state and adopts them into the browsers, so the session is complete and a
  later export includes them.
- A missing/renamed plugin bundle degrades to the built-in rig with a message.

**Deferred:** AU opaque ClassInfo state (parameters only); clip reposition/move.
