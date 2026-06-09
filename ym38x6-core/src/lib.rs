use std::collections::HashMap;

/// Internal wave table format (ymfm-compatible).
///
/// 1024 entries × u16
///   bit14~0 : -log2(|amplitude|) in 4.8 fixed point  (0 = peak, 0x7FFF = silence)
///   bit15   : sign flag (1 = negative sample)
const WAVE_SIZE: usize = 1024;

pub struct WaveTable {
    data: [u16; WAVE_SIZE],
}

impl WaveTable {
    fn new() -> Self {
        Self { data: [LOG_SILENCE; WAVE_SIZE] }
    }
}

const LOG_SILENCE: u16 = 0x7FFF;

fn linear_to_log(sample: f32) -> u16 {
    let sign: u16 = if sample < 0.0 { 0x8000 } else { 0 };
    let abs = sample.abs().min(1.0);
    if abs < 1e-9 {
        return sign | LOG_SILENCE;
    }
    let log_val = ((-abs.log2()) * 256.0) as i32;
    sign | (log_val.clamp(0, LOG_SILENCE as i32) as u16)
}

fn log_to_linear(entry: u16) -> f32 {
    let log_val = (entry & 0x7FFF) as f32;
    if log_val >= 0x7E00 as f32 {
        return 0.0;
    }
    let sign = if entry & 0x8000 != 0 { -1.0f32 } else { 1.0 };
    sign * 2.0f32.powf(-(log_val / 256.0))
}

fn gen_sine() -> WaveTable {
    let mut t = WaveTable::new();
    for i in 0..WAVE_SIZE {
        let phase = i as f32 / WAVE_SIZE as f32;
        t.data[i] = linear_to_log((2.0 * std::f32::consts::PI * phase).sin());
    }
    t
}

fn gen_square() -> WaveTable {
    let mut t = WaveTable::new();
    for i in 0..WAVE_SIZE {
        t.data[i] = linear_to_log(if i < WAVE_SIZE / 2 { 1.0 } else { -1.0 });
    }
    t
}

fn gen_sawtooth() -> WaveTable {
    let mut t = WaveTable::new();
    for i in 0..WAVE_SIZE {
        let phase = i as f32 / WAVE_SIZE as f32;
        t.data[i] = linear_to_log(2.0 * phase - 1.0);
    }
    t
}

fn gen_triangle() -> WaveTable {
    let mut t = WaveTable::new();
    for i in 0..WAVE_SIZE {
        let p = i as f32 / WAVE_SIZE as f32;
        let s = if p < 0.5 { 4.0 * p - 1.0 } else { 3.0 - 4.0 * p };
        t.data[i] = linear_to_log(s);
    }
    t
}

/// Convert 32 × i8 user wave input to internal 1024-entry log format.
///
/// Upsamples with linear interpolation, then converts to log domain.
pub fn convert_wave_32(input: &[i8; 32]) -> WaveTable {
    let mut t = WaveTable::new();
    for i in 0..WAVE_SIZE {
        let pos = i as f32 * 32.0 / WAVE_SIZE as f32;
        let idx = pos as usize;
        let frac = pos - idx as f32;
        let a = input[idx % 32] as f32 / 128.0;
        let b = input[(idx + 1) % 32] as f32 / 128.0;
        t.data[i] = linear_to_log((a + frac * (b - a)).clamp(-1.0, 1.0));
    }
    t
}

// ---------------------------------------------------------------------------
// ADSR
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct AdsrParams {
    /// Time from key-on to peak level (0 = slowest, 255 = fastest)
    pub attack: u8,
    /// Time from peak to sustain level
    pub decay: u8,
    /// Sustain amplitude (0–255 maps to 0.0–1.0)
    pub sustain: u8,
    /// Time from key-off to silence
    pub release: u8,
}

impl Default for AdsrParams {
    fn default() -> Self {
        Self { attack: 200, decay: 150, sustain: 180, release: 100 }
    }
}

/// Maps rate byte (0–255) to per-sample delta.
/// rate=0 → ~10 s, rate=255 → ~1 ms (exponential)
fn rate_to_delta(rate: u8, sample_rate: f32) -> f32 {
    const T_MAX: f32 = 10.0;
    const T_MIN: f32 = 0.001;
    let t = T_MIN * (T_MAX / T_MIN).powf(1.0 - rate as f32 / 255.0);
    1.0 / (t * sample_rate)
}

#[derive(Clone, Copy, PartialEq)]
enum EnvPhase {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

// ---------------------------------------------------------------------------
// Channel (oscillator + envelope)
// ---------------------------------------------------------------------------

struct Channel {
    wave_slot: u8,
    adsr: AdsrParams,
    env_phase: EnvPhase,
    env_level: f32,
    osc_phase: f32,
    frequency: f32,
}

impl Channel {
    fn new(wave_slot: u8, frequency: f32, adsr: AdsrParams) -> Self {
        Self {
            wave_slot,
            adsr,
            env_phase: EnvPhase::Attack,
            env_level: 0.0,
            osc_phase: 0.0,
            frequency,
        }
    }

    fn note_off(&mut self) {
        if self.env_phase != EnvPhase::Idle {
            self.env_phase = EnvPhase::Release;
        }
    }

    fn is_idle(&self) -> bool {
        self.env_phase == EnvPhase::Idle
    }

    fn tick(&mut self, sample_rate: f32, wave: &WaveTable) -> f32 {
        let sustain_level = self.adsr.sustain as f32 / 255.0;

        match self.env_phase {
            EnvPhase::Attack => {
                self.env_level += rate_to_delta(self.adsr.attack, sample_rate);
                if self.env_level >= 1.0 {
                    self.env_level = 1.0;
                    self.env_phase = EnvPhase::Decay;
                }
            }
            EnvPhase::Decay => {
                self.env_level -= rate_to_delta(self.adsr.decay, sample_rate);
                if self.env_level <= sustain_level {
                    self.env_level = sustain_level;
                    self.env_phase = EnvPhase::Sustain;
                }
            }
            EnvPhase::Sustain => {}
            EnvPhase::Release => {
                self.env_level -= rate_to_delta(self.adsr.release, sample_rate);
                if self.env_level <= 0.0 {
                    self.env_level = 0.0;
                    self.env_phase = EnvPhase::Idle;
                }
            }
            EnvPhase::Idle => return 0.0,
        }

        self.osc_phase = (self.osc_phase + self.frequency / sample_rate).fract();
        let idx = (self.osc_phase * WAVE_SIZE as f32) as usize;
        log_to_linear(wave.data[idx]) * self.env_level
    }
}

// ---------------------------------------------------------------------------
// WMS-1 engine
// ---------------------------------------------------------------------------

const TOTAL_SLOTS: usize = 256;

pub struct Wms1Engine {
    sample_rate: f32,
    channels: HashMap<usize, Channel>,
    next_id: usize,
    wave_tables: Vec<Option<WaveTable>>,
}

impl Wms1Engine {
    pub fn new(sample_rate: f32) -> Self {
        let mut wave_tables: Vec<Option<WaveTable>> = (0..TOTAL_SLOTS).map(|_| None).collect();
        wave_tables[0] = Some(gen_sine());
        wave_tables[1] = Some(gen_square());
        wave_tables[2] = Some(gen_sawtooth());
        wave_tables[3] = Some(gen_triangle());
        Self { sample_rate, channels: HashMap::new(), next_id: 0, wave_tables }
    }

    /// Load a user-defined wave into slot 8–255.
    pub fn set_user_wave(&mut self, slot: u8, input: &[i8; 32]) {
        assert!(slot >= 8, "slots 0–7 are reserved for builtin waves");
        self.wave_tables[slot as usize] = Some(convert_wave_32(input));
    }

    /// Key-on: start a new note. Returns a stable ID for later note_off.
    ///
    /// IDs are monotonically increasing and never reused, so retain() removing
    /// idle channels never invalidates IDs held by the caller.
    pub fn note_on(&mut self, wave_slot: u8, frequency: f32, adsr: AdsrParams) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.channels.insert(id, Channel::new(wave_slot, frequency, adsr));
        id
    }

    /// Key-off: begin release phase for the given channel ID.
    pub fn note_off(&mut self, channel: usize) {
        if let Some(ch) = self.channels.get_mut(&channel) {
            ch.note_off();
        }
    }

    /// Render and mix into `output`.
    ///
    /// `output` is interleaved with `num_channels` channels (1 = mono, 2 = stereo).
    /// Caller should zero `output` before the first engine mixed into it.
    pub fn render(&mut self, output: &mut [f32], num_channels: usize) {
        let num_channels = num_channels.max(1);
        let sample_rate = self.sample_rate;
        let wave_tables = &self.wave_tables;

        for frame in output.chunks_mut(num_channels) {
            let mut mix = 0.0f32;
            for ch in self.channels.values_mut() {
                if ch.is_idle() {
                    continue;
                }
                if let Some(wave) = &wave_tables[ch.wave_slot as usize] {
                    mix += ch.tick(sample_rate, wave);
                }
            }
            for s in frame.iter_mut() {
                *s += mix;
            }
        }

        self.channels.retain(|_, ch| !ch.is_idle());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_roundtrip_log() {
        for i in 0..100 {
            let x = i as f32 / 100.0;
            let encoded = linear_to_log(x);
            let decoded = log_to_linear(encoded);
            assert!((decoded - x).abs() < 0.01, "roundtrip failed at x={x}: got {decoded}");
        }
    }

    #[test]
    fn wave_convert_32_length() {
        let input = [0i8; 32];
        let t = convert_wave_32(&input);
        assert_eq!(t.data.len(), WAVE_SIZE);
    }

    #[test]
    fn engine_note_on_off_renders() {
        let mut engine = Wms1Engine::new(44100.0);
        let adsr = AdsrParams::default();
        let ch = engine.note_on(0, 440.0, adsr);
        let mut buf = vec![0.0f32; 512];
        engine.render(&mut buf, 1);
        assert!(buf.iter().any(|&s| s != 0.0), "expected non-silent output");
        engine.note_off(ch);
        let mut buf2 = vec![0.0f32; 44100 * 2]; // plenty of time to release
        engine.render(&mut buf2, 1);
    }
}
