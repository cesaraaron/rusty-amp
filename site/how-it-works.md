---
layout: page.njk
permalink: how-it-works.html
title: "How it works · rusty-amp"
ogTitle: "rusty-amp · how it works"
description: "Under the hood of rusty-amp: the full sample-by-sample signal chain, 8× oversampled amp stages, passive FMV tone stacks, and partitioned-FFT cabinet convolution."
eyebrow: "Under the hood"
heading: "How rusty-amp works"
lead: "Your guitar runs through a full signal chain — pedals, amp, cabinet, and a stereo effects rack — sample by sample. Here's every stage."
toc:
  - { href: "#summary", label: "The short version" }
  - { href: "#chain", label: "Full signal chain" }
  - { href: "#amp", label: "Amp stages" }
  - { href: "#cabinet", label: "Cabinet convolution" }
prev: { href: "tools.html", label: "Tools" }
next: { href: "https://github.com/danylokravchenko/rusty-amp/blob/main/CONTRIBUTING.md", label: "Contributing on GitHub ↗" }
---

## The short version {#summary}

<div class="grid grid--3">
  <div class="card">
    <div class="ico">⚡</div>
    <h3>8× oversampled drive</h3>
    <p>The amp's distortion stages run at 8× oversampling (linear-phase polyphase-FIR) for smooth, alias-free high-gain saturation.</p>
  </div>
  <div class="card">
    <div class="ico">🎚️</div>
    <h3>Real FMV tone stack</h3>
    <p>The tube amps use a passive FMV tone stack — interacting controls and inherent mid scoop — plus modelled power-amp ↔ speaker interaction.</p>
  </div>
  <div class="card">
    <div class="ico">📐</div>
    <h3>IR convolution cabs</h3>
    <p>Cabinets are rendered by multi-mic impulse-response convolution via a partitioned-FFT engine, for three-dimensional depth.</p>
  </div>
</div>

## Full signal chain {#chain}

Every block below is processed per sample. Bracketed stages are `[bypassable]` — remove or bypass them and they drop out of the path entirely.

The order below is the shipped default, not a fixed circuit: press <kbd>1</kbd> for the live-order ribbon, pick a stage with <kbd>←</kbd>/<kbd>→</kbd>, and move it earlier or later with <kbd>[</kbd> / <kbd>]</kbd> — the amp and cab move independently, though the cab must always follow its amp. The ribbon at the top of the app always shows the live order. A mono pedal placed on a stereo signal sums to mono and duplicates back out; a stereo pedal placed before the amp/cab promotes the signal to dual mono — so putting e.g. the compressor after the cabinet is fair game. Only a **live** effect does that conversion: a bypassed stage is wire-transparent, passing the signal through untouched in whatever domain it arrived, so moving a bypassed pedal never changes the stereo image. Custom orders are saved into [presets](presets.html#chain).

<div class="flow">

  <div class="flow__stage flow__stage--io" style="--c:var(--rust-light)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Guitar</span><span class="flow__badge flow__badge--mono">Mono in</span></div>
      <div class="flow__sig">Dry signal from your high-impedance instrument input.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--gray)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Noise Gate</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Envelope follower → gain ramp (smooth open/close to avoid clicks).</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--cyan)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Whammy</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Two-tap crossfaded varispeed delay line → ±12-semitone transpose → wet tone LP, blended with dry.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--orchid)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Auto-Wah</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Envelope follower (fast attack / slow release) → sweeps a resonant TPT state-variable bandpass (~300 Hz–3 kHz) → dry/wet blend · FREQ base position · SENS auto-sweep · Q resonance.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--gold)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Compressor</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Peak-follower detector → hard-knee gain computer (2:1–10:1) → smoothed gain + auto makeup.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--magenta)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Fuzz</span><span class="flow__tag">Big Muff / Fuzz Face / Tone Bender</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">DC block → 70 Hz HP → <span class="os">[4× OS: two or three cascaded asymmetric soft-clip stages]</span> → DC block → 700 Hz mid scoop (Big Muff) or no scoop with a softer germanium-style clip (Fuzz Face) or a hotter, harder-kneed three-transistor chain (Tone Bender MkII) → variable tone LP.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--green)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">TS-808 Tube Screamer</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">DC block → 340 Hz HP → 720 Hz mid-peak → <span class="os">[4× OS: asymmetric diode soft-clip]</span> → output coupling cap (DC block) → variable tone LP.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:#e08840">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">DS-1 Distortion</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">DC block → 80 Hz HP → 800 Hz mid-emphasis → <span class="os">[4× OS: pre-clip HP → 4.5 kHz pre-clip LP → asymmetric tanh diode clip]</span> → post-clip HP → tilt tone → 6.5 kHz post-clip LP.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--steel)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">ML-2 Metal Core</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">DC block → 90 Hz HP → fixed 650 Hz mid-scoop → <span class="os">[8× OS: pre-clip HP → 3.8 kHz pre-clip LP → two cascaded asymmetric tanh soft-clip stages]</span> → post-clip HP → active Low/High shelves (±15 dB) → 7 kHz post-clip LP.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--lime)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Pre-amp EQ</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Low shelf (100 Hz) → Mid peak (650 Hz) → High shelf (3 kHz) — each ±12 dB · shapes what the amp clips.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--magenta)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Uni-Vibe</span><span class="flow__tag">Front of amp</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Four LFO-swept, staggered first-order all-passes with a photocell-shaped (dwelling) sweep → chorus (dry + phase) or vibrato (phase only, pitch shimmer) blend — mono, last before the amp, so it interacts with the fuzz like the real pedal.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--rust)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Amp</span><span class="flow__badge flow__badge--live">Switchable live</span></div>
      <div class="flow__sig"><span class="os">8× oversampled</span> nonlinear stages (linear-phase polyphase-FIR anti-alias) + dynamic grid-bias “bloom” for touch sensitivity.</div>
      <div class="flow__sub"><b>JCM800</b> — dual 12AX7 atan soft-clip + grid-blocking → passive FMV tone stack → tube sag with 100 Hz supply ripple (ghost notes) → speaker-load bloom → dynamic NFB presence</div>
      <div class="flow__sub"><b>Mesa DR</b> — triple gain stage (atan → atan → exponential) + grid-blocking → passive FMV tone stack → silicon sag with 120 Hz supply ripple → speaker-load bloom → dynamic NFB presence</div>
      <div class="flow__sub"><b>Randall</b> — FET → BJT → rail-clip → active tone stack → stiff solid-state power section → static speaker load</div>
      <div class="flow__sub"><b>Vox AC30</b> — dual 12AX7 atan soft-clip → passive FMV tone stack (brighter, less scoop) → no-NFB Class A sag → speaker-load bloom</div>
      <div class="flow__sub"><b>Hiwatt DR103</b> — dual 12AX7 atan soft-clip → passive FMV tone stack (flatter mid, bright top) → stiff solid-state sag with a strong Partridge-style output transformer → speaker-load bloom</div>
      <div class="flow__sub"><b>Plexi 1959</b> — dual 12AX7 atan soft-clip (lower preamp gain) + grid-blocking → jumpered Bright/Normal channels → passive FMV tone stack → tube-rectified sag with 100 Hz supply ripple → early output-transformer saturation → dynamic speaker-load bloom</div>
      <div class="flow__sub"><b>Fender Twin</b> — dual 12AX7 atan soft-clip (high headroom) + grid-blocking → blackface passive FMV tone stack → onboard spring reverb → bias tremolo → clean 6L6 power section → speaker-load bloom</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--amber)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Cabinet</span><span class="flow__badge flow__badge--mono">Mono → Stereo</span><span class="flow__badge flow__badge--live">Switchable live</span></div>
      <div class="flow__sig">Nonlinear speaker drive (excursion-driven motor droop · cone breakup · thermal power compression · Doppler FM growl — calibrated to engage at real amp levels) → neighbour-cone interference from the box's real geometry (three cones on a 4×12, one stacked partner on a 2×12) → dust-cap &amp; grille echoes (2–8 kHz presence/air comb) → blended multi-mic impulse-response convolution — close SM57 dynamic + R121 ribbon + room mic, each a ~93 ms voiced-EQ skeleton + dense reflection tail + cone-resonance ring + mid/breakup scatter texture; mono-solid close blend + decorrelated room pair → solid centre with natural width · mic-position shelf.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--sand)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Graphic EQ</span><span class="flow__badge flow__badge--stereo">Stereo</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Boss GE-7 style seven-band peak bank (100 · 220 · 470 · 1k · 2.2k · 4.7k · 10k Hz, constant Q ≈ 2) in series, each fader ±12 dB → output level ±15 dB — a fixed-frequency curve you draw in the effects loop, before the parametric shelves.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--teal)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Parametric EQ</span><span class="flow__badge flow__badge--stereo">Stereo</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Low shelf (120 Hz) → Mid peak (800 Hz, Q 1.5) → High shelf (5 kHz) — each ±15 dB.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--indigo)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Flanger</span><span class="flow__badge flow__badge--stereo">Stereo</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">LFO-swept short delay (~0.5–5 ms) mixed with the dry signal — moving comb notches · RATE 0.05–5 Hz · DEPTH · FEEDBACK 0–90% · dry/wet MIX · L/R read a quarter-cycle apart for stereo drift.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--pink)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Chorus</span><span class="flow__badge flow__badge--stereo">Stereo</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">LFO-swept long delay (~8–20 ms), no feedback, mixed with the dry signal — lush pitch-shimmer, not a comb sweep · RATE 0.05–5 Hz · DEPTH · dry/wet MIX · L/R read half a cycle apart for stereo width.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--yellow)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Phaser</span><span class="flow__badge flow__badge--stereo">Stereo</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">4-stage LFO-swept all-pass cascade summed with the dry signal — sweeping notches in the dry+wet sum · RATE 0.05–5 Hz · DEPTH (~200 Hz–1.6 kHz) · FEEDBACK 0–90% resonance · dry/wet MIX · L/R read a quarter-cycle apart for stereo width.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--rose)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Tremolo / Vibrato</span><span class="flow__badge flow__badge--stereo">Stereo</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">One LFO driving both modulations, blended by MODE — amplitude tremolo (shaped sine → soft-square chop) and pitch vibrato (a ±3 ms LFO-swept delay tap) · RATE 0.5–14 Hz · DEPTH · SHAPE sine→square · runs on both channels for a coherent pulse/wobble.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--purple)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Delay</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Digital: stereo ping-pong — repeats bounce L↔R. Tape: same-channel repeats with wow/flutter transport drift, feedback high-cut and soft saturation. TIME 0–500 ms · FEEDBACK 0–70/85% · dry/wet MIX.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--blue)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Stereo Reverb</span><span class="flow__badge flow__badge--bypass">Bypassable</span></div>
      <div class="flow__sig">Dual decorrelated Freeverb cores (8 parallel combs → 4 series allpasses each) → dry/wet mix.</div>
    </div>
  </div>

  <div class="flow__stage" style="--c:var(--gray)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Master bus</span></div>
      <div class="flow__sig">Mid/side stereo widener (mono centre preserved) — width is the configurable studio master: <kbd>W</kbd> toggles neutral (1.0) ↔ studio wide (1.3).</div>
    </div>
  </div>

  <div class="flow__stage flow__stage--io" style="--c:var(--rust-light)">
    <div class="flow__card">
      <div class="flow__head"><span class="flow__name">Output</span><span class="flow__badge flow__badge--stereo">Stereo L / R</span></div>
      <div class="flow__sig">Per-channel output soft limiter — independent of the widener, so peak protection never changes with the width setting.</div>
    </div>
  </div>

</div>

For the per-knob behaviour of each pedal, see [Pedals & effects](pedals.html).

<div class="note note--info">
<b>Two rigs, one monitor.</b> The live guitar feeds the main rig. Finished <b>raw takes</b> are summed into a single guitar bus and run through a <b>second, independent rig instance</b> that shares the same knob settings but keeps separate DSP state — so your takes are re-amped live by the current pedals, amp and cab, while their reverb/delay tails never leak into the live signal. Imported <b>backing tracks</b> and the practice <b>metronome</b> stay monitor-only and are summed in after the capture tap, so they are never recorded. A raw take captures the <b>dry</b> selected input channel before the gate and pedals, which is why changing the rig changes the take non-destructively.
</div>

## The amp stages {#amp}

All seven amps share an **8× oversampled** nonlinear core with a linear-phase polyphase-FIR anti-alias filter, plus a dynamic grid-bias "bloom" that makes the gain respond to how hard you play. Beyond that, each model diverges:

| Model | Character | Tone stack | Rectifier / power | Gain stages |
| ----- | --------- | ---------- | ----------------- | ----------- |
| **Marshall JCM800** | Punchy, dynamic, touch-sensitive | Passive FMV (Marshall values) | Tube sag (5 ms / 150 ms) + 100 Hz supply ripple + dynamic speaker-load bloom | 2 × 12AX7 atan soft-clip |
| **Marshall Plexi** | Cranked, power-amp grind, non-master | Passive FMV (Marshall values) | Tube-rectified sag (6.7 ms / 200 ms) + 100 Hz supply ripple + early output-transformer saturation + dynamic speaker-load bloom | 2 × 12AX7 atan soft-clip (lower preamp gain) |
| **Mesa Dual Rectifier** | Compressed, aggressive, modern | Passive FMV (Fender values) | Silicon sag (0.5 ms / 80 ms) + 120 Hz supply ripple + dynamic speaker-load bloom | 3-stage: atan → atan → exponential |
| **Randall Warhead** | Tight, crushing, solid-state | Active, independent bands + fixed +3 dB presence | No sag — stiff solid-state rails + static speaker resonance | FET (x/√(1+x²)) → BJT (tanh) → rail-clip |
| **Vox AC30** | Chimey, touch-sensitive, Class A | Passive FMV (Vox values — lighter scoop, brighter) | Class A sag (3.8 ms / 260 ms, no NFB) + dynamic speaker-load bloom | 2 × 12AX7 atan soft-clip |
| **Hiwatt DR103** | Clean, hi-fi, high-headroom | Passive FMV (Hiwatt values — flatter mid, bright top) | Stiff solid-state sag (4 ms / 90 ms) + strong Partridge-style output transformer + dynamic speaker-load bloom | 2 × 12AX7 atan soft-clip |
| **Fender Twin Reverb** | Glassy clean, high-headroom | Passive FMV (Fender blackface values) | 6L6 clean sag (5.3 ms / 125 ms) + big output transformer + dynamic speaker-load bloom | 2 × 12AX7 atan soft-clip + onboard spring reverb &amp; bias tremolo |

The **passive FMV tone stack** is a single RC network where bass, mid, and treble interact and the mids inherently scoop — exactly like a real amp — followed by a **power-amp ↔ speaker interaction** model: the speaker's impedance resonance blooms the low end dynamically as the supply sags under hard playing. The Vox has no global negative-feedback loop, so it sags more readily and blooms harder than the Marshall/Mesa; the Hiwatt's stiff solid-state-rectified supply sags the least and stays tight and clean; the Plexi's tube rectifier sags the most for its elastic cranked grind. The Randall keeps an active, independent-band stack and a small static speaker resonance, true to its stiff solid-state design. See [Amps & cabinets](amps-cabs.html#amp) for the per-knob breakdown.

Three further pieces of tube-amp physics live in the tube models:

- **Ghost notes (supply ripple).** A real B+ rail is rectified mains, so a ripple at twice the mains frequency (100 Hz for the UK-built JCM800, 120 Hz for the US-built Recto) rides on the supply and grows as hard playing loads it down. That ripple amplitude-modulates the power stage, putting faint sidebands ±100/120 Hz around every note — the subliminal "big amp working hard" texture. At idle it all but vanishes.
- **Grid-blocking distortion.** Beyond the gentle cathode-bias give, a *truly slammed* input (a boost into a cranked front end, a violent transient) drives grid current that charges the coupling cap near-instantly, choking the stage — the note's attack spits, then the charge bleeds off over the grid-leak RC (~30 ms) and the gain recovers into the note. Ordinary playing never touches it.
- **Dynamic presence.** Presence lives inside the power-amp negative-feedback loop, so it changes with drive: as the power stage saturates the loop loses authority, the knob's range shrinks, and the top end settles toward the amp's fixed open-loop lift — rather than sitting on a static shelf.

## Cabinet convolution {#cabinet}

Each cabinet is rendered by **impulse-response convolution** rather than a plain EQ. The built-in IRs are synthesized in-code (nothing to ship or download — though you can also [load your own `.wav` IR](amps-cabs.html#irs)): the model's voiced EQ provides the magnitude skeleton, then early reflections (comb filtering), a dense 6–21 ms cabinet/room reflection tail, and speaker modal resonances add the time-domain depth of a real miked cab. The cone "thump" ring is deliberately short — measured reference captures are gated tight, and a long synthetic ring reads as boxy mud rather than depth. Every cab's tonal balance, body-over-pocket tilt, spectral texture, echo density and stereo image are measured against real commercial captures (God's Cab, Softube) with `examples/cab_analysis.rs`, so a regression on any of those axes shows up as numbers, not vibes.

Each IR runs ~93 ms (~4500 taps at 48 kHz) — long enough for the reflection tail to fully develop. On top of the hand-authored modes, two seeded **scatter clusters** per channel add small, irregularly-placed high-Q resonances: one across the cone-breakup band, and one through the 0.55–2.3 kHz mids, where real captures carry a couple of dB of fine reflection ripple that hand-authored taps alone can't reproduce (without it the mids read as statistically "airbrushed"). The close-mic textures are **shared between left and right**: real close captures are effectively mono (L/R correlation 1.0), and detuned per-side textures measure as phasey width that smears the centre image. Only the scatter seeds differ per channel — like a real speaker pair, whose breakup patterns never match — while the room-mic pair stays genuinely decorrelated, for a solid centre with natural width.

### The convolution engine

The convolution is computed with a **partitioned-FFT (uniformly-partitioned overlap-save)** engine rather than a direct tap-by-tap loop. At ~4500 taps per channel this is the single heaviest DSP stage, and the frequency-domain approach cuts its cost several-fold while producing the exact same linear convolution — so the tone is unchanged. The only trade-off is a fixed ~2.7 ms of latency (128 samples at 48 kHz), shared equally by both channels so the stereo image stays aligned.

### Three mics, one blend

Each cabinet is captured by **three mics** — a close SM57 dynamic, a close R121 ribbon, and a room mic — each with its own voicing and reflection texture (the room mic carries extra pre-delay and denser late reflections for air). The **Blend** and **Room** knobs mix these captures. Because convolution is linear, the blend is just a weighted **sum of the three IRs**, recombined into the live convolver only when a knob moves — so any mic mix costs exactly two convolutions per sample, no more.

The **Mic** knob applies a high-shelf filter (±6 dB at 5 kHz) per channel after convolution, modelling the tonal difference between an on-axis and off-axis close-mic placement. See [cabinet models](amps-cabs.html#cabs) for the per-cab voicing.

### Neighbour cones and the last drop of iron

A close mic on one cone of a 4×12 also hears the **three neighbouring cones** — the same signal arriving late and dull (heard far off-axis, where a 12" cone beams its top end away). rusty-amp derives these arrivals from the actual box geometry: 12" drivers on a ~28 cm pitch with the mic capsule ~10 cm from the near cone ("an inch from the grille" plus the grille standoff and the cone's recess) put the two adjacent cones ~0.6 ms late and the diagonal one ~0.9 ms late, each lowpassed above ~2.2 kHz and 13–23 dB down. An open-back 2×12 has just one stacked partner at a slightly wider pitch. Each neighbour is an **extended source** — a 12" cone, not a point — so its arrival is smeared across a short cluster of sub-taps: the comb ripples the body by a couple of dB, like the echo scans of real captures, instead of carving a deep notch through the 0.5–1 kHz body the way a single point tap measures.

An octave higher, the last acoustic surfaces between the cone and the capsule add their own **dust-cap & grille echoes**. The metal front grille sits a couple of cm proud of the cone, so treble radiated forward reflects off it and back to the mic — a ~0.13 ms round-trip echo that combs the presence band (peak ~3.9 kHz, null ~7.8 kHz). Long wavelengths simply diffract past the perforations, so the reflected copy is highpassed and only the top is combed — the metallic sheen a close capture picks up off the grille (the effect God's Cab's *grill* captures lean on). A second, much shorter reflection — ~45 µs off the rigid central dust cap, which sits proud of the surrounding cone and beams the extreme top from nearer the capsule — ripples only the very top. Both are power-normalised so they add 2–8 kHz comb *structure*, not level, and their depth is kept gentle: measured against the reference captures, a hotter grille bounce starts carving the 1.6–2.6 kHz octave real IRs hold. Finally, a genuinely tiny **mic/transformer saturation** (the SM57's output iron, the ribbon's step-up transformer) squeezes the hottest peaks by a fraction of a dB — the last, subtlest nonlinearity in the capture chain.
