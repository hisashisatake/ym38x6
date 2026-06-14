#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;
mod ym38x6_dto;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use wms1_core::{AdsrParams, ChorusType, LfoDestination, LfoWaveform, MasterEffects, ReverbType,
    SoundEngine, Wms1Engine, pitch_depth_cents, volume_depth};
use ym38x6_core::{presets_dir, PresetBank, Ym38x6Engine, Ym38x6LfoDestination};
use ym38x6_dto::Ym38x6PatchDto;

/// 起動時に選択されたエンジンの実体を保持する（settings.engine_typeで切り替え）。
/// `render`は両エンジン共通だが、note_on等の音色設定APIはエンジンごとに異なる形で
/// 公開するため（CLAUDE.md方針）、各Tauriコマンドはmatchで対応するエンジンに振り分ける。
enum EngineHandle {
    Wms1(Wms1Engine),
    Ym38x6(Ym38x6Engine),
}

impl EngineHandle {
    fn render(&mut self, output: &mut [f32], num_channels: usize) {
        match self {
            EngineHandle::Wms1(e) => e.render(output, num_channels),
            EngineHandle::Ym38x6(e) => e.render(output, num_channels),
        }
    }
}

/// フロントエンドが起動時にどちらのコマンド群（note_on系/ym38x6_note_on系）を
/// 使うべきかを判定するために呼ぶ。"wms1" / "ym38x6"を返す。
#[tauri::command]
fn engine_type(engine_type: tauri::State<'_, String>) -> String {
    (*engine_type).clone()
}

/// 指定チャンネルIDへキーオンする。チャンネルIDは呼び出し側（フロントエンド）が
/// 安定したスロット番号として供給する。発音中/リリース中のチャンネルが既にあっても、
/// エンベロープを即座にカットしてAttackから再開する（実機Key-On挙動に準拠＝同音チョーク）。
/// 押し直し時に同じスロットIDを渡すことで、直前のリリーステールがチョークされる。
#[tauri::command]
fn note_on(
    engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>,
    channel: usize,
    wave_slot: u8,
    frequency: f32,
) {
    match &mut *engine.lock().unwrap() {
        EngineHandle::Wms1(e) => e.note_on(channel, wave_slot, frequency, AdsrParams::default()),
        // ym38x6_set_program/ym38x6_set_patchで設定したcurrent_patchで発音する
        EngineHandle::Ym38x6(e) => e.note_on(channel, wave_slot, frequency, AdsrParams::default()),
    }
}

#[tauri::command]
fn note_off(engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>, channel: usize) {
    match &mut *engine.lock().unwrap() {
        EngineHandle::Wms1(e) => e.note_off(channel),
        EngineHandle::Ym38x6(e) => e.note_off(channel),
    }
}

/// パフォーマンスLFOを設定する。
/// `waveform`: 0=Triangle / 1=Sine / 2=Square / 3=S&H（Performance LFO Waveform enum準拠）
/// `destination`: 0=Pitch（ビブラート） / 1=Volume（トレモロ）
/// `cc77`/`cc1`/`mod_depth_range`は仕様の実効Depth計算式（CC77/CC1/RPN0,5）への入力
#[tauri::command]
fn set_performance_lfo(
    engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>,
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
    match &mut *engine.lock().unwrap() {
        EngineHandle::Wms1(e) => e.set_performance_lfo(channel, rate, delay, waveform, destination, depth),
        EngineHandle::Ym38x6(_) => {}
    }
}

/// 38x6エンジンで指定チャンネルIDへNote-Onする。`patch`は4オペレーター分のパラメーターと
/// チャンネルパラメーター一式。チャンネルIDの扱い（同音チョーク）は`note_on`と同じ。
#[tauri::command]
fn ym38x6_note_on(
    engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>,
    channel: usize,
    frequency: f32,
    velocity: u8,
    patch: Ym38x6PatchDto,
) {
    match &mut *engine.lock().unwrap() {
        EngineHandle::Ym38x6(e) => e.note_on_with_velocity(channel, frequency, velocity, patch.into()),
        EngineHandle::Wms1(_) => {}
    }
}

#[tauri::command]
fn ym38x6_note_off(engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>, channel: usize) {
    match &mut *engine.lock().unwrap() {
        EngineHandle::Ym38x6(e) => e.note_off(channel),
        EngineHandle::Wms1(_) => {}
    }
}

/// 以降のNote-Onで使われるカレントパッチを設定する。
#[tauri::command]
fn ym38x6_set_patch(engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>, patch: Ym38x6PatchDto) {
    match &mut *engine.lock().unwrap() {
        EngineHandle::Ym38x6(e) => e.set_patch(patch.into()),
        EngineHandle::Wms1(_) => {}
    }
}

/// (bank, program)に対応するプリセットへ切り替える。ym38x6-vstのProgramパラメーターと
/// 同じ`PresetBank::patch_for_program`を使うため、音はVSTと完全に同一になる。
#[tauri::command]
fn ym38x6_set_program(
    engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>,
    preset_bank: tauri::State<'_, PresetBank>,
    bank: u16,
    program: u8,
) {
    let patch = preset_bank.patch_for_program(bank, program);
    match &mut *engine.lock().unwrap() {
        EngineHandle::Ym38x6(e) => e.set_patch(patch),
        EngineHandle::Wms1(_) => {}
    }
}

/// 38x6エンジンのパフォーマンスLFOを設定する。
/// `destination`: 0=Pitch（ビブラート） / 1=Volume（トレモロ） / 2=TL（キャリア一括、38x6拡張）
/// その他の引数は`set_performance_lfo`と同様。
#[tauri::command]
fn ym38x6_set_performance_lfo(
    engine: tauri::State<'_, Arc<Mutex<EngineHandle>>>,
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
    let destination = match destination {
        1 => Ym38x6LfoDestination::Volume,
        2 => Ym38x6LfoDestination::TlCarrier,
        _ => Ym38x6LfoDestination::Pitch,
    };
    let depth = match destination {
        Ym38x6LfoDestination::Pitch => pitch_depth_cents(cc77, cc1, mod_depth_range),
        Ym38x6LfoDestination::Volume | Ym38x6LfoDestination::TlCarrier => volume_depth(cc77, cc1),
    };
    match &mut *engine.lock().unwrap() {
        EngineHandle::Ym38x6(e) => e.set_performance_lfo(channel, rate, delay, waveform, destination, depth),
        EngineHandle::Wms1(_) => {}
    }
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

    let engine_handle = if settings.engine_type == "ym38x6" {
        EngineHandle::Ym38x6(Ym38x6Engine::new(sample_rate))
    } else {
        EngineHandle::Wms1(Wms1Engine::new(sample_rate))
    };
    let engine = Arc::new(Mutex::new(engine_handle));
    let engine_audio = Arc::clone(&engine);
    let effects = Arc::new(Mutex::new(MasterEffects::new(sample_rate)));
    let effects_audio = Arc::clone(&effects);
    let preset_bank = PresetBank::load_from_dir(&presets_dir());

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
        .manage(settings.engine_type)
        .manage(preset_bank)
        .invoke_handler(tauri::generate_handler![
            engine_type,
            note_on,
            note_off,
            set_performance_lfo,
            set_master_effects,
            ym38x6_note_on,
            ym38x6_note_off,
            ym38x6_set_patch,
            ym38x6_set_program,
            ym38x6_set_performance_lfo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    drop(stream);
}
