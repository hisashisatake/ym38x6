// ---------------------------------------------------------------------------
// パラメーターマッピング
// ---------------------------------------------------------------------------

/// time=0 → base（タイプごとの最小値）, time=255 → 0.98（発振しない上限）
fn time_to_feedback(time: u8, base: f32) -> f32 {
    (base + (time as f32 / 255.0) * (0.98 - base)).min(0.98)
}

// ---------------------------------------------------------------------------
// Reverb Type
// ---------------------------------------------------------------------------

/// GM2/GS準拠のReverbタイプ（spec.md マスターエフェクトセクション参照）。
/// 宣言順 = NRPN値（0〜7）。Room1〜Plateは拡散リバーブ、
/// Delay/PanningDelayはフィードバックディレイラインで実装する。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ReverbType {
    Room1,
    Room2,
    Room3,
    #[default]
    Hall1,
    Hall2,
    Plate,
    Delay,
    PanningDelay,
}

impl ReverbType {
    /// NRPN値（0〜7）からの変換。範囲外はPanningDelay（最大値）にclampする。
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => ReverbType::Room1,
            1 => ReverbType::Room2,
            2 => ReverbType::Room3,
            3 => ReverbType::Hall1,
            4 => ReverbType::Hall2,
            5 => ReverbType::Plate,
            6 => ReverbType::Delay,
            _ => ReverbType::PanningDelay,
        }
    }
}

// ---------------------------------------------------------------------------
// 拡散リバーブ用フィルター（Freeverb方式）
// ---------------------------------------------------------------------------

/// ダンピング付きコムフィルター。フィードバック経路に1次ローパスを挿入し、
/// 高域を減衰させながら反復させる。
struct CombFilter {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
    damp: f32,
    filter_store: f32,
}

impl CombFilter {
    fn new(delay_samples: usize, feedback: f32, damp: f32) -> Self {
        Self {
            buffer: vec![0.0; delay_samples.max(1)],
            pos: 0,
            feedback,
            damp,
            filter_store: 0.0,
        }
    }

    fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.pos];
        self.filter_store = output * (1.0 - self.damp) + self.filter_store * self.damp;
        self.buffer[self.pos] = input + self.filter_store * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }
}

/// Schroederオールパスフィルター（feedback固定0.5）。
struct AllpassFilter {
    buffer: Vec<f32>,
    pos: usize,
    feedback: f32,
}

impl AllpassFilter {
    fn new(delay_samples: usize) -> Self {
        Self { buffer: vec![0.0; delay_samples.max(1)], pos: 0, feedback: 0.5 }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.pos];
        let output = buffered - input;
        self.buffer[self.pos] = input + buffered * self.feedback;
        self.pos = (self.pos + 1) % self.buffer.len();
        output
    }
}

// ---------------------------------------------------------------------------
// 拡散リバーブ（Room1〜Plate）
// ---------------------------------------------------------------------------

struct DiffuseTuning {
    /// コム/オールパスのディレイ長スケール（44.1kHz基準）。値が大きいほど広い空間。
    room_scale: f32,
    /// `time`=0のときのコムフィルターfeedback（最小残響時間）。
    base_feedback: f32,
    /// コムフィルターのダンピング量（高域減衰の強さ）。
    damping: f32,
}

/// Room1〜Plateのチューニング表（宣言順 = ReverbTypeの宣言順0〜5）。
const DIFFUSE_TUNINGS: [DiffuseTuning; 6] = [
    DiffuseTuning { room_scale: 0.40, base_feedback: 0.60, damping: 0.50 }, // Room1
    DiffuseTuning { room_scale: 0.55, base_feedback: 0.65, damping: 0.45 }, // Room2
    DiffuseTuning { room_scale: 0.70, base_feedback: 0.70, damping: 0.40 }, // Room3
    DiffuseTuning { room_scale: 1.00, base_feedback: 0.78, damping: 0.35 }, // Hall1
    DiffuseTuning { room_scale: 1.30, base_feedback: 0.84, damping: 0.25 }, // Hall2
    DiffuseTuning { room_scale: 0.85, base_feedback: 0.88, damping: 0.10 }, // Plate
];

/// Freeverb標準のコムフィルターディレイ長（サンプル数, 44.1kHz基準）。
const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
/// Freeverb標準のオールパスフィルターディレイ長（サンプル数, 44.1kHz基準）。
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
/// L/Rのディレイ長を変えてステレオ感を出すためのオフセット。
const STEREO_SPREAD: usize = 23;
/// チューニング表の基準サンプルレート。
const REFERENCE_SAMPLE_RATE: f32 = 44100.0;
/// コムフィルター入力段のゲイン（Freeverb標準値）。
const INPUT_GAIN: f32 = 0.05;

/// 並列コムフィルター×8 + 直列オールパスフィルター×4によるFreeverb方式の拡散リバーブ。
struct DiffuseReverb {
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
    base_feedback: f32,
}

impl DiffuseReverb {
    fn new(sample_rate: f32, tuning: &DiffuseTuning, time: u8) -> Self {
        let sr_scale = sample_rate / REFERENCE_SAMPLE_RATE;
        let feedback = time_to_feedback(time, tuning.base_feedback);

        let combs_l = COMB_TUNINGS
            .iter()
            .map(|&len| {
                let delay = (len as f32 * tuning.room_scale * sr_scale) as usize;
                CombFilter::new(delay, feedback, tuning.damping)
            })
            .collect();
        let combs_r = COMB_TUNINGS
            .iter()
            .map(|&len| {
                let delay = ((len + STEREO_SPREAD) as f32 * tuning.room_scale * sr_scale) as usize;
                CombFilter::new(delay, feedback, tuning.damping)
            })
            .collect();

        let allpasses_l = ALLPASS_TUNINGS
            .iter()
            .map(|&len| AllpassFilter::new((len as f32 * sr_scale) as usize))
            .collect();
        let allpasses_r = ALLPASS_TUNINGS
            .iter()
            .map(|&len| AllpassFilter::new(((len + STEREO_SPREAD) as f32 * sr_scale) as usize))
            .collect();

        Self { combs_l, combs_r, allpasses_l, allpasses_r, base_feedback: tuning.base_feedback }
    }

    /// `time`に応じてコムフィルターのfeedback係数のみ更新する（バッファは再確保しない）。
    fn set_time(&mut self, time: u8) {
        let feedback = time_to_feedback(time, self.base_feedback);
        for comb in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            comb.set_feedback(feedback);
        }
    }

    fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let input_l = in_l * INPUT_GAIN;
        let input_r = in_r * INPUT_GAIN;

        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for comb in self.combs_l.iter_mut() {
            out_l += comb.process(input_l);
        }
        for comb in self.combs_r.iter_mut() {
            out_r += comb.process(input_r);
        }

        for allpass in self.allpasses_l.iter_mut() {
            out_l = allpass.process(out_l);
        }
        for allpass in self.allpasses_r.iter_mut() {
            out_r = allpass.process(out_r);
        }

        (out_l, out_r)
    }
}

// ---------------------------------------------------------------------------
// フィードバックディレイリバーブ（Delay / Panning Delay）
// ---------------------------------------------------------------------------

const DELAY_TIME_MIN_MS: f32 = 200.0;
const DELAY_TIME_MAX_MS: f32 = 800.0;
const DELAY_FEEDBACK_MIN: f32 = 0.3;
const DELAY_FEEDBACK_MAX: f32 = 0.85;

/// フィードバックディレイライン。`panning=true`の場合、L入力をRchへ、
/// R入力をLchへ交互にフィードバックするピンポンディレイになる。
struct DelayReverb {
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    pos: usize,
    feedback: f32,
    panning: bool,
}

impl DelayReverb {
    fn new(sample_rate: f32, time: u8, panning: bool) -> Self {
        let delay_ms = DELAY_TIME_MIN_MS + (time as f32 / 255.0) * (DELAY_TIME_MAX_MS - DELAY_TIME_MIN_MS);
        let len = (delay_ms / 1000.0 * sample_rate) as usize + 1;
        let feedback = DELAY_FEEDBACK_MIN + (time as f32 / 255.0) * (DELAY_FEEDBACK_MAX - DELAY_FEEDBACK_MIN);
        Self { buffer_l: vec![0.0; len], buffer_r: vec![0.0; len], pos: 0, feedback, panning }
    }

    fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let out_l = self.buffer_l[self.pos];
        let out_r = self.buffer_r[self.pos];

        if self.panning {
            // L入力の遅延出力をRchへ、R入力の遅延出力をLchへ書き戻す（ピンポン）。
            self.buffer_r[self.pos] = in_l + out_l * self.feedback;
            self.buffer_l[self.pos] = in_r + out_r * self.feedback;
        } else {
            self.buffer_l[self.pos] = in_l + out_l * self.feedback;
            self.buffer_r[self.pos] = in_r + out_r * self.feedback;
        }

        self.pos = (self.pos + 1) % self.buffer_l.len();
        (out_l, out_r)
    }
}

// ---------------------------------------------------------------------------
// Reverb
// ---------------------------------------------------------------------------

enum ReverbAlgorithm {
    Diffuse(DiffuseReverb),
    Delay(DelayReverb),
}

fn build_algorithm(sample_rate: f32, reverb_type: ReverbType, time: u8) -> ReverbAlgorithm {
    match reverb_type {
        ReverbType::Delay => ReverbAlgorithm::Delay(DelayReverb::new(sample_rate, time, false)),
        ReverbType::PanningDelay => ReverbAlgorithm::Delay(DelayReverb::new(sample_rate, time, true)),
        _ => {
            let tuning = &DIFFUSE_TUNINGS[reverb_type as usize];
            ReverbAlgorithm::Diffuse(DiffuseReverb::new(sample_rate, tuning, time))
        }
    }
}

/// マスターリバーブ。`ReverbType`に応じて拡散リバーブ/フィードバックディレイの
/// いずれかのアルゴリズムを内部に保持する。
pub struct Reverb {
    sample_rate: f32,
    reverb_type: ReverbType,
    time: u8,
    algorithm: ReverbAlgorithm,
}

impl Reverb {
    pub fn new(sample_rate: f32) -> Self {
        let reverb_type = ReverbType::default();
        let time = 128;
        let algorithm = build_algorithm(sample_rate, reverb_type, time);
        Self { sample_rate, reverb_type, time, algorithm }
    }

    /// タイプを切り替える。内部アルゴリズムを再構築するため、残響テールはリセットされる。
    pub fn set_type(&mut self, reverb_type: ReverbType) {
        self.reverb_type = reverb_type;
        self.algorithm = build_algorithm(self.sample_rate, reverb_type, self.time);
    }

    /// 残響時間を設定する。拡散リバーブはfeedback係数のみ更新（再構築なし）、
    /// ディレイ系はディレイ長自体が変わるため再構築する。
    pub fn set_time(&mut self, time: u8) {
        self.time = time;
        match &mut self.algorithm {
            ReverbAlgorithm::Diffuse(reverb) => reverb.set_time(time),
            ReverbAlgorithm::Delay(_) => {
                self.algorithm = build_algorithm(self.sample_rate, self.reverb_type, time);
            }
        }
    }

    /// 1サンプル処理する。
    pub fn process(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        match &mut self.algorithm {
            ReverbAlgorithm::Diffuse(reverb) => reverb.process(in_l, in_r),
            ReverbAlgorithm::Delay(reverb) => reverb.process(in_l, in_r),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_type_is_hall1() {
        assert_eq!(ReverbType::default(), ReverbType::Hall1);
    }

    #[test]
    fn from_u8_mapping() {
        assert_eq!(ReverbType::from_u8(0), ReverbType::Room1);
        assert_eq!(ReverbType::from_u8(3), ReverbType::Hall1);
        assert_eq!(ReverbType::from_u8(7), ReverbType::PanningDelay);
        assert_eq!(ReverbType::from_u8(255), ReverbType::PanningDelay);
    }

    #[test]
    fn diffuse_impulse_creates_decaying_tail() {
        let mut reverb = Reverb::new(44100.0);
        reverb.set_type(ReverbType::Hall1);

        let (first_l, _) = reverb.process(1.0, 1.0);
        assert!(first_l.abs() < 1.0, "インパルス直後の出力は入力より小さいはず: {first_l}");

        let mut tail_energy = 0.0;
        for _ in 0..44100 {
            let (l, r) = reverb.process(0.0, 0.0);
            tail_energy += l * l + r * r;
        }
        assert!(tail_energy > 0.0, "テールが減衰しきって無音になっている");
    }

    #[test]
    fn no_nan_long_run() {
        for &t in &[
            ReverbType::Room1,
            ReverbType::Hall1,
            ReverbType::Hall2,
            ReverbType::Plate,
            ReverbType::Delay,
            ReverbType::PanningDelay,
        ] {
            let mut reverb = Reverb::new(44100.0);
            reverb.set_type(t);
            reverb.set_time(255);
            for i in 0..(44100 * 2) {
                let input = if i % 4410 == 0 { 1.0 } else { 0.0 };
                let (l, r) = reverb.process(input, -input);
                assert!(l.is_finite() && r.is_finite(), "{t:?}: 発散またはNaN: {l}, {r}");
                assert!(l.abs() < 100.0 && r.abs() < 100.0, "{t:?}: 発散している: {l}, {r}");
            }
        }
    }

    #[test]
    fn delay_type_produces_echo() {
        let mut reverb = Reverb::new(44100.0);
        reverb.set_type(ReverbType::Delay);
        reverb.set_time(0); // 最短ディレイ（200ms）

        let (first_l, _) = reverb.process(1.0, 1.0);
        assert_eq!(first_l, 0.0, "ディレイ直後はまだエコーが返ってこないはず");

        let delay_samples = (DELAY_TIME_MIN_MS / 1000.0 * 44100.0) as usize;
        let mut found_at = None;
        for i in 1..delay_samples + 10 {
            let (l, _) = reverb.process(0.0, 0.0);
            if l.abs() > 1e-6 {
                found_at = Some(i);
                break;
            }
        }
        let i = found_at.expect("ディレイのエコーが検出できない");
        assert!((i as i64 - delay_samples as i64).abs() <= 5, "エコーのタイミングがおかしい: i={i}, expected≈{delay_samples}");
    }

    #[test]
    fn panning_delay_crosses_channels() {
        let mut reverb = Reverb::new(44100.0);
        reverb.set_type(ReverbType::PanningDelay);
        reverb.set_time(0);

        reverb.process(1.0, 0.0);

        let delay_samples = (DELAY_TIME_MIN_MS / 1000.0 * 44100.0) as usize;
        for _ in 0..delay_samples {
            reverb.process(0.0, 0.0);
        }
        // Lに入力した信号は、1ディレイ周期後にRchから出力される（ピンポン）
        let (out_l, out_r) = reverb.process(0.0, 0.0);
        assert!(out_r.abs() > out_l.abs(), "PanningDelayはL入力をRに出すはず: l={out_l}, r={out_r}");
    }
}
