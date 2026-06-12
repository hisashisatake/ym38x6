# 架空FM音源「38x6」設計仕様書

## 概要

- YAMAHAのYM3806(OPQ)をベースに、OPZ系の波形拡張を加えた架空のFM音源
- 梅本竜氏がSynthEdit+VOPMで構築したYM-2609（2008年）と同じ発想：
  「PCM音源へ移行する前に、FM音源があと一歩進化していたとしたら」
- ソフトウェア実装（Rust）なので制約なし
- 作曲支援アプリのエンジンとしての役割も持つ
- 作曲支援アプリはTauriで実装。まずWindowsデスクトップ版から開始

---

## WMS-1（波形メモリ音源）

フェーズ1で使用するプロトタイプ音源。38x6の1オペレーター分に相当する。

### 位置づけ

```
WMS-1の1チャンネル ≅ 38x6の1オペレーター

WMS-1:    波形スロット + ADSR = 1音色（変調なし）
38x6:  （波形スロット + AR/DR/SR/RR/SL） × 4op + FM変調 = 1音色
```

### チャンネル構成

| 項目 | 値 | 備考 |
|------|-----|------|
| チャンネル数 | 無制限 | ソフトウェアなので制約なし |
| 構成 | 波形オシレーター × 1 + ADSR | 変調なし |
| 出力 | ステレオ | |

### ビルトイン波形（スロット0〜7）

```
0: サイン波
1: 矩形波
2: ノコギリ波
3: 三角波
4〜7: 予備
```

### ADSRエンベロープ（チャンネルごと、全8bit）

| パラメーター | 値域 | 備考 |
|------------|------|------|
| A（Attack） | 0〜255 | キーオンから最大音量までの時間 |
| D（Decay） | 0〜255 | 最大音量からサスティンレベルまでの時間 |
| S（Sustain） | 0〜255 | キーオン中に保持する音量レベル |
| R（Release） | 0〜255 | キーオフから無音までの時間 |

### 内部波形フォーマット（38x6と共通）

WMS-1も38x6も内部では同一フォーマットを使用する。

```
内部表現: 1024エントリ × uint16_t（2KB / 1波形）
  bit14〜0: -log|amplitude|（4.8 fixed point）
  bit15:    符号フラグ（負の半周期）
```

### ユーザー入力フォーマット（変換パイプライン）

```
ユーザー入力:
  32サンプル × int8（線形振幅、-128〜127）

          ↓ 変換パイプライン（38x6-coreに実装、WMS-1/38x6共通）

  1. 32サンプル → 1024サンプルにリサンプリング
  2. 線形振幅 → 4.8固定小数点対数減衰フォーマットに変換

内部表現: 1024 × uint16_t（38x6ユーザー波形スロットと同一）
```

ユーザー入力は32サンプルの単純なテーブルで済む。高品質な内部フォーマットへの変換は自動。

### 波形スロット番号（38x6と共通）

```
0〜7:   ビルトイン波形（固定）
8〜255: ユーザー定義波形スロット（WMS-1/38x6共用）
```

### 音色ファイル

```
形式: .wms1
内容: 波形スロット番号 + ADSRパラメーター + （スロット8以上の場合）波形データ
移行: 38x6移行時にOp0のベースとして読み込み可能
```

### 38x6への移行パス

```
WMS-1音色（.wms1）
  └─ 38x6移行時: Op0に割り当て
                    波形データはそのまま流用（フォーマット共通）
                    残りOp1/2/3は追加設定
```

---

## 38x6（FMオペレーター音源）

---

## 基本構成

| 項目 | 値 | 備考 |
|------|-----|------|
| チャンネル数 | 無制限 | ソフトウェアなので制約なし |
| オペレーター | 4op / ch | |
| アルゴリズム | 8種類（固定） | OPQ由来、将来的にルーティング拡張モード追加可 |
| 出力 | ステレオ | |

---

## 周波数・音程

38x6では常にOp0〜3それぞれが独立したオクターブ/F-Numberを持つ（OPNの拡張モード相当を標準仕様として全チャンネルに適用）。

| 項目 | 値 | 備考 |
|------|-----|------|
| オクターブ（指数部） | 3bit（0〜7、OP単位 × 4） | |
| F-Number（仮数部） | 13bit、OP単位 × 4 | OPQの12bitより精密。オクターブと合わせて16bit |

通常のノートはNote-On時に全Opへ同一のオクターブ/F-Numberが設定されるが、NRPN「Operator F-Number」でOP単位にF-Numberを上書き可能（オクターブは全Op共通のまま、詳細はNRPNセクション参照）。
OPQ由来の「1ch2周波数」（Op0/2ペアとOp1/3ペアが独立した周波数を持つ仕様）は、このOP単位F-Numberに内包される形で一般化された。

オペレーター間の周波数比が整数比（MUL/DT由来）に縛られなくなり、インハーモニックなFM変調（ベル系・金属的な音色）が可能になる。
アルゴリズム「全並列（4オペレーターが全てキャリア）」と組み合わせれば、独立した波形・周波数・エンベロープを持つ4オシレーターとしても利用できる。

PSR-70ファームウェアの周波数テーブル（OPQプログラマーズガイドより）：
```
C  → 4CAH, C# → 513H, D  → 560H, D# → 5B2H
E  → 609H, F  → 665H, F# → 6C6H, G  → 72DH
G# → 79AH, A  → 80EH, A# → 889H, B  → 90AH
```

---

## パラメーター設計方針

- **全パラメーター 0〜255（8bit統一）**
- 元チップ値域のスケーリングではなく、**0〜255全域に対して独自の対数カーブを設計**
- OPQの内部カーブ（YAMAHAの対数エンベロープ）を再現した形でマッピング
- OPQオリジナル値からの**可逆変換（コンバーター）**を提供
- 周波数（F-Number + オクターブ）のみ**16bitのまま例外**

---

## オペレーターパラメーター（全8bit）

| パラメーター | 元bit幅 | 8bit設計 | 備考 |
|------------|--------|---------|------|
| デチューン | 6bit（OPQ）| 0〜255、中心128 | OPQ中心値32→128にマッピング |
| マルチプル | 4bit | 0〜255 | |
| トータルレベル | 7bit | 0〜255 | 0=-95.25dB相当 |
| 波形選択 | 3bit | 0〜255（実質0〜7使用） | OPZ由来の8波形 |
| AR（アタックレート） | 5bit | 0〜255 | 対数カーブ |
| DR（ディケイレート） | 5bit | 0〜255 | 対数カーブ |
| SR（サスティンレート） | 5bit | 0〜255 | OPQ由来（DR2相当） |
| RR（リリースレート） | 4bit | 0〜255 | 対数カーブ |
| SL（サスティンレベル） | 4bit | 0〜255 | |
| KSR | 2bit | 0〜255 | |
| AMオン/オフ | 1bit | 0 or 1（8bitで保持） | |
| Velocity Sensitivity | なし | 0〜255（デフォルト0） | 38x6独自拡張（DX7/OPS由来）。OPQ/OPZ系チップにはハードウェア機能として存在しない |

**Velocity Sensitivityの加算モデル：**
```
実効TL = clamp(TLベース値 + (Velocity / 127) × VelocitySensitivity, 0, 255)
```
モジュレーターに設定すると「強く弾くと音色が明るくなる」、キャリアに設定すると音量変化になる。

### 波形8種類（OPZ由来、番号0〜7固定）
```
0: サイン波
1: ハーフサイン
2: 絶対値サイン
3: 矩形波
4: ノコギリ波
5〜7: 変形波形
```

### ユーザー定義波形（番号8〜255）

波形番号8〜255はユーザー定義波形スロットとして使用可能。

**内部波形テーブルフォーマット（ymfm準拠）：**
```
サイズ:   1024エントリ × uint16_t = 2KB / 1波形
フォーマット: 4.8固定小数点の対数減衰値
  bit14〜0: -log|amplitude|（4.8 fixed point）
  bit15:    符号フラグ（負の半周期）
```
これはYAMAHAのOPN系ダイショットから実測された値と同一フォーマット。
エンベロープ適用が加算のみで済む（乗算不要）という設計上の利点がある。

**ユーザー入力フォーマット（線形サンプル）：**
```
入力:  32サンプル × int8（WMS-1互換、-128〜127）  ← WMS-1からの移行パス
       1024サンプル × int16_t（線形振幅、-32768〜32767）
       または任意サンプル数（内部で1024点にリサンプリング）
変換:  線形振幅 → 4.8対数減衰フォーマットに自動変換（変換パイプラインはWMS-1と共通）
ソース: 波形エディタUIで直接描画
       WAVファイルからインポート（1周期分を切り出し）
       WMS-1音色ファイル（.wms1）からの読み込み
```

**スロット仕様：**
```
スロット番号: 8〜255（248スロット）
1スロット:   2KB（1024 × uint16_t）
合計最大:    248 × 2KB = 496KB
             → ソフトウェアなので問題なし
```

**利用例：**
```
スロット8:  ユーザー描画の倍音豊富な波形
スロット9:  WAVからインポートしたピアノの1周期
スロット10: プログラムで生成したチェビシェフ多項式波形
  ...
→ これらをFMのモジュレーターやキャリアとして使用可能
```

**プリセットへの保存：**
```
音色ファイル（.38x6）に波形データも埋め込み
→ 音色ファイル単体で完全再現可能
```

---

## チャンネルパラメーター（全8bit）

| パラメーター | 元bit幅 | 8bit設計 | 備考 |
|------------|--------|---------|------|
| アルゴリズム | 3bit | 0〜7（8bitで保持） | |
| フィードバック | 3bit | 0〜255 | |
| AM感度 | 2bit | 0〜255 | |
| PM感度 | 3bit | 0〜255 | |

### フィルター（State Variable Filter、ボイス単位）

FM合成出力にかけるアナログシンセ的なVCF相当。OPQ由来パラメーターとは独立した38x6独自拡張。

| パラメーター | 値域 | 備考 |
|------------|------|------|
| Cutoff | 0〜255 | カットオフ周波数。対数スケール（0≒20Hz、255≒20kHz） |
| Resonance | 0〜255 | レゾナンス。Self-Oscillation ON時は255でカットオフ周波数のサイン波が自己発振 |
| Self-Oscillation | 0 or 1（8bitで保持） | デフォルト=1（ON）。OFF時は255でも発振寸前で安定動作 |
| Filter Type | 0〜2（8bitで保持） | 0=LP、1=HP、2=BP |
| Filter EG A（Attack） | 0〜255 | キーオンからピークまでの時間 |
| Filter EG D（Decay） | 0〜255 | ピークからサスティンレベルまでの時間 |
| Filter EG S（Sustain） | 0〜255 | キーオン中に保持するレベル |
| Filter EG R（Release） | 0〜255 | キーオフから0までの時間 |
| Filter EG Depth | 0〜255 | Filter EGがCutoffに与える変調量 |

**Filter EGの加算モデル：**
```
実効Cutoff = clamp(Cutoffベース値 + Filter EG出力 × Filter EG Depth, 0, 255)
```
キーオンでA→D→Sの順に推移し、キーオフでRに移行する（オペレーターのエンベロープと同様の挙動、MC-404等のフィルターエンベロープ相当）。

**実装方式：** State Variable Filter（SVF）
- LP/HP/BPを同一回路から同時出力できる構造で、Filter Typeによる切り替えと相性が良い
- 高Resonanceでも数値的に安定（Self-Oscillation時の発振も含めて安定動作）

Self-Oscillation ON + Filter EGでCutoffをスイープすると、発振に突入する効果音的な表現が可能。

**OPQコンバーターとの関係：**
フィルターはOPQ由来パラメーターではないため、OPQ変換対象外。38x6独自フォーマット（.38x6）にのみ保存される。

---

## マスターエフェクト（Reverb / Chorus）

GM2準拠のセンドエフェクト2系統。各ボイス（FM合成 → SVFフィルター後の信号）からセンドレベルでReverb/Chorusバスに送り、マスターでミックスする。

| パラメーター | 制御 | 値域 | 備考 |
|------------|------|------|------|
| Reverb Send | CC91 | 0〜255 | チャンネル単位 |
| Reverb Type | NRPN | 0〜7 | enum（下記） |
| Reverb Time | NRPN | 0〜255 | マスター |
| Chorus Send | CC93 | 0〜255 | チャンネル単位 |
| Chorus Type | NRPN | 0〜7 | enum（下記） |
| Chorus Mod Rate | NRPN | 0〜255 | マスター |
| Chorus Mod Depth | NRPN | 0〜255 | マスター |
| Chorus Feedback | NRPN | 0〜255 | マスター |
| Chorus Send To Reverb | NRPN | 0〜255 | マスター。GM2準拠、ChorusバスからReverbバスへの送り量 |

※「Reverb Time」「Chorus Mod Rate/Depth/Feedback」「Chorus Send To Reverb」は、
NRPNに加えてnih-plugのマスターパラメーターとしても公開する
（MIDI実装方針のDAWオートメーション参照）。
「Reverb Type」「Chorus Type」はNRPN専用（DAWオートメーション対象外）。

**Reverb Type enum（GM2/GS準拠）：**

| 値 | タイプ |
|---|---|
| 0 | Room1 |
| 1 | Room2 |
| 2 | Room3 |
| 3 | Hall1（デフォルト） |
| 4 | Hall2 |
| 5 | Plate |
| 6 | Delay |
| 7 | Panning Delay |

**Chorus Type enum（GM2/GS準拠）：**

| 値 | タイプ |
|---|---|
| 0 | Chorus1（デフォルト） |
| 1 | Chorus2 |
| 2 | Chorus3 |
| 3 | Chorus4 |
| 4 | Feedback Chorus |
| 5 | Flanger |
| 6 | Short Delay |
| 7 | Short Delay (FB) |

**信号フロー：**
```
[各ボイス: FM合成 → SVF] → Dry ──┬─ ×Reverb Send(CC91) ──→ Reverbバス ─┐
                                  │                                     ├→ Master Out
                                  └─ ×Chorus Send(CC93) ──→ Chorusバス ─┤
                                                  │                     │
                                                  └ ×Chorus Send To Reverb → Reverbバスへ
```

**実装方式：**
- Reverb：コムフィルタ＋オールパスフィルタ構成のアルゴリズミックリバーブ（Room1〜Plate）。Delay/Panning Delayタイプはフィードバックディレイラインで実現
- Chorus：LFO変調ディレイライン（Chorus1〜4、Flanger、Feedback Chorus）。Short Delay系タイプは変調なしの短ディレイ
- `sound-core`に依存ゼロのDSPモジュールとして実装し、各`SoundEngine::render()`の出力に対してapp/plugin側のレンダリング後段で適用する

**OPQコンバーターとの関係：**
エフェクトはOPQ由来パラメーターではないため、OPQ変換対象外。38x6独自フォーマット（.38x6）にのみ保存される。

---

## キーオン（OPQ由来）

- オペレーターごとに独立してキーオン/オフ可能
- **Op3がマスター**：Op3がOffになると全Op強制Off
- Op3がOnの間、Op0/1/2は個別に制御可能
- 作曲支援アプリでのアボイドノート音量制御に応用可能
- NRPN「Operator Key Mask」でOP単位のキーオン/オフを制御可能（詳細はNRPNセクション参照）

---

## 音色LFO

プリセット・NRPNで設定する「音作り」用のLFO。MIDI CC（後述のパフォーマンスLFO）からは独立しており、演奏時のビブラート/トレモロには影響しない。

| 項目 | 値 | 備考 |
|------|-----|------|
| 波形 | 三角波 | OPQ由来・固定 |
| 周波数 | 3bit → 8bit（0〜255） | |
| PMD（ピッチ変調深さ） | 0〜255 | |
| AMD（振幅変調深さ） | 0〜255 | |
| PM感度（PMS） | チャンネルごと 3bit → 8bit | |
| AM感度（AMS） | チャンネルごと 2bit → 8bit | |
| Delay | 0〜255 | キーオンからLFO効果開始までの遅延時間。38x6独自拡張 |
| AMオン/オフ | オペレーターごと | |

周波数/PMD/AMD/Delay/PMS/AMSの6項目は、チャンネル単位のDAWパラメーターとして公開する（MIDI実装方針セクション参照）。

---

## パフォーマンスLFO（ビブラート/トレモロ）

GM2のCC1/76/77/78に対応する、演奏時のビブラート/トレモロ専用LFO。
音色LFO（PMD/AMD/PMS/AMS）とは完全に独立しており、音色設計には影響しない。

| 項目 | 制御 | 備考 |
|------|-----|------|
| Rate | CC76（Vibrato Rate） | 0〜255 → 0.01Hz〜20Hz（指数マッピング） |
| Depth | CC77（ベース値）+ CC1（加算分） | Destinationにより単位・モデルが異なる（下記） |
| Delay | CC78（Vibrato Delay） | キーオンから効果開始までの遅延。0〜255 → 0〜10秒（線形マッピング） |
| 波形 | NRPN「Performance LFO Waveform」で選択（下記） | デフォルト = 三角波 |
| Destination | NRPN「Performance LFO Destination」で選択（下記） | デフォルト = Pitch（ビブラート） |

**Waveform enum：**

| 値 | 波形 |
|---|---|
| 0（デフォルト） | 三角波 |
| 1 | サイン波 |
| 2 | 矩形波 |
| 3 | S&H（ランダム） |

**Destination enum：**

| 値 | 宛先 | Depthのモデル | 対応エンジン |
|---|---|---|---|
| 0（デフォルト） | Pitch（ビブラート） | `実効Depth(セント) = CC77値 + CC1値 × RPN0,5値 / 127` をピッチに加算 | 共通（WMS-1: 周波数 / 38x6: F-Number全Op） |
| 1 | Volume（トレモロ） | `実効Depth = clamp(CC77値 + CC1値, 0, 255)` を各ノートの実効音量（Velocity Sensitivity適用後）に加算 | 共通（WMS-1: ADSR出力レベル / 38x6: TL全オペレーター一括） |
| 2 | TL（キャリア一括、トレモロ） | 同上（キャリアのみ） | 38x6拡張のみ |

トレモロ（Destination=1/2）は各ノートの実効音量に対して相対的に作用するため、ベロシティによる音量差は維持されたまま揺れる。
RPN 0,5（Modulation Depth Range）はDestination=Pitchの場合のみ意味を持つ（詳細はRPNセクション参照）。

**実装方式：**
`PerformanceLfo`（Rate/Depth/Delay/Waveform）はエンジン非依存の共通コンポーネントとして`sound-core`に実装する。
適用先は`PerformanceLfoTarget`トレイトとして定義し、共通Destination（0=Pitch、1=Volume）はWMS-1・38x6の両方が実装する。拡張Destination（2=TLキャリア一括）は38x6エンジンのみが実装する。

---

## アルゴリズム拡張モード（将来実装）

SY77/TG77（AFM音源, 1989年）の設計を参考に：
- 表向き：固定8アルゴリズムから選択（初期UI）
- 内部：オペレーターごとのルーティングレジスタ
- 将来的にルーティングビットを公開する拡張モードを追加可能

---

## あえて省いたもの

| 機能 | 理由 |
|------|------|
| SSG-EG | 需要なし |
| ノイズ | 需要なし |
| DT2 | 需要なし |
| CSMモード（タイマー駆動の自動キーオン） | 需要なし。OP単位F-Number＋NRPN「Operator Key Mask」による手動キーオン/オフで代替可能 |

---

## MIDI実装方針

### DAWオートメーション（nih-plugパラメーター）

以下の全パラメーターをnih-plugのParam として公開し、Cubase・Logic等でのDAWオートメーションに対応する。

**チャンネル単位（19個）：**
Feedback / パフォーマンスLFO Rate / パフォーマンスLFO Depth（ベース値）/ パフォーマンスLFO Delay / 音色LFO Freq / 音色LFO PMD / 音色LFO AMD / 音色LFO Delay / PMS / AMS / Filter Cutoff / Filter Resonance / Filter EG A / Filter EG D / Filter EG S / Filter EG R / Filter EG Depth / Reverb Send / Chorus Send

**オペレーター単位（11個 × 4op = 44個）：**
TL / AR / D1R / D2R / D1L / RR / MUL / DT1 / KS / AME / Velocity Sensitivity

**マスター単位（5個）：**
Reverb Time / Chorus Mod Rate / Chorus Mod Depth / Chorus Feedback / Chorus Send To Reverb

**離散パラメーター（NRPN専用、DAWオートメーション対象外）：**
以下はnih-plugパラメーターを持たず、NRPN/GUI操作でのみ設定する
（CC91/93やマスターエフェクトの「マスター単位」パラメーターのように、
NRPN/CCとnih-plugパラメーターを併用する項目とは異なる点に注意）。

Algorithm / Waveform（WF）per op / Filter Type（LP/HP/BP）/ Filter Self-Oscillation / AT Destination / Poly AT Destination / Performance LFO Destination / Performance LFO Waveform / Reverb Type / Chorus Type

### MIDI CC（GM2準拠）

| CC | GM2定義 | 38x6割り当て | GM2との関係 |
|---|---|---|---|
| CC 1 | Modulation Wheel | パフォーマンスLFO Depth加算分 | 準拠（パフォーマンスLFO参照） |
| CC 5 | Portamento Time | Portamento Time | 完全準拠（下記参照） |
| CC 7 | Channel Volume | Volume | 完全準拠 |
| CC 10 | Pan | Pan | 完全準拠 |
| CC 11 | Expression | Expression | 完全準拠 |
| CC 64 | Damper Pedal | Sustain | 完全準拠 |
| CC 65 | Portamento On/Off | Portamento | 完全準拠（下記参照） |
| CC 66 | Sostenuto | Sostenuto | 完全準拠（下記参照） |
| CC 67 | Soft Pedal | Soft Pedal | 完全準拠（下記参照） |
| CC 71 | Resonance | Filter Resonance | 完全準拠 |
| CC 72 | Release Time | RR（キャリア一括） | 準拠 |
| CC 73 | Attack Time | AR（キャリア一括） | 準拠 |
| CC 74 | Brightness | Filter Cutoff | 完全準拠 |
| CC 75 | Decay Time | D1R（キャリア一括） | 準拠 |
| CC 76 | Vibrato Rate | パフォーマンスLFO Rate | 完全準拠 |
| CC 77 | Vibrato Depth | パフォーマンスLFO Depthベース値 | 完全準拠 |
| CC 78 | Vibrato Delay | パフォーマンスLFO Delay | 完全準拠 |
| CC 91 | Reverb Send Level | Reverb Send | 完全準拠（マスターエフェクト参照） |
| CC 93 | Chorus Send Level | Chorus Send | 完全準拠（マスターエフェクト参照） |
| CC 120 | All Sound Off | All Sound Off | 完全準拠 |
| CC 121 | Reset All Controllers | Reset All Controllers | 完全準拠 |
| CC 123 | All Notes Off | All Notes Off | 完全準拠 |
| CC 126 | Mono Mode On | Mono Mode | 完全準拠 |
| CC 127 | Poly Mode On | Poly Mode | 完全準拠 |

**Portamento（CC5/CC65）：**

CC65 ON時、新しいノート（新チャンネル）のF-Numberは、同一MIDIチャンネルで直前に発音したノートのF-Numberから、CC5（Portamento Time、0=即座〜127=数秒程度）で指定した時間をかけて目標値へ線形にグライドする。
直前のノートは別チャンネルで独立してリリース/サステインペダル等の影響を受けながら鳴り続けるため、グライドとの相互作用は発生しない。
作曲支援アプリのジェスチャーUIの「ゆっくり移動 → ポルタメント」（ジェスチャーレパートリー参照）も、この仕組み（Note-On + CC65 ON + CC5）で実現する。

**Sostenuto（CC66）：**

CC66 ON時点で発音中（Note-On済みかつNote-Off未到達）の全チャンネルに「サステイン保持」フラグを立てる。該当チャンネルはNote-OffされてもCC66 OFFまでReleaseに入らない（CC64と同じ仕組みを対象チャンネルのみに適用）。CC66 ON以降に新規キーオンしたノートは対象外。

**Soft Pedal（CC67）：**

CC67 ON中に新規キーオンしたノートに対してのみ、実効TLとFilter Cutoffを減算する。
```
実効TL = clamp(TLベース値 - CC67値, 0, 255)
実効Cutoff = clamp(Cutoffベース値 - CC67値, 0, 255)
```

### Pitch Bend

0xEn（14bit、中央値8192 = ベンドなし）でF-Numberを直接変化させる。
ベンドレンジはRPN 0,0（Pitch Bend Sensitivity）で設定する。

### RPN（GM2準拠）

| RPN (MSB,LSB) | 内容 | デフォルト | 備考 |
|---|---|---|---|
| 0, 0 | Pitch Bend Sensitivity | ±2半音 | Pitch BendのF-Number換算レンジ（半音 + セント） |
| 0, 1 | Channel Fine Tuning | 0セント | F-Numberオフセット（±100セント） |
| 0, 2 | Channel Coarse Tuning | 0半音 | F-Numberオフセット（±64半音） |
| 0, 5 | Modulation Depth Range | 64（約50セント相当） | パフォーマンスLFO Destination=Pitchの場合のCC1セント換算係数（パフォーマンスLFOセクション参照） |
| 127, 127 (7F,7F) | RPN/NRPN Null | - | 選択解除（誤操作防止のため必須） |

### Bank Select / Program Change

CC 0（MSB）+ CC 32（LSB）によるBank SelectとProgram Changeを実装する。
GM2のプログラム番号定義（0〜127の楽器カテゴリ）に準拠したバンク構成を採用する。

**バンク構成：**

| バンク | 内容 |
|---|---|
| Bank 0 | GM2プログラムマップ準拠（0〜127）。ym38x6-mlによるインバース合成でML自動生成 |
| Bank 1以降 | ユーザー定義プリセット |

**音色作成方針（フェーズ5で実施）：**
- ym38x6-mlで目標音（GM2リファレンス音源等）からFMパラメーターを逆算し、Bank 0の128音色を自動生成
- FMが苦手なカテゴリ（アコースティックピアノ・弦楽器・合唱等）は最近似音色で代替
- 実際のGM2→音色マッピング表はフェーズ5で別途作成
- OPQ実機・PSR-70のプリセットデータは権利関係のため使用しない

### NRPN

DAWオートメーション非対応の離散パラメーターおよびハードコントローラー向けの詳細制御に使用。

CC99（NRPN MSB）/CC98（NRPN LSB）でパラメーター番号を選択し、CC6（Data Entry MSB）で値を設定する（GM2準拠の標準的なNRPN手順）。
CC99/98またはCC101/100（RPN）に127,127（Null）を送ると選択解除される。

| 対象 | 備考 |
|---|---|
| Algorithm（CON） | 8種類、信号ルーティングが変わるため離散制御 |
| Waveform（WF）per op | 0〜7（ビルトイン）+ 8〜255（ユーザー定義） |
| Filter Type | 0=LP / 1=HP / 2=BP |
| AT Destination | Channel Pressureの加算先（destination enum、下記） |
| Poly AT Destination | Poly Key Pressureの加算先（destination enum、下記） |
| Performance LFO Destination | パフォーマンスLFOの加算先（destination enum、パフォーマンスLFOセクション参照） |
| Performance LFO Waveform | パフォーマンスLFOの波形（waveform enum、パフォーマンスLFOセクション参照） |
| Reverb Type | Reverbのタイプ（type enum、マスターエフェクトセクション参照） |
| Chorus Type | Chorusのタイプ（type enum、マスターエフェクトセクション参照） |
| Operator Key Mask | OP単位キーオン/オフ（4bit、下記参照） |
| Operator F-Number (Op0〜3) | OP単位F-Numberの上書き（13bit × 4、下記参照） |

**NRPN番号（MSB,LSB）：**

NRPN番号は本実装（パフォーマンスLFO）で初めて定義する。MSB=0を「離散パラメーター」用に予約し、LSBを実装順に割り当てる。他の離散パラメーターのNRPN番号は実装時に追記する。

| 対象 | NRPN (MSB,LSB) | 値 |
|---|---|---|
| Performance LFO Destination | 0, 0 | 0=Pitch（ビブラート） / 1=Volume（トレモロ） / 2=TL（キャリア一括、トレモロ、38x6拡張のみ） |
| Performance LFO Waveform | 0, 1 | 0=三角波 / 1=サイン波 / 2=矩形波 / 3=S&H |
| Reverb Type | 0, 2 | 0〜7（マスターエフェクトセクションのenum参照） |
| Chorus Type | 0, 3 | 0〜7（マスターエフェクトセクションのenum参照） |
| Reverb Time | 0, 4 | 0〜255 |
| Chorus Mod Rate | 0, 5 | 0〜255 |
| Chorus Mod Depth | 0, 6 | 0〜255 |
| Chorus Feedback | 0, 7 | 0〜255 |
| Chorus Send To Reverb | 0, 8 | 0〜255 |

**AT Destination / Poly AT Destination（アフタータッチの加算先）：**

Channel PressureとPoly Key Pressureは、それぞれ独立に「揺らぎ系」パラメーターへ加算するモデルで実装する。
加算先（Destination）はNRPN（AT Destination / Poly AT Destination）で選択可能。デフォルトはLFO PMD。

destination enum（共通）：

| 値 | 宛先 |
|---|---|
| 0 | LFO PMD（デフォルト） |
| 1 | LFO AMD |
| 2 | Filter Cutoff |
| 3 | Filter Resonance |
| 4 | TL（全オペレーター一括） |
| 5 | TL（キャリア一括） |

加算モデル：
```
実効値 = clamp(ベース値 + プレッシャー値, 0, 255)
```

Channel PressureとPoly Key Pressureが同じdestinationを指す場合、両方の値が加算される。
Poly Key Pressure対応コントローラーは少数（MPE等）のため、多くの環境ではChannel Pressureのみが機能する。

**Operator Key Mask（OP単位キーオン/オフ）：**

4bit（bit0=Op0 〜 bit3=Op3、1=発音中、デフォルト=1111）。

38x6はチャンネル数無制限のため、1ノート=1チャンネルとして扱うことで、チャンネル単位のNRPNがそのままノート単位のOP制御になる。

- bit3（Op3）= 0 → Op3がマスターのため全OP強制Off（そのノートのキーオフ相当）
- bit0〜2を個別に0にする → 該当オペレーターのみ消音（Op3は鳴り続ける）

**Operator F-Number（OP単位F-Number上書き）：**

Op0〜Op3それぞれに対応する4つのNRPNパラメーター。13bit値（0〜8191）をそのまま送信する（NRPNのデータエントリ精度14bitに対し1bit余裕がある）。

デフォルトはNote-Onで設定された値（全Op共通）。NRPN送信時点から、該当オペレーターのF-Numberのみを独立して上書きする（オクターブは全Op共通のまま変化しない）。

---

## OPQから38x6へのコンバーター設計

PSR-70の`def_seqs.h`（450エントリの音色データ）を架空音源プリセット形式に変換可能。

スケーリング方針（線形・可逆）：
```
5bit（0〜31）  → 8bit（0〜255）: × 8
4bit（0〜15）  → 8bit（0〜255）: × 17
3bit（0〜7）   → 8bit（0〜255）: × 36
2bit（0〜3）   → 8bit（0〜255）: × 85
6bit（0〜63）  → 8bit（0〜255）: × 4（デチューン：中心32→128）
```

**トータルレベル（7bit, 0〜127）の変換：極性反転 + × 2**
```
38x6_TL = (127 - OPQ_TL) × 2
```
OPQのTLレジスタは「0=0dB（最大音量）、127=-95.25dB（最小音量）」という減衰量。
38x6のTLは「0=-95.25dB（最小音量）、254=0dB（最大音量）」という音量ノブ的な極性（オペレーターパラメーター参照）のため、単純な×2ではなく反転が必要。

逆変換：`OPQ_TL = 127 - (38x6_TL / 2)`（38x6_TLが奇数または255の場合は丸め誤差あり）

可逆変換が保証されるため、OPQ実機で再生できる形式に戻すことも可能。

**Velocity Sensitivity（38x6独自拡張）：**
OPQ/OPZにはベロシティ感度のレジスタが存在しないため、変換時は全オペレーターVelocity Sensitivity = 0とする。
これによりベロシティに関わらず常に同じ音量・音色となり、OPQ実機と同じ挙動を再現できる。

---

## 実装参照元

| 資料 | 内容 | ライセンス |
|------|------|-----------|
| ymfm（Aaron Giles） | OPQ/OPZ/OPN実装 | BSD 3-Clause |
| OPQプログラマーズガイド V1.1（Jari Kangas） | レジスタ仕様・周波数テーブル | 変更なしなら自由配布可 |
| PSR-70 def_seqs.h（Jari Kangas） | 音色データ450エントリ | LICENSEファイルなし→要確認 |

**注記**：PSR-70音色データの利用にはJari Kangasへの許諾確認を推奨。
連絡先：https://github.com/JKN0/PSR70-reverse（Issues）
または https://hackaday.io/project/177168

---

## 作曲支援アプリ UI設計

### コンセプト

- **グリッド・ガイドなし**の自由な平面
- **キャリブレーションベース**：ユーザーの癖に合わせて基準を設定
- **タッチ前提**の設計（本命はiPad/Androidタブレット）
- デスクトップではマウスで代替（UIロジックは共通）

### 入力モデル

```
タッチ版（タブレット）:
  複数の指を平面に置く
  左端の指の位置     → ルート音
  各指の間隔         → コードのインターバル
  間隔を変化させる   → コードの種類が変化（maj/min/7th/dim...）
  指全体を左右に動かす → ルート音がシフト

マウス版（デスクトップ）:
  縦軸（Y） → ルート音に相当
  横軸（X） → 指の間隔（コードの種類）に相当
  マウス移動だけでコードが変化
```

### キャリブレーション

タッチ版・マウス版ともに使用前にキャリブレーションを実施。

**タッチ版:**
```
「自然に指を置いてください」
  → その指の間隔パターン = Cメジャーの基準
  → 以降の偏差でコードの種類を判定
```

**マウス版:**
```
「Cメジャーだと思う位置でクリック」→ (Xc, Yc) 記録
「Fメジャーだと思う位置でクリック」→ (Xf, Yf) 記録
「Gメジャーだと思う位置でクリック」→ (Xg, Yg) 記録
  → I-IV-Vの3点からルート音軸とコード種類軸を導出
  → 以降の座標はキャリブレーション基準の相対値で解釈
```

C-F-G（I-IV-V）を基準とする理由：最も基本的な進行であり、これだけで座標系のスケールと方向が両方決まるため。

### 統一UIアーキテクチャ

```
入力レイヤー（プラットフォームごとに異なる）
  Mouse:  mousemove → (x, y)
  Touch:  touchmove → [(x1,y1), (x2,y2), ...]

          ↓ 正規化

キャリブレーションレイヤー（共通）
  キャリブレーション基準点からの相対座標に変換

          ↓

コード判定エンジン（共通）
  相対座標 → インターバル → コード判定 → 禁則進行チェック

          ↓

38x6エンジン（共通）
  コード → 発音

          ↓

UI描画（共通）
  グリッドなし、現在のコード名などを最小限表示
```

入力だけが異なり、それより上のレイヤーはすべて共通。

### 動作モード

**モード1: コード録音**
```
1. リズムトラックを選択してテンポを設定（カシオ的なパターン）
2. リズム再生開始
3. 画面をタップ/ジェスチャーでコードを入力
   → タイムライン上に「この拍でCmaj、この拍でFmaj...」と記録
4. 記録完了
```

**モード2: メロディ即興**
```
1. 記録したコード進行 + リズムを再生
2. 現在再生中のコードが自動的に切り替わる
3. 画面を指でなぞる
   → 現在のコードのスケール音がリアルタイムで鳴る
   → コードが変わると使える音程も自動的に変わる
4. ∞ジェスチャーで装飾音・ビブラートを表現
```

両モードとも**同一のキャリブレーション・同一のジェスチャーシステム**を使用。

### ジェスチャーレパートリー

| ジェスチャー | 表現 | 実装 |
|---|---|---|
| 静止タップ | 単音・コード | 座標 → 音程に変換 |
| ゆっくり移動 | ポルタメント | 移動先座標でNote-On + CC65（Portamento ON）+ CC5（移動速度に応じたPortamento Time） |
| 横∞（∞）を描く | ビブラート・装飾音 | 軌跡がそのままピッチに追従 |

ジェスチャーに専用の認識アルゴリズムは不要。すべて「座標 → 音程」の連続マッピングで統一。∞の往復運動が自然にビブラートになる。

**∞ジェスチャーのパラメーター:**
```
∞の大きさ   → 装飾の幅（音程の揺れの深さ）
∞の速さ     → 装飾の速さ（ビブラートのレート）
∞の中心位置 → 基準音程
```

**LFOとの関係:**
∞ジェスチャーによる装飾・ビブラートはF-Numberの直接制御（連続更新）であり、LFOは使わない。
ゆっくり移動によるポルタメントはCC65/CC5を介した合成エンジン側のグライドで実現するため、こちらもLFOとは無関係。
LFOはジェスチャーとは独立した自動ビブラート等の別用途に使用可能。

### リアルタイムピッチ追従の要件

```
タッチイベントのサンプリング: 60〜120Hz（iPadのProMotionなら120Hz）
F-Numberの更新:             フレームごと（≒16ms以内）
音程変化のスムージング:      急激な変化を補間してなめらかにする
                            ただしレスポンスを損なわない程度に
```

### ターゲットデバイス

| デバイス | 画面 | 入力 | 評価 |
|---|---|---|---|
| iPadタブレット | 大 | マルチタッチ | 本命 |
| Androidタブレット | 大 | マルチタッチ | 有力な代替 |
| iPhone | 小（4本指で画面いっぱい） | マルチタッチ | 制約大 |
| Windowsデスクトップ | 大 | マウス | 開発・検証環境 |

### OP単位F-Numberの活用
- 1チャンネル（1ノート）内で、オペレーターごとに異なる周波数を割り当て可能
- 例：スケール音とアボイドノートを異なるOpに割り当てて同時に鳴らす

### オペレーター独立キーオンの活用
```
アボイドノートをタップした場合:
  Op3: キーオン（マスターなので必須）
  Op0/1/2: 音量を絞った状態でキーオン
  → 「鳴るが控えめ」という警告的発音が可能
```

### アボイドノートの挙動モード（作曲支援アプリ側の設計）
```
モード1: スナップ      → 最近傍のスケール音に自動補正
モード2: ランダムシフト → 上か下のスケール音にランダム移動
モード3: 無音          → 発音しない
モード4: 警告発音       → 音量を絞って発音（独立キーオン活用）
```

### 禁則コード進行の回避

前のコードの状態を記憶し、次のコードに遷移制約をかける。
ジェスチャーが禁則進行を引き起こす場合はスナップで補正。

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

フェーズ3: 38x6 FMエンジン導入、波形選択・デチューン拡張
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

フェーズ6: VST/CLAPプラグイン化（オプション）
  → nih-plugでym38x6-coreをラップ

フェーズ7: スケール判定・アボイド挙動の検証

フェーズ8: タブレット対応（Tauri v2 iOS/Android）
  → マルチタッチ入力の実装（UIロジックは共通）

フェーズ9: アルゴリズム拡張モード（オプション）
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

  （フェーズ3以降: ym38x6-core/ を追加し4opFM合成を実装）
  （フェーズ6以降: ym38x6-vst/ を追加し、nih-plugでym38x6-coreをラップ）
```

### 各層の技術

```
言語:           Rust
アプリ:         Tauri（VST3/CLAP両対応）
音声出力:       cpal（デスクトップ）/ Core Audio（iOS、将来）
参照実装:       ymfm（C++、BSD 3-Clause）
VSTプラグイン:  nih-plug（wms1-vstは実装済み、ym38x6-vstはフェーズ6以降）
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
