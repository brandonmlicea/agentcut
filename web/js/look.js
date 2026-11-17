import { S, h, edit, find, post, toast, fmt, duration, kindOf, FONTS } from "./state.js";
import { section } from "./panels.js";
import { hex } from "./inspector.js";
import { record } from "./player.js";

const SIZES = [
  ["16:9", 1920, 1080],
  ["9:16", 1080, 1920],
  ["1:1", 1080, 1080],
  ["4:5", 1080, 1350],
];

const pair = { foreground: "#ffffff", background: "#111110", goal: "AA", from: null };

function contrast() {
  const picked = S.selected && find(S.selected)?.clip;
  const text = picked && kindOf(picked) === "text" ? picked : null;
  if (text && pair.from !== text.id) Object.assign(pair, { foreground: hex(text.style.color), background: hex(text.style.bg || S.project.background), from: text.id });
  const result = h("div", { className: "contrast" });
  const sample = h("div", { className: "contrast-sample" }, h("strong", {}, "Readable title"), h("span", {}, "Body text at a smaller size"));
  const run = async () => {
    Object.assign(sample.style, { color: pair.foreground, background: pair.background });
    const r = await post("/api/contrast", pair).catch((e) => ({ error: e.message }));
    if (r.error) return result.replaceChildren(r.error);
    const fix = r.fix && typeof r.fix === "object" ? r.fix.foreground : null;
    result.replaceChildren(
      h("div", { className: "contrast-score" }, h("b", {}, `${r.wcag}:1`), h("span", {}, `APCA Lc ${r.apca}`)),
      h("div", { className: "pills" }, h("span", { className: `pill ${r.AA ? "ok" : "no"}` }, `AA ${r.AA ? "✓" : "✗"}`), h("span", { className: `pill ${r.AAA ? "ok" : "no"}` }, `AAA ${r.AAA ? "✓" : "✗"}`), h("span", { className: `pill ${r.passes ? "ok" : "no"}` }, `${r.goal} goal ${r.passes ? "met" : "missed"}`)),
      h("p", { className: "muted" }, r.use),
      typeof r.fix === "string" ? h("p", { className: "muted" }, r.fix) : "",
      fix && h("button", { className: "soft", onclick: () => ((pair.foreground = fix), fg.value = fix, run()) }, `Use nearest pass ${fix} (${r.fix.wcag}:1)`),
      text && h("button", { className: "press", onclick: () => edit([{ op: "update_clip", clip: text.id, style: { color: pair.foreground, ...(text.style.bg ? { bg: pair.background } : {}) } }]) }, `Apply to “${text.name}”`),
    );
  };
  const fg = h("input", { type: "color", value: pair.foreground, oninput: (e) => ((pair.foreground = e.target.value), run()) });
  const bg = h("input", { type: "color", value: pair.background, oninput: (e) => ((pair.background = e.target.value), run()) });
  run();
  return section(
    "Contrast",
    h("p", { className: "muted" }, text ? `Checking “${text.name}”.` : "Check text colours before they go on screen."),
    h("div", { className: "row" }, h("span", { className: "row-label" }, "Text"), fg, h("span", { className: "row-label" }, "Behind"), bg),
    h("div", { className: "row seg" }, h("span", { className: "row-label" }, "Goal"), ["AA", "AAA"].map((g) => h("button", { className: pair.goal === g ? "on" : "", onclick: (e) => ((pair.goal = g), e.target.parentElement.querySelectorAll("button").forEach((b) => b.classList.toggle("on", b === e.target)), run()) }, g))),
    sample,
    result,
  );
}

export function look() {
  const P = S.project;
  return h(
    "div",
    { className: "pane", id: "pane-look" },
    h("h2", {}, "Look"),
    section(
      "Frame",
      h("div", { className: "row seg" }, SIZES.map(([name, w, hh]) => h("button", { className: P.width === w && P.height === hh ? "on" : "", onclick: () => edit([{ op: "set_project", width: w, height: hh }]) }, name))),
      h("div", { className: "row seg" }, h("span", { className: "row-label" }, "Frame rate"), [24, 30, 60].map((f) => h("button", { className: P.fps === f ? "on" : "", onclick: () => edit([{ op: "set_project", fps: f }]) }, `${f}`))),
      h("label", { className: "row" }, h("span", { className: "row-label" }, "Background"), h("input", { type: "color", value: hex(P.background), onchange: (e) => edit([{ op: "set_project", background: e.target.value }]) })),
      h("label", { className: "row" }, h("span", { className: "row-label" }, "Title font"), h("select", { onchange: (e) => edit([{ op: "set_project", font: e.target.value }]) }, FONTS.map((f) => h("option", { selected: P.font === f }, f)))),
    ),
    contrast(),
  );
}

export function exporter() {
  const bar = h("i", { className: "bar" });
  const status = h("p", { className: "muted" }, `${S.project.width}×${S.project.height} at ${S.project.fps} fps · ${fmt(duration())}`);
  const link = h("div");
  const go = async (e) => {
    if (!duration()) return toast("Add something to the timeline first.", "bad");
    e.target.disabled = true;
    status.textContent = "Rendering in real time… keep this tab open.";
    try {
      const blob = await record((p) => (bar.style.width = `${Math.round(p * 100)}%`));
      const ext = blob.type.includes("mp4") ? "mp4" : "webm";
      const url = URL.createObjectURL(blob);
      const name = `${S.project.name.replace(/[^\w -]+/g, "").trim() || "video"}.${ext}`;
      link.replaceChildren(h("a", { className: "press", href: url, download: name }, `Download ${name}`), h("small", { className: "muted" }, ` ${(blob.size / 1e6).toFixed(1)} MB`));
      status.textContent = "Done.";
    } catch (err) {
      status.textContent = err.message;
    }
    e.target.disabled = false;
  };
  return h("div", { className: "pane", id: "pane-export" }, h("h2", {}, "Export"), status, h("div", { className: "progress" }, bar), h("button", { className: "press", onclick: go }, "Export video"), link, h("p", { className: "muted" }, "The edit plays once with its sound to make the file: MP4 where the browser can, otherwise WebM."));
}

export function mcp() {
  const out = h("div");
  const make = async () => {
    const r = await post("/api/mcp-key", {});
    const cli = `claude mcp add --transport http agentcut ${r.url} --header "Authorization: Bearer ${r.key}"`;
    const json = JSON.stringify({ mcpServers: { agentcut: { type: "http", url: r.url, headers: { Authorization: `Bearer ${r.key}` } } } }, null, 2);
    const copy = (t) => h("button", { className: "soft", onclick: () => navigator.clipboard.writeText(t).then(() => toast("Copied")) }, "Copy");
    out.replaceChildren(section("Claude Code", h("pre", {}, cli), copy(cli)), section("Any MCP client", h("pre", {}, json), copy(json)), h("p", { className: "muted" }, "This key is shown once. Making a new one turns this one off."));
  };
  return h(
    "div",
    { className: "pane", id: "pane-mcp" },
    h("h2", {}, "MCP"),
    h("p", { className: "muted" }, "Let any agent edit your videos. One tool, edit_video, reads the timeline and applies cuts, speed, mutes, keyframes, music, effects and layers; check_contrast checks text colours."),
    h("button", { className: "press", onclick: make }, "Make an MCP key"),
    out,
  );
}
