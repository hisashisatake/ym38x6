// ---------------------------------------------------------------------------
// パラメーターマッピング関数群（すべて純粋関数）
//
// 数式はすべて初期案（暫定）。CLAUDE.mdのテスト方針に従い、
// 実装後に音を聴いて係数を調整する。
// ---------------------------------------------------------------------------

/// MUL値(0〜15)→周波数比。OPM/OPN/OPQ/OPZ共通のMultiple(4bit、0=0.5倍、1〜15=等倍)に準拠。
/// 8bit統一の例外（[operator.rs](operator.rs)のOperatorParams::mul参照）。
pub fn mul_to_ratio(mul: u8) -> f32 {
    const TABLE: [f32; 16] = [
        0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
    ];
    TABLE[(mul as usize).min(15)]
}

/// DT1値(0〜255、中心128)→セント。中心128で±0、両端で±DETUNE_RANGE_CENTS（暫定50セント）。
pub fn dt1_to_cents(dt1: u8) -> f32 {
    const DETUNE_RANGE_CENTS: f32 = 50.0;
    (dt1 as f32 - 128.0) / 128.0 * DETUNE_RANGE_CENTS
}

/// TL値(0〜255)→リニアゲイン。255=0dB、0≈-95.6dB（0.375dB/step、暫定）。
pub fn tl_to_gain(tl: u8) -> f32 {
    const DB_PER_STEP: f32 = 95.25 / 254.0;
    let db = (255 - tl as u16) as f32 * -DB_PER_STEP;
    10f32.powf(db / 20.0)
}

/// レート値(0〜255)→1サンプルあたりのEG変化量。
/// rate=0は特殊値で「変化なし」（OPM/OPNのAR=0/D1R=0/D2R=0と同じフリーズ状態）。
/// rate=1〜255はt_max（rate=1、最遅）〜t_min（rate=255、最速）の指数マッピング。
fn rate_to_delta(rate: u8, sample_rate: f32, t_min: f32, t_max: f32) -> f32 {
    if rate == 0 {
        return 0.0;
    }
    let t = t_min * (t_max / t_min).powf(1.0 - (rate as f32 - 1.0) / 254.0);
    1.0 / (t * sample_rate)
}

/// AR: 0.68ms〜20.2秒。OPM AR(5bit)のreg=31〜1(eg_rate=62〜2、KSRなし)の理論値が基準。
/// reg=31(eg_rate=62)はキーオン時に瞬時attenuation=0となる特殊仕様だが、
/// 増分テーブルの値自体はreg=30(eg_rate=60)と同一のため0.68msを採用。
/// rate=0はreg=0相当のフリーズ（発音しない）。
pub fn ar_to_delta(rate: u8, sample_rate: f32) -> f32 {
    rate_to_delta(rate, sample_rate, 0.00068, 20.2)
}

/// D1R/D2R/RR: 8.71ms〜284.9秒。OPM D1R/D2R(5bit)のreg=31〜1、およびRR(4bit)の
/// reg=15〜0の理論値が基準（KSRなし、両者ともeg_rateの範囲が一致するため共通）。
/// rate=0はD1R/D2R=0相当のフリーズ（サスティンレベルを無限保持）。
pub fn decay_to_delta(rate: u8, sample_rate: f32) -> f32 {
    rate_to_delta(rate, sample_rate, 0.00871, 284.9)
}

/// RR: [decay_to_delta]と同一の理論範囲（8.71ms〜284.9秒）。
/// rate=0はOPMのRRレジスタには無い「リリースしない」拡張。
pub fn rr_to_delta(rate: u8, sample_rate: f32) -> f32 {
    rate_to_delta(rate, sample_rate, 0.00871, 284.9)
}

/// SL値(0〜255)→サスティンレベル比率(0.0〜1.0)。2乗カーブで減衰感を表現（暫定）。
pub fn sl_to_level(sl: u8) -> f32 {
    let x = sl as f32 / 255.0;
    x * x
}

/// KSR値(0〜255)→A4(note=69)からのオクターブ差に対するレート倍率（暫定）。
/// ksr=0で常に1.0（KSR無効）、ksr=255でオクターブ差に比例して倍率が変化する。
pub fn ksr_rate_multiplier(ksr: u8, note: u8) -> f32 {
    let octave_diff = (note as f32 - 69.0) / 12.0;
    2f32.powf(octave_diff * (ksr as f32 / 255.0))
}

/// 実効TL = clamp(TLベース値 + (Velocity/127) × VelocitySensitivity, 0, 255)
pub fn effective_tl(base_tl: u8, velocity: u8, sensitivity: u8) -> u8 {
    let add = (velocity as f32 / 127.0) * sensitivity as f32;
    (base_tl as f32 + add).round().clamp(0.0, 255.0) as u8
}

/// フィードバック値(0〜255)→自己変調の強さ（位相オフセット換算）。
/// OPM/OPNのFB(3bit、0=オフ・1〜7は1段ごとに2倍)を踏まえ、feedback=0は実機FB=0と
/// 同じ「フィードバックなし」の特殊値。feedback=1〜255は7オクターブ
/// （約36刻みごとに2倍、feedback=255で最大8.0）の指数カーブにマッピングする。
pub fn feedback_to_scale(feedback: u8) -> f32 {
    if feedback == 0 {
        return 0.0;
    }
    8.0 * 2.0f32.powf(7.0 * (feedback as f32 / 255.0 - 1.0))
}

/// オペレーター間FM変調の深さスケール（固定の内部定数、暫定値）。
/// 実機FM音源にチャンネル単位の「PM感度」相当のパラメーターは無く、
/// モジュレーターのTL（出力レベル）がそのまま変調量になる。
/// このスケールは出力波形の振幅(-1.0〜1.0)を位相オフセット量に変換するための係数。
pub const FM_MODULATION_INDEX_SCALE: f32 = 4.0;

/// 周波数(Hz)→近似MIDIノート番号（KSR計算用）。
pub fn frequency_to_note(frequency: f32) -> u8 {
    (69.0 + 12.0 * (frequency / 440.0).log2())
        .round()
        .clamp(0.0, 127.0) as u8
}

/// 13bit F-Number(0〜8191)の中心値（2^12）。OP単位F-Number上書き(NRPN 0,18〜21)で
/// 比率1.0（上書きなし、Note-On時と同じ周波数）を表す基準値（暫定）。
pub const F_NUMBER_CENTER: u16 = 4096;

/// F-Number(0〜8191)→周波数比。F_NUMBER_CENTERで1.0倍（上書きなし）、
/// 全域で約0.0〜2.0倍（約2オクターブ分）の可変範囲を持つ（暫定）。
pub fn f_number_to_ratio(f_number: u16) -> f32 {
    f_number as f32 / F_NUMBER_CENTER as f32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_to_ratio_bounds() {
        assert_eq!(mul_to_ratio(0), 0.5);
        assert_eq!(mul_to_ratio(15), 15.0);
        assert_eq!(mul_to_ratio(1), 1.0);
        // 範囲外(16〜255)は15にクランプ
        assert_eq!(mul_to_ratio(255), 15.0);
    }

    #[test]
    fn dt1_to_cents_center_and_bounds() {
        assert_eq!(dt1_to_cents(128), 0.0);
        assert_eq!(dt1_to_cents(0), -50.0);
        assert!((dt1_to_cents(255) - 49.609375).abs() < 1e-3);
    }

    #[test]
    fn tl_to_gain_bounds_and_monotonic() {
        assert!((tl_to_gain(255) - 1.0).abs() < 1e-6);
        assert!(tl_to_gain(0) > 0.0 && tl_to_gain(0) < tl_to_gain(255));
        assert!(tl_to_gain(128) < tl_to_gain(255));
    }

    #[test]
    fn ar_to_delta_bounds() {
        let sr = 44100.0;
        // rate=0はフリーズ（変化なし）
        assert_eq!(ar_to_delta(0, sr), 0.0);
        let slowest = ar_to_delta(1, sr);
        let fastest = ar_to_delta(255, sr);
        assert!((slowest - 1.0 / (20.2 * sr)).abs() < 1e-9);
        assert!((fastest - 1.0 / (0.00068 * sr)).abs() < 1e-9);
        assert!(fastest > slowest);
    }

    #[test]
    fn decay_and_release_to_delta_bounds() {
        let sr = 44100.0;
        // rate=0はフリーズ（変化なし）
        assert_eq!(decay_to_delta(0, sr), 0.0);
        assert_eq!(rr_to_delta(0, sr), 0.0);
        assert!((decay_to_delta(255, sr) - 1.0 / (0.00871 * sr)).abs() < 1e-9);
        assert!((decay_to_delta(1, sr) - 1.0 / (284.9 * sr)).abs() < 1e-9);
        assert!((rr_to_delta(255, sr) - 1.0 / (0.00871 * sr)).abs() < 1e-9);
        assert!((rr_to_delta(1, sr) - 1.0 / (284.9 * sr)).abs() < 1e-9);
    }

    #[test]
    fn sl_to_level_bounds() {
        assert_eq!(sl_to_level(0), 0.0);
        assert!((sl_to_level(255) - 1.0).abs() < 1e-6);
        assert!(sl_to_level(128) < 1.0);
    }

    #[test]
    fn ksr_rate_multiplier_bounds() {
        // ksr=0なら常に1.0（KSR無効）
        assert!((ksr_rate_multiplier(0, 81) - 1.0).abs() < 1e-6);
        assert!((ksr_rate_multiplier(0, 57) - 1.0).abs() < 1e-6);
        // note=69（A4）なら常に1.0（オクターブ差0）
        assert!((ksr_rate_multiplier(255, 69) - 1.0).abs() < 1e-6);
        // ksr=255、1オクターブ上（note=81）なら2倍速
        assert!((ksr_rate_multiplier(255, 81) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn effective_tl_clamps() {
        assert_eq!(effective_tl(250, 127, 255), 255);
        assert_eq!(effective_tl(0, 0, 255), 0);
        assert_eq!(effective_tl(100, 0, 100), 100);
    }

    #[test]
    fn feedback_to_scale_bounds() {
        assert_eq!(feedback_to_scale(0), 0.0);
        assert!((feedback_to_scale(255) - 8.0).abs() < 1e-3);
        // 指数カーブ：feedback=0(オフ)以外は全域で滑らかに増加する
        assert!(feedback_to_scale(1) > 0.0);
        assert!(feedback_to_scale(64) < feedback_to_scale(128));
        assert!(feedback_to_scale(128) < feedback_to_scale(192));
        assert!(feedback_to_scale(192) < feedback_to_scale(255));
    }

    #[test]
    fn frequency_to_note_reference_points() {
        assert_eq!(frequency_to_note(440.0), 69);
        assert_eq!(frequency_to_note(880.0), 81);
        assert_eq!(frequency_to_note(220.0), 57);
    }

    #[test]
    fn f_number_to_ratio_bounds() {
        assert_eq!(f_number_to_ratio(F_NUMBER_CENTER), 1.0);
        assert_eq!(f_number_to_ratio(0), 0.0);
        assert!((f_number_to_ratio(8191) - 1.99976).abs() < 1e-4);
    }
}
