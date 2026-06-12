use nih_plug::prelude::*;
use std::sync::Arc;
use ym38x6_core::{SoundEngine, Ym38x6Engine};

struct Ym38x6Plugin {
    params: Arc<Ym38x6Params>,
    engine: Ym38x6Engine,
    render_buffer: Vec<f32>, // プロセスコールバック用インターリーブ作業バッファ
    sample_rate: f32,
}

#[derive(Params)]
struct Ym38x6Params {}

impl Default for Ym38x6Params {
    fn default() -> Self {
        Self {}
    }
}

impl Default for Ym38x6Plugin {
    fn default() -> Self {
        const DEFAULT_SR: f32 = 44100.0;
        Self {
            params: Arc::new(Ym38x6Params::default()),
            engine: Ym38x6Engine::new(DEFAULT_SR),
            render_buffer: Vec::new(),
            sample_rate: DEFAULT_SR,
        }
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
        self.engine = Ym38x6Engine::new(self.sample_rate);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
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
