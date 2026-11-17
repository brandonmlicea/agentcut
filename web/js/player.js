import { S, on, off, emit, $, assetOf, duration, valueAt, fadeAt, clamp, seek, useFont } from "./state.js";

const canvas = $("#stage-canvas");
const ctx = canvas.getContext("2d");
const media = new Map();
let audio = null;
let master = null;
let t0 = 0;
let w0 = 0;

function context() {
  if (!audio) {
    audio = new AudioContext();
    master = audio.createGain();
    master.connect(audio.destination);
    for (const m of media.values()) wire(m);
  }
  if (audio.state === "suspended") audio.resume();
  return audio;
}

function wire(m) {
  if (!audio || m.gain || m.kind === "image") return;
  m.gain = audio.createGain();
  audio.createMediaElementSource(m.el).connect(m.gain);
  m.gain.connect(master);
}

function mediaFor(clip) {
  const asset = assetOf(clip.asset);
  if (!asset) return null;
  let m = media.get(clip.id);
  if (m && m.src === asset.id) return m;
  if (asset.kind === "image") {
    const el = new Image();
    el.onload = () => draw();
    el.src = `/media/${asset.id}`;
    m = { el, kind: "image", src: asset.id };
  } else {
    const el = document.createElement(asset.kind === "video" ? "video" : "audio");
    Object.assign(el, { preload: "auto", playsInline: true, src: `/media/${asset.id}` });
    el.addEventListener("loadeddata", () => draw());
    el.addEventListener("seeked", () => !S.playing && draw());
    el.addEventListener("loadedmetadata", () => measured(asset, el));
    m = { el, kind: asset.kind, src: asset.id };
    wire(m);
  }
  media.set(clip.id, m);
  return m;
}

function measured(asset, el) {
  if (asset.duration && Math.abs(asset.duration - el.duration) < 0.05) return;
  if (!Number.isFinite(el.duration)) return;
  asset.duration = el.duration;
  fetch(`/api/assets/${asset.id}`, { method: "PATCH", headers: { "content-type": "application/json" }, body: JSON.stringify({ duration: el.duration, width: el.videoWidth || null, height: el.videoHeight || null }) });
}

function clips() {
  return (S.project?.tracks || []).flatMap((track) => track.clips.map((clip) => ({ track, clip })));
}

function sync(playing) {
  for (const { track, clip } of clips()) {
    if (!clip.asset) continue;
    const m = mediaFor(clip);
    if (!m || m.kind === "image") continue;
    const el = m.el;
    const local = S.time - clip.start;
    const live = local >= 0 && local < clip.dur;
    if (!live) {
      if (!el.paused) el.pause();
      continue;
    }
    const want = clip.in + local * clip.speed;
    const silent = clip.muted || track.muted;
    const volume = silent ? 0 : clamp(valueAt(clip, "volume", local) * fadeAt(clip, local), 0, 2);
    if (m.gain) m.gain.gain.value = volume;
    else el.volume = Math.min(1, volume);
    el.playbackRate = clamp(clip.speed, 0.0625, 16);
    if (playing) {
      if (Math.abs(el.currentTime - want) > 0.3) el.currentTime = want;
      if (el.paused) el.play().catch(() => {});
    } else {
      if (!el.paused) el.pause();
      if (Math.abs(el.currentTime - want) > 0.02) el.currentTime = want;
    }
  }
}

function paint(fill, w, h) {
  const [a, b] = fill.split(">");
  if (!b) return a;
  const g = ctx.createLinearGradient(-w / 2, -h / 2, w / 2, h / 2);
  g.addColorStop(0, a.trim());
  g.addColorStop(1, b.trim());
  return g;
}

function filter(e, blur, k) {
  const parts = [];
  if (e.brightness !== 1) parts.push(`brightness(${e.brightness})`);
  if (e.contrast !== 1) parts.push(`contrast(${e.contrast})`);
  if (e.saturate !== 1) parts.push(`saturate(${e.saturate})`);
  if (e.grayscale) parts.push(`grayscale(${e.grayscale})`);
  if (e.sepia) parts.push(`sepia(${e.sepia})`);
  if (e.hue) parts.push(`hue-rotate(${e.hue}deg)`);
  if (blur > 0) parts.push(`blur(${blur * k}px)`);
  return parts.join(" ") || "none";
}

function text(clip, k) {
  const st = clip.style;
  const font = st.font || S.project.font;
  useFont(font);
  const size = st.size * k;
  ctx.font = `${st.weight} ${size}px "${font}", Inter, system-ui, sans-serif`;
  ctx.textBaseline = "middle";
  ctx.textAlign = st.align;
  const lines = String(clip.text).split("\n");
  const lh = size * 1.15;
  const w = Math.max(...lines.map((l) => ctx.measureText(l).width));
  const hgt = lh * lines.length;
  const pad = size * 0.35;
  if (st.bg) {
    ctx.fillStyle = st.bg;
    ctx.beginPath();
    ctx.roundRect(-w / 2 - pad, -hgt / 2 - pad * 0.55, w + pad * 2, hgt + pad * 1.1, size * 0.22);
    ctx.fill();
  }
  ctx.fillStyle = st.color;
  const x = st.align === "left" ? -w / 2 : st.align === "right" ? w / 2 : 0;
  lines.forEach((line, i) => ctx.fillText(line, x, -hgt / 2 + lh * (i + 0.5)));
}

function layer(clip, W, H, k) {
  const local = S.time - clip.start;
  const v = (p) => valueAt(clip, p, local);
  const alpha = clamp(v("opacity"), 0, 1) * fadeAt(clip, local);
  if (alpha <= 0) return;
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.translate(v("x") * W, v("y") * H);
  ctx.rotate((v("rotation") * Math.PI) / 180);
  ctx.scale(v("scale"), v("scale"));
  ctx.filter = filter(clip.effects, v("blur"), k);
  let w = W;
  let h = H;
  if (clip.fill) {
    ctx.fillStyle = paint(clip.fill, W, H);
    ctx.fillRect(-W / 2, -H / 2, W, H);
  } else if (clip.text != null) {
    text(clip, k);
  } else {
    const m = mediaFor(clip);
    const el = m?.el;
    const sw = el?.videoWidth || el?.naturalWidth;
    const sh = el?.videoHeight || el?.naturalHeight;
    if (sw && sh && (m.kind === "image" ? el.complete : el.readyState >= 2)) {
      const s = Math.min(W / sw, H / sh);
      [w, h] = [sw * s, sh * s];
      ctx.drawImage(el, -w / 2, -h / 2, w, h);
    }
  }
  ctx.filter = "none";
  if (clip.effects.vignette > 0 && clip.text == null) {
    const g = ctx.createRadialGradient(0, 0, Math.min(w, h) * 0.3, 0, 0, Math.hypot(w, h) / 2);
    g.addColorStop(0, "rgba(0,0,0,0)");
    g.addColorStop(1, `rgba(0,0,0,${clip.effects.vignette})`);
    ctx.fillStyle = g;
    ctx.fillRect(-w / 2, -h / 2, w, h);
  }
  ctx.restore();
}

export function draw() {
  const P = S.project;
  if (!P) return;
  const scale = Math.min(1, 1920 / Math.max(P.width, P.height));
  const [W, H] = [Math.round(P.width * scale), Math.round(P.height * scale)];
  if (canvas.width !== W || canvas.height !== H) [canvas.width, canvas.height] = [W, H];
  ctx.fillStyle = P.background;
  ctx.fillRect(0, 0, W, H);
  for (const track of P.tracks) {
    if (track.kind === "audio" || track.hidden) continue;
    for (const clip of track.clips) if (S.time >= clip.start && S.time < clip.start + clip.dur) layer(clip, W, H, H / P.height);
  }
}

function tick(now) {
  if (!S.playing) return;
  S.time = t0 + (now - w0) / 1000;
  const end = duration();
  if (S.time >= end) {
    S.time = end;
    pause();
    emit("ended");
  } else {
    sync(true);
    requestAnimationFrame(tick);
  }
  draw();
  emit("time");
}

export function play() {
  if (S.playing) return;
  context();
  if (S.time >= duration() - 0.01) S.time = 0;
  S.playing = true;
  t0 = S.time;
  w0 = performance.now();
  sync(true);
  requestAnimationFrame(tick);
  emit("play");
}

export function pause() {
  S.playing = false;
  sync(false);
  emit("play");
}

export const toggle = () => (S.playing ? pause() : play());

async function settle() {
  sync(false);
  const busy = [...media.values()].filter((m) => m.kind !== "image" && m.el.seeking).map((m) => new Promise((r) => m.el.addEventListener("seeked", r, { once: true })));
  await Promise.race([Promise.all(busy), new Promise((r) => setTimeout(r, 3000))]);
}

export async function record(progress) {
  pause();
  context();
  for (const { clip } of clips()) if (clip.asset) mediaFor(clip);
  const fps = S.project.fps || 30;
  const stream = canvas.captureStream(fps);
  const dest = audio.createMediaStreamDestination();
  master.connect(dest);
  dest.stream.getAudioTracks().forEach((t) => stream.addTrack(t));
  const type = ["video/mp4;codecs=avc1.640028,mp4a.40.2", "video/mp4", "video/webm;codecs=vp9,opus", "video/webm"].find((t) => MediaRecorder.isTypeSupported(t));
  const rec = new MediaRecorder(stream, { mimeType: type, videoBitsPerSecond: 10_000_000 });
  const chunks = [];
  rec.ondataavailable = (e) => e.data.size && chunks.push(e.data);
  seek(0);
  await settle();
  draw();
  const stopped = new Promise((r) => (rec.onstop = r));
  rec.start(250);
  const end = duration();
  const onTime = () => progress(S.time / end);
  on("time", onTime);
  const ended = new Promise((r) => {
    const done = () => (off("ended", done), r());
    on("ended", done);
  });
  play();
  await ended;
  off("time", onTime);
  rec.stop();
  await stopped;
  master.disconnect(dest);
  progress(1);
  return new Blob(chunks, { type: type.split(";")[0] });
}

on("project", () => {
  const ids = new Set(clips().map(({ clip }) => clip.id));
  for (const [id, m] of media) if (!ids.has(id)) (m.el.pause?.(), media.delete(id));
  if (!S.playing) sync(false);
  draw();
});
on("seek", () => {
  if (S.playing) {
    t0 = S.time;
    w0 = performance.now();
    sync(true);
  } else sync(false);
  draw();
});
on("font", draw);
