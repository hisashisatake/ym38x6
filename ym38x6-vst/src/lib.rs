use nih_plug::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use ym38x6_core::{ChannelParams, OperatorParams, SoundEngine, Ym38x6Engine, Ym38x6Patch};

/// マスター単位5パラメーターのデフォルト値（wms1-vstと同じ値、`MasterEffects::new()`の内部初期値と一致）
const DEFAULT_REVERB_TIME: u8 = 128;
const DEFAULT_CHORUS_MOD_RATE: u8 = 128;
const DEFAULT_CHORUS_MOD_DEPTH: u8 = 128;
const DEFAULT_CHORUS_FEEDBACK: u8 = 0;
const DEFAULT_CHORUS_SEND_TO_REVERB: u8 = 0;

struct Ym38x6Plugin {
    params: Arc<Ym38x6Params>,
    engine: Ym38x6Engine,
    note_channels: HashMap<u8, usize>, // MIDIノート番号 → エンジンチャンネルID
    render_buffer: Vec<f32>, // プロセスコールバック用インターリーブ作業バッファ
    sample_rate: f32,
    // NRPN専用パラメーター（DAWオートメーション非公開、3.3.4でNRPN(0,9)〜(0,15)から配線）。
    // デフォルト値はChannelParams::default()/OperatorParams::default()に合わせる。
    algorithm: u8,
    filter_type: u8,
    filter_self_oscillation: bool,
    operator_waveforms: [u8; 4],
}

/// オペレーター単位パラメーター一式（11個）。`Ym38x6Params`側で`[OperatorVstParams; 4]`として
/// `#[nested(array, ...)]`展開し、各IDに`_1`〜`_4`が付与される（DAW上は「Operator 1」〜「Operator 4」）。
#[derive(Params)]
struct OperatorVstParams {
    #[id = "tl"]
    pub tl: IntParam,
    #[id = "ar"]
    pub ar: IntParam,
    #[id = "d1r"]
    pub d1r: IntParam,
    #[id = "d2r"]
    pub d2r: IntParam,
    #[id = "d1l"]
    pub d1l: IntParam,
    #[id = "rr"]
    pub rr: IntParam,
    #[id = "mul"]
    pub mul: IntParam,
    #[id = "dt1"]
    pub dt1: IntParam,
    #[id = "ksr"]
    pub ksr: IntParam,
    #[id = "ame"]
    pub ame: BoolParam,
    #[id = "vel_sens"]
    pub vel_sens: IntParam,
}

impl Default for OperatorVstParams {
    /// 「鳴る」状態を初期値とする（コアの`OperatorParams::default()`は全0で
    /// TL=0≒無音・AR=0≒極端に遅いアタックのため、VST起動直後に無音にならないよう
    /// 個別に明示値を設定する）。
    fn default() -> Self {
        Self {
            tl: IntParam::new("TL", 200, IntRange::Linear { min: 0, max: 255 }),
            ar: IntParam::new("AR", 255, IntRange::Linear { min: 0, max: 255 }),
            d1r: IntParam::new("D1R", 100, IntRange::Linear { min: 0, max: 255 }),
            d2r: IntParam::new("D2R", 80, IntRange::Linear { min: 0, max: 255 }),
            d1l: IntParam::new("D1L", 180, IntRange::Linear { min: 0, max: 255 }),
            rr: IntParam::new("RR", 150, IntRange::Linear { min: 0, max: 255 }),
            mul: IntParam::new("MUL", 16, IntRange::Linear { min: 0, max: 255 }),
            dt1: IntParam::new("DT1", 128, IntRange::Linear { min: 0, max: 255 }),
            ksr: IntParam::new("KSR", 64, IntRange::Linear { min: 0, max: 255 }),
            ame: BoolParam::new("AM Enable", false),
            vel_sens: IntParam::new("Velocity Sensitivity", 0, IntRange::Linear { min: 0, max: 255 }),
        }
    }
}

#[derive(Params)]
struct Ym38x6Params {
    // ---- チャンネル単位（19個、spec.md MIDI実装方針参照） ----
    #[id = "feedback"]
    pub feedback: IntParam,
    #[id = "lfo_rate"]
    pub lfo_rate: IntParam,
    #[id = "lfo_depth"]
    pub lfo_depth: IntParam,
    #[id = "lfo_delay"]
    pub lfo_delay: IntParam,
    #[id = "tone_freq"]
    pub tone_freq: IntParam,
    #[id = "tone_pmd"]
    pub tone_pmd: IntParam,
    #[id = "tone_amd"]
    pub tone_amd: IntParam,
    #[id = "tone_delay"]
    pub tone_delay: IntParam,
    #[id = "pms"]
    pub pms: IntParam,
    #[id = "ams"]
    pub ams: IntParam,
    #[id = "cutoff"]
    pub cutoff: IntParam,
    #[id = "resonance"]
    pub resonance: IntParam,
    #[id = "feg_a"]
    pub feg_a: IntParam,
    #[id = "feg_d"]
    pub feg_d: IntParam,
    #[id = "feg_s"]
    pub feg_s: IntParam,
    #[id = "feg_r"]
    pub feg_r: IntParam,
    #[id = "feg_depth"]
    pub feg_depth: IntParam,
    #[id = "rev_send"]
    pub rev_send: IntParam,
    #[id = "cho_send"]
    pub cho_send: IntParam,

    // ---- オペレーター単位（11個 × 4op = 44個） ----
    #[nested(array, group = "Operator")]
    pub operators: [OperatorVstParams; 4],

    // ---- マスター単位（5個、3.0のwms1-vstと同型） ----
    #[id = "rev_time"]
    pub reverb_time: IntParam,
    #[id = "cho_rate"]
    pub chorus_mod_rate: IntParam,
    #[id = "cho_depth"]
    pub chorus_mod_depth: IntParam,
    #[id = "cho_fb"]
    pub chorus_feedback: IntParam,
    #[id = "cho_to_rev"]
    pub chorus_send_to_reverb: IntParam,
}

impl Default for Ym38x6Params {
    fn default() -> Self {
        Self {
            feedback: IntParam::new("Feedback", 0, IntRange::Linear { min: 0, max: 255 }),
            lfo_rate: IntParam::new("Perf LFO Rate", 0, IntRange::Linear { min: 0, max: 255 }),
            lfo_depth: IntParam::new("Perf LFO Depth", 0, IntRange::Linear { min: 0, max: 255 }),
            lfo_delay: IntParam::new("Perf LFO Delay", 0, IntRange::Linear { min: 0, max: 255 }),
            tone_freq: IntParam::new("Tone LFO Freq", 0, IntRange::Linear { min: 0, max: 255 }),
            tone_pmd: IntParam::new("Tone LFO PMD", 0, IntRange::Linear { min: 0, max: 255 }),
            tone_amd: IntParam::new("Tone LFO AMD", 0, IntRange::Linear { min: 0, max: 255 }),
            tone_delay: IntParam::new("Tone LFO Delay", 0, IntRange::Linear { min: 0, max: 255 }),
            pms: IntParam::new("PMS", 0, IntRange::Linear { min: 0, max: 255 }),
            ams: IntParam::new("AMS", 0, IntRange::Linear { min: 0, max: 255 }),
            cutoff: IntParam::new("Filter Cutoff", 255, IntRange::Linear { min: 0, max: 255 }),
            resonance: IntParam::new("Filter Resonance", 0, IntRange::Linear { min: 0, max: 255 }),
            feg_a: IntParam::new("Filter EG Attack", 0, IntRange::Linear { min: 0, max: 255 }),
            feg_d: IntParam::new("Filter EG Decay", 0, IntRange::Linear { min: 0, max: 255 }),
            feg_s: IntParam::new("Filter EG Sustain", 0, IntRange::Linear { min: 0, max: 255 }),
            feg_r: IntParam::new("Filter EG Release", 0, IntRange::Linear { min: 0, max: 255 }),
            feg_depth: IntParam::new("Filter EG Depth", 0, IntRange::Linear { min: 0, max: 255 }),
            rev_send: IntParam::new("Reverb Send", 0, IntRange::Linear { min: 0, max: 255 }),
            cho_send: IntParam::new("Chorus Send", 0, IntRange::Linear { min: 0, max: 255 }),
            operators: Default::default(),
            reverb_time: IntParam::new("Reverb Time", DEFAULT_REVERB_TIME as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_mod_rate: IntParam::new("Chorus Mod Rate", DEFAULT_CHORUS_MOD_RATE as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_mod_depth: IntParam::new("Chorus Mod Depth", DEFAULT_CHORUS_MOD_DEPTH as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_feedback: IntParam::new("Chorus Feedback", DEFAULT_CHORUS_FEEDBACK as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_send_to_reverb: IntParam::new("Chorus Send To Reverb", DEFAULT_CHORUS_SEND_TO_REVERB as i32, IntRange::Linear { min: 0, max: 255 }),
        }
    }
}

impl Default for Ym38x6Plugin {
    fn default() -> Self {
        const DEFAULT_SR: f32 = 44100.0;
        Self {
            params: Arc::new(Ym38x6Params::default()),
            engine: Ym38x6Engine::new(DEFAULT_SR),
            note_channels: HashMap::new(),
            render_buffer: Vec::new(),
            sample_rate: DEFAULT_SR,
            algorithm: 0,
            filter_type: 0,
            filter_self_oscillation: true,
            operator_waveforms: [0; 4],
        }
    }
}

impl Ym38x6Plugin {
    /// 現在のDAWパラメーターとNRPN専用状態から`Ym38x6Patch`を構築する。
    fn build_patch(&self) -> Ym38x6Patch {
        let p = &self.params;
        let operators = std::array::from_fn(|i| {
            let op = &p.operators[i];
            OperatorParams {
                tl: op.tl.value() as u8,
                ar: op.ar.value() as u8,
                d1r: op.d1r.value() as u8,
                d2r: op.d2r.value() as u8,
                d1l: op.d1l.value() as u8,
                rr: op.rr.value() as u8,
                mul: op.mul.value() as u8,
                dt1: op.dt1.value() as u8,
                ksr: op.ksr.value() as u8,
                am_enable: op.ame.value(),
                velocity_sensitivity: op.vel_sens.value() as u8,
                waveform: self.operator_waveforms[i],
            }
        });

        let channel = ChannelParams {
            algorithm: self.algorithm,
            feedback: p.feedback.value() as u8,
            tone_lfo_freq: p.tone_freq.value() as u8,
            tone_lfo_pmd: p.tone_pmd.value() as u8,
            tone_lfo_amd: p.tone_amd.value() as u8,
            tone_lfo_delay: p.tone_delay.value() as u8,
            pms: p.pms.value() as u8,
            ams: p.ams.value() as u8,
            filter_cutoff: p.cutoff.value() as u8,
            filter_resonance: p.resonance.value() as u8,
            filter_type: self.filter_type,
            filter_self_oscillation: self.filter_self_oscillation,
            filter_eg_attack: p.feg_a.value() as u8,
            filter_eg_decay: p.feg_d.value() as u8,
            filter_eg_sustain: p.feg_s.value() as u8,
            filter_eg_release: p.feg_r.value() as u8,
            filter_eg_depth: p.feg_depth.value() as u8,
        };

        Ym38x6Patch { operators, channel }
    }
}

impl Plugin for Ym38x6Plugin {
    const NAME: &'static str = "38x6";
    const VENDOR: &'static str = "ym38x6";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.engine = Ym38x6Engine::new(self.sample_rate);
        let num_out = audio_io_layout
            .main_output_channels
            .map(|n| n.get() as usize)
            .unwrap_or(2);
        // プロセスコールバック内でアロケーションしないよう最大サイズで確保
        self.render_buffer
            .resize(buffer_config.max_buffer_size as usize * num_out, 0.0);
        true
    }

    fn reset(&mut self) {
        self.note_channels.clear();
        self.engine = Ym38x6Engine::new(self.sample_rate);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let patch = self.build_patch();
        self.engine.set_patch(patch);

        // 発音中チャンネルへDAWオートメーションの変更を反映する
        for &ch_id in self.note_channels.values() {
            self.engine.set_channel_params(ch_id, patch.channel);
            for (op_index, op) in patch.operators.iter().enumerate() {
                self.engine.set_operator_params(ch_id, op_index, *op);
            }
        }

        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } if velocity > 0.0 => {
                    // 同じキーが押しっぱなしの場合は旧チャンネルをリリース
                    if let Some(&old_id) = self.note_channels.get(&note) {
                        self.engine.note_off(old_id);
                    }
                    let freq = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
                    let velocity_u8 = (velocity * 127.0).round() as u8;
                    let ch_id = self.engine.note_on_with_velocity(freq, velocity_u8, patch);
                    self.note_channels.insert(note, ch_id);
                }
                NoteEvent::NoteOn { note, .. } | NoteEvent::NoteOff { note, .. } => {
                    // velocity=0 の NoteOn も NoteOff として扱う（MIDI仕様）
                    if let Some(&ch_id) = self.note_channels.get(&note) {
                        self.engine.note_off(ch_id);
                        self.note_channels.remove(&note);
                    }
                }
                _ => {}
            }
        }

        let num_channels = buffer.channels();
        let num_samples = buffer.samples();
        let interleaved_len = num_samples * num_channels;

        // 作業バッファが足りない場合（ホスト規約違反）は拡張
        if interleaved_len > self.render_buffer.len() {
            self.render_buffer.resize(interleaved_len, 0.0);
        }
        let buf = &mut self.render_buffer[..interleaved_len];
        buf.fill(0.0);
        self.engine.render(buf, num_channels);

        // インターリーブ → nih-plugのチャンネル分離レイアウトに変換
        let output_slices = buffer.as_slice();
        for ch in 0..num_channels {
            for s in 0..num_samples {
                output_slices[ch][s] += buf[s * num_channels + ch];
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Ym38x6Plugin {
    const CLAP_ID: &'static str = "com.ym38x6.ym38x6";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("38x6 FM Synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
    ];
}

impl Vst3Plugin for Ym38x6Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"Ym38x6--FM4-----";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
    ];
}

nih_export_clap!(Ym38x6Plugin);
nih_export_vst3!(Ym38x6Plugin);
