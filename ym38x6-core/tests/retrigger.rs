use ym38x6_core::{AdsrParams, ChannelParams, OperatorParams, SoundEngine, Ym38x6Engine, Ym38x6Patch};

/// AR最速・サスティン無限・RR=200（中速リリース）の4op並列(algorithm 7)パッチ。
/// frequency=440.0(note=69)でKSRの影響を受けず、レート計算が単純になる。
fn sustained_release_patch() -> Ym38x6Patch {
    let op = OperatorParams {
        tl: 255,
        ar: 255,
        d1r: 0,
        d2r: 0,
        d1l: 255,
        rr: 200,
        mul: 1,
        dt1: 128,
        ksr: 0,
        am_enable: false,
        velocity_sensitivity: 0,
        waveform: 3, // 矩形波
    };
    Ym38x6Patch {
        operators: [op; 4],
        channel: ChannelParams { algorithm: 7, ..ChannelParams::default() },
    }
}

/// リリース中にretrigger_with_velocityすると、リリースの続行を待たずに即座にエンベロープが
/// カットされ、同じチャンネルIDでAttackから再開する（実機FM音源のKey-On挙動に準拠）。
#[test]
fn retrigger_with_velocity_cuts_release_and_restarts_attack_on_same_channel() {
    let mut engine = Ym38x6Engine::new(44100.0);
    let patch = sustained_release_patch();
    let ch = engine.note_on_with_velocity(440.0, 127, patch);

    // AR=255で約30サンプルでenv_level=1.0に到達し、D1L=255・D2R=0でDecay2に固定される
    let mut warmup = vec![0.0f32; 100];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    // RR=200のリリースで1000サンプル分減衰させる（env_level: 1.0 → 約0.72）
    let mut release_buf = vec![0.0f32; 1000];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[900..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    // 同じチャンネルIDでretrigger → リリースを即座にカットしてAttackから再開する
    assert!(engine.retrigger_with_velocity(ch, 440.0, 127, patch));

    let mut after = vec![0.0f32; 200];
    engine.render(&mut after, 1);
    let just_after = after[0].abs();
    let after_peak = after[100..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(
        just_after < release_peak * 0.5,
        "retrigger should cut the release tail instantly: just_after={just_after}, release_peak={release_peak}"
    );
    assert!(
        after_peak > release_peak,
        "retrigger should restart Attack toward full level: after_peak={after_peak}, release_peak={release_peak}"
    );

    // 存在しないチャンネルへのretrigger_with_velocityはfalseを返す
    assert!(!engine.retrigger_with_velocity(9999, 440.0, 127, patch));
}

/// SoundEngine::retriggerはカレントパッチを使って同じチャンネルIDでリトリガーする。
#[test]
fn trait_retrigger_uses_current_patch_on_same_channel() {
    let mut engine = Ym38x6Engine::new(44100.0);
    engine.set_patch(sustained_release_patch());
    let ch = engine.note_on(0, 440.0, AdsrParams::default());

    let mut warmup = vec![0.0f32; 100];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    let mut release_buf = vec![0.0f32; 1000];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[900..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(engine.retrigger(ch, 0, 440.0, AdsrParams::default()));

    let mut after = vec![0.0f32; 200];
    engine.render(&mut after, 1);
    let just_after = after[0].abs();
    let after_peak = after[100..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(just_after < release_peak * 0.5);
    assert!(after_peak > release_peak);

    // 存在しないチャンネルへのretriggerはfalseを返す
    assert!(!engine.retrigger(9999, 0, 440.0, AdsrParams::default()));
}
