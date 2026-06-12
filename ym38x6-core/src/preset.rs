use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::operator::OperatorParams;
use crate::Ym38x6Patch;

/// Bank Select（CC0×128+CC32）とProgram Change（0〜127）から決定的にパッチを生成する
/// 暫定プレースホルダー。GM2準拠のBank0音色はym38x6-ml（フェーズ5、インバース合成）で、
/// Bank1以降のユーザープリセットはプリセットライブラリ（フェーズ5）で生成・管理する予定。
/// 実データができるまでの間、Bank/Programの値域を一通り確認できるダミーパッチを返す
/// （bank/programの値はseedとして使うのみで、bankによる音色の区別は未実装）。
pub fn placeholder_patch(bank: u16, program: u8) -> Ym38x6Patch {
    let seed = program.wrapping_add(bank as u8);

    let mut patch = Ym38x6Patch::default();
    patch.channel.algorithm = seed % 8;
    patch.channel.feedback = seed.wrapping_mul(2);
    patch.channel.filter_cutoff = 255;
    patch.channel.filter_self_oscillation = true;

    // tests::loud_patchと同じ「即音量最大・サスティン無限」の基本設定（聴感確認用）
    let base = OperatorParams {
        tl: 255,
        ar: 255,
        d1r: 0,
        d2r: 0,
        d1l: 255,
        rr: 255,
        mul: 1,
        dt1: 128,
        ksr: 0,
        am_enable: false,
        velocity_sensitivity: 0,
        waveform: 0,
    };
    for (i, op) in patch.operators.iter_mut().enumerate() {
        *op = OperatorParams { waveform: seed.wrapping_add(i as u8) % 8, ..base };
    }
    patch
}

/// GM2 Bank0の一部音色について、手動チューニングしたパッチを返す。
/// ym38x6-mlによるML自動生成（フェーズ5「音色作成方針」）が完成するまでの間、
/// 動作確認用に少数の代表音色のみ手動で作成したもの。該当しないprogram番号は
/// Noneを返し、呼び出し側で`placeholder_patch`へフォールバックする。
pub fn gm2_bank0_patch(program: u8) -> Option<Ym38x6Patch> {
    match program {
        0 => Some(acoustic_grand_piano_patch()),
        4 => Some(electric_piano_1_patch()),
        80 => Some(lead_1_square_patch()),
        _ => None,
    }
}

/// Program 0: Acoustic Grand Piano。
/// Algorithm 4（(O1→O2)+(O3→O4)）で2つの倍音グループを構成し、片方をわずかに
/// デチューンしてコーラス感を出す。モジュレーター(O1/O3)はキャリアより速く減衰させ、
/// 打鍵直後だけ倍音が立つハンマーアタックを表現する。
fn acoustic_grand_piano_patch() -> Ym38x6Patch {
    let mut patch = Ym38x6Patch::default();
    patch.channel.algorithm = 4;
    patch.channel.feedback = 40;

    // O1: ペア1のモジュレーター（ハンマーアタックの倍音、速い減衰）
    patch.operators[0] = OperatorParams {
        tl: 200,
        ar: 255,
        d1r: 180,
        d2r: 80,
        d1l: 100,
        rr: 180,
        mul: 1,
        dt1: 128,
        ksr: 100,
        am_enable: false,
        velocity_sensitivity: 80,
        waveform: 0,
    };
    // O2: ペア1のキャリア（基音、緩やかな自然減衰）
    patch.operators[1] = OperatorParams {
        tl: 255,
        ar: 255,
        d1r: 60,
        d2r: 30,
        d1l: 200,
        rr: 120,
        mul: 1,
        dt1: 128,
        ksr: 120,
        am_enable: false,
        velocity_sensitivity: 40,
        waveform: 0,
    };
    // O3: ペア2のモジュレーター（高次倍音、わずかに高めデチューン）
    patch.operators[2] = OperatorParams {
        tl: 150,
        ar: 255,
        d1r: 200,
        d2r: 100,
        d1l: 60,
        rr: 200,
        mul: 2,
        dt1: 138,
        ksr: 100,
        am_enable: false,
        velocity_sensitivity: 100,
        waveform: 0,
    };
    // O4: ペア2のキャリア（基音、わずかに低めデチューンでコーラス感）
    patch.operators[3] = OperatorParams {
        tl: 220,
        ar: 255,
        d1r: 70,
        d2r: 35,
        d1l: 180,
        rr: 130,
        mul: 1,
        dt1: 118,
        ksr: 120,
        am_enable: false,
        velocity_sensitivity: 30,
        waveform: 0,
    };
    patch
}

/// Program 4: Electric Piano 1。
/// Algorithm 4（(O1→O2)+(O3→O4)）で、O1→O2をベル成分（高次倍音MUL=14 + 強いフィードバック
/// でメタリックな質感）、O3→O4をメインのトーン成分とする、DX7系E.PIANOの定番構成。
fn electric_piano_1_patch() -> Ym38x6Patch {
    let mut patch = Ym38x6Patch::default();
    patch.channel.algorithm = 4;
    patch.channel.feedback = 180;

    // O1: ベル成分のモジュレーター（高次倍音、フィードバック対象）
    patch.operators[0] = OperatorParams {
        tl: 230,
        ar: 255,
        d1r: 220,
        d2r: 150,
        d1l: 40,
        rr: 200,
        mul: 14,
        dt1: 128,
        ksr: 80,
        am_enable: false,
        velocity_sensitivity: 120,
        waveform: 0,
    };
    // O2: ベル成分のキャリア
    patch.operators[1] = OperatorParams {
        tl: 160,
        ar: 255,
        d1r: 200,
        d2r: 120,
        d1l: 30,
        rr: 180,
        mul: 1,
        dt1: 128,
        ksr: 100,
        am_enable: false,
        velocity_sensitivity: 60,
        waveform: 0,
    };
    // O3: メイン音のモジュレーター
    patch.operators[2] = OperatorParams {
        tl: 180,
        ar: 255,
        d1r: 100,
        d2r: 50,
        d1l: 150,
        rr: 150,
        mul: 1,
        dt1: 128,
        ksr: 90,
        am_enable: false,
        velocity_sensitivity: 50,
        waveform: 0,
    };
    // O4: メイン音のキャリア
    patch.operators[3] = OperatorParams {
        tl: 255,
        ar: 255,
        d1r: 50,
        d2r: 25,
        d1l: 220,
        rr: 120,
        mul: 1,
        dt1: 128,
        ksr: 110,
        am_enable: false,
        velocity_sensitivity: 20,
        waveform: 0,
    };
    patch
}

/// Program 80: Lead 1 (Square)。
/// Algorithm 7（全並列）で矩形波(waveform=3)を3本デチューンして重ね、O4を1オクターブ上の
/// 矩形波で薄く重ねる、デチューンユニゾン構成のシンセリード。d1l=255 + d2r=0でキーオン中は
/// 減衰しない（無限サスティン）。フィルターでわずかに角を取りつつ共振を効かせる。
fn lead_1_square_patch() -> Ym38x6Patch {
    let mut patch = Ym38x6Patch::default();
    patch.channel.algorithm = 7;
    patch.channel.feedback = 0;
    patch.channel.filter_cutoff = 180;
    patch.channel.filter_resonance = 60;

    let base = OperatorParams {
        tl: 200,
        ar: 255,
        d1r: 0,
        d2r: 0,
        d1l: 255,
        rr: 80,
        mul: 1,
        dt1: 128,
        ksr: 0,
        am_enable: false,
        velocity_sensitivity: 40,
        waveform: 3,
    };
    patch.operators[0] = OperatorParams { tl: 255, ..base };
    patch.operators[1] = OperatorParams { dt1: 138, ..base };
    patch.operators[2] = OperatorParams { dt1: 118, ..base };
    patch.operators[3] =
        OperatorParams { mul: 2, tl: 140, velocity_sensitivity: 20, ..base };
    patch
}

/// 1音色分のプリセット（名前 + パッチ本体）。`.38x6`拡張子のJSONファイルとして
/// 保存・読み込みする。現状は`Ym38x6Patch`のフィールドのみを保存する
/// （拡張波形スロット(8〜255)データの埋め込みは、当該機能実装後にフォーマット拡張で対応）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub patch: Ym38x6Patch,
}

impl Preset {
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

/// ディレクトリから読み込んだ`.38x6`プリセット集合。ファイル名`b<bank>_p<program>.38x6`から
/// バンク/プログラム番号を読み取り、Bank Select + Program Changeでのルックアップに使う
/// （命名規則は暫定。本格的なプリセット管理UIができたら見直す）。
#[derive(Clone, Debug, Default)]
pub struct PresetBank {
    presets: HashMap<(u16, u8), Preset>,
}

impl PresetBank {
    /// 指定ディレクトリ内の`b<bank>_p<program>.38x6`ファイルを読み込む。
    /// ディレクトリが存在しない・読めない場合は空の集合を返す
    /// （ユーザープリセット未作成時はplaceholder_patchへフォールバックするため、エラーにしない）。
    pub fn load_from_dir(dir: &Path) -> Self {
        let mut presets = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Self { presets };
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("38x6") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let Some((bank, program)) = parse_bank_program(stem) else { continue };
            let Ok(json) = std::fs::read_to_string(&path) else { continue };
            let Ok(preset) = Preset::from_json(&json) else { continue };
            presets.insert((bank, program), preset);
        }
        Self { presets }
    }

    pub fn get(&self, bank: u16, program: u8) -> Option<&Preset> {
        self.presets.get(&(bank, program))
    }
}

/// `b<bank>_p<program>`形式のファイル名（拡張子除く）からbank/program番号を取り出す。
fn parse_bank_program(stem: &str) -> Option<(u16, u8)> {
    let rest = stem.strip_prefix('b')?;
    let (bank_str, program_str) = rest.split_once("_p")?;
    Some((bank_str.parse().ok()?, program_str.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SoundEngine, Ym38x6Engine};

    #[test]
    fn algorithm_and_waveform_stay_in_range_for_all_programs() {
        for bank in [0u16, 1, 128] {
            for program in 0..=255u8 {
                let patch = placeholder_patch(bank, program);
                assert!(patch.channel.algorithm < 8);
                for op in patch.operators {
                    assert!(op.waveform < 8);
                }
            }
        }
    }

    #[test]
    fn placeholder_patch_is_audible() {
        for (bank, program) in [(0u16, 0u8), (0, 64), (1, 42), (128, 127)] {
            let mut engine = Ym38x6Engine::new(44100.0);
            let ch = engine.note_on_with_velocity(440.0, 127, placeholder_patch(bank, program));
            let mut buf = vec![0.0f32; 512];
            engine.render(&mut buf, 1);
            assert!(buf.iter().all(|&s| s.is_finite()));
            assert!(buf.iter().any(|&s| s != 0.0), "bank={bank} program={program} is silent");
            let _ = ch;
        }
    }

    #[test]
    fn gm2_bank0_patch_returns_some_for_implemented_programs_only() {
        assert!(gm2_bank0_patch(0).is_some());
        assert!(gm2_bank0_patch(4).is_some());
        assert!(gm2_bank0_patch(80).is_some());
        assert!(gm2_bank0_patch(1).is_none());
        assert!(gm2_bank0_patch(127).is_none());
    }

    #[test]
    fn gm2_bank0_patch_is_audible_and_finite() {
        for program in [0u8, 4, 80] {
            let mut engine = Ym38x6Engine::new(44100.0);
            let patch = gm2_bank0_patch(program).expect("implemented program");
            engine.note_on_with_velocity(440.0, 100, patch);

            let mut buf = vec![0.0f32; 44100];
            engine.render(&mut buf, 1);
            assert!(buf.iter().all(|&s| s.is_finite()), "program {program}: non-finite sample");
            assert!(buf.iter().any(|&s| s != 0.0), "program {program} is silent");
        }
    }

    #[test]
    fn preset_json_round_trip() {
        let preset = Preset { name: "Test Patch".to_string(), patch: placeholder_patch(1, 42) };
        let json = preset.to_json().expect("serialize");
        let loaded = Preset::from_json(&json).expect("deserialize");
        assert_eq!(loaded.name, preset.name);
        assert_eq!(loaded.patch, preset.patch);
    }

    #[test]
    fn parse_bank_program_valid_and_invalid() {
        assert_eq!(parse_bank_program("b1_p64"), Some((1, 64)));
        assert_eq!(parse_bank_program("b0_p0"), Some((0, 0)));
        assert_eq!(parse_bank_program("invalid"), None);
        assert_eq!(parse_bank_program("b1"), None);
        assert_eq!(parse_bank_program("b1_p300"), None); // programはu8範囲外
        assert_eq!(parse_bank_program("bx_p64"), None);
    }

    #[test]
    fn preset_bank_load_from_dir_reads_matching_files_and_ignores_others() {
        let dir = std::env::temp_dir().join(format!("ym38x6_preset_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let preset = Preset { name: "My Preset".to_string(), patch: placeholder_patch(1, 64) };
        std::fs::write(dir.join("b1_p64.38x6"), preset.to_json().unwrap()).unwrap();
        std::fs::write(dir.join("ignore.txt"), "not a preset").unwrap();

        let bank = PresetBank::load_from_dir(&dir);
        std::fs::remove_dir_all(&dir).expect("cleanup temp dir");

        let loaded = bank.get(1, 64).expect("b1_p64 should be loaded");
        assert_eq!(loaded.name, "My Preset");
        assert_eq!(loaded.patch, preset.patch);
        assert!(bank.get(0, 0).is_none());
    }

    #[test]
    fn preset_bank_load_from_dir_missing_dir_returns_empty() {
        let dir = std::env::temp_dir().join("ym38x6_preset_test_nonexistent_dir");
        let bank = PresetBank::load_from_dir(&dir);
        assert!(bank.get(0, 0).is_none());
    }
}
