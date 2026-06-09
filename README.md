# ym38x6

A fictional FM synthesizer and composition support app.

## Concept

**38x6** is an imaginary FM sound chip — what if FM synthesis had taken one more step forward before PCM took over? It is based on YAMAHA's YM3806 (OPQ) with waveform extensions inspired by the OPZ, implemented entirely in Rust.

The companion **composition app** lets anyone play musically coherent chords without music theory knowledge, using a calibration-based gesture UI with no grids or guides.

Inspired by Ryu Umemoto's YM-2609, which explored a similar "what if" premise using SynthEdit + VOPM.

## Architecture

```
ym38x6/
  ym38x6-core/        # Sound engine — pure Rust, no framework dependencies
  ym38x6-app/         # Composition app (Tauri v2, Windows desktop)
    src/              # Frontend: calibration + gesture UI (HTML/JS)
    src-tauri/        # Backend: cpal WASAPI output, Tauri commands
  ym38x6-vst/         # VST3/CLAP plugin — phase 6+
```

`ym38x6-core` has zero dependencies on nih-plug, Tauri, or cpal. The audio engine is fully isolated.

## Sound Engine

### WMS-1 (Phase 1)

A waveform memory sound source — equivalent to one operator of the 38x6 FM engine. Used as the prototype for verifying the gesture UI and chord logic.

- Internal wave format: 1024 × u16, log encoding (ymfm-compatible)
  - `bit14~0`: −log₂|amplitude| in 4.8 fixed point
  - `bit15`: sign flag
- Built-in waveforms: sine, square, sawtooth, triangle (slots 0–3)
- User waveforms: 32 × i8 linear input → auto-converted to internal format (slots 8–255)
- ADSR envelope: all parameters 0–255 (8-bit unified), exponential rate mapping
- Unlimited polyphony via `HashMap`-based stable channel IDs

### 38x6 FM Engine (Phase 2+)

4-operator FM synthesis, OPQ-derived with OPZ waveform extensions.

- 4op / channel, 8 algorithms
- 12-bit F-Number (more precise than OPN's 11-bit)
- Per-channel dual frequency (OPQ-derived: Op0/2 and Op1/3 pairs independent)
- Per-operator key-on (Op3 as master)
- All parameters 0–255 (8-bit unified), F-Number is the only 16-bit exception

## Composition App

### Gesture UI

No grids, no guides. The coordinate space is defined by the player's own calibration.

**Calibration** (mouse or touch):
Click C major, F major, and G major at positions that feel natural. The I–IV–V triangle defines the entire coordinate system — both the root note axis and the chord type axis.

**Playing** (mouse version):
- Hold and drag to play
- Y direction (along root axis) → root note
- X direction (perpendicular) → chord type: `dim` ← `m` ← `maj` → `7` → `maj7`
- Release → note-off (ADSR release)
- `R` key → recalibrate

The gesture system requires no recognition algorithm — everything is continuous coordinate-to-pitch mapping. An ∞ motion naturally produces vibrato.

### Avoid Note Handling (Phase 3+)

Uses OPQ-derived per-operator key-on: avoid notes play at reduced volume rather than being silenced, giving musical feedback instead of a hard block.

## Development Roadmap

| Phase | Scope |
|-------|-------|
| 1 | WMS-1 + Tauri desktop app + gesture UI (current) |
| 2 | 38x6 FM engine, waveform selection, detune |
| 3 | Dual frequency, per-operator key-on |
| 4 | Parameter UI, preset save/load, PSR-70 converter |
| 5 | Tablet support (Tauri v2 iOS/Android) |
| 6 | VST3/CLAP plugin via nih-plug |
| 7 | Algorithm routing extension (SY77-style) |

## Building

```powershell
# Check workspace
cargo check --workspace --message-format=short

# Run tests
cargo test -p ym38x6-core

# Run app (first run compiles all dependencies, ~5 min)
cd ym38x6-app
npm install
npm run tauri dev
```

Requires: Rust (rustup), Node.js, WebView2 runtime (pre-installed on Windows 11).

## References

- [ymfm](https://github.com/aaronsgiles/ymfm) — OPQ/OPZ/OPN reference implementation (Aaron Giles, BSD 3-Clause)
- [PSR70-reverse](https://github.com/JKN0/PSR70-reverse) — OPQ programmer's guide and 450-entry PSR-70 preset data (Jari Kangas)
- [MDSound / fmvgen](https://github.com/kuma4649/MDSound) — YM2609 emulator (kuma4649, C#) — the original implementation of Ryu Umemoto's fictional chip concept
- [YM2609](https://github.com/LTVA1/YM2609) — C++ port of the above (LTVA1, GPL-3.0)
