pub mod algorithm;
pub mod mapping;
pub mod operator;
pub mod tone_lfo;
pub mod waveform;

use std::collections::HashMap;

use algorithm::ALGORITHMS;
use mapping::{feedback_to_scale, frequency_to_note, FM_MODULATION_INDEX_SCALE};
use operator::{Operator, OperatorParams};
use sound_core::{
    apply_lfo_modulation, convert_wave_32, PerformanceLfo, PerformanceLfoTarget, WaveTable,
};
use tone_lfo::{ams_to_depth, pms_to_cents_range, ToneLfo};
use waveform::gen_builtin_waveform;

// 呼び出し側がsound-coreに直接依存しなくて済むようre-export
pub use sound_core::{
    pitch_depth_cents, volume_depth, AdsrParams, LfoDestination, LfoWaveform, SoundEngine,
};

// ---------------------------------------------------------------------------
// パッチ（チャンネル + オペレーター4個分のパラメーター一式）
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default)]
pub struct ChannelParams {
    /// アルゴリズム番号(0〜7)。
    pub algorithm: u8,
    /// フィードバック深さ(0〜255)。
    pub feedback: u8,
    /// 音色LFO周波数（3.1.5で使用）。
    pub tone_lfo_freq: u8,
    /// 音色LFO ピッチ変調深さ（3.1.5で使用）。
    pub tone_lfo_pmd: u8,
    /// 音色LFO 振幅変調深さ（3.1.5で使用）。
    pub tone_lfo_amd: u8,
    /// 音色LFO Delay（3.1.5で使用）。
    pub tone_lfo_delay: u8,
    /// PM感度（音色LFOのピッチ変調感度、3.1.5で使用）。
    pub pms: u8,
    /// AM感度（音色LFOの振幅変調感度、3.1.5で使用）。
    pub ams: u8,
}

/// 4op分のオペレーターパラメーター + チャンネルパラメーターの一式。
#[derive(Clone, Copy, Debug, Default)]
pub struct Ym38x6Patch {
    pub operators: [OperatorParams; 4],
    pub channel: ChannelParams,
}

// ---------------------------------------------------------------------------
// パフォーマンスLFOの適用先（38x6拡張Destination）
// ---------------------------------------------------------------------------

/// パフォーマンスLFOの適用先。共通Destination（Pitch/Volume）に加え、
/// 38x6固有の拡張Destination（TLキャリア一括）を持つ。
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Ym38x6LfoDestination {
    #[default]
    Pitch,
    Volume,
    TlCarrier,
}

// ---------------------------------------------------------------------------
// チャンネル（4オペレーター + アルゴリズム結線）
// ---------------------------------------------------------------------------

struct Channel {
    operators: [Operator; 4],
    channel_params: ChannelParams,
    /// フィードバックオペレーターの直前の出力（自己変調に使う）。
    feedback_buffer: f32,
    /// KSR計算用のノート番号（Note-On時の周波数から近似）。
    note: u8,
    perf_lfo: PerformanceLfo,
    lfo_destination: Ym38x6LfoDestination,
    lfo_depth: f32,
    pitch_mod_cents: f32,
    volume_mod_delta: f32,
    /// 拡張Destination=TlCarrier用：キャリア出力にかかる乗算ゲインのオフセット。
    tl_carrier_mod_delta: f32,
    /// 音色LFO本体（PMS/AMS×PMD/AMD、spec.md「音色LFO」セクション参照）。
    tone_lfo: ToneLfo,
}

impl Channel {
    fn new(frequency: f32, velocity: u8, patch: Ym38x6Patch) -> Self {
        let note = frequency_to_note(frequency);
        let operators = patch.operators.map(|params| {
            let mut op = Operator::new(params);
            op.note_on(frequency, velocity);
            op
        });
        Self {
            operators,
            channel_params: patch.channel,
            feedback_buffer: 0.0,
            note,
            perf_lfo: PerformanceLfo::new(),
            lfo_destination: Ym38x6LfoDestination::default(),
            lfo_depth: 0.0,
            pitch_mod_cents: 0.0,
            volume_mod_delta: 0.0,
            tl_carrier_mod_delta: 0.0,
            tone_lfo: ToneLfo::new(),
        }
    }

    fn set_performance_lfo(
        &mut self,
        rate: u8,
        delay: u8,
        waveform: LfoWaveform,
        destination: Ym38x6LfoDestination,
        depth: f32,
    ) {
        self.perf_lfo.set_rate(rate);
        self.perf_lfo.set_delay(delay);
        self.perf_lfo.set_waveform(waveform);
        self.lfo_destination = destination;
        self.lfo_depth = depth;
    }

    fn note_off(&mut self) {
        for op in self.operators.iter_mut() {
            op.note_off();
        }
    }

    fn is_idle(&self) -> bool {
        self.operators.iter().all(|op| op.is_idle())
    }

    fn tick(&mut self, sample_rate: f32, wave_tables: &[Option<WaveTable>]) -> f32 {
        if self.is_idle() {
            return 0.0;
        }

        // パフォーマンスLFO（ビブラート/トレモロ/TLキャリア一括）
        let lfo_value = self.perf_lfo.tick(sample_rate);
        match self.lfo_destination {
            Ym38x6LfoDestination::Pitch => {
                apply_lfo_modulation(lfo_value, LfoDestination::Pitch, self.lfo_depth, self);
            }
            Ym38x6LfoDestination::Volume => {
                apply_lfo_modulation(lfo_value, LfoDestination::Volume, self.lfo_depth, self);
            }
            Ym38x6LfoDestination::TlCarrier => {
                self.tl_carrier_mod_delta = lfo_value * self.lfo_depth;
            }
        }
        for op in self.operators.iter_mut() {
            op.set_pitch_modulation(self.pitch_mod_cents);
        }

        // 音色LFO（プリセット・NRPNで設定する音作り用、PMS/AMS×PMD/AMD）
        let tone_lfo_value = self.tone_lfo.tick(
            sample_rate,
            self.channel_params.tone_lfo_freq,
            self.channel_params.tone_lfo_delay,
        );
        let tone_pitch_mod_cents = tone_lfo_value
            * pms_to_cents_range(self.channel_params.pms)
            * (self.channel_params.tone_lfo_pmd as f32 / 255.0);
        let tone_amp_mod = tone_lfo_value
            * ams_to_depth(self.channel_params.ams)
            * (self.channel_params.tone_lfo_amd as f32 / 255.0);
        for op in self.operators.iter_mut() {
            let am = if op.params.am_enable { tone_amp_mod } else { 0.0 };
            op.set_tone_lfo_modulation(tone_pitch_mod_cents, am);
        }

        // アルゴリズム結線に基づく4op合成
        let algo = &ALGORITHMS[(self.channel_params.algorithm as usize).min(7)];
        let mut op_outputs = [0.0f32; 4];
        for &op_idx in algo.eval_order.iter() {
            let mut modulation = 0.0;
            for &(from, to) in algo.routes {
                if to == op_idx {
                    modulation += op_outputs[from] * FM_MODULATION_INDEX_SCALE;
                }
            }
            if op_idx == algo.feedback_op {
                modulation +=
                    self.feedback_buffer * feedback_to_scale(self.channel_params.feedback);
            }
            let wave = wave_table_for(wave_tables, self.operators[op_idx].params.waveform);
            let out = self.operators[op_idx].tick(sample_rate, wave, modulation, self.note);
            op_outputs[op_idx] = out;
            if op_idx == algo.feedback_op {
                self.feedback_buffer = out;
            }
        }

        let carrier_sum: f32 = algo.carriers.iter().map(|&i| op_outputs[i]).sum();
        let tl_carrier_gain = (1.0 + self.tl_carrier_mod_delta).max(0.0);
        let volume_gain = (1.0 + self.volume_mod_delta).max(0.0);
        carrier_sum * tl_carrier_gain * volume_gain
    }
}

impl PerformanceLfoTarget for Channel {
    fn apply_pitch_modulation(&mut self, cents: f32) {
        self.pitch_mod_cents = cents;
    }

    fn apply_volume_modulation(&mut self, delta: f32) {
        self.volume_mod_delta = delta;
    }
}

/// 指定スロットの波形テーブルを返す。未割り当てスロット（ユーザー波形未設定）の場合は
/// 常に存在するスロット0（サイン波）にフォールバックする。
fn wave_table_for(wave_tables: &[Option<WaveTable>], slot: u8) -> &WaveTable {
    wave_tables[slot as usize]
        .as_ref()
        .unwrap_or_else(|| wave_tables[0].as_ref().unwrap())
}

// ---------------------------------------------------------------------------
// 38x6 エンジン
// ---------------------------------------------------------------------------

const TOTAL_SLOTS: usize = 256;

pub struct Ym38x6Engine {
    sample_rate: f32,
    channels: HashMap<usize, Channel>,
    next_id: usize,
    wave_tables: Vec<Option<WaveTable>>,
    current_patch: Ym38x6Patch,
}

impl Ym38x6Engine {
    pub fn new(sample_rate: f32) -> Self {
        let mut wave_tables: Vec<Option<WaveTable>> = (0..TOTAL_SLOTS).map(|_| None).collect();
        for i in 0..8u8 {
            wave_tables[i as usize] = Some(gen_builtin_waveform(i));
        }
        Self {
            sample_rate,
            channels: HashMap::new(),
            next_id: 0,
            wave_tables,
            current_patch: Ym38x6Patch::default(),
        }
    }

    /// 以降の`SoundEngine::note_on`はこのパッチで発音する。
    pub fn set_patch(&mut self, patch: Ym38x6Patch) {
        self.current_patch = patch;
    }

    /// ベロシティ・パッチを明示指定してNote-Onする（VST/Tauriから使用）。
    pub fn note_on_with_velocity(&mut self, frequency: f32, velocity: u8, patch: Ym38x6Patch) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.channels.insert(id, Channel::new(frequency, velocity, patch));
        id
    }

    /// 発音中チャンネルのチャンネルパラメーターを更新する（DAWオートメーション/NRPN用）。
    pub fn set_channel_params(&mut self, channel: usize, params: ChannelParams) {
        if let Some(ch) = self.channels.get_mut(&channel) {
            ch.channel_params = params;
        }
    }

    /// 発音中チャンネルの指定オペレーターのパラメーターを更新する。
    pub fn set_operator_params(&mut self, channel: usize, op_index: usize, params: OperatorParams) {
        if let Some(ch) = self.channels.get_mut(&channel) {
            ch.operators[op_index].params = params;
        }
    }

    /// スロット8〜255にユーザー定義波形をロードする（wms1-coreと同一シグネチャ）。
    pub fn set_user_wave(&mut self, slot: u8, input: &[i8; 32]) {
        assert!(slot >= 8, "slots 0-7 are reserved for builtin waves");
        self.wave_tables[slot as usize] = Some(convert_wave_32(input));
    }

    /// 指定チャンネルのパフォーマンスLFO（Rate/Delay/Waveform/Destination/Depth）を設定する。
    pub fn set_performance_lfo(
        &mut self,
        channel: usize,
        rate: u8,
        delay: u8,
        waveform: LfoWaveform,
        destination: Ym38x6LfoDestination,
        depth: f32,
    ) {
        if let Some(ch) = self.channels.get_mut(&channel) {
            ch.set_performance_lfo(rate, delay, waveform, destination, depth);
        }
    }
}

impl SoundEngine for Ym38x6Engine {
    /// wave_slot/adsrはトレイト互換のため残すが未使用。velocity=127固定でカレントパッチを使う。
    fn note_on(&mut self, _wave_slot: u8, frequency: f32, _adsr: AdsrParams) -> usize {
        let patch = self.current_patch;
        self.note_on_with_velocity(frequency, 127, patch)
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
                if ch.is_idle() {
                    continue;
                }
                mix += ch.tick(sample_rate, wave_tables);
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

    /// 全Opがアルゴリズム7（全並列）で即音量最大・サスティン無限のテスト用パッチ。
    fn loud_patch(velocity_sensitivity: u8) -> Ym38x6Patch {
        let op_params = OperatorParams {
            tl: 255,
            ar: 255,
            d1r: 0,
            d2r: 0,
            d1l: 255,
            rr: 255,
            mul: 16,
            dt1: 128,
            ksr: 0,
            am_enable: false,
            velocity_sensitivity,
            waveform: 0,
        };
        let mut patch = Ym38x6Patch::default();
        patch.operators = [op_params; 4];
        patch.channel.algorithm = 7;
        patch
    }

    #[test]
    fn note_on_produces_non_silent_output() {
        let mut engine = Ym38x6Engine::new(44100.0);
        engine.set_patch(loud_patch(0));
        engine.note_on(0, 440.0, AdsrParams::default());

        let mut buf = vec![0.0f32; 512];
        engine.render(&mut buf, 1);
        assert!(buf.iter().any(|&s| s != 0.0), "expected non-silent output");
    }

    #[test]
    fn velocity_sensitivity_changes_output_amplitude() {
        let mut patch = loud_patch(255);
        for op in patch.operators.iter_mut() {
            op.tl = 100;
        }

        let mut engine_lo = Ym38x6Engine::new(44100.0);
        let mut engine_hi = Ym38x6Engine::new(44100.0);
        engine_lo.note_on_with_velocity(440.0, 0, patch);
        engine_hi.note_on_with_velocity(440.0, 127, patch);

        let mut buf_lo = vec![0.0f32; 100];
        let mut buf_hi = vec![0.0f32; 100];
        engine_lo.render(&mut buf_lo, 1);
        engine_hi.render(&mut buf_hi, 1);

        let peak_lo = buf_lo.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let peak_hi = buf_hi.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(peak_hi > peak_lo, "higher velocity should be louder: {peak_hi} vs {peak_lo}");
    }

    #[test]
    fn all_algorithms_long_run_no_nan() {
        let op_params = OperatorParams {
            tl: 200,
            ar: 255,
            d1r: 100,
            d2r: 80,
            d1l: 180,
            rr: 150,
            mul: 16,
            dt1: 128,
            ksr: 64,
            am_enable: false,
            velocity_sensitivity: 0,
            waveform: 0,
        };

        for algorithm in 0u8..8 {
            let mut patch = Ym38x6Patch::default();
            patch.operators = [op_params; 4];
            patch.channel.algorithm = algorithm;
            patch.channel.feedback = 128;

            let mut engine = Ym38x6Engine::new(44100.0);
            let ch = engine.note_on_with_velocity(440.0, 100, patch);

            let mut buf = vec![0.0f32; 44100];
            engine.render(&mut buf, 1);
            engine.note_off(ch);

            let mut buf2 = vec![0.0f32; 44100 * 2];
            engine.render(&mut buf2, 1);

            for &s in buf.iter().chain(buf2.iter()) {
                assert!(s.is_finite(), "algorithm {algorithm}: non-finite sample {s}");
            }
        }
    }

    #[test]
    fn tone_lfo_modulates_output_amplitude_periodically() {
        let op_params = OperatorParams {
            tl: 255,
            ar: 255,
            d1r: 0,
            d2r: 0,
            d1l: 255,
            rr: 255,
            mul: 16,
            dt1: 128,
            ksr: 0,
            am_enable: true,
            velocity_sensitivity: 0,
            waveform: 0,
        };
        let mut patch = Ym38x6Patch::default();
        patch.operators = [op_params; 4];
        patch.channel.algorithm = 7; // 全並列
        patch.channel.tone_lfo_freq = 200; // 速めのLFO（テストを短時間で完結させる）
        patch.channel.tone_lfo_pmd = 255;
        patch.channel.tone_lfo_amd = 255;
        patch.channel.pms = 255;
        patch.channel.ams = 255;

        let mut engine = Ym38x6Engine::new(44100.0);
        engine.note_on_with_velocity(440.0, 127, patch);

        let mut buf = vec![0.0f32; 4410]; // 0.1秒（音色LFO数周期分）
        engine.render(&mut buf, 1);

        // ウィンドウごとの最大振幅を比較し、音色LFOのAMにより振幅が周期的に変化することを確認
        let window = 200;
        let peaks: Vec<f32> = buf
            .chunks(window)
            .map(|chunk| chunk.iter().fold(0.0f32, |a, &b| a.max(b.abs())))
            .collect();

        let max_peak = peaks.iter().cloned().fold(0.0f32, f32::max);
        let min_peak = peaks.iter().cloned().fold(f32::MAX, f32::min);

        assert!(max_peak > 0.5, "expected a loud window: max_peak={max_peak}");
        assert!(min_peak < max_peak * 0.6, "expected amplitude to vary with LFO: min={min_peak} max={max_peak}");
    }
}
