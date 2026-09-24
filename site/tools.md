---
layout: page.njk
permalink: tools.html
title: "Tools · rusty-amp"
ogTitle: "rusty-amp · tools"
description: "The built-in chromatic tuner, practice metronome, and one-key WAV recording in rusty-amp — how to use them and what they capture."
eyebrow: "Tools"
heading: "Tools"
lead: "Built-in utilities you can reach at any time — a chromatic tuner that mutes the rig to a clean signal, a practice metronome that stays out of your recordings, and one-key recording of the processed output."
toc:
  - { href: "#tuner", label: "Tuner" }
  - { href: "#metronome", label: "Metronome" }
  - { href: "#practice", label: "Practice / jam-along" }
  - { href: "#recording", label: "Recording" }
prev: { href: "plugins.html", label: "CLAP plugins" }
next: { href: "how-it-works.html", label: "How it works (under the hood)" }
---

## Tuner <span class="muted">(<kbd>T</kbd>)</span> {#tuner}

<figure class="shot">
  <div class="shot__bar"><i></i><i></i><i></i></div>
  <img src="assets/tuner.png" alt="rusty-amp guitar tuner" />
</figure>

Press <kbd>T</kbd> to open the chromatic tuner. While it's open the **entire rig is bypassed** — every pedal, the amp, and the cabinet are taken out of the path and the dry guitar passes straight to the output, so you hear (and tune against) a clean signal. Pitch is estimated with the McLeod normalised square-difference function (NSDF), accurate to a few cents.

The tuner shows:

- **The detected note** — large, with its octave (e.g. `E2`), green when in tune.
- **A ±cents needle** — `♭ ◄ … centre … ► ♯`. Left when flat, right when sharp; green within ±5 cents, amber within ±15, red beyond.
- **A verdict** — `IN TUNE`, `TUNE UP ▲` (flat), or `TUNE DOWN ▼` (sharp), plus the raw frequency in Hz.
- **A live note spectrum** — a log-spaced magnitude display from ~60 Hz to ~1.2 kHz, with the played fundamental highlighted.

<div class="note">
Standard tuning reference:
<b>E2</b> 82.41 Hz · <b>A2</b> 110.00 · <b>D3</b> 146.83 · <b>G3</b> 196.00 · <b>B3</b> 246.94 · <b>E4</b> 329.63.
Press <kbd>Esc</kbd> / <kbd>T</kbd> to close and restore the full rig.
</div>

## Metronome <span class="muted">(<kbd>M</kbd>)</span> {#metronome}

<figure class="shot">
  <div class="shot__bar"><i></i><i></i><i></i></div>
  <img src="assets/metronome.png" alt="rusty-amp metronome" />
</figure>

Press <kbd>M</kbd> to open the practice metronome. It plays a steady click through your monitor so you can lock in your timing while you play — the rig stays fully live, so you keep hearing your tone alongside the beat.

The modal shows:

- **The current tempo** — a big BPM readout that turns green while the click is running.
- **A tempo slider** — from **40** to **240 BPM**. Adjust it with <kbd>←</kbd> / <kbd>→</kbd> (or <kbd>↑</kbd> / <kbd>↓</kbd>), one BPM at a time.
- **A start/stop toggle** — <kbd>Space</kbd> (or <kbd>Enter</kbd>) turns the click on and off.

The metronome keeps ticking after you close the modal, so you can dial in a tempo, press <kbd>Esc</kbd> / <kbd>M</kbd>, and play along with the full board and meters on screen. Re-open it any time to change tempo or stop the click.

<div class="note note--info">
<b>Never on the tape.</b> The click is mixed into the monitor output <em>after</em> the recording tap, so an active metronome is <b>never captured</b> in your WAV — you can practise to the beat and record a clean take at the same time.
</div>

## Practice / jam-along <span class="muted">(<kbd>B</kbd>)</span> {#practice}

Play along with a backing track and loop the hard bits. Press <kbd>B</kbd> to open the **Practice tracks** browser and pick an audio file — **MP3, WAV, or FLAC** — from `~/Music`, `~/Desktop`, the current directory, `$RUSTY_AMP_PRACTICE_DIR`, or a path you type in. The file is decoded and resampled to your interface's rate off the audio thread, so the TUI never stalls.

The **practice timeline** is the horizontal pane under the amp panel. It shows a transport line and one line per track, with a mini waveform and the playhead:

- **Transport** — press <kbd>3</kbd> or <kbd>Tab</kbd> to focus the timeline, then <kbd>↑</kbd>/<kbd>↓</kbd> to pick the transport, <kbd>Space</kbd> to play/pause, and <kbd>←</kbd>/<kbd>→</kbd> to seek ±5&nbsp;s.
- **Loop a section** — park the playhead and press <kbd>[</kbd> for the in-point and <kbd>]</kbd> for the out-point; <kbd>L</kbd> toggles looping. The loop region is shaded on the timeline.
- **Mute a track** — select the backing or take line and press <kbd>Space</kbd> (the LED turns hollow).
- **Delete a track** — select it and press <kbd>Delete</kbd>/<kbd>Backspace</kbd>.
- **Your take** — press <kbd>R</kbd> to record; when you stop, the take is saved as a WAV **and** dropped onto the timeline, aligned to where the backing was when you started.

<div class="note note--info">
<b>Monitor-only, like the metronome.</b> The backing track and your take are mixed into your monitor <em>after</em> the recording tap, so they are <b>never captured</b> in a new WAV — you always record just your guitar.
</div>

### Hiding panels {#panels}

The screen is four numbered panels. <kbd>1</kbd> focuses the live-order ribbon (always visible), <kbd>2</kbd> the **amp &amp; cabinet** panel, <kbd>3</kbd> the **practice timeline**, <kbd>4</kbd> the guitar **pedalboard** — pressing a number again on its focused panel hides it (`1` never hides).

Hidden panels are skipped by <kbd>Tab</kbd> and give their space back to the rest of the rig. The choice lasts for the session.

## Recording <span class="muted">(<kbd>R</kbd>)</span> {#recording}

Press <kbd>R</kbd> to start recording. A blinking `●REC` lamp lights up on the practice timeline's transport line (next to `▶`). Press <kbd>R</kbd> again to stop — the file is written immediately and the saved path is shown briefly in the footer.

<div class="widget rec" data-rec>
  <div class="rec__header"><span>⏸</span><span class="rec__air">○REC</span><span>00:00 / 00:00</span></div>
  <div class="rec__wave"></div>
  <div class="rec__time">00:00</div>
  <div class="rec__controls"><button class="rec__btn"><span class="dot">●</span>Record</button></div>
  <div class="rec__file"></div>
</div>
<p class="widget__demo"><b>Interactive demo</b> — hit <em>Record</em> to see the on-air flow; nothing is captured here.</p>

Recordings capture the fully-processed signal (after the entire effects chain and output limiter) as a 32-bit float **stereo** WAV at the same sample rate as your audio interface — the full multi-mic cab spread and stereo effects are preserved. Files are named `rusty-amp-<unix-timestamp>.wav` and saved to your home directory (`~/`).
