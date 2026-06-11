#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use wms1_core::{AdsrParams, ChorusType, LfoDestination, LfoWaveform, MasterEffects, ReverbType,
    SoundEngine, Wms1Engine, pitch_depth_cents, volume_depth};

#[tauri::command]
fn note_on(
    engine: tauri::State<'_, Arc<Mutex<Wms1Engine>>>,
    wave_slot: u8,
    frequency: f32,
) -> usize {
    engine.lock().unwrap().note_on(wave_slot, frequency, AdsrParams::default())
}

#[tauri::command]
fn note_off(engine: tauri::State<'_, Arc<Mutex<Wms1Engine>>>, channel: usize) {
    engine.lock().unwrap().note_off(channel);
}

/// パフォーマンスLFOを設定する。
/// `waveform`: 0=Triangle / 1=Sine / 2=Square / 3=S&H（Performance LFO Waveform enum準拠）
/// `destination`: 0=Pitch（ビブラート） / 1=Volume（トレモロ）
/// `cc77`/`cc1`/`mod_depth_range`は仕様の実効Depth計算式（CC77/CC1/RPN0,5）への入力
#[tauri::command]
fn set_performance_lfo(
    engine: tauri::State<'_, Arc<Mutex<Wms1Engine>>>,
    channel: usize,
    rate: u8,
    delay: u8,
    waveform: u8,
    destination: u8,
    cc77: u8,
    cc1: u8,
    mod_depth_range: u8,
) {
    let waveform = match waveform {
        1 => LfoWaveform::Sine,
        2 => LfoWaveform::Square,
        3 => LfoWaveform::SampleHold,
        _ => LfoWaveform::Triangle,
    };
    let destination = if destination == 1 { LfoDestination::Volume } else { LfoDestination::Pitch };
    let depth = match destination {
        LfoDestination::Pitch => pitch_depth_cents(cc77, cc1, mod_depth_range),
        LfoDestination::Volume => volume_depth(cc77, cc1),
    };
    engine.lock().unwrap().set_performance_lfo(channel, rate, delay, waveform, destination, depth);
}

/// マスターエフェクト（Reverb/Chorus）を設定する。
/// `reverb_type`/`chorus_type`は0〜7（spec.md マスターエフェクトセクションのenum参照）
#[tauri::command]
fn set_master_effects(
    effects: tauri::State<'_, Arc<Mutex<MasterEffects>>>,
    reverb_send: u8,
    reverb_type: u8,
    reverb_time: u8,
    chorus_send: u8,
    chorus_type: u8,
    chorus_mod_rate: u8,
    chorus_mod_depth: u8,
    chorus_feedback: u8,
    chorus_send_to_reverb: u8,
) {
    let mut fx = effects.lock().unwrap();
    fx.set_reverb_send(reverb_send);
    fx.set_reverb_type(ReverbType::from_u8(reverb_type));
    fx.set_reverb_time(reverb_time);
    fx.set_chorus_send(chorus_send);
    fx.set_chorus_type(ChorusType::from_u8(chorus_type));
    fx.set_chorus_mod_rate(chorus_mod_rate);
    fx.set_chorus_mod_depth(chorus_mod_depth);
    fx.set_chorus_feedback(chorus_feedback);
    fx.set_chorus_send_to_reverb(chorus_send_to_reverb);
}

fn main() {
    let settings = settings::Settings::load();
    // ステップ5でエンジン切り替えを実装。現時点は wms1 固定
    let _ = &settings.engine_type;

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let supported = device
        .default_output_config()
        .expect("no default output config");

    let num_channels = supported.channels() as usize;
    let sample_rate = supported.sample_rate().0 as f32;
    let stream_config: cpal::StreamConfig = supported.into();

    let engine = Arc::new(Mutex::new(Wms1Engine::new(sample_rate)));
    let engine_audio = Arc::clone(&engine);
    let effects = Arc::new(Mutex::new(MasterEffects::new(sample_rate)));
    let effects_audio = Arc::clone(&effects);

    let stream = device
        .build_output_stream::<f32, _, _>(
            &stream_config,
            move |output: &mut [f32], _| {
                output.fill(0.0);
                if let Ok(mut eng) = engine_audio.try_lock() {
                    eng.render(output, num_channels);
                }
                if let Ok(mut fx) = effects_audio.try_lock() {
                    fx.process(output, num_channels);
                }
            },
            |err| eprintln!("audio error: {err}"),
            None,
        )
        .expect("failed to build output stream");

    stream.play().expect("failed to start audio stream");

    tauri::Builder::default()
        .manage(engine)
        .manage(effects)
        .invoke_handler(tauri::generate_handler![note_on, note_off, set_performance_lfo, set_master_effects])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    drop(stream);
}
