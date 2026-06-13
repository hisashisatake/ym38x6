#!/usr/bin/env python3
"""VOPM形式の.opm音色ファイル(YM2151/OPM)をym38x6の.38x6プリセットJSONに変換する。

各変換式の根拠はREADME.mdを参照。ym38x6-core/src/mapping.rs・tone_lfo.rsで
定義された「実機OPM理論値アンカー+指数カーブ」のパラメーター空間に、
.opmのレジスタ値を当てはめる。
"""

import argparse
import json
import re
import sys
from pathlib import Path

# .opmファイルのオペレーター行(M1/C1/M2/C2)をoperators[0..3]へ対応させる順序。
# デフォルトは「ファイルに書かれた順そのまま」(M1,C1,M2,C2 -> Op0,Op1,Op2,Op3)。
OPERATOR_ORDER_DIRECT = ["M1", "C1", "M2", "C2"]
# YM2151のレジスタ順(M1,M2,C1,C2)で並べたい場合の代替順序。
OPERATOR_ORDER_REGISTER = ["M1", "M2", "C1", "C2"]


# ---------------------------------------------------------------------------
# .opm パーサー
# ---------------------------------------------------------------------------

def parse_opm(text):
    """.opmテキストをvoiceの配列に変換する。各voiceは@:で始まるブロック。"""
    voices = []
    current = None
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("//"):
            continue
        if ":" not in line:
            continue
        key, _, rest = line.partition(":")
        key = key.strip().upper()
        rest = rest.strip()
        if key == "@":
            if current is not None:
                voices.append(current)
            num_str, _, name = rest.partition(" ")
            current = {
                "number": int(num_str),
                "name": name.strip(),
                "lfo": None,
                "ch": None,
                "ops": {},
            }
        elif current is None:
            continue
        elif key == "LFO":
            current["lfo"] = [int(x) for x in rest.split()]
        elif key == "CH":
            current["ch"] = [int(x) for x in rest.split()]
        elif key in ("M1", "C1", "M2", "C2"):
            current["ops"][key] = [int(x) for x in rest.split()]
    if current is not None:
        voices.append(current)
    return voices


# ---------------------------------------------------------------------------
# パラメーター変換（ym38x6-core/src/mapping.rs・tone_lfo.rsの逆変換）
# ---------------------------------------------------------------------------

def ar_dr_to_rate(reg):
    """AR/D1R/D2R(5bit, 0-31, eg_rate=reg*2) -> ym38x6 rate(0-255)。
    reg=0はeg_rate=0(フリーズ)でym38x6のrate=0に対応する。"""
    if reg == 0:
        return 0
    eg_rate = reg * 2  # 2..62
    x = (eg_rate - 2) / 60.0  # 0..1
    return 1 + round(x * 254)


def rr_to_rate(reg):
    """RR(4bit, 0-15, eg_rate=reg*4+2) -> ym38x6 rate(0-255)。
    実機RRはeg_rate=0にならない(フリーズしない)ためrr_to_deltaは0-255全域を使う。"""
    eg_rate = reg * 4 + 2  # 2..62
    x = (eg_rate - 2) / 60.0  # 0..1
    return round(x * 255)


def sl_to_value(reg):
    """D1L/SL(4bit, 0-15) -> ym38x6 SL値(0-255)。
    reg=0..14はeg_sustain=reg*32(=-3dB/step)、reg=15は-93dBへジャンプ(実機準拠)。"""
    if reg == 15:
        db = -93.0
    else:
        db = -3.0 * reg
    return round(255 * (1 + db / 93.0))


def tl_to_value(reg):
    """TL(7bit, 0-127, 0.75dB/step) -> ym38x6 TL値(0-255)。"""
    return round((127 - reg) * 255 / 127)


def ksr_to_value(reg):
    """KS(2bit, 0-3) -> ym38x6 KSR値(0-255)。レジスタの等間隔割り当て。"""
    return round(reg * 255 / 3)


def feedback_to_value(reg):
    """FL(3bit, 0-7) -> ym38x6 feedback値(0-255)。
    feedback_to_scaleは約36刻みごとに2倍の指数カーブのため、reg*255/7で
    1段=約36刻み=2倍のFLの倍々特性と一致する。"""
    return round(reg * 255 / 7)


def ams_to_value(reg):
    """AMS(2bit, 0-3) -> ym38x6 ams値(0-255)。
    reg=0は無効(0)、reg=1..3はams_to_depthのAMS=1(23.9dB)〜AMS=3(95.6dB)アンカーに一致。"""
    if reg == 0:
        return 0
    return round(1 + 127 * (reg - 1))


def pms_to_value(reg):
    """PMS(3bit, 0-7) -> ym38x6 pms値(0-255)。
    reg=0は無効(0)、reg=1..7はpms_to_cents_rangeのPMS=1(5cent)〜PMS=7(700cent)アンカーに一致。"""
    if reg == 0:
        return 0
    return round(1 + 254 * (reg - 1) / 6)


def dt1_to_value(reg):
    """DT1(3bit大きさ+符号, 0-7) -> ym38x6 dt1値(0-255、中心128)の簡易近似。
    reg&3が大きさ(0-3)、reg&4が符号(0=正/非0=負)。dt1=128 +/- (magnitude/3)*127。"""
    magnitude = reg & 0x03
    sign = -1 if (reg & 0x04) else 1
    return 128 + sign * round(magnitude / 3 * 127)


def tone_lfo_depth_to_value(reg):
    """AMD/PMD(7bit, 0-127) -> ym38x6 tone_lfo_amd/pmd(0-255)。
    PMD=127(最大)がPMSで定まる変調幅いっぱい(tone_lfo_pmd=255)に対応する。"""
    return round(reg * 255 / 127)


# ---------------------------------------------------------------------------
# voice -> .38x6 Preset変換
# ---------------------------------------------------------------------------

def convert_operator(op_reg):
    ar, d1r, d2r, rr, d1l, tl, ks, mul, dt1, dt2, ams_en = op_reg
    return {
        "tl": tl_to_value(tl),
        "ar": ar_dr_to_rate(ar),
        "d1r": ar_dr_to_rate(d1r),
        "d2r": ar_dr_to_rate(d2r),
        "d1l": sl_to_value(d1l),
        "rr": rr_to_rate(rr),
        # MUL(4bit, 0=x0.5/1〜15=x1〜x15)はym38x6のmul_to_ratioテーブルと共通のため直接コピー。
        "mul": mul,
        "dt1": dt1_to_value(dt1),
        "ksr": ksr_to_value(ks),
        "am_enable": bool(ams_en),
        # OPMにvelocity感度・OP波形の相当パラメーターは無いため0(感度なし/サイン波)固定。
        "velocity_sensitivity": 0,
        "waveform": 0,
    }


def convert_channel(ch_reg, lfo_reg):
    _pan, fl, con, ams, pms, _slot, _ne = ch_reg
    lfrq, amd, pmd, _wf, _nfrq = lfo_reg
    return {
        # CON(0-7)はym38x6のalgorithm(0-7)と同一トポロジーのため直接コピー。
        "algorithm": con,
        "feedback": feedback_to_value(fl),
        # LFRQ(0-255)はym38x6 tone_lfo_freq(0-255)へ直接コピー(簡易近似、Hzの厳密対応はしない)。
        "tone_lfo_freq": lfrq,
        "tone_lfo_pmd": tone_lfo_depth_to_value(pmd),
        "tone_lfo_amd": tone_lfo_depth_to_value(amd),
        # OPMにLFO Delay相当のパラメーターは無いため0(なし)固定。
        "tone_lfo_delay": 0,
        "pms": pms_to_value(pms),
        "ams": ams_to_value(ams),
        # フィルターはOPMに相当パラメーターが無いためデフォルト(全開・無効)。
        "filter_cutoff": 255,
        "filter_resonance": 0,
        "filter_type": 0,
        "filter_self_oscillation": True,
        "filter_eg_attack": 0,
        "filter_eg_decay": 0,
        "filter_eg_sustain": 0,
        "filter_eg_release": 0,
        "filter_eg_depth": 0,
    }


def convert_voice(voice, operator_order):
    ch = voice["ch"] or [0, 0, 0, 0, 0, 120, 0]
    lfo = voice["lfo"] or [0, 0, 0, 0, 0]
    ops = voice["ops"]

    operators = []
    for key in operator_order:
        op_reg = ops.get(key)
        if op_reg is None:
            raise ValueError(f"voice {voice['number']} ({voice['name']!r}): operator {key} がありません")
        operators.append(convert_operator(op_reg))

    patch = {
        "operators": operators,
        "channel": convert_channel(ch, lfo),
    }
    name = voice["name"] or f"voice{voice['number']}"
    return {"name": name, "patch": patch}


def sanitize_filename(name):
    return re.sub(r"[^A-Za-z0-9_-]+", "_", name).strip("_")


# ---------------------------------------------------------------------------
# メイン
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="VOPM .opm -> ym38x6 .38x6 コンバーター")
    parser.add_argument("input", type=Path, help=".opmファイルのパス")
    parser.add_argument(
        "output_dir",
        type=Path,
        nargs="?",
        default=None,
        help="出力先ディレクトリ（省略時は入力ファイルと同じディレクトリ）",
    )
    parser.add_argument(
        "--operator-order",
        choices=["direct", "register"],
        default="direct",
        help="opmファイルのオペレーター行(M1/C1/M2/C2)をoperators[0..3]へ並べる順序。"
        "direct(デフォルト)はM1,C1,M2,C2をそのままOp0-3に対応させる。"
        "アルゴリズム0系の聴感が構造的に違う場合はregister(M1,M2,C1,C2)を試す",
    )
    args = parser.parse_args()

    output_dir = args.output_dir or args.input.parent
    output_dir.mkdir(parents=True, exist_ok=True)

    operator_order = (
        OPERATOR_ORDER_DIRECT if args.operator_order == "direct" else OPERATOR_ORDER_REGISTER
    )

    text = args.input.read_text(encoding="utf-8")
    voices = parse_opm(text)
    if not voices:
        print(f"warning: {args.input} に @: で始まる音色定義が見つかりませんでした", file=sys.stderr)

    for voice in voices:
        preset = convert_voice(voice, operator_order)

        ch = voice["ch"] or [0, 0, 0, 0, 0, 120, 0]
        if ch[5] != 120:
            print(
                f"note: voice {voice['number']} ({voice['name']!r}): SLOT={ch[5]} "
                "(実機では一部オペレーターの出力が無効ですが、38x6では全オペレーターが鳴ります)",
                file=sys.stderr,
            )
        lfo = voice["lfo"] or [0, 0, 0, 0, 0]
        if lfo[3] != 2:
            print(
                f"note: voice {voice['number']} ({voice['name']!r}): LFO WF={lfo[3]} "
                "(38x6の音色LFOは三角波固定のため波形差は反映されません)",
                file=sys.stderr,
            )

        safe_name = sanitize_filename(voice["name"]) or f"voice{voice['number']}"
        out_path = output_dir / f"{voice['number']:03d}_{safe_name}.38x6"
        out_path.write_text(json.dumps(preset, indent=2, ensure_ascii=False), encoding="utf-8")
        print(f"wrote {out_path}")


if __name__ == "__main__":
    main()
