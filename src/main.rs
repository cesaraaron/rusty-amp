use anyhow::Result;
use std::sync::Arc;

use rusty_riff::{dsp, practice, preset, recording, ui};

fn main() -> Result<()> {
    rusty_riff::migrate_legacy_config_dir();

    let params = Arc::new(dsp::Params::new());
    let levels = Arc::new(dsp::Levels::new());
    let tuner = Arc::new(dsp::Tuner::new());
    let metronome = Arc::new(dsp::Metronome::new());
    let presets = preset::load_all();
    let capture = Arc::new(recording::CaptureState::new());
    let practice = Arc::new(practice::Practice::new());

    // TUI starts immediately; device selection happens inside via modals.
    ui::run(params, levels, tuner, metronome, presets, capture, practice)?;

    Ok(())
}
