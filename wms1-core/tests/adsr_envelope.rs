use wms1_core::{AdsrParams, SoundEngine, Wms1Engine};

/// ADSRエンベロープの各フェーズが正しく遷移するかを検証する。
///
/// 矩形波（wave_slot=1）はテーブル全域で振幅が常に±1.0なので、
/// 出力サンプルの絶対値がそのままenv_levelに一致する。
/// この性質を利用して、波形の影響を受けずにエンベロープ単体の挙動を観測する。
#[test]
fn attack_phase_ramps_to_full_level() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 0, release: 0 };
    let mut engine = Wms1Engine::new(sample_rate);
    engine.note_on(0, 1, 1.0, adsr);

    let mut buf = vec![0.0f32; 60];
    engine.render(&mut buf, 1);
    let levels: Vec<f32> = buf.iter().map(|s| s.abs()).collect();

    // attack=255（最速）はサンプリングレート/1000なので、約44サンプルで頂点(1.0)に到達する
    for i in 1..45 {
        assert!(levels[i] >= levels[i - 1], "envelope should rise monotonically at sample {i}");
    }
    assert!(levels[44] > 0.99, "envelope should reach ~1.0 by sample 44, got {}", levels[44]);
}

#[test]
fn decay_phase_settles_to_sustain_level() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 255, sustain: 128, release: 0 };
    let mut engine = Wms1Engine::new(sample_rate);
    engine.note_on(0, 1, 1.0, adsr);

    let mut buf = vec![0.0f32; 200];
    engine.render(&mut buf, 1);
    let levels: Vec<f32> = buf.iter().map(|s| s.abs()).collect();

    // attack終了直後は1.0付近まで到達する
    let peak = levels.iter().cloned().fold(0.0f32, f32::max);
    assert!(peak > 0.99, "peak should reach ~1.0, got {peak}");

    // decay終了後はsustainレベルに落ち着いて維持される
    let sustain_level = adsr.sustain as f32 / 255.0;
    for &l in &levels[150..] {
        assert!((l - sustain_level).abs() < 0.001, "expected sustain level {sustain_level}, got {l}");
    }
}

#[test]
fn release_phase_fades_to_silence() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 255, sustain: 128, release: 255 };
    let mut engine = Wms1Engine::new(sample_rate);
    let ch = 0;
    engine.note_on(ch, 1, 1.0, adsr);

    // attack+decayを終え、sustainレベルに達するまで進める
    let mut warmup = vec![0.0f32; 100];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    let mut buf = vec![0.0f32; 200];
    engine.render(&mut buf, 1);
    let levels: Vec<f32> = buf.iter().map(|s| s.abs()).collect();

    // releaseの間、レベルは単調に減少する
    for i in 1..levels.len() {
        assert!(levels[i] <= levels[i - 1] + 1e-6, "level should decrease during release at sample {i}");
    }

    // release=255（最速）はsustainレベル(~0.5)から約25サンプルで無音に達する
    for &l in &levels[100..] {
        assert_eq!(l, 0.0, "expected silence after release completes");
    }
}
