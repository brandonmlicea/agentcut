import { S, h, edit, emit, toast, api, find, assetOf, fmt, useFont, FONTS, kindOf } from "./state.js";

export const FILLS = [
  ["Ink", "#111110"],
  ["Paper", "#f4f4f0"],
  ["Night", "#0b1d3a>#1337b4"],
  ["Sky", "#1337b4>#7fb2ff"],
  ["Dusk", "#2b1055>#d53369"],
  ["Meadow", "#134e5e>#71b280"],
  ["Sun", "#f7971e>#ffd200"],
  ["Mist", "#e0eafc>#cfdef3"],
  ["Ember", "#3a0d0d>#e2543f"],
];

export const TEXTS = [
  ["Title", { text: "Your title", style: { size: 132, weight: 700, color: "#ffffff" } }],
  ["Subtitle", { text: "A short line under it", y: 0.62, style: { size: 60, weight: 500, color: "#ffffff" } }],
  ["Caption", { text: "Caption text", y: 0.86, style: { size: 48, weight: 500, color: "#ffffff", bg: "rgba(0,0,0,0.62)" } }],
  ["Lower third", { text: "Name Surname\nRole", x: 0.24, y: 0.8, style: { size: 52, weight: 600, color: "#141413", bg: "#ffffff", align: "left" } }],
  ["Big number", { text: "10×", style: { size: 300, weight: 400, color: "#ffffff", font: "Bebas Neue" } }],
  ["Quote", { text: "“Make it simple.”", style: { size: 96, weight: 400, color: "#ffffff", font: "Instrument Serif" } }],
];

export const PRESETS = {
  none: [1, 1, 1, 0, 0, 0, 0, 0],
  bw: [1.05, 1.15, 1, 0, 1, 0, 0, 0.2],
  vintage: [1.05, 0.9, 0.8, 0, 0, 0.45, -8, 0.35],
  vivid: [1.05, 1.15, 1.45, 0, 0, 0, 0, 0],
  cool: [1, 1.05, 0.95, 0, 0, 0, 15, 0.1],
  warm: [1.04, 1.02, 1.1, 0, 0, 0.18, -6, 0.1],
  dreamy: [1.12, 0.88, 1.1, 1.5, 0, 0.1, 0, 0.25],
  noir: [0.9, 1.45, 1, 0, 1, 0, 0, 0.55],
  fade: [1.1, 0.8, 0.85, 0, 0, 0.08, 0, 0],
};
export const EFFECTS = [
  ["brightness", "Brightness", 0, 3, 0.01],
  ["contrast", "Contrast", 0, 3, 0.01],
  ["saturate", "Saturation", 0, 3, 0.01],
  ["blur", "Blur", 0, 40, 0.5],
  ["grayscale", "Grayscale", 0, 1, 0.01],
  ["sepia", "Sepia", 0, 1, 0.01],
  ["hue", "Hue", -180, 180, 1],
  ["vignette", "Vignette", 0, 1, 0.01],
];

export const css = (fill) => (fill.includes(">") ? `linear-gradient(135deg, ${fill.split(">").join(", ")})` : fill);
const filterCss = ([b, c, s, bl, g, se, hu]) => `brightness(${b}) contrast(${c}) saturate(${s}) blur(${bl / 2}px) grayscale(${g}) sepia(${se}) hue-rotate(${hu}deg)`;

export function slider(label, value, [min, max, step], live, commit, extra) {
  const out = h("output", {}, (+value).toFixed(step < 1 ? 2 : 0));
  const input = h("input", { type: "range", min, max, step, value, oninput: () => ((out.textContent = (+input.value).toFixed(step < 1 ? 2 : 0)), live?.(+input.value)), onchange: () => commit(+input.value) });
  return h("label", { className: "row slider" }, h("span", { className: "row-label" }, label), input, out, extra || "");
}

export const section = (title, ...kids) => h("section", { className: "pane-section" }, h("h3", {}, title), ...kids);
const at = () => +S.time.toFixed(3);

function probe(file, kind) {
  const url = URL.createObjectURL(file);
  return new Promise((resolve) => {
    const done = (meta) => (URL.revokeObjectURL(url), resolve(meta));
    setTimeout(() => done({}), 6000);
    if (kind === "image") {
      const img = new Image();
      img.onload = () => done({ width: img.naturalWidth, height: img.naturalHeight });
      img.onerror = () => done({});
      img.src = url;
      return;
    }
    const el = document.createElement(kind);
    el.preload = "metadata";
    el.onloadedmetadata = () => done({ duration: el.duration, ...(el.videoWidth ? { width: el.videoWidth, height: el.videoHeight } : {}) });
    el.onerror = () => done({});
    el.src = url;
  });
}

export async function upload(file) {
  const kind = file.type.split("/")[0];
  if (!["video", "audio", "image"].includes(kind)) return toast(`${file.name} is not video, audio or an image.`, "bad"), null;
  const meta = await probe(file, kind);
  const q = new URLSearchParams({ name: file.name });
  for (const [k, v] of Object.entries(meta)) if (Number.isFinite(v)) q.set(k, v);
  const pending = { id: `up-${Math.random()}`, name: file.name, kind, progress: 0 };
  S.uploads = [...(S.uploads || []), pending];
  emit("assets");
  const asset = await new Promise((resolve) => {
    const xhr = new XMLHttpRequest();
    xhr.open("PUT", `/api/assets?${q}`);
    xhr.setRequestHeader("content-type", file.type);
    xhr.upload.onprogress = (e) => ((pending.progress = e.loaded / e.total), emit("upload"));
    xhr.onload = () => resolve(xhr.status < 300 ? JSON.parse(xhr.responseText) : (toast(`Upload failed: ${xhr.responseText}`, "bad"), null));
    xhr.onerror = () => resolve((toast("Upload failed", "bad"), null));
    xhr.send(file);
  });
  S.uploads = S.uploads.filter((u) => u !== pending);
  if (asset) S.assets.unshift(asset);
  emit("assets");
  return asset;
}

function card(a) {
  const thumb =
    a.kind === "video" ? h("video", { src: `/media/${a.id}#t=0.5`, preload: "metadata", muted: true }) : a.kind === "image" ? h("img", { src: `/media/${a.id}`, alt: "" }) : h("span", { className: "audio-thumb" }, "♪");
  return h(
    "div",
    { className: "asset", draggable: true, id: `asset-${a.id}`, title: `${a.name}\nDrag onto the timeline or press +`, ondragstart: (e) => e.dataTransfer.setData("text/agentcut-asset", a.id) },
    h("div", { className: `asset-thumb k-${a.kind}` }, thumb, a.duration ? h("em", {}, fmt(a.duration)) : ""),
    h("div", { className: "asset-name" }, a.name),
    h("button", { className: "asset-add", title: "Add at the playhead", onclick: () => edit([{ op: "add_clip", asset: a.id, start: at() }]) }, "+"),
    h("button", {
      className: "asset-del",
      title: "Delete from library",
      onclick: async () => {
        if (!confirm(`Delete ${a.name} from your media?`)) return;
        await api(`/api/assets/${a.id}`, { method: "DELETE" });
        S.assets = S.assets.filter((x) => x.id !== a.id);
        emit("assets");
      },
    }, "×"),
  );
}

export function media() {
  const input = h("input", { type: "file", multiple: true, accept: "video/*,audio/*,image/*", hidden: true, onchange: async () => { for (const f of input.files) await upload(f); input.value = ""; } });
  const url = h("input", { type: "url", placeholder: "https://…/clip.mp4", required: true });
  const importing = async (e) => {
    e.preventDefault();
    try {
      const asset = await api("/api/import", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ url: url.value }) });
      S.assets.unshift(asset);
      url.value = "";
      emit("assets");
    } catch (err) {
      toast(err.message, "bad");
    }
  };
  const ups = (S.uploads || []).map((u) => h("div", { className: "asset pending" }, h("div", { className: "asset-thumb" }, h("i", { className: "bar", style: { width: `${Math.round(u.progress * 100)}%` } })), h("div", { className: "asset-name" }, `Uploading ${u.name}`)));
  return h(
    "div",
    { className: "pane", id: "pane-media" },
    h("h2", {}, "Media"),
    h("div", { className: "drop", ondragover: (e) => e.preventDefault(), ondrop: async (e) => { e.preventDefault(); for (const f of e.dataTransfer.files) await upload(f); } }, input, h("button", { className: "press", onclick: () => input.click() }, "Upload media"), h("p", {}, "or drop video, music and images here")),
    h("form", { className: "inline", onsubmit: importing }, url, h("button", { className: "soft" }, "Import")),
    section("Your media", S.assets.length || ups.length ? h("div", { className: "assets" }, ups, S.assets.map(card)) : h("p", { className: "muted" }, "Nothing yet. Upload a clip or a song to start.")),
    section("Backgrounds", h("div", { className: "fills" }, FILLS.map(([name, fill]) => h("button", { className: "fill", title: `Add ${name} at the playhead`, draggable: true, ondragstart: (e) => e.dataTransfer.setData("text/agentcut-fill", fill), style: { background: css(fill) }, onclick: () => edit([{ op: "add_clip", fill, name, start: at() }]) }, h("span", {}, name))))),
  );
}

export function text() {
  const picked = S.selected && find(S.selected)?.clip;
  const textClip = picked && kindOf(picked) === "text" ? picked : null;
  const current = textClip?.style.font || S.project.font;
  FONTS.forEach(useFont);
  return h(
    "div",
    { className: "pane", id: "pane-text" },
    h("h2", {}, "Text"),
    section("Add text", h("div", { className: "presets" }, TEXTS.map(([name, p]) => h("button", { className: "text-preset", style: { fontFamily: `"${p.style.font || S.project.font}"`, fontWeight: p.style.weight }, onclick: () => edit([{ op: "add_clip", name, start: at(), ...p }]) }, name)))),
    section(
      textClip ? `Font for “${textClip.name}”` : "Default font",
      h("div", { className: "fonts" }, FONTS.map((f) => h("button", { className: `font${f === current ? " on" : ""}`, style: { fontFamily: `"${f}"` }, onclick: () => edit([textClip ? { op: "update_clip", clip: textClip.id, style: { font: f } } : { op: "set_project", font: f }]) }, f))),
    ),
  );
}

export function effects() {
  const picked = S.selected && find(S.selected)?.clip;
  if (!picked || kindOf(picked) === "audio") return h("div", { className: "pane", id: "pane-effects" }, h("h2", {}, "Effects"), h("p", { className: "muted" }, "Select a video, image, colour or text clip on the timeline to give it a look."));
  const thumb = picked.fill ? css(picked.fill) : "linear-gradient(135deg, #1337b4, #e2543f 55%, #ffd200)";
  return h(
    "div",
    { className: "pane", id: "pane-effects" },
    h("h2", {}, "Effects"),
    section("Looks", h("div", { className: "looks" }, Object.entries(PRESETS).map(([name, v]) => h("button", { className: "look", onclick: () => edit([{ op: "effect", clip: picked.id, preset: name }]) }, h("i", { style: { background: thumb, filter: filterCss(v) } }), name)))),
    section("Adjust", ...EFFECTS.map(([key, label, min, max, step]) => slider(label, picked.effects[key], [min, max, step], (v) => ((picked.effects[key] = v), emit("seek")), (v) => edit([{ op: "update_clip", clip: picked.id, effects: { [key]: v } }])))),
  );
}

