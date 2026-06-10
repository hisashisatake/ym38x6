use sound_core::{LfoWaveform, PerformanceLfo};

/// rate=255は約20Hzにマッピングされる。sample_rate=160Hzにすると
/// 1周期がちょうど8サンプルになり、各位相での出力値を直接比較できる。
const SAMPLE_RATE: f32 = 160.0;

fn ticks(lfo: &mut PerformanceLfo, n: usize) -> Vec<f32> {
    (0..n).map(|_| lfo.tick(SAMPLE_RATE)).collect()
}

#[test]
fn triangle_lfo_shape() {
    let mut lfo = PerformanceLfo::new();
    lfo.set_rate(255);
    lfo.set_delay(0);
    lfo.set_waveform(LfoWaveform::Triangle);
    lfo.note_on();

    let v = ticks(&mut lfo, 8);
    assert!((v[1] - 0.0).abs() < 0.02, "phase 1/4: {}", v[1]);
    assert!((v[3] - 1.0).abs() < 0.02, "phase 1/2: {}", v[3]);
    assert!((v[5] - 0.0).abs() < 0.02, "phase 3/4: {}", v[5]);
    assert!((v[7] + 1.0).abs() < 0.02, "phase ~1: {}", v[7]);
}

#[test]
fn sine_lfo_shape() {
    let mut lfo = PerformanceLfo::new();
    lfo.set_rate(255);
    lfo.set_delay(0);
    lfo.set_waveform(LfoWaveform::Sine);
    lfo.note_on();

    let v = ticks(&mut lfo, 8);
    assert!((v[1] - 1.0).abs() < 0.02, "phase 1/4: {}", v[1]);
    assert!((v[3] - 0.0).abs() < 0.02, "phase 1/2: {}", v[3]);
    assert!((v[5] + 1.0).abs() < 0.02, "phase 3/4: {}", v[5]);
    assert!((v[7] - 0.0).abs() < 0.02, "phase ~1: {}", v[7]);
}

#[test]
fn square_lfo_shape() {
    let mut lfo = PerformanceLfo::new();
    lfo.set_rate(255);
    lfo.set_delay(0);
    lfo.set_waveform(LfoWaveform::Square);
    lfo.note_on();

    let v = ticks(&mut lfo, 8);
    assert_eq!(v[0], 1.0, "phase 1/8 should be +1");
    assert_eq!(v[4], -1.0, "phase 5/8 should be -1");
}

/// S&H波形は値域内に収まり、周期ごとに更新されるため毎サンプルは変化しない
/// （= 複数サンプルにわたって同じ値を保持する区間がある）。
#[test]
fn sample_hold_lfo_updates_and_stays_in_range() {
    let mut lfo = PerformanceLfo::new();
    lfo.set_rate(255);
    lfo.set_delay(0);
    lfo.set_waveform(LfoWaveform::SampleHold);
    lfo.note_on();

    let v = ticks(&mut lfo, 32); // 約4周期分

    for &x in &v {
        assert!((-1.0..=1.0).contains(&x), "S&H value out of range: {x}");
    }

    let distinct: std::collections::HashSet<_> = v.iter().map(|x| x.to_bits()).collect();
    assert!(distinct.len() >= 2, "S&H should produce more than one value over several cycles");
    assert!(distinct.len() < v.len(), "S&H should hold each value across multiple samples");
}

/// ディレイ経過前は常に0.0、経過後は変調値が出力される
#[test]
fn delay_gates_output_until_elapsed() {
    let mut lfo = PerformanceLfo::new();
    lfo.set_rate(255);
    lfo.set_delay(128); // delay_to_seconds(128) ≈ 5.02秒
    lfo.set_waveform(LfoWaveform::Triangle);
    lfo.note_on();

    let sample_rate = 1000.0;

    let before: Vec<f32> = (0..5000).map(|_| lfo.tick(sample_rate)).collect();
    assert!(before.iter().all(|&v| v == 0.0), "expected silence during delay");

    let after: Vec<f32> = (0..100).map(|_| lfo.tick(sample_rate)).collect();
    assert!(after.iter().any(|&v| v != 0.0), "expected non-zero modulation after delay");
}

/// note_onで位相とディレイ経過時間がリセットされる
#[test]
fn note_on_resets_delay() {
    let mut lfo = PerformanceLfo::new();
    lfo.set_rate(255);
    lfo.set_delay(128);
    lfo.set_waveform(LfoWaveform::Triangle);
    lfo.note_on();

    let sample_rate = 1000.0;
    for _ in 0..3000 {
        lfo.tick(sample_rate);
    }

    lfo.note_on();
    assert_eq!(lfo.tick(sample_rate), 0.0, "note_on直後はディレイが再びかかるはず");
}
