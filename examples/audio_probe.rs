//! Diagnoses audio-device startup failures on this machine.
//!
//! For every (non-null) input and output device, this tries to build an `f32`
//! stream first with a fixed 256-frame buffer (what rusty-amp prefers) and then
//! with the backend default. It prints `OK` / `ERR` per attempt so you can see
//! exactly which device rejects the small buffer, which one is busy (usually
//! because PipeWire already owns it), and so on.
//!
//! Pair mode rebuilds the app's exact sequence — input stream then output
//! stream, both live at once — for one input/output pair:
//!
//! ```text
//! cargo run --release --example audio_probe                 # per-device scan
//! cargo run --release --example audio_probe -- 2 3         # test input #2 + output #3
//! ```
//!
//! Indices are 1-based and match the scan output below (null/duplicate entries
//! are already filtered out).

use std::io::Write;

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, Device, StreamConfig};

const FRAMES: u32 = 256;

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();

    let host = cpal::default_host();

    let inputs = collect(host.input_devices(), true);
    let outputs = collect(host.output_devices(), false);

    println!("=== Input devices (index, name, channels) ===");
    for (i, (d, cfg)) in inputs.iter().enumerate() {
        let name = device_name(d, i);
        println!("{:>2}. {name}  ({} ch)", i + 1, cfg.channels);
    }
    println!("\n=== Output devices (index, name, channels) ===");
    for (i, (d, cfg)) in outputs.iter().enumerate() {
        let name = device_name(d, i);
        println!("{:>2}. {name}  ({} ch)", i + 1, cfg.channels);
    }

    if let [in_idx, out_idx] = args.as_slice() {
        probe_pair(&inputs, &outputs, *in_idx, *out_idx);
        return;
    }

    println!("\n=== Fixed({FRAMES}) vs Default, one stream at a time ===");
    for (i, (d, cfg)) in inputs.iter().enumerate() {
        probe_one(&device_name(d, i), true, d, cfg);
    }
    for (i, (d, cfg)) in outputs.iter().enumerate() {
        probe_one(&device_name(d, i), false, d, cfg);
    }

    println!(
        "\nTip: run `cargo run --release --example audio_probe -- <input> <output>` to \
         reproduce the app's simultaneous input+output startup for one pair."
    );
}

/// Filter `null` and duplicate `(name, channels)` entries, returning each device
/// with its default config. Mirrors `rusty_amp::audio`'s own device filtering.
fn collect(
    devices: Result<impl Iterator<Item = Device>, cpal::Error>,
    input: bool,
) -> Vec<(Device, StreamConfig)> {
    let Ok(devices) = devices else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, d) in devices.enumerate() {
        let name = device_name(&d, i);
        if name.contains("Discard all samples") || name.eq_ignore_ascii_case("null") {
            continue;
        }
        let cfg = if input {
            d.default_input_config().map(StreamConfig::from)
        } else {
            d.default_output_config().map(StreamConfig::from)
        };
        let Ok(cfg) = cfg else { continue };
        if seen.insert((name, cfg.channels)) {
            out.push((d, cfg));
        }
    }
    out
}

fn device_name(device: &Device, idx: usize) -> String {
    device
        .description()
        .map(|desc| desc.name().to_owned())
        .unwrap_or_else(|_| format!("device-{idx}"))
}

fn with_buffer(cfg: &StreamConfig, size: BufferSize) -> StreamConfig {
    StreamConfig {
        buffer_size: size,
        ..*cfg
    }
}

fn probe_one(name: &str, input: bool, device: &Device, base: &StreamConfig) {
    let label = if input { "IN " } else { "OUT" };
    for (tag, size) in [
        (format!("Fixed({FRAMES})"), BufferSize::Fixed(FRAMES)),
        ("Default".to_owned(), BufferSize::Default),
    ] {
        let cfg = with_buffer(base, size);
        let result = build(input, device, cfg);
        match result {
            Ok(()) => println!("{label} {name:<40} {tag:<12} OK"),
            Err(e) => println!("{label} {name:<40} {tag:<12} ERR: {e}"),
        }
        let _ = std::io::stdout().flush();
    }
}

fn build(input: bool, device: &Device, cfg: StreamConfig) -> Result<(), cpal::Error> {
    if input {
        device
            .build_input_stream::<f32, _, _>(cfg, |_d: &[f32], _| {}, |_e| {}, None)
            .map(|_| ())
    } else {
        device
            .build_output_stream::<f32, _, _>(cfg, |_d: &mut [f32], _| {}, |_e| {}, None)
            .map(|_| ())
    }
}

fn probe_pair(
    inputs: &[(Device, StreamConfig)],
    outputs: &[(Device, StreamConfig)],
    in_idx: usize,
    out_idx: usize,
) {
    let Some((in_dev, in_base)) = inputs.get(in_idx.saturating_sub(1)) else {
        println!("No input #{in_idx}");
        return;
    };
    let Some((out_dev, out_base)) = outputs.get(out_idx.saturating_sub(1)) else {
        println!("No output #{out_idx}");
        return;
    };

    println!(
        "\n=== Pair: in '{}' -> out '{}' ===",
        device_name(in_dev, in_idx),
        device_name(out_dev, out_idx),
    );

    // Match the app: output runs at the input's sample rate.
    let out_base = StreamConfig {
        sample_rate: in_base.sample_rate,
        ..*out_base
    };

    for (tag, size) in [
        (format!("Fixed({FRAMES})"), BufferSize::Fixed(FRAMES)),
        ("Default".to_owned(), BufferSize::Default),
    ] {
        let istream = build(true, in_dev, with_buffer(in_base, size));
        match istream {
            Err(e) => {
                println!("  {tag:<12} input ERR: {e}");
                continue;
            }
            Ok(_istream) => {
                let ostream = build(false, out_dev, with_buffer(&out_base, size));
                match ostream {
                    Ok(_ostream) => println!("  {tag:<12} OK (input + output live)"),
                    Err(e) => println!("  {tag:<12} output ERR: {e} (input opened OK)"),
                }
            }
        }
        let _ = std::io::stdout().flush();
    }
}
