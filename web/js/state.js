export const S = { pid: null, project: null, assets: [], projects: [], selected: null, time: 0, playing: false, zoom: 80, tab: "media", past: [], future: [], model: "" };

const handlers = {};
export const on = (name, fn) => (handlers[name] ||= []).push(fn);
export const off = (name, fn) => (handlers[name] = (handlers[name] || []).filter((f) => f !== fn));
export const emit = (name, data) => (handlers[name] || []).forEach((fn) => fn(data));

export const $ = (sel, root = document) => root.querySelector(sel);
export function h(tag, attrs = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "style" && typeof v === "object") Object.assign(el.style, v);
    else if (k in el && k !== "list") el[k] = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  el.append(...kids.flat().filter((k) => k != null && k !== false));
  return el;
}

export function toast(text, kind = "") {
  const box = $("#toast");
  const item = h("div", { className: `toast ${kind}` }, text);
  box.append(item);
  setTimeout(() => item.remove(), 4200);
}

export async function api(path, opts = {}) {
  const res = await fetch(path, opts);
  if (res.status === 401) location.href = "/login";
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

export const post = (path, body, method = "POST") => api(path, { method, body: JSON.stringify(body), headers: { "content-type": "application/json" } });

let chain = Promise.resolve();
export function edit(ops) {
  const run = async () => {
    const before = JSON.stringify(S.project);
    const out = await post(`/api/projects/${S.pid}/ops`, { ops });
    if (out.done.length) remember(before);
    setProject(out.project, out.assets);
    if (out.errors.length) toast(out.errors.join("\n"), "bad");
    if (out.seek != null && !S.playing && ops.some((o) => o.op === "seek")) seek(out.seek);
    return out;
  };
  chain = chain.then(run, run);
  return chain.catch((e) => toast(e.message, "bad"));
}

export function remember(before) {
  S.past.push(before);
  if (S.past.length > 80) S.past.shift();
  S.future = [];
}

export function setProject(project, assets) {
  S.project = project;
  if (assets) S.assets = assets;
  if (S.selected && !find(S.selected)) S.selected = null;
  emit("project");
}

async function swap(from, to) {
  const snap = from.pop();
  if (!snap) return;
  to.push(JSON.stringify(S.project));
  setProject(JSON.parse(snap));
  await post(`/api/projects/${S.pid}`, { project: S.project }, "PUT").catch((e) => toast(e.message, "bad"));
}
export const undo = () => swap(S.past, S.future);
export const redo = () => swap(S.future, S.past);

export function find(id) {
  for (const track of S.project?.tracks || []) {
    const clip = track.clips.find((c) => c.id === id);
    if (clip) return { clip, track };
  }
  return null;
}

export const assetOf = (id) => S.assets.find((a) => a.id === id);
export const duration = () => Math.max(0, ...(S.project?.tracks || []).flatMap((t) => t.clips.map((c) => c.start + c.dur)));
export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

export function fmt(t) {
  t = Math.max(0, t || 0);
  const m = Math.floor(t / 60);
  const s = t - m * 60;
  return `${m}:${s.toFixed(2).padStart(5, "0")}`;
}

export function kindOf(clip) {
  if (clip.text != null) return "text";
  if (clip.fill) return "fill";
  return assetOf(clip.asset)?.kind || "video";
}

export function valueAt(clip, prop, local) {
  const base = prop === "blur" ? clip.effects.blur : clip[prop];
  const keys = clip.keyframes?.[prop];
  if (!keys?.length) return base;
  if (local <= keys[0].t) return keys[0].v;
  for (let i = 1; i < keys.length; i++) {
    const a = keys[i - 1];
    const b = keys[i];
    if (local > b.t) continue;
    if (a.ease === "hold") return a.v;
    let p = (local - a.t) / (b.t - a.t || 1);
    if (a.ease === "ease") p = p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
    return a.v + (b.v - a.v) * p;
  }
  return keys[keys.length - 1].v;
}

export function fadeAt(clip, local) {
  const a = clip.fade_in > 0 ? local / clip.fade_in : 1;
  const b = clip.fade_out > 0 ? (clip.dur - local) / clip.fade_out : 1;
  return clamp(Math.min(a, b), 0, 1);
}

export function seek(t) {
  S.time = clamp(t, 0, Math.max(duration(), 0));
  emit("seek");
}

export function select(id) {
  S.selected = id;
  if (id && S.tab !== "clip" && find(id)) S.tab = "clip";
  emit("select");
}

const loaded = new Set();
export function useFont(name) {
  if (!name || loaded.has(name)) return;
  loaded.add(name);
  const family = name.replace(/ /g, "+");
  document.head.append(h("link", { rel: "stylesheet", href: `https://fonts.googleapis.com/css2?family=${family}:wght@400;500;600;700;800&display=swap` }));
  document.fonts.load(`600 48px "${name}"`).then(() => emit("font")).catch(() => {});
}

export const FONTS = ["Inter", "Geist", "Instrument Serif", "DM Serif Display", "Playfair Display", "Fraunces", "Space Grotesk", "Bricolage Grotesque", "Syne", "Bebas Neue", "Anton", "Archivo Black", "JetBrains Mono", "Caveat", "Permanent Marker"];
