use crate::operator::OperatorParams;
use crate::Ym38x6Patch;

/// Bank Select（CC0×128+CC32）とProgram Change（0〜127）から決定的にパッチを生成する
/// 暫定プレースホルダー。GM2準拠のBank0音色はym38x6-ml（フェーズ5、インバース合成）で、
/// Bank1以降のユーザープリセットはプリセットライブラリ（フェーズ5）で生成・管理する予定。
/// 実データができるまでの間、Bank/Programの値域を一通り確認できるダミーパッチを返す
/// （bank/programの値はseedとして使うのみで、bankによる音色の区別は未実装）。
pub fn placeholder_patch(bank: u16, program: u8) -> Ym38x6Patch {
    let seed = program.wrapping_add(bank as u8);

    let mut patch = Ym38x6Patch::default();
    patch.channel.algorithm = seed % 8;
    patch.channel.feedback = seed.wrapping_mul(2);
    patch.channel.filter_cutoff = 255;
    patch.channel.filter_self_oscillation = true;

    // tests::loud_patchと同じ「即音量最大・サスティン無限」の基本設定（聴感確認用）
    let base = OperatorParams {
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
        velocity_sensitivity: 0,
        waveform: 0,
    };
    for (i, op) in patch.operators.iter_mut().enumerate() {
        *op = OperatorParams { waveform: seed.wrapping_add(i as u8) % 8, ..base };
    }
    patch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SoundEngine, Ym38x6Engine};

    #[test]
    fn algorithm_and_waveform_stay_in_range_for_all_programs() {
        for bank in [0u16, 1, 128] {
            for program in 0..=255u8 {
                let patch = placeholder_patch(bank, program);
                assert!(patch.channel.algorithm < 8);
                for op in patch.operators {
                    assert!(op.waveform < 8);
                }
            }
        }
    }

    #[test]
    fn placeholder_patch_is_audible() {
        for (bank, program) in [(0u16, 0u8), (0, 64), (1, 42), (128, 127)] {
            let mut engine = Ym38x6Engine::new(44100.0);
            let ch = engine.note_on_with_velocity(440.0, 127, placeholder_patch(bank, program));
            let mut buf = vec![0.0f32; 512];
            engine.render(&mut buf, 1);
            assert!(buf.iter().all(|&s| s.is_finite()));
            assert!(buf.iter().any(|&s| s != 0.0), "bank={bank} program={program} is silent");
            let _ = ch;
        }
    }
}
