# CLAUDE.md

このファイルはClaude Codeがym38x6リポジトリで作業する際のガイドです。
設計の詳細は [spec.md](spec.md) を参照。

## 設計経緯
詳細な議論の経緯は `docs/session_history.txt` を参照。
なぜOPQベースなのか、各パラメーターの判断理由などが含まれる。

---

## 環境

### git

```powershell
$git = "C:\Users\satake\AppData\Local\Fork\gitInstance\2.50.1\cmd\git.exe"
```

PowerShellのPATHにgitが含まれていないため、上記フルパスで呼び出す。

### Rust

```powershell
cargo --version  # rustupでインストール済み前提
```

---

## プロジェクト概要

架空FM音源「38x6」と、それを使った作曲支援アプリ（Tauri）のワークスペース。

- **38x6**: YM3806(OPQ)ベース + OPZ系波形拡張の架空FM音源
- **WMS-1**: フェーズ1で使う波形メモリ音源（38x6の1op相当、プロトタイプ）
- **作曲支援アプリ**: グリッドなし・キャリブレーションベースのジェスチャーUIで、知識がなくても良い感じのコードが弾けることを目指す

---

## クレート構成

```
ym38x6/
  Cargo.toml           ← ワークスペース
  spec.md              ← 設計仕様書
  sound-core/          ← WaveTable・AdsrParams・SoundEngineトレイト（基盤ライブラリ）
  wms1-core/           ← WMS-1エンジン実装（sound-coreに依存）
  wms1-vst/            ← WMS-1 VST3/CLAPプラグイン（nih-plug）
  gesture-app/         ← 作曲支援Tauriアプリ
    src-tauri/         ← Rustバックエンド（cpalで音声出力）
    src/               ← フロントエンド
```

`sound-core` と `wms1-core` はnih-plugにもTauriにも依存しない純粋なRustライブラリ。
音源エンジンの変更はこの2クレートに閉じる。

---

## コマンド

### ビルド・チェック

```powershell
# ワークスペース全体のコンパイルチェック
cargo check --workspace --message-format=short

# コアライブラリのみ
cargo check -p sound-core -p wms1-core --message-format=short

# テスト
cargo test -p sound-core
cargo test -p wms1-core
```

### アプリ起動（フェーズ1以降、Tauri設定後）

```powershell
cd gesture-app
npm run tauri dev
```

### ビルド

```powershell
cd gesture-app
npm run tauri build
```

---

## アーキテクチャ

### 音源レイヤー（sound-core / wms1-core）

```
sound-core（基盤）
  WaveTable（1024×u16 log符号化）
  AdsrParams
  SoundEngineトレイト

wms1-core（WMS-1実装）
  Wms1Engine：波形オシレーター + ADSRエンベロープ + チャンネル管理（無制限）
  波形変換：32サンプルi8入力 → 1024サンプル対数フォーマット

38x6エンジン（フェーズ2以降）
  4opFM合成
  WMS-1と同一の波形フォーマット（移行コストなし）
```

コアは「この周波数でキーオン」「このパラメーターで発音」のAPIのみを提供する。
MIDI・ジェスチャー解釈・UIはコアの外側で行う。

### 音声出力（gesture-app/src-tauri）

cpalでWASAPIに直接出力。オーディオスレッドのコールバックでym38x6-coreを呼ぶ。

```rust
// コールバックイメージ
stream = device.build_output_stream(&config, move |output: &mut [f32], _| {
    engine.render(output);
}, ...);
```

### ジェスチャーUI

- キャリブレーションベース（C-F-Gの3点で座標系を定義）
- グリッドなし
- マウス版: 縦軸=ルート音、横軸=コード種類
- タッチ版（フェーズ5）: 指の間隔=インターバル、指の移動=ルート音シフト
- ∞ジェスチャー: 軌跡がそのままF-Numberに追従（ビブラート・装飾音）

---

## 開発方針

- `sound-core` と `wms1-core` は常にnih-plug・Tauri・cpalに無依存を保つ
- 波形フォーマットはWMS-1/38x6で共通（1024×uint16_t対数）。変換パイプラインはコアに実装
- フェーズ1の目的はジェスチャーUIとコード判定ロジックの検証。音色品質は後回しでよい
- パラメーターは全て0〜255（8bit）統一。周波数（オクターブ3bit + F-Number 13bit = 16bit、常にOP単位×4）のみ例外
