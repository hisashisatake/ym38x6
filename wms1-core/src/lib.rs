use std::collections::HashMap;
use ym38x6_engine::{WaveTable, gen_sine, gen_square, gen_sawtooth, gen_triangle};

// 呼び出し側が ym38x6-engine に直接依存しなくて済むよう re-export
pub use ym38x6_engine::{AdsrParams, SoundEngine, convert_wave_32};

// ---------------------------------------------------------------------------
// ADSR ヘルパー
// ---------------------------------------------------------------------------

/// rate=0 → ~10 s, rate=255 → ~1 ms (指数マッピング)
fn rate_to_delta(rate: u8, sample_rate: f32) -> f32 {
    const T_MAX: f32 = 10.0;
    const T_MIN: f32 = 0.001;
    let t = T_MIN * (T_MAX / T_MIN).powf(1.0 - rate as f32 / 255.0);
    1.0 / (t * sample_rate)
}

#[derive(Clone, Copy, PartialEq)]
enum EnvPhase { Idle, Attack, Decay, Sustain, Release }

// ---------------------------------------------------------------------------
// チャンネル（オシレーター + エンベロープ）
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
        Self { wave_slot, adsr, env_phase: EnvPhase::Attack,
               env_level: 0.0, osc_phase: 0.0, frequency }
    }

    fn note_off(&mut self) {
        if self.env_phase != EnvPhase::Idle {
            self.env_phase = EnvPhase::Release;
        }
    }

    fn is_idle(&self) -> bool { self.env_phase == EnvPhase::Idle }

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
        let idx = (self.osc_phase * wave.len() as f32) as usize;
        wave.sample_at(idx) * self.env_level
    }
}

// ---------------------------------------------------------------------------
// WMS-1 エンジン
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

    /// スロット 8–255 にユーザー定義波形をロード
    pub fn set_user_wave(&mut self, slot: u8, input: &[i8; 32]) {
        assert!(slot >= 8, "slots 0–7 are reserved for builtin waves");
        self.wave_tables[slot as usize] = Some(convert_wave_32(input));
    }
}

impl SoundEngine for Wms1Engine {
    fn note_on(&mut self, wave_slot: u8, frequency: f32, adsr: AdsrParams) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.channels.insert(id, Channel::new(wave_slot, frequency, adsr));
        id
    }

    fn note_off(&mut self, channel: usize) {
        if let Some(ch) = self.channels.get_mut(&channel) {
            ch.note_off();
        }
    }

    fn render(&mut self, output: &mut [f32], num_channels: usize) {
        let num_channels = num_channels.max(1);
        let sample_rate = self.sample_rate;
        let wave_tables = &self.wave_tables;
        for frame in output.chunks_mut(num_channels) {
            let mut mix = 0.0f32;
            for ch in self.channels.values_mut() {
                if ch.is_idle() { continue; }
                if let Some(wave) = &wave_tables[ch.wave_slot as usize] {
                    mix += ch.tick(sample_rate, wave);
                }
            }
            for s in frame.iter_mut() { *s += mix; }
        }
        self.channels.retain(|_, ch| !ch.is_idle());
    }
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_note_on_off_renders() {
        let mut engine = Wms1Engine::new(44100.0);
        let adsr = AdsrParams::default();
        let ch = engine.note_on(0, 440.0, adsr);
        let mut buf = vec![0.0f32; 512];
        engine.render(&mut buf, 1);
        assert!(buf.iter().any(|&s| s != 0.0), "expected non-silent output");
        engine.note_off(ch);
        let mut buf2 = vec![0.0f32; 44100 * 2];
        engine.render(&mut buf2, 1);
    }
}
