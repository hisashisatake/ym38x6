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
        waveform: 0, // サイン波（サンプル間の段差が小さく、デクリックの連続性を観測しやすい）
    };
    Ym38x6Patch {
        operators: [op; 4],
        channel: ChannelParams { algorithm: 7, ..ChannelParams::default() },
    }
}

/// リリース中の同じチャンネルIDへ再度note_onすると、旧ボイスは即座に瞬間消滅せず、
/// 数ms（DECLICK_SECONDS=4ms≒176サンプル@44.1kHz）かけて線形にフェードアウトしてから消える
/// （クリックノイズ緩和）。新ボイスの発音タイミングは遅れない（旧ボイスに重ねてフェードするため）。
///
/// 旧ボイスのフェードを単独で観測するため、チョーク後の新ボイスはアタック最遅(ar=0)にして
/// 観測ウィンドウ内ではほぼ無音に保つ。
#[test]
fn choke_declicks_old_voice_instead_of_hard_cut() {
    let mut engine = Ym38x6Engine::new(44100.0);
    let patch = sustained_release_patch();
    let ch = 0;
    engine.note_on_with_velocity(ch, 440.0, 127, patch);

    let mut warmup = vec![0.0f32; 100];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    // RR=200のリリースで1000サンプル分減衰させる（env_level: 1.0 → 約0.72）
    let mut release_buf = vec![0.0f32; 1000];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[900..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    // チョーク。新ボイスはアタック最遅(ar=0)にして、観測ウィンドウ内ではほぼ無音に保つ。
    let mut slow_attack = sustained_release_patch();
    for op in slow_attack.operators.iter_mut() {
        op.ar = 0;
    }
    engine.note_on_with_velocity(ch, 440.0, 127, slow_attack);

    let mut after = vec![0.0f32; 2000];
    engine.render(&mut after, 1);
    // デクリック開始直後：旧ボイスはまだ生きている（瞬間カットされていない）
    let early_peak = after[0..50].iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    // デクリック期間（≒176サンプル）を十分過ぎた後：旧ボイスは消え、新ボイスはまだほぼ無音
    let late_peak = after[400..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(
        early_peak > release_peak * 0.5,
        "choke should fade the old voice (not hard-cut it): early_peak={early_peak}, release_peak={release_peak}"
    );
    assert!(
        late_peak < release_peak * 0.5,
        "the choked voice should fade out within the declick window: late_peak={late_peak}, release_peak={release_peak}"
    );
}

/// SoundEngine::note_onはカレントパッチを使って同じチャンネルIDで同音チョークし、
/// 新ボイスがAttackから立ち上がってフルレベルへ向かう（チョークされた旧リリースは
/// 数msで消えるため、次の発音にかぶらない）。
#[test]
fn trait_note_on_chokes_release_and_restarts_attack() {
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

    let mut after = vec![0.0f32; 500];
    engine.render(&mut after, 1);
    // デクリック期間を過ぎた後半は、Attackで立ち上がった新ボイスがリリースレベルを超える
    let after_peak = after[300..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(
        after_peak > release_peak,
        "note_on should restart Attack toward full level: after_peak={after_peak}, release_peak={release_peak}"
    );
}
