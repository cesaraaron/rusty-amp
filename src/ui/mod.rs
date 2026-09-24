#[cfg(all(feature = "au", target_os = "macos"))]
mod amp_plugins;
mod config;
mod draw;
mod input;
mod ir_browser;
mod metronome;
#[cfg(feature = "clap")]
mod plugins;
mod practice;
mod presets;
mod setup;
mod styles;
mod tuner;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use crate::dsp::{Levels, Metronome, Params, Tuner};
use crate::practice::Practice;
use crate::preset::Preset;
use crate::recording::{RecordingState, save_wav};

use config::{ADD_TILE, PEDALS, PRACTICE_TILE, Panels, pedal_of};
use draw::{draw, render_add_pedal_modal, render_help_modal};
use input::{
    add_pedal, cycle_amp, cycle_cab, ensure_focus_visible, move_stage, nav_knob, next_section,
    nudge, prev_section, remove_pedal, toggle_pedal,
};
use practice::PracticeUi;
use presets::{PathDialogKind, render_path_dialog, render_preset_modal, render_save_dialog};

/// Board membership derived from the live enabled flags (one entry per pedal).
fn sync_board(params: &Params) -> Vec<bool> {
    PEDALS
        .iter()
        .map(|p| (p.enabled)(params).load(std::sync::atomic::Ordering::Relaxed))
        .collect()
}

/// Lists devices, logs them, and returns the user's choice — either the saved
/// selection (unless `force_prompt`) or a fresh pick from the modal. Returns
/// `Ok(None)` if the user quits from the picker. `notice` is shown atop the modal
/// when it reopens after a failed audio start.
fn select_devices(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    params: &Params,
    levels: &Levels,
    force_prompt: bool,
    notice: Option<&str>,
) -> Result<Option<setup::Selection>> {
    let devices = crate::audio::list_devices()?;
    // Same facts as the modals, on stderr: one paste shows exactly what the user
    // could pick, even if they misread the fullscreen list.
    for (i, d) in devices.inputs.iter().enumerate() {
        let line = format!("Input {}: '{}' ({} ch)", i + 1, d.name, d.channels);
        eprintln!("{line}");
        crate::audio::log_line(&line);
    }
    for (i, name) in devices.outputs.iter().enumerate() {
        let line = format!("Output {}: '{name}'", i + 1);
        eprintln!("{line}");
        crate::audio::log_line(&line);
    }

    // Reuse the last good selection unless asked to prompt (`RUSTY_AMP_DEVICE_PROMPT`
    // or the in-app device-change hotkey). Delete `~/.config/rusty-amp/audio.conf`
    // to be prompted again.
    let saved = if force_prompt || std::env::var_os("RUSTY_AMP_DEVICE_PROMPT").is_some() {
        None
    } else {
        crate::audio::load_selection(&devices)
    };

    if let Some((input_idx, guitar_ch, output_idx)) = saved {
        let input = &devices.inputs[input_idx];
        let output = &devices.outputs[output_idx];
        let line = format!(
            "Audio: reusing saved devices — in '{}' ({} ch) ch {} -> out '{}'",
            input.name,
            input.channels,
            guitar_ch + 1,
            output,
        );
        eprintln!("{line}");
        crate::audio::log_line(&line);
        return Ok(Some(setup::Selection {
            input_idx,
            guitar_ch,
            output_idx,
        }));
    }

    let Some(selection) = setup::run(terminal, &devices, params, levels, notice)? else {
        return Ok(None);
    };
    crate::audio::save_selection(
        &devices,
        selection.input_idx,
        selection.guitar_ch,
        selection.output_idx,
    );
    Ok(Some(selection))
}

pub fn run(
    params: Arc<Params>,
    levels: Arc<Levels>,
    tuner: Arc<Tuner>,
    metronome: Arc<Metronome>,
    presets: Vec<Preset>,
    recording: Arc<RecordingState>,
    practice: Arc<Practice>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // ── Session state (persists across a device change) ───────────────────────
    let mut focus: Option<usize> = None;
    // Board membership: a pedal is on the board iff it is enabled. Off-board
    // pedals are bypassed in the DSP and hidden from the rig. Rebuilt with
    // `sync_board` whenever a preset rewrites the enabled flags.
    let mut board: Vec<bool> = sync_board(&params);
    let mut add_open = false;
    let mut add_cursor = 0usize;
    let mut preset_open = false;
    let mut preset_cursor = 0usize;
    let mut presets = presets;
    let mut save_open = false;
    let mut save_name = String::new();
    let mut save_desc = String::new();
    let mut save_field = 0usize; // 0 = name, 1 = description
    let mut save_error: Option<String> = None;
    // Typed-path dialog for preset import/export (opened from the browser).
    let mut path_open: Option<PathDialogKind> = None;
    let mut path_input = String::new();
    let mut path_error: Option<String> = None;
    let mut tick: u64 = 0;
    let mut save_msg: Option<(String, std::time::Instant)> = None;
    let mut tuner_open = false;
    let mut metronome_open = false;
    // Keybinding cheat-sheet modal, toggled with K.
    let mut help_open = false;
    // Which top-level panels are shown (session-only; toggled with 1/2/3).
    let mut panels = Panels::all_visible();
    // Backing playhead captured when a recording starts, for take alignment.
    let mut record_start_offset = 0usize;

    // ── Session loop: (re)select devices, start the engine, run the UI ─────────
    // The `O` key drops the engine and loops back here so the picker runs again —
    // that is how input/output/channel can be changed at runtime on any platform.
    let mut force_prompt = false;
    // Surfaced atop the picker when audio fails to start, so the user learns why it
    // reopened and can choose a working device instead of being dropped out.
    let mut start_error: Option<String> = None;
    'session: loop {
        let selection = match select_devices(
            &mut terminal,
            &params,
            &levels,
            force_prompt,
            start_error.as_deref(),
        )? {
            Some(selection) => selection,
            // User quit from the picker.
            None => {
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                return Ok(());
            }
        };
        // Any later re-entry must show the picker rather than reuse the saved set.
        force_prompt = true;
        // The picker has now consumed last iteration's error.
        start_error = None;

        // ── Start audio engine ────────────────────────────────────────────────────
        #[cfg_attr(not(feature = "clap"), allow(unused_mut, unused_variables))]
        let mut engine = match crate::audio::start(
            selection.input_idx,
            selection.guitar_ch,
            selection.output_idx,
            Arc::clone(&params),
            Arc::clone(&levels),
            Arc::clone(&recording),
            Arc::clone(&tuner),
            Arc::clone(&metronome),
            Arc::clone(&practice),
        ) {
            Ok(engine) => engine,
            Err(err) => {
                // A saved (or freshly picked) device that won't open must not lock
                // the user out. Log it, reopen the picker with the reason, and let
                // them pick another device or quit. `start` owns no engine on
                // failure, so looping back is safe.
                let msg = format!(
                    "Could not start audio (input #{}, channel {}, output #{}): {err} — pick another device.",
                    selection.input_idx + 1,
                    selection.guitar_ch + 1,
                    selection.output_idx + 1,
                );
                crate::audio::log_line(&msg);
                eprintln!("{msg}");
                start_error = Some(msg);
                continue 'session;
            }
        };

        // ── Plugin browser (CLAP insert) ──────────────────────────────────────────
        #[cfg(feature = "clap")]
        let mut browser =
            plugins::PluginBrowser::new(engine.sample_rate(), crate::audio::MAX_BLOCK as u32);

        // ── Cabinet-IR browser (external .wav IRs) ────────────────────────────────
        let mut ir_browser = ir_browser::IrBrowser::new(engine.sample_rate());

        // ── Amp-plugin browser (AU amp-position override, macOS) ──────────────────
        #[cfg(all(feature = "au", target_os = "macos"))]
        let mut amp_browser =
            amp_plugins::AmpBrowser::new(engine.sample_rate(), crate::audio::MAX_BLOCK as u32);

        // ── Practice timeline (backing + take tracks) ─────────────────────────────
        let mut practice_ui = PracticeUi::new(engine.sample_rate());
        // A device change drops the engine, so any loaded tracks are gone with it.
        practice.reset();

        // ── Main UI loop ──────────────────────────────────────────────────────────
        let mut change_device = false;
        loop {
            tick = tick.wrapping_add(1);
            let blink = (tick / 15).is_multiple_of(2);
            let rec_active = recording.active.load(std::sync::atomic::Ordering::Relaxed);

            // Install a finished background decode (if any) before drawing.
            practice_ui.poll_decode(&mut engine, &practice);

            // Clear save message after 4 seconds
            if let Some((_, ts)) = &save_msg
                && ts.elapsed().as_secs() >= 4
            {
                save_msg = None;
            }

            let status = save_msg.as_ref().map(|(msg, _)| msg.as_str());
            // The loaded plugin (if any) is shown in the header, not the status line, so
            // the help/status footer stays intact while a plugin is active.
            #[cfg(feature = "clap")]
            let plugin_name = browser.loaded_name();
            #[cfg(not(feature = "clap"))]
            let plugin_name: Option<&str> = None;

            // The external IR name is shown in the header (replacing the built-in cab
            // label) only while it is the active cab.
            let ext_cab_name = if params
                .cab_external_active
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                ir_browser.loaded_name()
            } else {
                None
            };

            // The active external amp name replaces the built-in amp label in the header,
            // mirroring the external IR. Only meaningful on the AU-capable build.
            #[cfg(all(feature = "au", target_os = "macos"))]
            let ext_amp_name = if params
                .amp_external_active
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                amp_browser.loaded_name()
            } else {
                None
            };
            #[cfg(not(all(feature = "au", target_os = "macos")))]
            let ext_amp_name: Option<&str> = None;

            terminal.draw(|f| {
                draw(
                    f,
                    &params,
                    &levels,
                    focus,
                    &board,
                    rec_active,
                    blink,
                    status,
                    plugin_name,
                    ext_cab_name,
                    ext_amp_name,
                    panels,
                    Some((&practice, &practice_ui)),
                );
                if add_open {
                    let available: Vec<usize> = (0..PEDALS.len()).filter(|&i| !board[i]).collect();
                    render_add_pedal_modal(f, &available, add_cursor);
                }
                if preset_open {
                    render_preset_modal(f, &presets, preset_cursor);
                }
                if save_open {
                    render_save_dialog(
                        f,
                        &save_name,
                        &save_desc,
                        save_field,
                        save_error.as_deref(),
                    );
                }
                if let Some(kind) = path_open {
                    render_path_dialog(f, kind, &path_input, path_error.as_deref());
                }
                #[cfg(feature = "clap")]
                if browser.open {
                    browser.render(f);
                }
                if ir_browser.open {
                    use std::sync::atomic::Ordering::Relaxed;
                    // The IR is inert only when an AU is active *and* supplying its own cab
                    // (amp+cab mode); in amp-only mode the built-in cab/IR is in the path.
                    let amp_bypasses_cab = params.amp_external_active.load(Relaxed)
                        && !params.amp_external_amp_only.load(Relaxed);
                    ir_browser.render(f, amp_bypasses_cab);
                }
                #[cfg(all(feature = "au", target_os = "macos"))]
                if amp_browser.open {
                    amp_browser.render(
                        f,
                        params
                            .amp_external_amp_only
                            .load(std::sync::atomic::Ordering::Relaxed),
                    );
                }
                if practice_ui.browser_open {
                    practice_ui.render_browser(f);
                }
                if tuner_open {
                    tuner::render_tuner(f, &tuner);
                }
                if metronome_open {
                    metronome::render_metronome(f, &metronome, blink);
                }
                if help_open {
                    render_help_modal(f);
                }
            })?;

            if event::poll(Duration::from_millis(30))?
                && let Event::Key(key) = event::read()?
            {
                #[cfg(feature = "clap")]
                if browser.open {
                    browser.handle_key(key.code, &mut engine);
                    continue;
                }

                if ir_browser.open {
                    ir_browser.handle_key(key.code, &mut engine, &params);
                    continue;
                }

                #[cfg(all(feature = "au", target_os = "macos"))]
                if amp_browser.open {
                    amp_browser.handle_key(key.code, &mut engine, &params);
                    continue;
                }

                if practice_ui.browser_open {
                    practice_ui.handle_browser_key(key.code);
                    continue;
                }

                if help_open {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('k') | KeyCode::Char('K') => {
                            help_open = false;
                        }
                        _ => {}
                    }
                } else if tuner_open {
                    match key.code {
                        KeyCode::Esc
                        | KeyCode::Char('t')
                        | KeyCode::Char('T')
                        | KeyCode::Char('q') => {
                            tuner_open = false;
                            tuner
                                .active
                                .store(false, std::sync::atomic::Ordering::Relaxed);
                        }
                        _ => {}
                    }
                } else if metronome_open {
                    // The metronome keeps running after the modal is closed, so the
                    // player can play along; only `active` is toggled here.
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('m') | KeyCode::Char('M') => {
                            metronome_open = false;
                        }
                        KeyCode::Char(' ') | KeyCode::Enter => {
                            metronome.toggle();
                        }
                        KeyCode::Right | KeyCode::Up | KeyCode::Char('+') | KeyCode::Char('=') => {
                            metronome.nudge_bpm(1);
                        }
                        KeyCode::Left | KeyCode::Down | KeyCode::Char('-') => {
                            metronome.nudge_bpm(-1);
                        }
                        _ => {}
                    }
                } else if save_open {
                    match key.code {
                        KeyCode::Esc => {
                            save_open = false;
                            save_error = None;
                        }
                        KeyCode::Tab => {
                            save_field = 1 - save_field;
                        }
                        KeyCode::Enter => {
                            if save_name.trim().is_empty() {
                                save_error = Some("Name cannot be empty".to_string());
                            } else {
                                let preset = crate::preset::Preset::from_params(
                                    save_name.trim().to_string(),
                                    if save_desc.trim().is_empty() {
                                        None
                                    } else {
                                        Some(save_desc.trim().to_string())
                                    },
                                    &params,
                                );
                                match preset.save_to_user_dir() {
                                    Ok(_) => {
                                        presets = crate::preset::load_all();
                                        save_open = false;
                                        save_name.clear();
                                        save_desc.clear();
                                        save_error = None;
                                    }
                                    Err(e) => {
                                        save_error = Some(format!("Save failed: {e}"));
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            if save_field == 0 {
                                save_name.pop();
                            } else {
                                save_desc.pop();
                            }
                            save_error = None;
                        }
                        KeyCode::Char(c) => {
                            if save_field == 0 {
                                save_name.push(c);
                            } else {
                                save_desc.push(c);
                            }
                            save_error = None;
                        }
                        _ => {}
                    }
                } else if path_open.is_some() {
                    match key.code {
                        KeyCode::Esc => {
                            path_open = None;
                            path_error = None;
                        }
                        KeyCode::Enter => {
                            let input = path_input.trim();
                            if input.is_empty() {
                                path_error = Some("Enter a file path".to_string());
                            } else if path_open == Some(PathDialogKind::Export) {
                                let p = &presets[preset_cursor - 1];
                                match p.export_to(&PathBuf::from(input)) {
                                    Ok(dest) => {
                                        path_open = None;
                                        path_error = None;
                                        save_msg = Some((
                                            format!("Exported: {}", dest.display()),
                                            std::time::Instant::now(),
                                        ));
                                    }
                                    Err(e) => {
                                        path_error = Some(format!("Export failed: {e:#}"));
                                    }
                                }
                            } else {
                                match crate::preset::Preset::import_from(&PathBuf::from(input)) {
                                    Ok(dest) => {
                                        presets = crate::preset::load_all();
                                        // Land the cursor on the imported preset.
                                        if let Some(pos) = presets
                                            .iter()
                                            .position(|p| p.path.as_ref() == Some(&dest))
                                        {
                                            preset_cursor = pos + 1;
                                        }
                                        path_open = None;
                                        path_error = None;
                                        save_msg = Some((
                                            format!("Imported: {}", dest.display()),
                                            std::time::Instant::now(),
                                        ));
                                    }
                                    Err(e) => {
                                        path_error = Some(format!("Import failed: {e:#}"));
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            path_input.pop();
                            path_error = None;
                        }
                        KeyCode::Char(c) => {
                            path_input.push(c);
                            path_error = None;
                        }
                        _ => {}
                    }
                } else if preset_open {
                    let total = presets.len() + 1;
                    match key.code {
                        KeyCode::Up => {
                            preset_cursor = preset_cursor.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            preset_cursor = (preset_cursor + 1).min(total - 1);
                        }
                        KeyCode::Enter => {
                            if preset_cursor == 0 {
                                params.reset_to_defaults();
                            } else {
                                presets[preset_cursor - 1].apply(&params);
                            }
                            // The preset rewrote the enabled flags, so rebuild the
                            // board and drop focus if it landed on a removed pedal.
                            board = sync_board(&params);
                            if let Some(i) = focus
                                && let Some(pi) = pedal_of(i)
                                && !board[pi]
                            {
                                focus = None;
                            }
                            preset_open = false;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            preset_open = false;
                            save_open = true;
                            save_name.clear();
                            save_desc.clear();
                            save_field = 0;
                            save_error = None;
                        }
                        KeyCode::Char('d') | KeyCode::Char('D') if preset_cursor > 0 => {
                            let p = &presets[preset_cursor - 1];
                            if p.source == crate::preset::PresetSource::User {
                                let _ = p.delete();
                                presets = crate::preset::load_all();
                                preset_cursor = preset_cursor.saturating_sub(1);
                            }
                        }
                        KeyCode::Char('e') | KeyCode::Char('E') if preset_cursor > 0 => {
                            let p = &presets[preset_cursor - 1];
                            let stem: String = p
                                .name
                                .to_lowercase()
                                .chars()
                                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                                .collect();
                            path_input = format!("./{stem}.toml");
                            path_error = None;
                            path_open = Some(PathDialogKind::Export);
                        }
                        KeyCode::Char('i') | KeyCode::Char('I') => {
                            path_input.clear();
                            path_error = None;
                            path_open = Some(PathDialogKind::Import);
                        }
                        KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char('P') => {
                            preset_open = false;
                        }
                        _ => {}
                    }
                } else if add_open {
                    let available: Vec<usize> = (0..PEDALS.len()).filter(|&i| !board[i]).collect();
                    match key.code {
                        KeyCode::Up => add_cursor = add_cursor.saturating_sub(1),
                        KeyCode::Down if !available.is_empty() => {
                            add_cursor = (add_cursor + 1).min(available.len() - 1);
                        }
                        KeyCode::Enter => {
                            if let Some(&pi) = available.get(add_cursor) {
                                add_pedal(&params, &mut board, pi);
                                focus = Some(PEDALS[pi].start);
                            }
                            add_open = false;
                        }
                        KeyCode::Esc => add_open = false,
                        _ => {}
                    }
                } else {
                    match key.code {
                        // ── Practice timeline (when the pane owns focus) ───────────
                        KeyCode::Char(' ') if focus == Some(PRACTICE_TILE) => {
                            match practice_ui.selected {
                                0 => practice_ui.toggle_play(&practice),
                                _ => practice_ui.toggle_selected_mute(&practice),
                            }
                        }
                        KeyCode::Up if focus == Some(PRACTICE_TILE) => {
                            practice_ui.selected = practice_ui.selected.saturating_sub(1);
                        }
                        KeyCode::Down if focus == Some(PRACTICE_TILE) => {
                            practice_ui.selected = (practice_ui.selected + 1).min(2);
                        }
                        KeyCode::Left if focus == Some(PRACTICE_TILE) => {
                            practice_ui.seek_by(&practice, -5.0);
                        }
                        KeyCode::Right if focus == Some(PRACTICE_TILE) => {
                            practice_ui.seek_by(&practice, 5.0);
                        }
                        KeyCode::Char('[') if focus == Some(PRACTICE_TILE) => {
                            practice_ui.set_loop_start(&practice);
                        }
                        KeyCode::Char(']') if focus == Some(PRACTICE_TILE) => {
                            practice_ui.set_loop_end(&practice);
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') if focus == Some(PRACTICE_TILE) => {
                            practice_ui.toggle_loop(&practice);
                        }
                        KeyCode::Delete | KeyCode::Backspace if focus == Some(PRACTICE_TILE) => {
                            practice_ui.delete_selected(&mut engine, &practice);
                        }
                        // ── Global: browser + panel visibility ─────────────────────
                        KeyCode::Char('b') | KeyCode::Char('B') => {
                            practice_ui.open_browser();
                        }
                        KeyCode::Char('1') => {
                            panels.rig = !panels.rig;
                            focus =
                                ensure_focus_visible(focus, &board, &panels, &params.chain_slots());
                        }
                        KeyCode::Char('2') => {
                            panels.amp = !panels.amp;
                            focus =
                                ensure_focus_visible(focus, &board, &panels, &params.chain_slots());
                        }
                        KeyCode::Char('3') => {
                            panels.timeline = !panels.timeline;
                            focus =
                                ensure_focus_visible(focus, &board, &panels, &params.chain_slots());
                        }
                        KeyCode::Char('q') => break,
                        KeyCode::Char('k') | KeyCode::Char('K') => {
                            help_open = true;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break;
                        }
                        // Reopen the device picker at runtime. The engine is dropped
                        // when this session loops, freeing the devices for the picker.
                        KeyCode::Char('o') | KeyCode::Char('O') => {
                            use std::sync::atomic::Ordering::Relaxed;
                            // A fresh engine starts with no external plugin/IR/amp, so
                            // clear the flags that advertise them; stop recording first
                            // so the WAV is flushed at the old sample rate.
                            if rec_active {
                                let _ = recording.stop_and_save();
                            }
                            params.cab_external_loaded.store(false, Relaxed);
                            params.cab_external_active.store(false, Relaxed);
                            params.amp_external_loaded.store(false, Relaxed);
                            params.amp_external_active.store(false, Relaxed);
                            params.amp_external_amp_only.store(false, Relaxed);
                            change_device = true;
                            break;
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            if rec_active {
                                match recording.stop_take() {
                                    Ok((samples, sr)) => {
                                        // Place the take on the timeline, aligned to
                                        // where the backing was when it started.
                                        practice_ui.place_take(
                                            &mut engine,
                                            &practice,
                                            &samples,
                                            record_start_offset,
                                        );
                                        let msg = match save_wav(&samples, sr) {
                                            Ok(path) => {
                                                format!("Saved + placed: {}", path.display())
                                            }
                                            Err(e) => format!("Take placed, save failed: {e}"),
                                        };
                                        save_msg = Some((msg, std::time::Instant::now()));
                                    }
                                    Err(e) => {
                                        save_msg = Some((
                                            format!("Save failed: {e}"),
                                            std::time::Instant::now(),
                                        ));
                                    }
                                }
                            } else {
                                record_start_offset = practice.position();
                                recording.start();
                            }
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            preset_open = true;
                            preset_cursor = 0;
                        }
                        KeyCode::Char('t') | KeyCode::Char('T') => {
                            tuner_open = true;
                            tuner
                                .active
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        KeyCode::Char('m') | KeyCode::Char('M') => {
                            metronome_open = true;
                        }
                        #[cfg(feature = "clap")]
                        KeyCode::Char('v') | KeyCode::Char('V') => browser.open(),
                        #[cfg(all(feature = "au", target_os = "macos"))]
                        KeyCode::Char('u') | KeyCode::Char('U') => amp_browser.open(),
                        // Live A/B between the loaded AU amp and the built-in amp (no modal).
                        #[cfg(all(feature = "au", target_os = "macos"))]
                        KeyCode::Char('z') | KeyCode::Char('Z') => {
                            use std::sync::atomic::Ordering::Relaxed;
                            if params.amp_external_loaded.load(Relaxed) {
                                let now = !params.amp_external_active.load(Relaxed);
                                params.amp_external_active.store(now, Relaxed);
                            }
                        }
                        KeyCode::Char('i') | KeyCode::Char('I') => ir_browser.open(),
                        // Live A/B between the loaded IR and the built-in cab (no modal).
                        KeyCode::Char('x') | KeyCode::Char('X') => {
                            use std::sync::atomic::Ordering::Relaxed;
                            if params.cab_external_loaded.load(Relaxed) {
                                let now = !params.cab_external_active.load(Relaxed);
                                params.cab_external_active.store(now, Relaxed);
                            }
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            save_open = true;
                            save_name.clear();
                            save_desc.clear();
                            save_field = 0;
                            save_error = None;
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            cycle_amp(&params, 1);
                        }
                        KeyCode::Char('c') | KeyCode::Char('C') => {
                            cycle_cab(&params);
                        }
                        KeyCode::Tab => {
                            focus = next_section(focus, &board, &panels, &params.chain_slots());
                        }
                        KeyCode::BackTab => {
                            focus = prev_section(focus, &board, &panels, &params.chain_slots());
                        }
                        KeyCode::Right => {
                            focus = nav_knob(focus, &board, &panels, &params.chain_slots(), 1);
                        }
                        KeyCode::Left => {
                            focus = nav_knob(focus, &board, &panels, &params.chain_slots(), -1);
                        }
                        KeyCode::Up | KeyCode::Char('+') | KeyCode::Char('=') => match focus {
                            None => cycle_amp(&params, 1),
                            Some(ADD_TILE | PRACTICE_TILE) => {}
                            Some(i) => nudge(&params, i, 0.05),
                        },
                        KeyCode::Down | KeyCode::Char('-') => match focus {
                            None => cycle_amp(&params, -1),
                            Some(ADD_TILE | PRACTICE_TILE) => {}
                            Some(i) => nudge(&params, i, -0.05),
                        },
                        KeyCode::Enter if focus == Some(ADD_TILE) => {
                            add_open = true;
                            add_cursor = 0;
                        }
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            if let Some(i) = focus
                                && let Some(pi) = pedal_of(i)
                            {
                                remove_pedal(&params, &mut board, pi);
                                focus = Some(ADD_TILE);
                            }
                        }
                        // Reorder the chain: move the focused pedal (or the
                        // amp+cab block, from amp/mic focus) one slot earlier /
                        // later. The timeline's `[`/`]` arms above match first
                        // while it owns focus, so there is no conflict.
                        KeyCode::Char('[') => {
                            move_stage(&params, focus, -1);
                        }
                        KeyCode::Char(']') => {
                            move_stage(&params, focus, 1);
                        }
                        KeyCode::Char(' ') => match focus {
                            Some(ADD_TILE) => {
                                add_open = true;
                                add_cursor = 0;
                            }
                            Some(PRACTICE_TILE) => {}
                            Some(i) => toggle_pedal(&params, i),
                            None => {}
                        },
                        _ => {}
                    }
                }
            }
        }
        if !change_device {
            break 'session;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
