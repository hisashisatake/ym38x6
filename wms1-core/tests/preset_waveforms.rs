use sound_core::{gen_sawtooth, gen_sine, gen_square, gen_triangle, AdsrParams, SoundEngine};
use wms1_core::Wms1Engine;

/// プリセット波形スロット（0=sine, 1=square, 2=sawtooth, 3=triangle）が
/// 対応する波形テーブルを正しく出力するか検証する。
///
/// 周波数を sample_rate / table_len に設定すると、1サンプルごとに
/// 波形テーブルのインデックスが1つずつ進む（位相が浮動小数点誤差なく
/// k/table_len になる）ため、出力サンプルとテーブル値を直接比較できる。
#[test]
fn preset_wave_slots_match_tables() {
    let sample_rate = 44100.0f32;
    let adsr = AdsrParams { attack: 255, decay: 255, sustain: 255, release: 255 };
    let tables = [gen_sine(), gen_square(), gen_sawtooth(), gen_triangle()];
    let table_len = tables[0].len();

    for (slot, table) in tables.iter().enumerate() {
        let mut engine = Wms1Engine::new(sample_rate);
        engine.note_on(slot as u8, sample_rate / table_len as f32, adsr);

        // ウォームアップ: エンベロープがサスティンレベル(=1.0)に達するまで進める
        let mut warmup = vec![0.0f32; 100];
        engine.render(&mut warmup, 1);

        let mut buf = vec![0.0f32; table_len];
        engine.render(&mut buf, 1);

        for (i, &sample) in buf.iter().enumerate() {
            let expected = table.sample_at((101 + i) % table_len);
            assert!(
                (sample - expected).abs() < 1e-6,
                "slot {slot} sample {i}: expected {expected}, got {sample}"
            );
        }
    }
}
