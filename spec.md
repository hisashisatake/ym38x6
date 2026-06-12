# 架空FM音源「38x6」設計仕様書

## 概要

- YAMAHAのYM3806(OPQ)をベースに、OPZ系の波形拡張を加えた架空のFM音源
- 梅本竜氏がSynthEdit+VOPMで構築したYM-2609（2008年）と同じ発想：
  「PCM音源へ移行する前に、FM音源があと一歩進化していたとしたら」
- ソフトウェア実装（Rust）なので制約なし
- 作曲支援アプリのエンジンとしての役割も持つ
- 作曲支援アプリはTauriで実装。まずWindowsデスクトップ版から開始

---

## 構成

本ドキュメントは設計仕様の全体像（実装ロードマップ・技術スタック・参照資料）を扱う。
詳細仕様は以下の文書に分割されている。

- [spec-sound.md](spec-sound.md)：38x6 / WMS-1音源エンジンの仕様（パラメーター・MIDI実装・OPQコンバーター等）
- [spec-app.md](spec-app.md)：作曲支援アプリのUI設計仕様

---

## 実装ロードマップ

```
フェーズ1: WMS-1とTauriデスクトップアプリの基盤（完了）
  → WMS-1（波形メモリ音源 + ADSR）をwms1-coreに実装
  → 内部波形フォーマット（1024サンプル対数）と変換パイプラインを実装
  → cpalで音声出力
  → マウスによる2Dジェスチャー入力UIの実装
  → キャリブレーション（C-F-G基準点）の実装

フェーズ2: パフォーマンスLFO・マスターエフェクト実装
  → PerformanceLfo / PerformanceLfoTarget をsound-coreに実装
  → MasterEffects（Reverb/Chorus）をsound-coreに実装

フェーズ3: 38x6 FMエンジン導入、波形選択・デチューン拡張（完了）
  → OPZ系の音色表現を取り込む

フェーズ4: OP単位F-Number・独立キーオンを実装
  → OPQ由来の音楽的表現を一般化して活用

フェーズ5: パラメーターUI・音色保存・プリセットライブラリ・GM2 Bank0
  → ym38x6-ml: 目標音声 → FMパラメーター逆算（インバース合成）
  → 38x6エンジンのPythonバインディング（PyO3 + maturin）
  → ランダムサンプリングによる合成データ生成・学習
  → GM2プログラムマップ準拠のBank 0音色セットをMLで自動生成（OPQ実機プリセットの直接コンバートは行わない）
  → Bank Select / Program Change 実装
  → 同一リポジトリ内の ym38x6-ml/ に収録

フェーズ6: スケール判定・アボイド挙動の検証

フェーズ7: タブレット対応（Tauri v2 iOS/Android）
  → マルチタッチ入力の実装（UIロジックは共通）

フェーズ8: アルゴリズム拡張モード（オプション）
  → SY77スタイルのルーティングレジスタ公開
```

---

## 技術スタック

### クレート構成

```
ym38x6/                  ← ワークスペースルート
  Cargo.toml
  spec.md
  CLAUDE.md

  sound-core/            ← 基盤ライブラリ（WaveTable・AdsrParams・SoundEngineトレイト）
    Cargo.toml
    src/lib.rs             ← nih-plug・Tauri・cpal に無依存な純粋Rustロジック
                             波形変換パイプライン（32サンプルi8 → 1024サンプル対数フォーマット）

  wms1-core/             ← WMS-1エンジン実装（sound-coreに依存）
    Cargo.toml
    src/lib.rs             ← Wms1Engine（波形オシレーター + ADSRエンベロープ + チャンネル管理）

  wms1-vst/              ← WMS-1 VST3/CLAPプラグイン（nih-plug）

  ym38x6-core/           ← 38x6 FMエンジン実装（sound-coreに依存）
    Cargo.toml
    src/lib.rs             ← Ym38x6Engine（4opFM合成 + フィルター + 音色LFO + チャンネル管理）
    src/operator.rs        ← Operator（オシレーター + EG + パラメーター）
    src/algorithm.rs       ← アルゴリズム結線テーブル（ymfm由来）
    src/waveform.rs        ← OPZ系8波形生成
    src/mapping.rs         ← パラメーターマッピング関数群
    src/tone_lfo.rs        ← 音色LFO
    src/filter.rs          ← SVF + Filter EG

  ym38x6-vst/            ← 38x6 VST3/CLAPプラグイン（nih-plug）

  gesture-app/           ← 作曲支援デスクトップアプリ（メイン開発対象）
    package.json
    src/                   ← フロントエンド（HTML/JS）
      index.html
      main.js              ← キャリブレーション・ジェスチャーUI
    src-tauri/             ← Rustバックエンド
      Cargo.toml
      build.rs
      tauri.conf.json
      src/main.rs          ← cpalで音声出力、Tauriコマンド（note_on/note_off）
      icons/               ← アプリアイコン
      capabilities/        ← Tauri v2 パーミッション設定

```

### 各層の技術

```
言語:           Rust
アプリ:         Tauri（VST3/CLAP両対応）
音声出力:       cpal（デスクトップ）/ Core Audio（iOS、将来）
参照実装:       ymfm（C++、BSD 3-Clause）
VSTプラグイン:  nih-plug（wms1-vst・ym38x6-vstともに実装済み）
ターゲット:     Windowsデスクトップ → タブレット（iOS/Android）→ VST
```

---

## このセッションで参照した主要資料

- ymfm（Aaron Giles）: https://github.com/aaronsgiles/ymfm
- PSR70-reverse（Jari Kangas）: https://github.com/JKN0/PSR70-reverse
- OPQプログラマーズガイド: https://www.dtech.lv/files_ym/OPQ_ProgGuide_Jari20210423.pdf
- Retro&Reverseブログ: https://retroandreverse.blogspot.com/search/label/PSR-70%20reverse%20engineering
- Hackaday.io PSR-70プロジェクト: https://hackaday.io/project/177168
- あちゃぴー氏CLP-100解析: https://achapi.cloudfree.jp/sound/yamaha_clp100/index.html
