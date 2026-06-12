// ---------------------------------------------------------------------------
// 音色LFO（spec.md「音色LFO」セクション参照）
//
// プリセット・NRPNで設定する「音作り」用のLFO。波形は三角波固定。
// PMS/AMSはチャンネルごとの変調感度、PMD/AMDはLFOそのものの深さで、
// 両者の積が実際の変調量になる。AMはオペレーターごとのAME（OperatorParams::am_enable）
// でON/OFFする。パフォーマンスLFO（ビブラート/トレモロ）とは完全に独立した別系統。
//
// 数式はすべて初期案（暫定）。CLAUDE.mdのテスト方針に従い、
// 実装後に音を聴いて係数を調整する。
// ---------------------------------------------------------------------------

/// 音色LFOの周波数(0〜255)→Hz。OPN系LFOの周波数レンジ（約3〜80Hz）を指数マッピング（暫定）。
pub fn tone_lfo_freq_to_hz(freq: u8) -> f32 {
    const F_MIN: f32 = 3.0;
    const F_MAX: f32 = 80.0;
    F_MIN * (F_MAX / F_MIN).powf(freq as f32 / 255.0)
}

/// PMS(0〜255)→ピッチ変調の最大幅（セント、暫定で±800セント）。
pub fn pms_to_cents_range(pms: u8) -> f32 {
    const MAX_CENTS: f32 = 800.0;
    pms as f32 / 255.0 * MAX_CENTS
}

/// AMS(0〜255)→振幅変調の最大深さ(0.0〜1.0、暫定)。
pub fn ams_to_depth(ams: u8) -> f32 {
    ams as f32 / 255.0
}

/// 音色LFO本体：三角波固定（spec.md準拠）+ Delay。
pub struct ToneLfo {
    phase: f32,
    elapsed: f32,
}

impl ToneLfo {
    pub fn new() -> Self {
        Self { phase: 0.0, elapsed: 0.0 }
    }

    /// キーオン時に呼び出し、位相とディレイ経過時間をリセットする。
    pub fn note_on(&mut self) {
        self.phase = 0.0;
        self.elapsed = 0.0;
    }

    /// 戻り値: -1.0〜1.0の三角波。Delay中は0.0。
    pub fn tick(&mut self, sample_rate: f32, freq: u8, delay: u8) -> f32 {
        self.elapsed += 1.0 / sample_rate;
        // sound_core::lfo::delay_to_secondsと同型（0〜10秒、線形）。
        let delay_seconds = delay as f32 / 255.0 * 10.0;
        if self.elapsed < delay_seconds {
            return 0.0;
        }

        let hz = tone_lfo_freq_to_hz(freq);
        self.phase = (self.phase + hz / sample_rate).fract();
        if self.phase < 0.5 {
            4.0 * self.phase - 1.0
        } else {
            3.0 - 4.0 * self.phase
        }
    }
}

impl Default for ToneLfo {
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
    fn tone_lfo_freq_to_hz_bounds() {
        assert!((tone_lfo_freq_to_hz(0) - 3.0).abs() < 1e-3);
        assert!((tone_lfo_freq_to_hz(255) - 80.0).abs() < 1e-2);
        assert!(tone_lfo_freq_to_hz(255) > tone_lfo_freq_to_hz(0));
    }

    #[test]
    fn pms_to_cents_range_bounds() {
        assert_eq!(pms_to_cents_range(0), 0.0);
        assert!((pms_to_cents_range(255) - 800.0).abs() < 1e-3);
    }

    #[test]
    fn ams_to_depth_bounds() {
        assert_eq!(ams_to_depth(0), 0.0);
        assert!((ams_to_depth(255) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn delay_holds_output_at_zero() {
        let sr = 44100.0;
        let mut lfo = ToneLfo::new();
        // delay=255 → 10秒。1秒分ティックしても出力0のはず
        for _ in 0..44100 {
            assert_eq!(lfo.tick(sr, 128, 255), 0.0);
        }
    }

    #[test]
    fn triangle_wave_is_periodic_and_bounded() {
        let sr = 44100.0;
        let mut lfo = ToneLfo::new();
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for _ in 0..(sr as usize) {
            let v = lfo.tick(sr, 255, 0); // freq=255 → 約80Hz, delay=0
            assert!((-1.0..=1.0).contains(&v), "out of range: {v}");
            min = min.min(v);
            max = max.max(v);
        }
        assert!(min < -0.9, "min={min}");
        assert!(max > 0.9, "max={max}");
    }

    #[test]
    fn note_on_resets_phase_and_elapsed() {
        let sr = 44100.0;
        let mut lfo = ToneLfo::new();
        for _ in 0..1000 {
            lfo.tick(sr, 200, 0);
        }
        lfo.note_on();
        // リセット直後はphase≈0付近 → 三角波の谷(-1.0)からスタート
        let v = lfo.tick(sr, 200, 0);
        assert!(v < -0.9, "expected near -1.0 right after note_on, got {v}");
    }
}
