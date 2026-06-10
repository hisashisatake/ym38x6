use wms1_core::{AdsrParams, LfoDestination, LfoWaveform, SoundEngine, Wms1Engine};

/// Destination=Volume・Square波・Depth=1.0のとき、sustain=1.0の音は
/// LFOが-1.0を出力する半周期の間、実効音量が0になり完全に無音化する。
#[test]
fn volume_destination_modulates_amplitude_with_square_lfo() {
    let sample_rate = 160.0; // rate=255 → 20Hz、1周期=8サンプル
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 0 };

    let mut engine = Wms1Engine::new(sample_rate);
    let ch = engine.note_on(1, 1.0, adsr); // wave_slot=1: 矩形波（振幅は常に±1）
    engine.set_performance_lfo(ch, 255, 0, LfoWaveform::Square, LfoDestination::Volume, 1.0);

    // attackを終えてsustain(=1.0)に到達するまでウォームアップ
    let mut warmup = vec![0.0f32; 60];
    engine.render(&mut warmup, 1);

    let mut buf = vec![0.0f32; 16]; // LFO 2周期分
    engine.render(&mut buf, 1);

    let zero_count = buf.iter().filter(|&&s| s == 0.0).count();
    let nonzero_count = buf.iter().filter(|&&s| s != 0.0).count();
    assert_eq!(zero_count, 8, "LFOが-1.0の半周期は無音になるはず: {buf:?}");
    assert_eq!(nonzero_count, 8, "LFOが+1.0の半周期は元の振幅のままのはず: {buf:?}");
}

/// Destination=Pitch・Depth>0のとき、オシレーターの実効周波数が変化し、
/// Depth=0の場合と出力波形が乖離する。
#[test]
fn pitch_destination_shifts_oscillator_frequency() {
    let sample_rate = 44100.0;
    let adsr = AdsrParams { attack: 255, decay: 0, sustain: 255, release: 0 };

    let mut engine_no_depth = Wms1Engine::new(sample_rate);
    let ch0 = engine_no_depth.note_on(0, 440.0, adsr); // sine
    engine_no_depth.set_performance_lfo(ch0, 255, 0, LfoWaveform::Sine, LfoDestination::Pitch, 0.0);
    let mut buf_no_depth = vec![0.0f32; 200];
    engine_no_depth.render(&mut buf_no_depth, 1);

    let mut engine_with_depth = Wms1Engine::new(sample_rate);
    let ch1 = engine_with_depth.note_on(0, 440.0, adsr);
    engine_with_depth.set_performance_lfo(ch1, 255, 0, LfoWaveform::Sine, LfoDestination::Pitch, 1200.0); // ±1オクターブ
    let mut buf_with_depth = vec![0.0f32; 200];
    engine_with_depth.render(&mut buf_with_depth, 1);

    let differs = buf_no_depth.iter().zip(buf_with_depth.iter())
        .any(|(a, b)| (a - b).abs() > 1e-4);
    assert!(differs, "ピッチ変調により出力波形が変化するはず");
}
