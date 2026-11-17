import { S, h, edit, find, emit, post, valueAt, kindOf, fmt, FONTS, assetOf } from "./state.js";
import { slider, section } from "./panels.js";

const RANGES = { x: [-0.5, 1.5, 0.005], y: [-0.5, 1.5, 0.005], scale: [0.05, 5, 0.01], rotation: [-180, 180, 1], opacity: [0, 1, 0.01], volume: [0, 2, 0.01] };
const LABELS = { x: "Position X", y: "Position Y", scale: "Scale", rotation: "Rotation", opacity: "Opacity", volume: "Volume" };

function keyAt(clip, prop) {
  const local = S.time - clip.start;
  return (clip.keyframes?.[prop] || []).find((k) => Math.abs(k.t - local) < 0.05);
}

function animated(clip, prop) {
  const local = S.time - clip.start;
  const inside = local >= 0 && local <= clip.dur;
  const keys = clip.keyframes?.[prop] || [];
  const here = keyAt(clip, prop);
  const diamond = h(
    "button",
    {
      className: `kf-btn${here ? " on" : keys.length ? " some" : ""}`,
      title: !inside ? "Move the playhead over this clip to keyframe" : here ? "Remove this keyframe" : "Add a keyframe at the playhead",
      disabled: !inside,
      onclick: (e) => {
        e.preventDefault();
        const t = +S.time.toFixed(3);
        edit([here ? { op: "remove_keyframe", clip: clip.id, prop, at: clip.start + here.t } : { op: "keyframe", clip: clip.id, prop, at: t, value: valueAt(clip, prop, local), ease: "ease" }]);
      },
    },
    "◆",
  );
  const value = keys.length && inside ? valueAt(clip, prop, local) : clip[prop];
  const commit = (v) => edit([keys.length && inside ? { op: "keyframe", clip: clip.id, prop, at: +S.time.toFixed(3), value: v, ease: "ease" } : { op: "update_clip", clip: clip.id, [prop]: v }]);
  const live = keys.length ? null : (v) => ((clip[prop] = v), emit("seek"));
  return slider(LABELS[prop], value, RANGES[prop], live, commit, diamond);
}

const num = (label, value, change, step = 0.01) => h("label", { className: "row" }, h("span", { className: "row-label" }, label), h("input", { type: "number", step, value: +(+value).toFixed(3), onchange: (e) => change(+e.target.value) }));

function contrastBadge(clip) {
  const box = h("div", { className: "contrast mini" }, "Checking contrast…");
  const bg = clip.style.bg || S.project.background;
  post("/api/contrast", { foreground: clip.style.color, background: bg }).then((r) => {
    if (r.error) return box.replaceChildren(r.error);
    const fix = r.fix?.foreground;
    box.replaceChildren(
      h("span", { className: `pill ${r.passes ? "ok" : "no"}` }, r.passes ? "AA" : "Below AA"),
      ` ${r.wcag}:1 · Lc ${r.apca}`,
      fix && h("button", { className: "soft", onclick: () => edit([{ op: "update_clip", clip: clip.id, style: { color: fix } }]) }, `Fix → ${fix}`),
      !clip.style.bg && h("small", {}, " Over footage, give the text a background."),
    );
  });
  return box;
}

function textSection(clip) {
  const st = clip.style;
  const set = (style) => edit([{ op: "update_clip", clip: clip.id, style }]);
  return section(
    "Text",
    h("textarea", { rows: 2, value: clip.text, onchange: (e) => edit([{ op: "update_clip", clip: clip.id, text: e.target.value }]) }),
    h("label", { className: "row" }, h("span", { className: "row-label" }, "Font"), h("select", { onchange: (e) => set({ font: e.target.value || null }) }, h("option", { value: "" }, `Project (${S.project.font})`), FONTS.map((f) => h("option", { value: f, selected: st.font === f }, f)))),
    num("Size", st.size, (v) => set({ size: v }), 1),
    h("label", { className: "row" }, h("span", { className: "row-label" }, "Weight"), h("select", { onchange: (e) => set({ weight: +e.target.value }) }, [400, 500, 600, 700, 800].map((w) => h("option", { value: w, selected: st.weight === w }, w)))),
    h("label", { className: "row" }, h("span", { className: "row-label" }, "Colour"), h("input", { type: "color", value: hex(st.color), onchange: (e) => set({ color: e.target.value }) })),
    h("label", { className: "row" }, h("span", { className: "row-label" }, "Background"), h("input", { type: "color", value: hex(st.bg || "#000000"), onchange: (e) => set({ bg: e.target.value }) }), st.bg && h("button", { className: "soft", onclick: () => set({ bg: null }) }, "None")),
    h("div", { className: "row seg" }, h("span", { className: "row-label" }, "Align"), ["left", "center", "right"].map((a) => h("button", { className: st.align === a ? "on" : "", onclick: () => set({ align: a }) }, a))),
    contrastBadge(clip),
  );
}

export function hex(c) {
  if (/^#[0-9a-f]{6}$/i.test(c)) return c;
  if (/^#[0-9a-f]{3}$/i.test(c)) return `#${[...c.slice(1)].map((x) => x + x).join("")}`;
  const m = c.match(/\d+(\.\d+)?/g);
  return m && m.length >= 3 ? `#${m.slice(0, 3).map((v) => Math.round(+v).toString(16).padStart(2, "0")).join("")}` : "#ffffff";
}

function keyList(clip) {
  const rows = Object.entries(clip.keyframes || {}).flatMap(([prop, keys]) =>
    keys.map((k) => h("div", { className: "kf-row" }, h("button", { className: "link", onclick: () => ((S.time = clip.start + k.t), emit("seek"), emit("select")) }, `${fmt(clip.start + k.t)}`), ` ${LABELS[prop] || prop} ${+k.v.toFixed(3)} · ${k.ease}`, h("button", { className: "x", title: "Remove", onclick: () => edit([{ op: "remove_keyframe", clip: clip.id, prop, at: clip.start + k.t }]) }, "×"))),
  );
  return rows.length ? section("Keyframes", ...rows) : "";
}

export function inspector() {
  const hit = S.selected && find(S.selected);
  if (!hit) return h("div", { className: "pane", id: "pane-clip" }, h("h2", {}, "Clip"), h("p", { className: "muted" }, "Click a clip on the timeline to edit its timing, speed, sound, position and keyframes."));
  const { clip, track } = hit;
  const kind = kindOf(clip);
  const media = kind === "video" || kind === "audio";
  const visual = kind !== "audio";
  const up = (patch) => edit([{ op: "update_clip", clip: clip.id, ...patch }]);
  const full = assetOf(clip.asset)?.duration;
  return h(
    "div",
    { className: "pane", id: "pane-clip" },
    h("div", { className: "pane-title" }, h("input", { className: "title-input", value: clip.name, onchange: (e) => up({ name: e.target.value }) }), h("span", { className: `kind k-${kind}` }, kind)),
    h(
      "div",
      { className: "actions" },
      h("button", { className: "soft", onclick: () => edit([{ op: "split", clip: clip.id, at: +S.time.toFixed(3) }]) }, "Split at playhead"),
      h("button", { className: "soft", onclick: () => ((S.time = clip.start), emit("seek")) }, "Go to start"),
      h("button", { className: "soft danger", onclick: () => edit([{ op: "remove_clip", clip: clip.id }]) }, "Delete"),
    ),
    section(
      "Timing",
      num("Start", clip.start, (v) => up({ start: v })),
      num("Duration", clip.dur, (v) => up({ dur: v })),
      media && num("Source in", clip.in, (v) => up({ in: v })),
      media && full ? h("small", { className: "muted" }, `Source is ${fmt(full)} long`) : "",
      media && h("div", { className: "row seg" }, h("span", { className: "row-label" }, "Speed"), [0.25, 0.5, 1, 1.5, 2, 4].map((s) => h("button", { className: Math.abs(clip.speed - s) < 1e-3 ? "on" : "", onclick: () => up({ speed: s }) }, `${s}×`))),
      media && num("Custom speed", clip.speed, (v) => up({ speed: v })),
    ),
    media && section(
      "Sound",
      h("div", { className: "row seg" }, h("span", { className: "row-label" }, "Clip audio"), h("button", { className: clip.muted ? "" : "on", onclick: () => up({ muted: false }) }, "On"), h("button", { className: clip.muted ? "on" : "", onclick: () => up({ muted: true }) }, "Muted")),
      track.muted ? h("small", { className: "muted" }, "This whole track is muted.") : "",
      animated(clip, "volume"),
    ),
    section("Fades", slider("Fade in", clip.fade_in, [0, Math.min(10, clip.dur), 0.05], null, (v) => up({ fade_in: v })), slider("Fade out", clip.fade_out, [0, Math.min(10, clip.dur), 0.05], null, (v) => up({ fade_out: v }))),
    visual && section("Transform", ...["x", "y", "scale", "rotation", "opacity"].map((p) => animated(clip, p))),
    kind === "text" ? textSection(clip) : "",
    kind === "fill" ? section("Colour", h("label", { className: "row" }, h("span", { className: "row-label" }, "Fill"), h("input", { type: "text", value: clip.fill, onchange: (e) => up({ fill: e.target.value }) })), h("small", { className: "muted" }, "A colour, or two joined by > for a gradient.")) : "",
    keyList(clip),
  );
}
