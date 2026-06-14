use wms1_core::{AdsrParams, SoundEngine, Wms1Engine};

/// リリース中にretriggerすると、リリースの続行を待たずに即座にエンベロープがカットされ、
/// 同じチャンネルIDでAttackから再開する（実機FM音源のKey-On挙動に準拠）。
#[test]
fn retrigger_cuts_release_and_restarts_attack_on_same_channel() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 200 };

    let mut engine = Wms1Engine::new(sample_rate);
    let ch = engine.note_on(1, 1.0, adsr);

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

    // 同じチャンネルIDでretrigger → リリースを即座にカットしてAttackから再開する
    assert!(engine.retrigger(ch, 1, 1.0, adsr));

    let mut after = vec![0.0f32; 60];
    engine.render(&mut after, 1);
    assert!(
        after[0] < release_level * 0.5,
        "retrigger should cut the release instantly: after[0]={}, release_level={release_level}", after[0]
    );
    assert!(
        (after[59] - 1.0).abs() < 1e-3,
        "retrigger should restart attack toward full level: after[59]={}", after[59]
    );

    // 存在しないチャンネルへのretriggerはfalseを返す
    assert!(!engine.retrigger(9999, 1, 1.0, adsr));
}
