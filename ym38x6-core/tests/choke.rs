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

/// リリース中の同じチャンネルIDへ再度note_on_with_velocityすると、リリースの続行を待たずに
/// 即座にエンベロープがカットされ、Attackから再開する（実機FM音源のKey-On挙動に準拠＝同音チョーク）。
#[test]
fn note_on_with_velocity_chokes_release_and_restarts_attack_on_same_channel() {
    let mut engine = Ym38x6Engine::new(44100.0);
    let patch = sustained_release_patch();
    let ch = 0;
    engine.note_on_with_velocity(ch, 440.0, 127, patch);

    // AR=255で約30サンプルでenv_level=1.0に到達し、D1L=255・D2R=0でDecay2に固定される
    let mut warmup = vec![0.0f32; 100];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    // RR=200のリリースで1000サンプル分減衰させる（env_level: 1.0 → 約0.72）
    let mut release_buf = vec![0.0f32; 1000];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[900..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    // 同じチャンネルIDへ再度note_on_with_velocity → リリースを即座にカットしてAttackから再開する（チョーク）
    engine.note_on_with_velocity(ch, 440.0, 127, patch);

    let mut after = vec![0.0f32; 200];
    engine.render(&mut after, 1);
    let just_after = after[0].abs();
    let after_peak = after[100..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(
        just_after < release_peak * 0.5,
        "note_on_with_velocity should choke the release instantly: just_after={just_after}, release_peak={release_peak}"
    );
    assert!(
        after_peak > release_peak,
        "note_on_with_velocity should restart Attack toward full level: after_peak={after_peak}, release_peak={release_peak}"
    );
}

/// SoundEngine::note_onはカレントパッチを使って同じチャンネルIDで同音チョークする。
#[test]
fn trait_note_on_chokes_release_on_same_channel() {
    let mut engine = Ym38x6Engine::new(44100.0);
    engine.set_patch(sustained_release_patch());
    let ch = 0;
    engine.note_on(ch, 0, 440.0, AdsrParams::default());

    let mut warmup = vec![0.0f32; 100];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    let mut release_buf = vec![0.0f32; 1000];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[900..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    engine.note_on(ch, 0, 440.0, AdsrParams::default());

    let mut after = vec![0.0f32; 200];
    engine.render(&mut after, 1);
    let just_after = after[0].abs();
    let after_peak = after[100..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(just_after < release_peak * 0.5);
    assert!(after_peak > release_peak);
}
