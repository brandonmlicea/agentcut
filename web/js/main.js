import { S, on, emit, $, api, post, edit, undo, redo, find, seek, select, fmt, duration, toast, useFont } from "./state.js";
import { toggle, draw } from "./player.js";
import "./timeline.js";
import { media, text, effects, upload } from "./panels.js";
import { inspector } from "./inspector.js";
import { look, exporter, mcp } from "./look.js";
import { renderChat, menu } from "./chat.js";

const PANES = { media, text, clip: inspector, effects, look, export: exporter, mcp };
const side = $("#side");

function renderSide() {
  if (!S.project) return;
  side.replaceChildren(PANES[S.tab]());
  for (const b of document.querySelectorAll("#dock [data-tab]")) b.classList.toggle("on", b.dataset.tab === S.tab);
}

function clock() {
  $("#clock").textContent = `${fmt(S.time)} / ${fmt(duration())}`;
}

function underPlayhead() {
  return S.project.tracks.flatMap((t) => (t.hidden && t.kind !== "audio" ? [] : t.clips)).filter((c) => c.start + 0.05 < S.time && S.time < c.start + c.dur - 0.05);
}

function split() {
  const picked = S.selected && find(S.selected)?.clip;
  const targets = picked && underPlayhead().includes(picked) ? [picked] : underPlayhead();
  if (!targets.length) return toast("Move the playhead over a clip to split it.");
  edit(targets.map((c) => ({ op: "split", clip: c.id, at: +S.time.toFixed(3) })));
}

function remove() {
  if (S.selected) edit([{ op: "remove_clip", clip: S.selected }]);
}

const frame = () => 1 / (S.project?.fps || 30);
const ACTIONS = {
  "#play": toggle,
  "#to-start": () => seek(0),
  "#to-end": () => seek(duration()),
  "#back": () => seek(S.time - 1),
  "#ahead": () => seek(S.time + 1),
  "#split": split,
  "#delete": remove,
  "#undo": undo,
  "#redo": redo,
};
for (const [sel, fn] of Object.entries(ACTIONS)) $(sel).addEventListener("click", fn);

document.addEventListener("keydown", (e) => {
  if (e.target.closest("input, textarea, select, [contenteditable]")) return;
  const mod = e.ctrlKey || e.metaKey;
  const keys = {
    " ": toggle,
    s: split,
    S: split,
    Delete: remove,
    Backspace: remove,
    ArrowLeft: () => seek(S.time - (e.shiftKey ? 1 : frame())),
    ArrowRight: () => seek(S.time + (e.shiftKey ? 1 : frame())),
    Home: () => seek(0),
    End: () => seek(duration()),
    Escape: () => select(null),
  };
  const fn = mod ? { z: e.shiftKey ? redo : undo, Z: redo, y: redo }[e.key] : keys[e.key];
  if (!fn) return;
  e.preventDefault();
  fn();
});

for (const b of document.querySelectorAll("#dock [data-tab]")) b.addEventListener("click", () => ((S.tab = b.dataset.tab), renderSide()));

const desk = $("#desk");
desk.addEventListener("dragover", (e) => e.preventDefault());
desk.addEventListener("drop", async (e) => {
  if (e.defaultPrevented || !e.dataTransfer.files.length) return;
  e.preventDefault();
  S.tab = "media";
  renderSide();
  for (const f of e.dataTransfer.files) await upload(f);
});

async function open(pid) {
  const r = await api(`/api/projects/${pid}`);
  Object.assign(S, { pid, project: r.project, past: [], future: [], selected: null, time: 0 });
  localStorage.setItem("agentcut.project", pid);
  useFont(S.project.font);
  renderChat(r.messages);
  emit("project");
  emit("seek");
}

async function refreshProjects() {
  S.projects = (await api("/api/me")).projects;
}

$("#project-pill").addEventListener("click", async (e) => {
  await refreshProjects();
  const anchor = e.currentTarget;
  menu(anchor, [
    ...S.projects.map((p) => [`${p.id === S.pid ? "● " : ""}${p.name}`, () => open(p.id)]),
    ["-"],
    ["New project", newProject],
    ["Rename this project", () => { const name = prompt("Project name", S.project.name); if (name) edit([{ op: "set_project", name }]); }],
    ["Delete this project", async () => { if (!confirm(`Delete ${S.project.name}?`)) return; await api(`/api/projects/${S.pid}`, { method: "DELETE" }); await refreshProjects(); S.projects.length ? open(S.projects[0].id) : newProject(); }, "danger"],
  ]);
});

async function newProject() {
  const { id } = await post("/api/projects", { name: "Untitled video" });
  await open(id);
  toast("New project ready.");
}
$("#new-project").addEventListener("click", newProject);

const askAgent = (text) => {
  const box = $("#prompt");
  if (text) box.value = text;
  box.focus();
};
$("#ask-agent").addEventListener("click", () => askAgent());
$("#empty-ask").addEventListener("click", () => askAgent("Make a 15 second cut from my clips, put the music under it and add a title"));
$("#empty-upload").addEventListener("click", () => {
  S.tab = "media";
  renderSide();
  $("#pane-media input[type=file]").click();
});

$("#account").addEventListener("click", (e) =>
  menu(e.currentTarget, [["Sign out", async () => { await post("/auth/logout", {}); location.href = "/login"; }, "danger"]]),
);

on("project", () => {
  $("#project-name").textContent = S.project.name;
  $("#win-meta").textContent = `${S.project.width}×${S.project.height} · ${S.project.fps} fps`;
  document.title = `${S.project.name} · AgentCut`;
  renderSide();
  clock();
  $("#stage-empty").hidden = duration() > 0;
});
on("select", renderSide);
on("assets", () => S.tab === "media" && renderSide());
on("upload", () => S.tab === "media" && renderSide());
on("seek", () => {
  clock();
  if (S.tab === "clip" && !S.playing && !side.contains(document.activeElement)) renderSide();
});
on("time", clock);
on("play", () => ($("#play").textContent = S.playing ? "❚❚" : "▶"));

(async () => {
  try {
    const me = await api("/api/me");
    Object.assign(S, { assets: me.assets, projects: me.projects, model: me.model });
    let pid = localStorage.getItem("agentcut.project");
    if (!me.projects.some((p) => p.id === pid)) pid = me.projects[0]?.id;
    if (!pid) pid = (await post("/api/projects", { name: "My first video" })).id;
    await open(pid);
  } catch (err) {
    toast(err.message, "bad");
  }
  draw();
})();
