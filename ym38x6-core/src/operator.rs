// ---------------------------------------------------------------------------
// オペレーター（オシレーター + EGエンベロープ）
// ---------------------------------------------------------------------------

use crate::mapping::*;
use sound_core::WaveTable;

/// OPN系5段階エンベロープ（Attack→Decay1→Decay2→Release、+Idle）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EnvPhase {
    Attack,
    Decay1,
    Decay2,
    Release,
    Idle,
}

/// オペレーター単位パラメーター一式（全8bit、11個）。NRPN/DAWパラメーターから直接コピー可能。
#[derive(Clone, Copy, Debug, Default)]
pub struct OperatorParams {
    pub tl: u8,
    pub ar: u8,
    pub d1r: u8,
    pub d2r: u8,
    pub d1l: u8,
    pub rr: u8,
    pub mul: u8,
    pub dt1: u8,
    pub ksr: u8,
    pub am_enable: bool,
    pub velocity_sensitivity: u8,
    /// 0〜255（0〜7=ビルトイン波形、8〜255=ユーザー波形スロット）
    pub waveform: u8,
}

pub struct Operator {
    pub params: OperatorParams,
    /// フェーズ3は全Op同一値（Note-On時に一括設定）。フェーズ4のOP単位上書きの土台。
    frequency: f32,
    phase: f32,
    env_phase: EnvPhase,
    env_level: f32,
    velocity: u8,
    /// 音色LFOによるピッチ変調（セント、全Op共通、3.1.5でChannelが設定）
    tone_lfo_pitch_mod_cents: f32,
    /// 音色LFOによる振幅変調（0.0〜1.0、am_enable時のみ非ゼロ、3.1.5でChannelが設定）
    tone_lfo_amp_mod: f32,
    /// パフォーマンスLFO（ビブラート）によるピッチ変調（セント、全Op共通、Channelが毎サンプル設定）
    perf_lfo_pitch_mod_cents: f32,
    /// OP単位F-Number上書き(NRPN Operator F-Number)による周波数比。1.0=上書きなし（Note-Onでリセット）。
    f_number_ratio: f32,
}

impl Operator {
    pub fn new(params: OperatorParams) -> Self {
        Self {
            params,
            frequency: 440.0,
            phase: 0.0,
            env_phase: EnvPhase::Idle,
            env_level: 0.0,
            velocity: 127,
            tone_lfo_pitch_mod_cents: 0.0,
            tone_lfo_amp_mod: 0.0,
            perf_lfo_pitch_mod_cents: 0.0,
            f_number_ratio: 1.0,
        }
    }

    pub fn note_on(&mut self, base_frequency: f32, velocity: u8) {
        self.frequency = base_frequency;
        self.velocity = velocity;
        self.phase = 0.0;
        self.env_phase = EnvPhase::Attack;
        self.env_level = 0.0;
        self.f_number_ratio = 1.0;
    }

    pub fn note_off(&mut self) {
        if self.env_phase != EnvPhase::Idle {
            self.env_phase = EnvPhase::Release;
        }
    }

    pub fn is_idle(&self) -> bool {
        self.env_phase == EnvPhase::Idle
    }

    /// 音色LFOによる変調値を設定する（毎サンプル、Channelから呼ばれる）。
    pub fn set_tone_lfo_modulation(&mut self, pitch_cents: f32, amp_mod: f32) {
        self.tone_lfo_pitch_mod_cents = pitch_cents;
        self.tone_lfo_amp_mod = amp_mod;
    }

    /// パフォーマンスLFO（ビブラート）によるピッチ変調を設定する（毎サンプル、Channelから呼ばれる）。
    pub fn set_pitch_modulation(&mut self, cents: f32) {
        self.perf_lfo_pitch_mod_cents = cents;
    }

    /// OP単位F-Number上書き（NRPN Operator F-Number、0〜8191）を設定する。
    pub fn set_f_number_override(&mut self, f_number: u16) {
        self.f_number_ratio = f_number_to_ratio(f_number);
    }

    fn effective_frequency(&self) -> f32 {
        let cents = dt1_to_cents(self.params.dt1) + self.tone_lfo_pitch_mod_cents + self.perf_lfo_pitch_mod_cents;
        self.frequency * self.f_number_ratio * mul_to_ratio(self.params.mul) * 2f32.powf(cents / 1200.0)
    }

    fn tick_envelope(&mut self, sample_rate: f32, note: u8) {
        let ksr_mul = ksr_rate_multiplier(self.params.ksr, note);
        let sustain_level = sl_to_level(self.params.d1l);
        match self.env_phase {
            EnvPhase::Attack => {
                self.env_level += ar_to_delta(self.params.ar, sample_rate) * ksr_mul;
                if self.env_level >= 1.0 {
                    self.env_level = 1.0;
                    self.env_phase = EnvPhase::Decay1;
                }
            }
            EnvPhase::Decay1 => {
                self.env_level -= decay_to_delta(self.params.d1r, sample_rate) * ksr_mul;
                if self.env_level <= sustain_level {
                    self.env_level = sustain_level;
                    self.env_phase = EnvPhase::Decay2;
                }
            }
            EnvPhase::Decay2 => {
                self.env_level -= decay_to_delta(self.params.d2r, sample_rate) * ksr_mul;
                if self.env_level <= 0.0 {
                    self.env_level = 0.0; // Idleにはせずキーオン継続中は0に張り付く
                }
            }
            EnvPhase::Release => {
                self.env_level -= rr_to_delta(self.params.rr, sample_rate) * ksr_mul;
                if self.env_level <= 0.0 {
                    self.env_level = 0.0;
                    self.env_phase = EnvPhase::Idle;
                }
            }
            EnvPhase::Idle => {}
        }
    }

    /// `modulation`: FM変調入力（位相オフセット、0.0〜1.0スケール）
    pub fn tick(&mut self, sample_rate: f32, wave: &WaveTable, modulation: f32, note: u8) -> f32 {
        if self.env_phase == EnvPhase::Idle {
            return 0.0;
        }
        self.tick_envelope(sample_rate, note);

        let freq = self.effective_frequency();
        self.phase = (self.phase + freq / sample_rate).fract();
        let modulated_phase = (self.phase + modulation).rem_euclid(1.0);
        let idx = (modulated_phase * wave.len() as f32) as usize;

        let eff_tl = effective_tl(self.params.tl, self.velocity, self.params.velocity_sensitivity);
        let amp_factor = (1.0 - self.tone_lfo_amp_mod).clamp(0.0, 1.0);
        wave.sample_at(idx) * self.env_level * tl_to_gain(eff_tl) * amp_factor
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveform::gen_op_sine;

    fn fast_params() -> OperatorParams {
        OperatorParams {
            tl: 255,
            ar: 255,
            d1r: 255,
            d2r: 255,
            d1l: 128,
            rr: 255,
            mul: 16,
            dt1: 128,
            ksr: 0,
            am_enable: false,
            velocity_sensitivity: 0,
            waveform: 0,
        }
    }

    #[test]
    fn envelope_transitions_through_phases() {
        let sr = 44100.0;
        let wave = gen_op_sine();
        let mut op = Operator::new(fast_params());
        op.note_on(440.0, 127);
        assert_eq!(op.env_phase, EnvPhase::Attack);

        // Attack → Decay1
        for _ in 0..200 {
            if op.env_phase != EnvPhase::Attack {
                break;
            }
            op.tick(sr, &wave, 0.0, 69);
        }
        assert_eq!(op.env_phase, EnvPhase::Decay1);

        // Decay1 → Decay2
        for _ in 0..200 {
            if op.env_phase != EnvPhase::Decay1 {
                break;
            }
            op.tick(sr, &wave, 0.0, 69);
        }
        assert_eq!(op.env_phase, EnvPhase::Decay2);

        // Decay2: env_levelが0に張り付き、Idleにはならない
        for _ in 0..200 {
            op.tick(sr, &wave, 0.0, 69);
        }
        assert_eq!(op.env_phase, EnvPhase::Decay2);
        assert_eq!(op.env_level, 0.0);

        // note_off → Release → Idle
        op.note_off();
        assert_eq!(op.env_phase, EnvPhase::Release);
        for _ in 0..200 {
            if op.is_idle() {
                break;
            }
            op.tick(sr, &wave, 0.0, 69);
        }
        assert!(op.is_idle());
    }

    #[test]
    fn idle_operator_is_silent() {
        let sr = 44100.0;
        let wave = gen_op_sine();
        let mut op = Operator::new(fast_params());
        assert!(op.is_idle());
        assert_eq!(op.tick(sr, &wave, 0.0, 69), 0.0);
    }

    #[test]
    fn mul_and_dt1_change_effective_frequency() {
        let mut params = fast_params();
        params.mul = 16; // ratio = 1.0
        params.dt1 = 128; // 0 cents
        let mut op = Operator::new(params);
        op.note_on(440.0, 127);
        let base = op.effective_frequency();
        assert!((base - 440.0).abs() < 1e-3);

        // MULを変えると周波数が変わる（mul=32 → ratio=2.0）
        op.params.mul = 32;
        let doubled = op.effective_frequency();
        assert!((doubled - 880.0).abs() < 1e-3);

        // DT1を変えると周波数が変わる（dt1=128に戻し、dt1=0で-50セント）
        op.params.mul = 16;
        op.params.dt1 = 0;
        let detuned = op.effective_frequency();
        assert!(detuned < base, "detune downward should lower frequency: {detuned} vs {base}");
    }

    #[test]
    fn f_number_override_changes_effective_frequency_and_resets_on_note_on() {
        let mut op = Operator::new(fast_params());
        op.note_on(440.0, 127);
        let base = op.effective_frequency();
        assert!((base - 440.0).abs() < 1e-3);

        // F_NUMBER_CENTERの半分（2048）→比率0.5倍
        op.set_f_number_override(F_NUMBER_CENTER / 2);
        let halved = op.effective_frequency();
        assert!((halved - 220.0).abs() < 1e-3);

        // note_onで比率1.0にリセットされる
        op.note_on(440.0, 127);
        let reset = op.effective_frequency();
        assert!((reset - 440.0).abs() < 1e-3);
    }

    #[test]
    fn tone_lfo_modulation_affects_frequency_and_amplitude() {
        let sr = 44100.0;
        let wave = gen_op_sine();
        let mut op = Operator::new(fast_params());
        op.note_on(440.0, 127);

        let base = op.effective_frequency();
        op.set_tone_lfo_modulation(100.0, 0.0);
        let pitched = op.effective_frequency();
        assert!(pitched > base, "positive pitch mod should raise frequency");

        op.set_tone_lfo_modulation(0.0, 1.0);
        // 振幅変調1.0でamp_factor=0 → 出力は常に0
        for _ in 0..10 {
            assert_eq!(op.tick(sr, &wave, 0.0, 69), 0.0);
        }
    }
}
