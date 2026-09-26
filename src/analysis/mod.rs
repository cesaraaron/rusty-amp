//! Offline analysis for the fidelity harness, tests, and examples.
//!
//! Nothing in this module runs on the audio thread: the metrics may allocate and
//! are written for clarity. See [`metrics`] for measurements, [`synth`] for the
//! deterministic DI corpus, and [`render`] for offline preset rendering.

pub mod metrics;
pub mod render;
pub mod synth;

pub use metrics::*;
pub use render::*;
pub use synth::*;
