use serde::Deserialize;
use ym38x6_core::{ChannelParams, OperatorParams, Ym38x6Patch};

/// フロントエンドから渡されるオペレーター単位パラメーター（`OperatorParams`のDTO）。
#[derive(Deserialize)]
pub struct OperatorParamsDto {
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
    pub waveform: u8,
    /// OP単位の追加チューニング（0〜255、中心128＝±0、±1オクターブ）。
    /// 未送信のフロントエンドでも中心128（オフセットなし）として扱う。
    #[serde(default = "default_op_fine_tune")]
    pub op_fine_tune: u8,
}

fn default_op_fine_tune() -> u8 {
    128
}

impl From<OperatorParamsDto> for OperatorParams {
    fn from(dto: OperatorParamsDto) -> Self {
        Self {
            tl: dto.tl,
            ar: dto.ar,
            d1r: dto.d1r,
            d2r: dto.d2r,
            d1l: dto.d1l,
            rr: dto.rr,
            mul: dto.mul,
            dt1: dto.dt1,
            ksr: dto.ksr,
            am_enable: dto.am_enable,
            velocity_sensitivity: dto.velocity_sensitivity,
            waveform: dto.waveform,
            op_fine_tune: dto.op_fine_tune,
        }
    }
}

/// フロントエンドから渡されるチャンネル単位パラメーター（`ChannelParams`のDTO）。
#[derive(Deserialize)]
pub struct ChannelParamsDto {
    pub algorithm: u8,
    pub feedback: u8,
    pub tone_lfo_freq: u8,
    pub tone_lfo_pmd: u8,
    pub tone_lfo_amd: u8,
    pub tone_lfo_delay: u8,
    pub pms: u8,
    pub ams: u8,
    pub filter_cutoff: u8,
    pub filter_resonance: u8,
    pub filter_type: u8,
    pub filter_self_oscillation: bool,
    pub filter_eg_attack: u8,
    pub filter_eg_decay: u8,
    pub filter_eg_sustain: u8,
    pub filter_eg_release: u8,
    pub filter_eg_depth: u8,
}

impl From<ChannelParamsDto> for ChannelParams {
    fn from(dto: ChannelParamsDto) -> Self {
        Self {
            algorithm: dto.algorithm,
            feedback: dto.feedback,
            tone_lfo_freq: dto.tone_lfo_freq,
            tone_lfo_pmd: dto.tone_lfo_pmd,
            tone_lfo_amd: dto.tone_lfo_amd,
            tone_lfo_delay: dto.tone_lfo_delay,
            pms: dto.pms,
            ams: dto.ams,
            filter_cutoff: dto.filter_cutoff,
            filter_resonance: dto.filter_resonance,
            filter_type: dto.filter_type,
            filter_self_oscillation: dto.filter_self_oscillation,
            filter_eg_attack: dto.filter_eg_attack,
            filter_eg_decay: dto.filter_eg_decay,
            filter_eg_sustain: dto.filter_eg_sustain,
            filter_eg_release: dto.filter_eg_release,
            filter_eg_depth: dto.filter_eg_depth,
        }
    }
}

/// `ym38x6_note_on`/`ym38x6_set_patch`で受け取るパッチ一式（`Ym38x6Patch`のDTO）。
#[derive(Deserialize)]
pub struct Ym38x6PatchDto {
    pub operators: [OperatorParamsDto; 4],
    pub channel: ChannelParamsDto,
}

impl From<Ym38x6PatchDto> for Ym38x6Patch {
    fn from(dto: Ym38x6PatchDto) -> Self {
        Self {
            operators: dto.operators.map(OperatorParams::from),
            channel: dto.channel.into(),
        }
    }
}
