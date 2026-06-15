// ---------------------------------------------------------------------------
// OPZ系8波形（spec.md オペレーター波形セクション参照）
// 波形0〜4は仕様確定、5〜7は方向性を踏まえた暫定形状（実装後に聴感調整する）
// ---------------------------------------------------------------------------

use sound_core::{WaveTable, gen_from_fn};
use std::f32::consts::PI;

/// 0: サイン波
pub fn gen_op_sine() -> WaveTable {
    gen_from_fn(|p| (2.0 * PI * p).sin())
}

/// 1: ハーフサイン（後半周期を無音化）
pub fn gen_op_half_sine() -> WaveTable {
    gen_from_fn(|p| if p < 0.5 { (2.0 * PI * p).sin() } else { 0.0 })
}

/// 2: 絶対値サイン（全波整流）
pub fn gen_op_abs_sine() -> WaveTable {
    gen_from_fn(|p| (2.0 * PI * p).sin().abs())
}

/// 3: 矩形波
pub fn gen_op_square() -> WaveTable {
    gen_from_fn(|p| if p < 0.5 { 1.0 } else { -1.0 })
}

/// 4: ノコギリ波
pub fn gen_op_sawtooth() -> WaveTable {
    gen_from_fn(|p| 2.0 * p - 1.0)
}

/// 5: クワンタイズドサイン（8段階に量子化、硬い質感、暫定）
pub fn gen_op_quantized_sine() -> WaveTable {
    const STEPS: f32 = 8.0;
    gen_from_fn(|p| ((2.0 * PI * p).sin() * STEPS).round() / STEPS)
}

/// 6: パルスサイン（正の半周期を1周期内で2回繰り返す、暫定）
pub fn gen_op_pulse_sine() -> WaveTable {
    gen_from_fn(|p| (2.0 * PI * (p * 2.0).fract()).sin().max(0.0))
}

/// 7: オクターブサイン（基音 + 1オクターブ上を合成、暫定）
pub fn gen_op_octave_sine() -> WaveTable {
    gen_from_fn(|p| {
        let f0 = (2.0 * PI * p).sin();
        let f1 = (2.0 * PI * p * 2.0).sin();
        (f0 * 0.7 + f1 * 0.3).clamp(-1.0, 1.0)
    })
}

/// 波形番号0〜7に対応する波形テーブルを生成する。
pub fn gen_builtin_waveform(index: u8) -> WaveTable {
    match index {
        0 => gen_op_sine(),
        1 => gen_op_half_sine(),
        2 => gen_op_abs_sine(),
        3 => gen_op_square(),
        4 => gen_op_sawtooth(),
        5 => gen_op_quantized_sine(),
        6 => gen_op_pulse_sine(),
        _ => gen_op_octave_sine(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const WAVE_LEN: usize = 1024;

    #[test]
    fn all_waveforms_have_correct_length() {
        for i in 0..8u8 {
            let t = gen_builtin_waveform(i);
            assert_eq!(t.len(), WAVE_LEN, "waveform {i}");
        }
    }

    #[test]
    fn sine_representative_points() {
        let t = gen_op_sine();
        assert!((t.sample_at(0) - 0.0).abs() < 0.01);
        assert!((t.sample_at(WAVE_LEN / 4) - 1.0).abs() < 0.01);
        assert!((t.sample_at(WAVE_LEN / 2) - 0.0).abs() < 0.01);
        assert!((t.sample_at(WAVE_LEN * 3 / 4) - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn half_sine_second_half_is_silent() {
        let t = gen_op_half_sine();
        assert!((t.sample_at(WAVE_LEN / 4) - 1.0).abs() < 0.01);
        assert!(t.sample_at(WAVE_LEN * 3 / 4).abs() < 1e-3);
    }

    #[test]
    fn abs_sine_is_non_negative() {
        let t = gen_op_abs_sine();
        for i in 0..WAVE_LEN {
            assert!(t.sample_at(i) >= -1e-3, "index {i}: {}", t.sample_at(i));
        }
        assert!((t.sample_at(WAVE_LEN * 3 / 4) - 1.0).abs() < 0.01);
    }

    #[test]
    fn square_wave_levels() {
        let t = gen_op_square();
        assert!((t.sample_at(0) - 1.0).abs() < 0.01);
        assert!((t.sample_at(WAVE_LEN / 2) - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn sawtooth_ramps_up() {
        let t = gen_op_sawtooth();
        assert!((t.sample_at(0) - (-1.0)).abs() < 0.01);
        assert!((t.sample_at(WAVE_LEN / 2) - 0.0).abs() < 0.01);
    }

    #[test]
    fn quantized_sine_within_range() {
        let t = gen_op_quantized_sine();
        for i in 0..WAVE_LEN {
            let s = t.sample_at(i);
            assert!(s >= -1.0 - 1e-3 && s <= 1.0 + 1e-3, "index {i}: {s}");
        }
    }

    #[test]
    fn pulse_sine_is_non_negative() {
        let t = gen_op_pulse_sine();
        for i in 0..WAVE_LEN {
            assert!(t.sample_at(i) >= -1e-3, "index {i}: {}", t.sample_at(i));
        }
    }

    #[test]
    fn octave_sine_within_range() {
        let t = gen_op_octave_sine();
        for i in 0..WAVE_LEN {
            let s = t.sample_at(i);
            assert!(s >= -1.0 - 1e-3 && s <= 1.0 + 1e-3, "index {i}: {s}");
        }
    }
}
