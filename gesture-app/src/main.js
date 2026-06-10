'use strict';

// Tauri invoke — フォールバックでブラウザ単体でも開ける
const invoke = window.__TAURI__?.core?.invoke ?? (async (_cmd, _args) => 0);

// ─────────────────────────────────────────────
// State
// ─────────────────────────────────────────────
const S = { CAL_C: 0, CAL_F: 1, CAL_G: 2, PLAYING: 3 };
let state = S.CAL_C;

// キャリブレーション点（画面座標）
const cal = { C: null, F: null, G: null };

// 変換済み座標系
let cs = null; // { origin, rdx, rdy, cdx, cdy, pixPerSemi, chordPixPerStep }

// 再生状態
let activeChannels = [];
let lastChordKey   = null;
let isUpdating     = false;
let pendingPos     = null;   // {x,y} — mousemove から animation tick へ橋渡し
let mouseHeld      = false;
let mousePos       = { x: 0, y: 0 };
let currentWaveSlot = 0;

const WAVE_NAMES = ['sine', 'square', 'saw', 'tri'];

// ─────────────────────────────────────────────
// Canvas
// ─────────────────────────────────────────────
const canvas  = document.getElementById('canvas');
const ctx     = canvas.getContext('2d');
const chordEl = document.getElementById('chord-display');
const hintEl  = document.getElementById('chord-type-hint');

function resize() {
  canvas.width  = window.innerWidth;
  canvas.height = window.innerHeight;
}
window.addEventListener('resize', resize);
resize();

// ─────────────────────────────────────────────
// 座標系の構築
// ─────────────────────────────────────────────
function buildCoords(C, F, G) {
  // C→F（5半音）と C→G（7半音）の両ベクトルを半音数で重み付けした加重平均で
  // ルート音軸を推定 — 1点だけ使うより方向・スケール両方が安定する
  const cfDx = F.x - C.x, cfDy = F.y - C.y;
  const cgDx = G.x - C.x, cgDy = G.y - C.y;
  const cfDist = Math.hypot(cfDx, cfDy);
  const cgDist = Math.hypot(cgDx, cgDy);
  if (cfDist < 5 || cgDist < 10) return null;

  // 単位ベクトルを半音数で重み付け
  const wx = (cfDx / cfDist) * 5 + (cgDx / cgDist) * 7;
  const wy = (cfDy / cfDist) * 5 + (cgDy / cgDist) * 7;
  const wLen = Math.hypot(wx, wy);
  const rdx = wx / wLen, rdy = wy / wLen;   // ルート音方向（単位ベクトル）

  // ピクセル/半音 = 両推定の平均
  const pixPerSemi = (cfDist / 5 + cgDist / 7) / 2;

  // コード種類軸 = ルート音軸を 90° 時計回りに回転
  // （右へ動く → dom7/maj7, 左 → m/dim）
  const cdx = rdy, cdy = -rdx;
  const chordPixPerStep = pixPerSemi * 2;    // 1ステップあたりのピクセル（調整可）

  return { origin: { ...C }, rdx, rdy, cdx, cdy, pixPerSemi, chordPixPerStep };
}

// 画面座標 → 音楽座標
function toMusicCoords(px, py) {
  const { origin, rdx, rdy, cdx, cdy, pixPerSemi, chordPixPerStep } = cs;
  const dx = px - origin.x, dy = py - origin.y;
  return {
    semitones:  (dx * rdx + dy * rdy) / pixPerSemi,
    chordParam: (dx * cdx + dy * cdy) / chordPixPerStep,
  };
}

// ─────────────────────────────────────────────
// 音楽ロジック
// ─────────────────────────────────────────────
const NOTE_NAMES = ['C','C#','D','D#','E','F','F#','G','G#','A','A#','B'];

function midiFreq(midi) { return 440 * Math.pow(2, (midi - 69) / 12); }

function semitoneToRoot(semi) {
  const n = Math.round(semi);
  const clamped = Math.max(-24, Math.min(24, n));   // C2 〜 C6 程度
  const name = NOTE_NAMES[((clamped % 12) + 12) % 12];
  return { midi: 60 + clamped, name };
}

// コード種類テーブル（chordParam の範囲 → インターバル）
const CHORD_TYPES = [
  { suffix: 'dim',  intervals: [0, 3, 6],        maxParam: -1.5 },
  { suffix: 'm',    intervals: [0, 3, 7],         maxParam: -0.5 },
  { suffix: '',     intervals: [0, 4, 7],         maxParam:  0.5 },
  { suffix: '7',    intervals: [0, 4, 7, 10],     maxParam:  1.5 },
  { suffix: 'maj7', intervals: [0, 4, 7, 11],     maxParam:  Infinity },
];

function chordFromParam(param) {
  return CHORD_TYPES.find(c => param < c.maxParam) ?? CHORD_TYPES[2];
}

function chordParamLabel(param) {
  if (param < -1.5) return '← dim';
  if (param < -0.5) return '← m';
  if (param <  0.5) return 'maj';
  if (param <  1.5) return '7 →';
  return 'maj7 →';
}

// ─────────────────────────────────────────────
// 発音制御
// ─────────────────────────────────────────────
async function stopChord() {
  const chs = activeChannels.splice(0);
  for (const ch of chs) {
    await invoke('note_off', { channel: ch });
  }
}

async function updateChord(px, py) {
  if (!cs || isUpdating) { pendingPos = { x: px, y: py }; return; }

  const { semitones, chordParam } = toMusicCoords(px, py);
  const root  = semitoneToRoot(semitones);
  const chord = chordFromParam(chordParam);
  const key   = `${root.midi}:${chord.suffix}`;
  if (key === lastChordKey) return;

  isUpdating = true;
  try {
    await stopChord();
    // マウスが await 中に離された場合は発音しない
    if (!mouseHeld) return;
    for (const interval of chord.intervals) {
      const ch = await invoke('note_on', {
        waveSlot: currentWaveSlot,
        frequency: midiFreq(root.midi + interval),
      });
      if (!mouseHeld) {
        // note_on を開始したが直後に離された — すぐ止める
        await invoke('note_off', { channel: ch });
        return;
      }
      activeChannels.push(ch);
    }
    lastChordKey = key;
    chordEl.textContent = root.name + chord.suffix;
    hintEl.textContent  = chordParamLabel(chordParam);
  } finally {
    isUpdating = false;
    if (pendingPos) {
      const p = pendingPos; pendingPos = null;
      updateChord(p.x, p.y);
    }
  }
}

// ─────────────────────────────────────────────
// 入力ハンドラ
// ─────────────────────────────────────────────
canvas.addEventListener('mousedown', async (e) => {
  if (e.button !== 0) return;
  if (state !== S.PLAYING) { handleCalibClick(e.clientX, e.clientY); return; }
  mouseHeld = true;
  await updateChord(e.clientX, e.clientY);
});

canvas.addEventListener('mousemove', (e) => {
  mousePos = { x: e.clientX, y: e.clientY };
  if (state === S.PLAYING && mouseHeld) pendingPos = { x: e.clientX, y: e.clientY };
});

async function releaseChord() {
  if (!mouseHeld) return;
  mouseHeld    = false;
  lastChordKey = null;
  await stopChord();
  chordEl.textContent = '—';
  hintEl.textContent  = '';
}
canvas.addEventListener('mouseup',    releaseChord);
canvas.addEventListener('mouseleave', releaseChord);

window.addEventListener('keydown', async (e) => {
  if (e.key.toLowerCase() === 'r') {
    await stopChord();
    cal.C = cal.F = cal.G = null;
    cs = null; state = S.CAL_C; lastChordKey = null;
    chordEl.textContent = '—'; hintEl.textContent = '';
  }
  const slot = parseInt(e.key) - 1;
  if (slot >= 0 && slot <= 3) {
    currentWaveSlot = slot;
    lastChordKey = null; // 同じコードでも即座に音色変更させる
  }
});

// ─────────────────────────────────────────────
// キャリブレーション
// ─────────────────────────────────────────────
function handleCalibClick(x, y) {
  if      (state === S.CAL_C) { cal.C = { x, y }; state = S.CAL_F; }
  else if (state === S.CAL_F) { cal.F = { x, y }; state = S.CAL_G; }
  else if (state === S.CAL_G) {
    cal.G = { x, y };
    const built = buildCoords(cal.C, cal.F, cal.G);
    if (built) { cs = built; state = S.PLAYING; }
    else { cal.G = null; } // C と G が近すぎた — やり直し
  }
}

// ─────────────────────────────────────────────
// 描画
// ─────────────────────────────────────────────
const CAL_INFO = [
  { label: 'C', color: '#4af', prompt: 'Cメジャーの位置でクリック', sub: 'ド・ミ・ソ（I）' },
  { label: 'F', color: '#fa4', prompt: 'Fメジャーの位置でクリック', sub: 'ファ・ラ・ド（IV）' },
  { label: 'G', color: '#4f8', prompt: 'Gメジャーの位置でクリック', sub: 'ソ・シ・レ（V）' },
];

function draw() {
  const W = canvas.width, H = canvas.height;
  ctx.clearRect(0, 0, W, H);

  // 背景
  ctx.fillStyle = '#111';
  ctx.fillRect(0, 0, W, H);

  if (state === S.PLAYING) {
    drawPlayingLayer(W, H);
  } else {
    drawCalibLayer(W, H);
  }
  drawWaveIndicator(W, H);
}

function drawCalibLayer(W, H) {
  const info = CAL_INFO[state];

  // 配置済み点
  const placed = [
    state > S.CAL_C && cal.C && { p: cal.C, ...CAL_INFO[0] },
    state > S.CAL_F && cal.F && { p: cal.F, ...CAL_INFO[1] },
  ].filter(Boolean);
  for (const { p, label, color } of placed) {
    ctx.beginPath();
    ctx.arc(p.x, p.y, 12, 0, Math.PI * 2);
    ctx.fillStyle = color + '44';
    ctx.fill();
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.fillStyle = color;
    ctx.font = 'bold 14px monospace';
    ctx.textAlign = 'left';
    ctx.fillText(label, p.x + 16, p.y + 5);
  }

  // 中央の指示
  ctx.textAlign = 'center';
  ctx.fillStyle = info.color;
  ctx.font = 'bold 30px monospace';
  ctx.fillText(info.prompt, W / 2, H / 2 - 18);

  ctx.fillStyle = '#888';
  ctx.font = '18px monospace';
  ctx.fillText(info.sub, W / 2, H / 2 + 18);

  ctx.fillStyle = '#444';
  ctx.font = '13px monospace';
  ctx.fillText('どこでも OK — 自分の感覚で自然な位置をクリック', W / 2, H / 2 + 52);
  ctx.fillText('後から R キーで再キャリブレーション', W / 2, H / 2 + 72);
}

function drawPlayingLayer(W, H) {
  // マウス周囲に光彩（コード種類で色調変化）
  if (mouseHeld && cs) {
    const { chordParam } = toMusicCoords(mousePos.x, mousePos.y);
    const t = Math.max(-1, Math.min(1, chordParam / 2));
    // 左（マイナー系）→青, 右（メジャー/7th系）→緑
    const rr = t < 0 ? Math.round(40 + (-t) * 80) : 20;
    const gg = t > 0 ? Math.round(40 + t * 80) : 20;
    const bb = Math.round(80 + (1 - Math.abs(t)) * 60);
    const grd = ctx.createRadialGradient(mousePos.x, mousePos.y, 0, mousePos.x, mousePos.y, 350);
    grd.addColorStop(0, `rgba(${rr},${gg},${bb},0.22)`);
    grd.addColorStop(1, 'rgba(0,0,0,0)');
    ctx.fillStyle = grd;
    ctx.fillRect(0, 0, W, H);
  }

  // ルート音軸をうっすら表示
  if (cs) {
    const { origin, rdx, rdy } = cs;
    const len = Math.max(W, H) * 1.5;
    ctx.save();
    ctx.strokeStyle = 'rgba(100,180,255,0.12)';
    ctx.lineWidth = 1;
    ctx.setLineDash([5, 10]);
    ctx.beginPath();
    ctx.moveTo(origin.x - rdx * len, origin.y - rdy * len);
    ctx.lineTo(origin.x + rdx * len, origin.y + rdy * len);
    ctx.stroke();
    ctx.restore();
  }

  // キャリブレーション点（小さめ）
  for (const [key, info] of [['C', CAL_INFO[0]], ['F', CAL_INFO[1]], ['G', CAL_INFO[2]]]) {
    const p = cal[key]; if (!p) continue;
    ctx.beginPath();
    ctx.arc(p.x, p.y, 4, 0, Math.PI * 2);
    ctx.fillStyle = info.color + '55';
    ctx.fill();
  }

  // コード種類スケール（画面下部中央）
  drawChordScale(W, H);
}

function drawChordScale(W, H) {
  const labels = ['dim', 'm', 'maj', '7', 'maj7'];
  const total = labels.length;
  const step = 72, startX = W / 2 - step * (total - 1) / 2;
  const y = H - 68;

  // 現在のコード種類をハイライト
  let activeIdx = 2; // default: maj
  if (cs && mouseHeld) {
    const { chordParam } = toMusicCoords(mousePos.x, mousePos.y);
    activeIdx = CHORD_TYPES.findIndex(c => chordParam < c.maxParam);
    if (activeIdx < 0) activeIdx = CHORD_TYPES.length - 1;
  }

  for (let i = 0; i < total; i++) {
    const x = startX + i * step;
    const isActive = i === activeIdx;
    ctx.fillStyle = isActive ? '#fff' : '#444';
    ctx.font = isActive ? 'bold 14px monospace' : '13px monospace';
    ctx.textAlign = 'center';
    ctx.fillText(labels[i], x, y);
    if (isActive) {
      ctx.beginPath();
      ctx.arc(x, y + 10, 3, 0, Math.PI * 2);
      ctx.fillStyle = '#4af';
      ctx.fill();
    }
  }

  // ← → ガイド
  ctx.fillStyle = '#333';
  ctx.font = '12px monospace';
  ctx.textAlign = 'left';
  ctx.fillText('← 暗い', startX - 60, y);
  ctx.textAlign = 'right';
  ctx.fillText('明るい →', startX + step * (total - 1) + 60, y);

}

function drawWaveIndicator(W, H) {
  ctx.textAlign = 'right';
  ctx.font = '13px monospace';
  for (let i = 0; i < WAVE_NAMES.length; i++) {
    const isActive = i === currentWaveSlot;
    ctx.fillStyle = isActive ? '#fa4' : '#444';
    ctx.fillText(`${i + 1}:${WAVE_NAMES[i]}`, W - 16, H - 16 - (WAVE_NAMES.length - 1 - i) * 20);
  }
}

// ─────────────────────────────────────────────
// アニメーションループ
// ─────────────────────────────────────────────
function tick() {
  // mousemove で溜まった pending を消化
  if (state === S.PLAYING && mouseHeld && pendingPos && !isUpdating) {
    const p = pendingPos; pendingPos = null;
    updateChord(p.x, p.y);
  }
  draw();
  requestAnimationFrame(tick);
}

tick();
