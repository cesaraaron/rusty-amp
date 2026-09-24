use anyhow::Result;
use std::sync::Arc;

use rusty_amp::{dsp, practice, preset, recording, ui};

fn main() -> Result<()> {
    let params = Arc::new(dsp::Params::new());
    let levels = Arc::new(dsp::Levels::new());
    let tuner = Arc::new(dsp::Tuner::new());
    let metronome = Arc::new(dsp::Metronome::new());
    let presets = preset::load_all();
    let recording = Arc::new(recording::RecordingState::new());
    let practice = Arc::new(practice::Practice::new());

    // TUI starts immediately; device selection happens inside via modals.
    ui::run(
        params, levels, tuner, metronome, presets, recording, practice,
    )?;

    Ok(())
}
