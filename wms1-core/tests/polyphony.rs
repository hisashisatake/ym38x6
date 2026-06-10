use wms1_core::{AdsrParams, SoundEngine, Wms1Engine};

/// 同一波形・同一ADSRの2音を同時に発音すると、出力は1音の場合の2倍になる（加算合成）。
#[test]
fn multiple_notes_mix_additively() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 0 };

    let mut engine_one = Wms1Engine::new(sample_rate);
    engine_one.note_on(1, 1.0, adsr);
    let mut buf_one = vec![0.0f32; 60];
    engine_one.render(&mut buf_one, 1);

    let mut engine_two = Wms1Engine::new(sample_rate);
    engine_two.note_on(1, 1.0, adsr);
    engine_two.note_on(1, 1.0, adsr);
    let mut buf_two = vec![0.0f32; 60];
    engine_two.render(&mut buf_two, 1);

    for i in 0..60 {
        assert!(
            (buf_two[i] - 2.0 * buf_one[i]).abs() < 1e-5,
            "sample {i}: expected {} (2x single voice), got {}", 2.0 * buf_one[i], buf_two[i]
        );
    }
}

/// リリースが完了して無音になったチャンネルは、以降のミックスに影響しなくなる。
#[test]
fn finished_channel_no_longer_affects_mix() {
    let sample_rate = 44100.0;
    let releasing_adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 255 };
    let sustained_adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 0 };

    let mut engine = Wms1Engine::new(sample_rate);
    let releasing = engine.note_on(1, 1.0, releasing_adsr);
    engine.note_on(1, 2.0, sustained_adsr);

    // 両方ともattackを終えてsustainレベルに達するまで進める
    let mut warmup = vec![0.0f32; 60];
    engine.render(&mut warmup, 1);

    engine.note_off(releasing);

    // release=255（最速）はsustainレベル(1.0)から約45サンプルで無音に達する
    let mut buf = vec![0.0f32; 100];
    engine.render(&mut buf, 1);

    // 比較用: 最初から持続音（2.0Hz）のみを発音したエンジン
    let mut engine_ref = Wms1Engine::new(sample_rate);
    engine_ref.note_on(1, 2.0, sustained_adsr);
    let mut warmup_ref = vec![0.0f32; 60];
    engine_ref.render(&mut warmup_ref, 1);
    let mut buf_ref = vec![0.0f32; 100];
    engine_ref.render(&mut buf_ref, 1);

    // releaseが完了した後ろ半分は、持続音単独の場合と一致する
    for i in 60..100 {
        assert!(
            (buf[i] - buf_ref[i]).abs() < 1e-5,
            "sample {i}: expected {} (sustained voice only), got {}", buf_ref[i], buf[i]
        );
    }
}
