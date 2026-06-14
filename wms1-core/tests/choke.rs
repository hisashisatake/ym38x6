use wms1_core::{AdsrParams, SoundEngine, Wms1Engine};

/// リリース中の同じチャンネルIDへ再度note_onすると、リリースの続行を待たずに即座に
/// エンベロープがカットされ、Attackから再開する（実機FM音源のKey-On挙動に準拠＝同音チョーク）。
#[test]
fn note_on_chokes_release_and_restarts_attack_on_same_channel() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 200 };

    let mut engine = Wms1Engine::new(sample_rate);
    let ch = 0;
    engine.note_on(ch, 1, 1.0, adsr);

    // attack=255で約44サンプルでsustainレベル(1.0)に達する
    let mut warmup = vec![0.0f32; 60];
    engine.render(&mut warmup, 1);
    assert!((warmup[59] - 1.0).abs() < 1e-3, "expected sustain level ~1.0, got {}", warmup[59]);

    engine.note_off(ch);

    // release=200で200サンプル分減衰させる
    let mut release_buf = vec![0.0f32; 200];
    engine.render(&mut release_buf, 1);
    let release_level = release_buf[199];
    assert!(release_level < 0.9, "expected release decay, got {release_level}");

    // 同じチャンネルIDへ再度note_on → リリースを即座にカットしてAttackから再開する（チョーク）
    engine.note_on(ch, 1, 1.0, adsr);

    let mut after = vec![0.0f32; 60];
    engine.render(&mut after, 1);
    assert!(
        after[0] < release_level * 0.5,
        "note_on should choke the release instantly: after[0]={}, release_level={release_level}", after[0]
    );
    assert!(
        (after[59] - 1.0).abs() < 1e-3,
        "note_on should restart attack toward full level: after[59]={}", after[59]
    );
}
