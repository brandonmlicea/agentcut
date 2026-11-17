import { S, on, $, h, edit, find, select, seek, duration, kindOf, clamp, assetOf, emit } from "./state.js";
import { addFiles } from "./panels.js";

const HEAD = 176;
const root = $("#timeline");
const scroller = h("div", { className: "tl-scroll", id: "tl-scroll" });
const inner = h("div", { className: "tl-inner" });
const zoom = h("input", { type: "range", min: 12, max: 320, value: S.zoom, className: "tl-zoom", oninput: () => ((S.zoom = +zoom.value), render()) });
root.append(
  h("div", { className: "tl-bar" }, h("span", { className: "tl-hint" }, "Drag to move or change layer · edges trim · S splits · Space plays"), h("label", { className: "tl-zoom-wrap" }, "Zoom", zoom)),
  scroller,
);
scroller.append(inner);

const px = (t) => t * S.zoom;
const ICON = { video: "▶", text: "T", audio: "♪" };

function ordered() {
  const tracks = S.project?.tracks || [];
  return [...tracks.filter((t) => t.kind !== "audio").reverse(), ...tracks.filter((t) => t.kind === "audio")];
}

function ruler(width) {
  const step = [0.5, 1, 2, 5, 10, 15, 30, 60, 120].find((s) => s * S.zoom >= 72) || 300;
  const marks = [];
  for (let t = 0; px(t) < width; t += step) marks.push(h("span", { style: { left: `${px(t)}px` } }, t >= 60 ? `${Math.floor(t / 60)}:${String(Math.round(t % 60)).padStart(2, "0")}` : `${t}s`));
  const bar = h("div", { className: "tl-ruler", onpointerdown: scrub }, marks);
  bar.style.backgroundSize = `${px(step) / 5}px 6px`;
  return bar;
}

function clipEl(track, clip) {
  const kind = kindOf(clip);
  const keys = Object.entries(clip.keyframes || {}).flatMap(([prop, ks]) => ks.map((k) => ({ prop, t: k.t })));
  const tags = [clip.speed !== 1 && `${+clip.speed.toFixed(2)}×`, (clip.muted || track.muted) && kind !== "text" && kind !== "fill" && kind !== "image" && "muted", clip.effects && Object.entries(clip.effects).some(([k, v]) => v !== (["brightness", "contrast", "saturate"].includes(k) ? 1 : 0)) && "fx"].filter(Boolean);
  const el = h(
    "div",
    { className: `clip k-${kind}${S.selected === clip.id ? " sel" : ""}`, id: `clip-${clip.id}`, style: { left: `${px(clip.start)}px`, width: `${Math.max(6, px(clip.dur))}px` }, onpointerdown: (e) => grab(e, track, clip, "move") },
    h("b", { className: "trim l", onpointerdown: (e) => grab(e, track, clip, "left") }),
    h("span", { className: "clip-name" }, kind === "text" ? `T  ${clip.text}` : clip.name || assetOf(clip.asset)?.name || "Clip"),
    tags.map((t) => h("em", { className: "clip-tag" }, t)),
    keys.map((k) => h("i", { className: "kf", title: `${k.prop} at ${(clip.start + k.t).toFixed(2)}s`, style: { left: `${px(k.t)}px` }, onpointerdown: (e) => (e.stopPropagation(), select(clip.id), seek(clip.start + k.t)) })),
    h("b", { className: "trim r", onpointerdown: (e) => grab(e, track, clip, "right") }),
  );
  if (clip.fade_in) el.append(h("i", { className: "fade in", style: { width: `${px(clip.fade_in)}px` } }));
  if (clip.fade_out) el.append(h("i", { className: "fade out", style: { width: `${px(clip.fade_out)}px` } }));
  return el;
}

function head(track) {
  const visual = track.kind !== "audio";
  const toggle = (patch) => edit([{ op: "update_track", track: track.id, ...patch }]);
  const tracks = S.project.tracks;
  const at = tracks.indexOf(track);
  return h(
    "div",
    { className: "tl-head" },
    h("span", { className: `tl-icon k-${track.kind}` }, ICON[track.kind]),
    h("span", { className: "tl-name" }, track.name),
    h(
      "span",
      { className: "tl-tools" },
      visual && h("button", { title: "Move layer up", onclick: () => edit([{ op: "move_track", track: track.id, index: at + 1 }]) }, "↑"),
      visual && h("button", { title: "Move layer down", onclick: () => edit([{ op: "move_track", track: track.id, index: Math.max(0, at - 1) }]) }, "↓"),
      visual && h("button", { title: track.hidden ? "Show layer" : "Hide layer", className: track.hidden ? "off" : "", onclick: () => toggle({ hidden: !track.hidden }) }, track.hidden ? "◌" : "◉"),
      track.kind !== "text" && h("button", { title: track.muted ? "Unmute track" : "Mute track", className: track.muted ? "off" : "", onclick: () => toggle({ muted: !track.muted }) }, track.muted ? "🔇" : "🔊"),
    ),
  );
}

export function render() {
  if (!S.project) return;
  const width = Math.max(px(duration() + 12), scroller.clientWidth - HEAD);
  inner.style.width = `${HEAD + width}px`;
  const rows = ordered().map((track) =>
    h("div", { className: `tl-row k-${track.kind}${track.hidden ? " hidden" : ""}` }, head(track), h("div", { className: "lane", "data-track": track.id, onpointerdown: scrub, ondragover: (e) => e.preventDefault(), ondrop: (e) => drop(e, track) }, track.clips.map((c) => clipEl(track, c)))),
  );
  const empty = !rows.length && h("div", { className: "tl-empty", ondragover: (e) => e.preventDefault(), ondrop: (e) => drop(e, null) }, "Drop video, music or images here, or ask the agent on the left.");
  inner.replaceChildren(h("div", { className: "tl-row tl-top" }, h("div", { className: "tl-corner" }), ruler(width)), ...rows, empty || "");
  place();
}

function place() {
  const x = px(S.time);
  if (S.playing && x > scroller.scrollLeft + scroller.clientWidth - HEAD - 40) scroller.scrollLeft = x - 80;
}

function timeAt(e) {
  return clamp((e.clientX - inner.getBoundingClientRect().left - HEAD) / S.zoom, 0, 36000);
}

function scrub(e) {
  if (e.target.closest(".clip")) return;
  if (e.target.classList.contains("lane")) select(null);
  seek(timeAt(e));
  const move = (ev) => seek(timeAt(ev));
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", () => window.removeEventListener("pointermove", move), { once: true });
}

function snaps(skip) {
  const points = [0, S.time];
  for (const t of S.project.tracks) for (const c of t.clips) if (c.id !== skip) points.push(c.start, c.start + c.dur);
  return points;
}

function snap(t, points) {
  const reach = 8 / S.zoom;
  let best = t;
  let gap = reach;
  for (const p of points) if (Math.abs(p - t) < gap) [best, gap] = [p, Math.abs(p - t)];
  return best;
}

function grab(e, track, clip, mode) {
  e.stopPropagation();
  if (e.button !== 0) return;
  select(clip.id);
  const el = $(`#clip-${clip.id}`);
  const x0 = e.clientX;
  const o = { ...clip };
  const points = snaps(clip.id);
  const full = assetOf(clip.asset)?.kind !== "image" && assetOf(clip.asset)?.duration;
  let moved = false;
  let next = {};
  let lane = null;
  const move = (ev) => {
    const dt = (ev.clientX - x0) / S.zoom;
    if (!moved && Math.abs(ev.clientX - x0) < 3 && mode === "move" && !lane) return;
    moved = true;
    if (mode === "move") {
      let start = Math.max(0, o.start + dt);
      const a = snap(start, points);
      const b = snap(start + o.dur, points) - o.dur;
      start = a !== start ? a : b;
      next = { start: Math.max(0, start) };
      const hit = document.elementFromPoint(ev.clientX, ev.clientY)?.closest(".lane");
      if (hit && hit !== el.parentElement && (hit.closest(".k-audio") != null) === (track.kind === "audio")) {
        lane = hit;
        hit.append(el);
      }
    } else if (mode === "left") {
      const lo = o.start - o.in / o.speed;
      const start = clamp(snap(o.start + dt, points), Math.max(0, lo), o.start + o.dur - 0.1);
      next = { start, in: o.in + (start - o.start) * o.speed, dur: o.start + o.dur - start };
    } else {
      const max = full ? (full - o.in) / o.speed : 36000;
      next = { dur: clamp(snap(o.start + o.dur + dt, points) - o.start, 0.1, max) };
    }
    Object.assign(clip, next);
    el.style.left = `${px(clip.start)}px`;
    el.style.width = `${Math.max(6, px(clip.dur))}px`;
    emit("seek");
  };
  const up = () => {
    window.removeEventListener("pointermove", move);
    if (!moved) return;
    const target = lane?.dataset.track;
    edit([{ op: "update_clip", clip: clip.id, ...next, ...(target && target !== track.id ? { track: target } : {}) }]);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up, { once: true });
}

async function drop(e, track) {
  e.preventDefault();
  const start = timeAt(e);
  const id = e.dataTransfer.getData("text/agentcut-asset");
  const fill = e.dataTransfer.getData("text/agentcut-fill");
  const target = track ? { track: track.id } : {};
  if (id) return edit([{ op: "add_clip", asset: id, start, ...(track && (assetOf(id)?.kind === "audio") === (track.kind === "audio") ? target : {}) }]);
  if (fill) return edit([{ op: "add_clip", fill, start, ...(track && track.kind !== "audio" ? target : {}) }]);
  await addFiles([...e.dataTransfer.files], start);
}

on("project", render);
on("select", render);
on("time", place);
on("seek", place);
window.addEventListener("resize", render);
