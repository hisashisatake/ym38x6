//! psr2x6 — PSR-70（OPQ/YM3806）音色データを ym38x6 の `.38x6` プリセットバンクへ変換するツール。
//!
//! 使い方:
//! ```text
//! psr2x6 <input def_seqs.h> <output_dir> [--bank <N>]
//! ```
//! - 入力・出力ファイルは本クレートには同梱しない（パスは引数指定）。
//! - `--bank` の既定は `WAVEFORM_MEMORY_BANK + 1`（Bank 0はML自動生成用に空けておく）。
//! - 出力は `<output_dir>/b<bank>.38x6`（128件超は連番バンクへ分割）。
//!
//! 実装状況: 変換ロジック（[`conv`]）とパイプラインは実装済み・テスト済み。
//! `def_seqs.h` のパース（[`parse_def_seqs`]）のみ、工程0（実データ入手・構造確定）待ちのスタブ。

mod conv;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use conv::{bank_of, preset_count, voices_to_preset_files, NamedVoice};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("psr2x6: {msg}");
            eprintln!("usage: psr2x6 <input def_seqs.h> <output_dir> [--bank <N>]");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (input, output_dir, start_bank) = parse_args(args)?;

    let voices = parse_def_seqs(&input)?;
    if voices.is_empty() {
        return Err("変換対象のボイスが0件でした".to_string());
    }

    let files = voices_to_preset_files(start_bank, &voices);
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("出力ディレクトリ作成に失敗: {}: {e}", output_dir.display()))?;

    for file in &files {
        let path = output_dir.join(format!("b{}.38x6", bank_of(file)));
        let json = file
            .to_json()
            .map_err(|e| format!("JSONシリアライズに失敗: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("書き込みに失敗: {}: {e}", path.display()))?;
        println!("書き出し: {} ({} 音色)", path.display(), preset_count(file));
    }
    println!("完了: {} バンク / {} 音色", files.len(), voices.len());
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf, u16), String> {
    let mut positional: Vec<&String> = Vec::new();
    // 既定: 波形メモリ音源バンクの直後(WAVEFORM_MEMORY_BANK+1)。Bank 0はML自動生成用に空けておく
    let mut start_bank: u16 = ym38x6_core::WAVEFORM_MEMORY_BANK + 1;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--bank" => {
                let v = args.get(i + 1).ok_or("--bank に値がありません")?;
                start_bank = v.parse().map_err(|_| format!("--bank の値が不正: {v}"))?;
                i += 2;
            }
            _ => {
                positional.push(&args[i]);
                i += 1;
            }
        }
    }
    if positional.len() != 2 {
        return Err("入力ファイルと出力ディレクトリの2引数が必要です".to_string());
    }
    Ok((
        PathBuf::from(positional[0]),
        PathBuf::from(positional[1]),
        start_bank,
    ))
}

/// def_seqs.h をパースしてボイス列を返す。
///
/// **未実装（工程0待ち）**: JKN0/PSR70-reverse の `def_seqs.h` を実際に入手し、
/// 「450エントリ」がボイス音色かシーケンス/デモデータかを判定し、OPQボイスレジスタの
/// バイト/ビット配置を確定してから実装する。確定後はここで `OpqVoice` を組み立て、
/// 変換は [`conv`] に委譲する（この関数以外は変更不要）。
fn parse_def_seqs(_path: &Path) -> Result<Vec<NamedVoice>, String> {
    Err("def_seqs.h パーサーは未実装です（工程0: 実データ入手・フォーマット確定後に実装）".to_string())
}
