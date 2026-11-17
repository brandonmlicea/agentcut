import { S, on, $, h, post, api, remember, setProject, seek, find, fmt } from "./state.js";

const log = $("#chat-log");
const form = $("#composer");
const prompt = $("#prompt");
const context = $("#chat-context");
let busy = false;

const STARTERS = [
  ["A", "Cut the clip at the playhead", "Splits whatever is under the playhead"],
  ["B", "Add a title that says Hello", "A white title over the first three seconds"],
  ["C", "Speed up the first clip 2× and mute it", "Faster, silent footage"],
];

function bubble(role, text, meta) {
  const done = meta?.done || [];
  const errors = meta?.errors || [];
  const notes = meta?.notes || [];
  const model = (meta?.model || S.model).split("/").pop();
  return h(
    "div",
    { className: `msg ${role}` },
    role === "assistant" && h("div", { className: "msg-who" }, h("img", { src: "/mark.svg", alt: "" }), "AgentCut", h("span", {}, model)),
    h("div", { className: "msg-body" }, text),
    done.length ? h("ul", { className: "msg-done" }, done.map((d) => h("li", {}, d))) : "",
    notes.length ? h("div", { className: "msg-notes" }, notes.map((n) => h("div", {}, n))) : "",
    errors.length ? h("details", { className: "msg-errors" }, h("summary", {}, `${errors.length} edit${errors.length > 1 ? "s" : ""} didn't apply`), errors.map((e) => h("div", {}, e))) : "",
  );
}

function starters() {
  return h(
    "div",
    { className: "starters", id: "chat-starters" },
    h("h4", {}, "What should we make?"),
    STARTERS.map(([key, line, note]) => h("button", { onclick: () => send(line) }, h("kbd", {}, key), h("span", {}, h("b", {}, line), h("small", {}, note)))),
    h("button", { onclick: () => prompt.focus() }, h("kbd", {}, "D"), h("span", { className: "muted" }, "Or tell me what to change")),
  );
}

export function renderChat(messages) {
  const rows = messages.map((m) => bubble(m.role, m.content, m.meta ? JSON.parse(m.meta) : null));
  log.replaceChildren(h("div", { className: "day" }, "Today"), h("div", { className: "msg assistant" }, h("div", { className: "msg-who" }, h("img", { src: "/mark.svg", alt: "" }), "AgentCut"), h("div", { className: "msg-body" }, "Upload footage or music on the right, then tell me what to do. I see the playhead and the clip you picked, so “cut here” and “mute this” just work.")), ...rows, rows.length ? "" : starters());
  log.scrollTop = log.scrollHeight;
}

function showContext() {
  if (!S.project) return;
  const hit = S.selected && find(S.selected);
  context.replaceChildren(h("span", { className: "dot" }), `Playhead ${fmt(S.time)}`, hit ? ` · “${hit.clip.name || hit.clip.text}” selected` : "");
}

async function send(text) {
  text = text.trim();
  if (!text || busy) return;
  busy = true;
  $("#chat-starters")?.remove();
  log.append(bubble("user", text));
  const thinking = h("div", { className: "msg assistant thinking" }, h("div", { className: "msg-who" }, h("img", { src: "/mark.svg", alt: "" }), "AgentCut", h("span", {}, "editing")), h("div", { className: "msg-body" }, h("i"), h("i"), h("i")));
  log.append(thinking);
  log.scrollTop = log.scrollHeight;
  prompt.value = "";
  grow();
  const before = JSON.stringify(S.project);
  try {
    const out = await post(`/api/projects/${S.pid}/chat`, { message: text, playhead: +S.time.toFixed(3), selected: S.selected });
    if (out.done.length) {
      remember(before);
      setProject(out.project);
      api("/api/assets").then((assets) => setProject(S.project, assets)).catch(() => {});
    }
    if (out.seek != null) seek(out.seek);
    thinking.replaceWith(bubble("assistant", out.reply, out));
  } catch (e) {
    thinking.replaceWith(bubble("assistant", `Something went wrong: ${e.message}`));
  }
  log.scrollTop = log.scrollHeight;
  busy = false;
}

function grow() {
  prompt.style.height = "auto";
  prompt.style.height = `${Math.min(prompt.scrollHeight, 180)}px`;
}

form.addEventListener("submit", (e) => (e.preventDefault(), send(prompt.value)));
prompt.addEventListener("input", grow);
prompt.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) (e.preventDefault(), send(prompt.value));
});

on("time", showContext);
on("seek", showContext);
on("select", showContext);
on("project", showContext);

export function menu(anchor, items) {
  document.querySelector(".menu")?.remove();
  const box = h("div", { className: "menu", id: "menu" }, items.map(([label, fn, cls]) => (label === "-" ? h("hr") : h("button", { className: cls || "", onclick: () => (box.remove(), fn()) }, label))));
  const r = anchor.getBoundingClientRect();
  Object.assign(box.style, { left: `${r.left}px`, top: `${r.bottom + 6}px` });
  document.body.append(box);
  setTimeout(() => document.addEventListener("pointerdown", (e) => !box.contains(e.target) && box.remove(), { once: true }));
}

