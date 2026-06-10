use std::f32::consts::TAU;

// ---------------------------------------------------------------------------
// パラメーターマッピング
// ---------------------------------------------------------------------------

/// rate=0 → 0.01Hz, rate=255 → 20Hz（指数マッピング）
fn rate_to_hz(rate: u8) -> f32 {
    const F_MIN: f32 = 0.01;
    const F_MAX: f32 = 20.0;
    F_MIN * (F_MAX / F_MIN).powf(rate as f32 / 255.0)
}

/// delay=0 → 0秒, delay=255 → 10秒（線形マッピング）
fn delay_to_seconds(delay: u8) -> f32 {
    const D_MAX: f32 = 10.0;
    delay as f32 / 255.0 * D_MAX
}

/// 三角波: phase=0→-1, 0.25→0, 0.5→1, 0.75→0
fn triangle(phase: f32) -> f32 {
    2.0 * (2.0 * (phase - (phase + 0.5).floor())).abs() - 1.0
}

/// xorshift32による簡易疑似乱数（S&H波形用、外部crateに依存しない）
fn next_random(state: &mut u32) -> f32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

// ---------------------------------------------------------------------------
// パフォーマンスLFO（ビブラート/トレモロ）
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LfoWaveform {
    #[default]
    Triangle,
    Sine,
    Square,
    SampleHold,
}

/// Rate/Delay/Waveformを持つ、エンジン非依存のパフォーマンスLFO。
///
/// Depth（変調の深さ）の適用方法はDestination（Pitch/Volumeなど）ごとに
/// モデルが異なるため、ここでは持たない。`tick`は-1.0〜1.0の変調値のみを
/// 返し、Depthの適用は呼び出し側（PerformanceLfoTarget実装側）が行う。
pub struct PerformanceLfo {
    rate: u8,
    delay: u8,
    waveform: LfoWaveform,
    phase: f32,
    elapsed: f32,
    rng_state: u32,
    sh_value: f32,
}

impl PerformanceLfo {
    pub fn new() -> Self {
        Self {
            rate: 0,
            delay: 0,
            waveform: LfoWaveform::default(),
            phase: 0.0,
            elapsed: 0.0,
            rng_state: 0x1234_5678,
            sh_value: 0.0,
        }
    }

    pub fn set_rate(&mut self, rate: u8) {
        self.rate = rate;
    }

    pub fn set_delay(&mut self, delay: u8) {
        self.delay = delay;
    }

    pub fn set_waveform(&mut self, waveform: LfoWaveform) {
        self.waveform = waveform;
    }

    /// キーオン時に呼び出し、位相とディレイ経過時間をリセットする
    pub fn note_on(&mut self) {
        self.phase = 0.0;
        self.elapsed = 0.0;
    }

    /// 1サンプル進めて変調値（-1.0〜1.0）を返す。ディレイ経過前は常に0.0。
    pub fn tick(&mut self, sample_rate: f32) -> f32 {
        let freq = rate_to_hz(self.rate);
        let prev_phase = self.phase;
        self.phase = (self.phase + freq / sample_rate).fract();
        self.elapsed += 1.0 / sample_rate;

        if self.waveform == LfoWaveform::SampleHold && self.phase < prev_phase {
            self.sh_value = next_random(&mut self.rng_state);
        }

        if self.elapsed < delay_to_seconds(self.delay) {
            return 0.0;
        }

        match self.waveform {
            LfoWaveform::Triangle => triangle(self.phase),
            LfoWaveform::Sine => (self.phase * TAU).sin(),
            LfoWaveform::Square => if self.phase < 0.5 { 1.0 } else { -1.0 },
            LfoWaveform::SampleHold => self.sh_value,
        }
    }
}

impl Default for PerformanceLfo {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_to_hz_bounds() {
        assert!((rate_to_hz(0) - 0.01).abs() < 1e-6, "{}", rate_to_hz(0));
        assert!((rate_to_hz(255) - 20.0).abs() < 1e-4, "{}", rate_to_hz(255));
        assert!(rate_to_hz(255) > rate_to_hz(0));
    }

    #[test]
    fn delay_to_seconds_bounds() {
        assert_eq!(delay_to_seconds(0), 0.0);
        assert!((delay_to_seconds(255) - 10.0).abs() < 1e-6);
    }
}
