use nih_plug::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use wms1_core::{AdsrParams, ChorusType, LfoDestination, LfoWaveform, MasterEffects, ReverbType,
    SoundEngine, Wms1Engine, pitch_depth_cents, volume_depth};

/// マスター単位5パラメーターのデフォルト値（`MasterEffects::new()`の内部初期値と一致させる）
const DEFAULT_REVERB_TIME: u8 = 128;
const DEFAULT_CHORUS_MOD_RATE: u8 = 128;
const DEFAULT_CHORUS_MOD_DEPTH: u8 = 128;
const DEFAULT_CHORUS_FEEDBACK: u8 = 0;
const DEFAULT_CHORUS_SEND_TO_REVERB: u8 = 0;

/// MIDI CC値（0.0〜1.0正規化）を本プロジェクトの内部表現（0〜255）に変換
fn cc_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// MIDI CC値（0.0〜1.0正規化）をGM2準拠の7bit値（0〜127）に変換
fn cc_to_u7(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 127.0).round() as u8
}

/// パフォーマンスLFO（CC1/76/77/78・RPN0,5・NRPN Destination/Waveform）の状態
struct PerformanceLfoState {
    cc1: u8,    // CC1 Modulation Wheel（Depth加算分）
    rate: u8,   // CC76 Vibrato Rate
    cc77: u8,   // CC77 Vibrato Depth（ベース値）
    delay: u8,  // CC78 Vibrato Delay
    rpn0_5: u8, // RPN0,5 Modulation Depth Range（GM2準拠 0〜127、デフォルト64）
    destination: LfoDestination, // NRPN(0,0) Performance LFO Destination
    waveform: LfoWaveform,       // NRPN(0,1) Performance LFO Waveform
}

impl Default for PerformanceLfoState {
    fn default() -> Self {
        Self {
            cc1: 0,
            rate: 0,
            cc77: 0,
            delay: 0,
            rpn0_5: 64,
            destination: LfoDestination::Pitch,
            waveform: LfoWaveform::Triangle,
        }
    }
}

impl PerformanceLfoState {
    /// Destinationに応じた実効Depth（spec.md パフォーマンスLFOセクション参照）
    fn depth(&self) -> f32 {
        match self.destination {
            LfoDestination::Pitch => pitch_depth_cents(self.cc77, self.cc1, self.rpn0_5),
            LfoDestination::Volume => volume_depth(self.cc77, self.cc1),
        }
    }
}

/// CC99/98(NRPN)・CC101/100(RPN) で選択中のパラメーター番号。
/// CC6(Data Entry MSB)はこの選択状態に応じて値を適用する。
#[derive(Clone, Copy, PartialEq, Default)]
enum RpnSelection {
    #[default]
    None,
    Rpn(u8, u8),
    Nrpn(u8, u8),
}

struct Wms1Plugin {
    params: Arc<Wms1Params>,
    engine: Wms1Engine,
    effects: MasterEffects,
    note_channels: HashMap<u8, usize>, // MIDIノート番号 → エンジンチャンネルID
    render_buffer: Vec<f32>,           // プロセスコールバック用インターリーブ作業バッファ
    sample_rate: f32,
    lfo_state: PerformanceLfoState,
    rpn_msb: u8,
    rpn_lsb: u8,
    nrpn_msb: u8,
    nrpn_lsb: u8,
    rpn_selection: RpnSelection,
    // マスター単位5パラメーターの「前回ブロックで適用したnih-plug値」。
    // NRPN(0,4)〜(0,8)はeffectsへ直接書き込むため、ここと一致している間は
    // process()側からの再適用をスキップしてNRPN設定値を保持する（差分検知方式）。
    last_reverb_time: u8,
    last_chorus_mod_rate: u8,
    last_chorus_mod_depth: u8,
    last_chorus_feedback: u8,
    last_chorus_send_to_reverb: u8,
}

#[derive(Params)]
struct Wms1Params {
    #[id = "wave"]
    pub wave_slot: IntParam,
    #[id = "atk"]
    pub attack: IntParam,
    #[id = "dec"]
    pub decay: IntParam,
    #[id = "sus"]
    pub sustain: IntParam,
    #[id = "rel"]
    pub release: IntParam,
    // マスター単位5パラメーター（spec.md マスターエフェクトセクション参照）。
    // NRPN(0,4)〜(0,8)とも対応するが、両者の併存は差分検知方式で行う（process()参照）。
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

impl Default for Wms1Params {
    fn default() -> Self {
        Self {
            wave_slot: IntParam::new("Wave", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v: i32| {
                    match v {
                        0 => "sine",
                        1 => "square",
                        2 => "saw",
                        _ => "tri",
                    }
                    .to_string()
                })),
            attack:  IntParam::new("Attack",  200, IntRange::Linear { min: 0, max: 255 }),
            decay:   IntParam::new("Decay",   150, IntRange::Linear { min: 0, max: 255 }),
            sustain: IntParam::new("Sustain", 180, IntRange::Linear { min: 0, max: 255 }),
            release: IntParam::new("Release", 100, IntRange::Linear { min: 0, max: 255 }),
            reverb_time: IntParam::new("Reverb Time", DEFAULT_REVERB_TIME as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_mod_rate: IntParam::new("Chorus Mod Rate", DEFAULT_CHORUS_MOD_RATE as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_mod_depth: IntParam::new("Chorus Mod Depth", DEFAULT_CHORUS_MOD_DEPTH as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_feedback: IntParam::new("Chorus Feedback", DEFAULT_CHORUS_FEEDBACK as i32, IntRange::Linear { min: 0, max: 255 }),
            chorus_send_to_reverb: IntParam::new("Chorus Send To Reverb", DEFAULT_CHORUS_SEND_TO_REVERB as i32, IntRange::Linear { min: 0, max: 255 }),
        }
    }
}

impl Default for Wms1Plugin {
    fn default() -> Self {
        const DEFAULT_SR: f32 = 44100.0;
        Self {
            params: Arc::new(Wms1Params::default()),
            engine: Wms1Engine::new(DEFAULT_SR),
            effects: MasterEffects::new(DEFAULT_SR),
            note_channels: HashMap::new(),
            render_buffer: Vec::new(),
            sample_rate: DEFAULT_SR,
            lfo_state: PerformanceLfoState::default(),
            rpn_msb: 0,
            rpn_lsb: 0,
            nrpn_msb: 0,
            nrpn_lsb: 0,
            rpn_selection: RpnSelection::default(),
            last_reverb_time: DEFAULT_REVERB_TIME,
            last_chorus_mod_rate: DEFAULT_CHORUS_MOD_RATE,
            last_chorus_mod_depth: DEFAULT_CHORUS_MOD_DEPTH,
            last_chorus_feedback: DEFAULT_CHORUS_FEEDBACK,
            last_chorus_send_to_reverb: DEFAULT_CHORUS_SEND_TO_REVERB,
        }
    }
}

impl Wms1Plugin {
    /// 指定チャンネルへ現在のパフォーマンスLFO設定を適用する
    fn apply_performance_lfo(&mut self, channel: usize) {
        let rate = self.lfo_state.rate;
        let delay = self.lfo_state.delay;
        let waveform = self.lfo_state.waveform;
        let destination = self.lfo_state.destination;
        let depth = self.lfo_state.depth();
        self.engine.set_performance_lfo(channel, rate, delay, waveform, destination, depth);
    }

    /// 発音中の全チャンネルへ現在のパフォーマンスLFO設定を再適用する
    fn apply_performance_lfo_to_active(&mut self) {
        let channels: Vec<usize> = self.note_channels.values().copied().collect();
        for ch in channels {
            self.apply_performance_lfo(ch);
        }
    }

    /// CC99/98(NRPN)・CC101/100(RPN)受信時に選択状態を更新する。
    /// MSB,LSB=127,127（Null）の場合は選択解除する
    fn update_rpn_selection(&mut self, is_nrpn: bool) {
        let (msb, lsb) = if is_nrpn { (self.nrpn_msb, self.nrpn_lsb) } else { (self.rpn_msb, self.rpn_lsb) };
        self.rpn_selection = if msb == 127 && lsb == 127 {
            RpnSelection::None
        } else if is_nrpn {
            RpnSelection::Nrpn(msb, lsb)
        } else {
            RpnSelection::Rpn(msb, lsb)
        };
    }

    /// CC6(Data Entry MSB)受信時、選択中のRPN/NRPNに応じて値を適用する。
    /// `value`はCC値の正規化値（0.0〜1.0）。enum系パラメーターは`cc_to_u7`、
    /// 0〜255連続値パラメーターは`cc_to_u8`で変換する
    fn handle_data_entry(&mut self, value: f32) {
        match self.rpn_selection {
            // RPN0,5: Modulation Depth Range
            RpnSelection::Rpn(0, 5) => {
                self.lfo_state.rpn0_5 = cc_to_u7(value);
                self.apply_performance_lfo_to_active();
            }
            // NRPN(0,0): Performance LFO Destination
            RpnSelection::Nrpn(0, 0) => {
                self.lfo_state.destination = if cc_to_u7(value) == 0 { LfoDestination::Pitch } else { LfoDestination::Volume };
                self.apply_performance_lfo_to_active();
            }
            // NRPN(0,1): Performance LFO Waveform
            RpnSelection::Nrpn(0, 1) => {
                self.lfo_state.waveform = match cc_to_u7(value) {
                    1 => LfoWaveform::Sine,
                    2 => LfoWaveform::Square,
                    3 => LfoWaveform::SampleHold,
                    _ => LfoWaveform::Triangle,
                };
                self.apply_performance_lfo_to_active();
            }
            // NRPN(0,2): Reverb Type
            RpnSelection::Nrpn(0, 2) => {
                self.effects.set_reverb_type(ReverbType::from_u8(cc_to_u7(value)));
            }
            // NRPN(0,3): Chorus Type
            RpnSelection::Nrpn(0, 3) => {
                self.effects.set_chorus_type(ChorusType::from_u8(cc_to_u7(value)));
            }
            // NRPN(0,4): Reverb Time
            RpnSelection::Nrpn(0, 4) => {
                self.effects.set_reverb_time(cc_to_u8(value));
            }
            // NRPN(0,5): Chorus Mod Rate
            RpnSelection::Nrpn(0, 5) => {
                self.effects.set_chorus_mod_rate(cc_to_u8(value));
            }
            // NRPN(0,6): Chorus Mod Depth
            RpnSelection::Nrpn(0, 6) => {
                self.effects.set_chorus_mod_depth(cc_to_u8(value));
            }
            // NRPN(0,7): Chorus Feedback
            RpnSelection::Nrpn(0, 7) => {
                self.effects.set_chorus_feedback(cc_to_u8(value));
            }
            // NRPN(0,8): Chorus Send To Reverb
            RpnSelection::Nrpn(0, 8) => {
                self.effects.set_chorus_send_to_reverb(cc_to_u8(value));
            }
            _ => {}
        }
    }
}

impl Plugin for Wms1Plugin {
    const NAME: &'static str = "WMS-1";
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
        self.engine = Wms1Engine::new(self.sample_rate);
        self.effects = MasterEffects::new(self.sample_rate);
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
        self.engine = Wms1Engine::new(self.sample_rate);
        self.effects = MasterEffects::new(self.sample_rate);
        self.lfo_state = PerformanceLfoState::default();
        self.rpn_msb = 0;
        self.rpn_lsb = 0;
        self.nrpn_msb = 0;
        self.nrpn_lsb = 0;
        self.rpn_selection = RpnSelection::default();
        self.last_reverb_time = DEFAULT_REVERB_TIME;
        self.last_chorus_mod_rate = DEFAULT_CHORUS_MOD_RATE;
        self.last_chorus_mod_depth = DEFAULT_CHORUS_MOD_DEPTH;
        self.last_chorus_feedback = DEFAULT_CHORUS_FEEDBACK;
        self.last_chorus_send_to_reverb = DEFAULT_CHORUS_SEND_TO_REVERB;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let adsr = AdsrParams {
            attack:  self.params.attack.value()  as u8,
            decay:   self.params.decay.value()   as u8,
            sustain: self.params.sustain.value() as u8,
            release: self.params.release.value() as u8,
        };
        let wave_slot = self.params.wave_slot.value() as u8;

        // マスター単位5パラメーター：DAWオートメーションで値が変化した場合のみeffectsへ反映する。
        // NRPN(0,4)〜(0,8)はeffectsへ直接書き込まれ、ここでの値が前回と同じ間は上書きされない
        // （差分検知方式。NRPNの変更はnih-plug側のパラメーター表示には反映されない）。
        let reverb_time = self.params.reverb_time.value() as u8;
        if reverb_time != self.last_reverb_time {
            self.effects.set_reverb_time(reverb_time);
            self.last_reverb_time = reverb_time;
        }
        let chorus_mod_rate = self.params.chorus_mod_rate.value() as u8;
        if chorus_mod_rate != self.last_chorus_mod_rate {
            self.effects.set_chorus_mod_rate(chorus_mod_rate);
            self.last_chorus_mod_rate = chorus_mod_rate;
        }
        let chorus_mod_depth = self.params.chorus_mod_depth.value() as u8;
        if chorus_mod_depth != self.last_chorus_mod_depth {
            self.effects.set_chorus_mod_depth(chorus_mod_depth);
            self.last_chorus_mod_depth = chorus_mod_depth;
        }
        let chorus_feedback = self.params.chorus_feedback.value() as u8;
        if chorus_feedback != self.last_chorus_feedback {
            self.effects.set_chorus_feedback(chorus_feedback);
            self.last_chorus_feedback = chorus_feedback;
        }
        let chorus_send_to_reverb = self.params.chorus_send_to_reverb.value() as u8;
        if chorus_send_to_reverb != self.last_chorus_send_to_reverb {
            self.effects.set_chorus_send_to_reverb(chorus_send_to_reverb);
            self.last_chorus_send_to_reverb = chorus_send_to_reverb;
        }

        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } if velocity > 0.0 => {
                    // 同じキーが押しっぱなしの場合は旧チャンネルをリリース
                    if let Some(&old_id) = self.note_channels.get(&note) {
                        self.engine.note_off(old_id);
                    }
                    let freq = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
                    let ch_id = self.engine.note_on(wave_slot, freq, adsr);
                    self.note_channels.insert(note, ch_id);
                    self.apply_performance_lfo(ch_id);
                }
                NoteEvent::NoteOn { note, .. } | NoteEvent::NoteOff { note, .. } => {
                    // velocity=0 の NoteOn も NoteOff として扱う（MIDI仕様）
                    if let Some(&ch_id) = self.note_channels.get(&note) {
                        self.engine.note_off(ch_id);
                        self.note_channels.remove(&note);
                    }
                }
                // パフォーマンスLFO（CC1/76/77/78・RPN0,5・NRPN Destination/Waveform）
                NoteEvent::MidiCC { cc, value, .. } => match cc {
                    1  => { self.lfo_state.cc1   = cc_to_u8(value); self.apply_performance_lfo_to_active(); }
                    76 => { self.lfo_state.rate  = cc_to_u8(value); self.apply_performance_lfo_to_active(); }
                    77 => { self.lfo_state.cc77  = cc_to_u8(value); self.apply_performance_lfo_to_active(); }
                    78 => { self.lfo_state.delay = cc_to_u8(value); self.apply_performance_lfo_to_active(); }
                    98  => { self.nrpn_lsb = cc_to_u7(value); self.update_rpn_selection(true); }
                    99  => { self.nrpn_msb = cc_to_u7(value); self.update_rpn_selection(true); }
                    100 => { self.rpn_lsb  = cc_to_u7(value); self.update_rpn_selection(false); }
                    101 => { self.rpn_msb  = cc_to_u7(value); self.update_rpn_selection(false); }
                    6   => self.handle_data_entry(value),
                    // マスターエフェクトセンドレベル
                    91 => self.effects.set_reverb_send(cc_to_u8(value)),
                    93 => self.effects.set_chorus_send(cc_to_u8(value)),
                    _ => {}
                },
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
        self.effects.process(buf, num_channels);

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

impl ClapPlugin for Wms1Plugin {
    const CLAP_ID: &'static str = "com.ym38x6.wms1";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("WMS-1 Waveform Memory Synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
    ];
}

impl Vst3Plugin for Wms1Plugin {
    const VST3_CLASS_ID: [u8; 16] = *b"Ym38x6--WMS1----";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
    ];
}

nih_export_clap!(Wms1Plugin);
nih_export_vst3!(Wms1Plugin);
