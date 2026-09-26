# 🎸 rusty-riff

A guitar amp and pedalboard rig that runs in your terminal. Keyboard-driven, with
live metering and artist-inspired presets.

## Requirements

- Rust 1.96+
- A guitar and an audio interface with a high-impedance instrument input
- Speakers or headphones

## Build & run

```bash
git clone https://github.com/cesaraaron/rusty-riff
cd rusty-riff
cargo run --release
```

Use `--release`: a debug build can underrun the audio callback. On first launch
the app asks for your input device, input channel, and output device.

## Essential keys

| Key | Action |
| --- | --- |
| `K` | Full in-app key reference |
| `A` / `C` | Amp / cabinet browser |
| `P` | Preset browser |
| `↑` / `↓` or `+` / `-` | Select a knob / adjust its value |
| `Space` | Bypass the focused stage |
| `[` / `]` | Move the selected stage earlier or later |
| `1`–`4` | Focus the chain, amp, timeline, or pedals |
| `R` | Record / stop a dry take |
| `Q` | Quit |

## Where things live

- Presets: `~/.config/rusty-riff/presets/`
- External IRs: `~/.config/rusty-riff/irs/`

## License

Apache 2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
