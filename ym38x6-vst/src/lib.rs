use nih_plug::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use wms1_core::{AdsrParams, SoundEngine, Wms1Engine};

struct Wms1Plugin {
    params: Arc<Wms1Params>,
    engine: Wms1Engine,
    note_channels: HashMap<u8, usize>, // MIDIノート番号 → エンジンチャンネルID
    render_buffer: Vec<f32>,           // プロセスコールバック用インターリーブ作業バッファ
    sample_rate: f32,
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
        }
    }
}

impl Default for Wms1Plugin {
    fn default() -> Self {
        const DEFAULT_SR: f32 = 44100.0;
        Self {
            params: Arc::new(Wms1Params::default()),
            engine: Wms1Engine::new(DEFAULT_SR),
            note_channels: HashMap::new(),
            render_buffer: Vec::new(),
            sample_rate: DEFAULT_SR,
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

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
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
