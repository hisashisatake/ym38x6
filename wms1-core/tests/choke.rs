use wms1_core::{AdsrParams, SoundEngine, Wms1Engine};

/// リリース中の同じチャンネルIDへ再度note_onすると、旧ボイスは即座に瞬間消滅せず、
/// 数ms（DECLICK_SECONDS=4ms≒176サンプル@44.1kHz）かけて線形にフェードアウトしてから消える
/// （クリックノイズ緩和）。新ボイスの発音タイミングは遅れない（旧ボイスに重ねてフェードするため）。
///
/// 旧ボイスのフェードを単独で観測するため、チョーク後の新ボイスはアタック最遅(attack=0)にして
/// 観測ウィンドウ内ではほぼ無音に保つ。
#[test]
fn choke_declicks_old_voice_instead_of_hard_cut() {
    let sample_rate = 44100.0;
    let sustained = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 200 };

    let mut engine = Wms1Engine::new(sample_rate);
    let ch = 0;
    engine.note_on(ch, 0, 440.0, sustained); // サイン波（サンプル間の段差が小さく連続性を観測しやすい）

    let mut warmup = vec![0.0f32; 60];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    // release=200で200サンプル分減衰させる
    let mut release_buf = vec![0.0f32; 200];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[150..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    // チョーク。新ボイスはアタック最遅(attack=0)で観測ウィンドウ内ではほぼ無音。
    let slow_attack = AdsrParams { attack: 0, decay: 0, sustain: 255, release: 0 };
    engine.note_on(ch, 0, 440.0, slow_attack);

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

/// 同じチャンネルIDへ再度note_onすると、新ボイスがAttackから立ち上がってフルレベルへ向かう
/// （チョークされた旧リリースは数msで消えるため、次の発音にかぶらない）。
#[test]
fn note_on_chokes_release_and_restarts_attack_on_same_channel() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 200 };

    let mut engine = Wms1Engine::new(sample_rate);
    let ch = 0;
    engine.note_on(ch, 0, 440.0, adsr);

    let mut warmup = vec![0.0f32; 60];
    engine.render(&mut warmup, 1);

    engine.note_off(ch);

    let mut release_buf = vec![0.0f32; 200];
    engine.render(&mut release_buf, 1);
    let release_peak = release_buf[150..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    // 同じチャンネルIDへ再度note_on → 旧リリースを数msでチョークし、Attackから再開する
    engine.note_on(ch, 0, 440.0, adsr);

    let mut after = vec![0.0f32; 500];
    engine.render(&mut after, 1);
    // デクリック期間を過ぎた後半は、Attackで立ち上がった新ボイスがリリースレベルを超える
    let after_peak = after[300..].iter().fold(0.0f32, |m, &s| m.max(s.abs()));

    assert!(
        after_peak > release_peak,
        "note_on should restart attack toward full level: after_peak={after_peak}, release_peak={release_peak}"
    );
}
