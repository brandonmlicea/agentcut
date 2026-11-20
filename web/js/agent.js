import { S, h, $, api, post, toast, setProject, remember, seek } from "./state.js";

const log = $("#agent-log");
const connect = $("#agent-connect");
const made = new Set();
let stamp = 0;

function row(m) {
  const meta = m.meta ? JSON.parse(m.meta) : {};
  const done = meta.done || [];
  const errors = meta.errors || [];
  const when = new Date(m.created).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return h(
    "div",
    { className: "msg assistant" },
    h("div", { className: "msg-who" }, h("img", { src: "/mark.svg", alt: "" }), "Your agent", h("span", {}, when)),
    h("div", { className: "msg-body" }, m.content),
    done.length ? h("ul", { className: "msg-done" }, done.map((d) => h("li", {}, d))) : "",
    errors.length ? h("details", { className: "msg-errors" }, h("summary", {}, `${errors.length} edit${errors.length > 1 ? "s" : ""} didn't apply`), errors.map((e) => h("div", {}, e))) : "",
  );
}

export function renderAgent(messages) {
  const rows = messages.filter((m) => m.role === "mcp").map(row);
  log.replaceChildren(...(rows.length ? rows : [h("p", { className: "muted agent-none" }, "Nothing yet. Edits your agent makes over MCP show up here, and the editor follows them live.")]));
  log.scrollTop = log.scrollHeight;
}

async function key() {
  const r = await post("/api/mcp-key", {});
  const cli = `claude mcp add --transport http agentcut ${r.url} --header "Authorization: Bearer ${r.key}"`;
  const json = JSON.stringify({ mcpServers: { agentcut: { type: "http", url: r.url, headers: { Authorization: `Bearer ${r.key}` } } } }, null, 2);
  const copy = (t) => h("button", { className: "soft", onclick: () => navigator.clipboard.writeText(t).then(() => toast("Copied")) }, "Copy");
  connect.replaceChildren(
    h("h4", {}, "Claude Code"), h("pre", {}, cli), copy(cli),
    h("h4", {}, "Any MCP client"), h("pre", {}, json), copy(json),
    h("p", { className: "muted" }, "This key is shown once. Making a new one turns this one off."),
  );
}
$("#agent-key").addEventListener("click", key);
$("#empty-connect").addEventListener("click", () => ($("#agent-key")?.click(), $("#agent").scrollIntoView({ behavior: "smooth" })));

async function follow() {
  if (!S.pid || document.hidden) return;
  const r = await api(`/api/projects/${S.pid}/stamp`).catch(() => null);
  if (!r?.updated || r.updated === stamp) return;
  const first = !stamp;
  stamp = r.updated;
  if (first) return;
  const p = await api(`/api/projects/${S.pid}`);
  const before = JSON.stringify(S.project);
  if (JSON.stringify(p.project) === before) return;
  remember(before);
  setProject(p.project);
  renderAgent(p.messages);
  const last = p.messages.filter((m) => m.role === "mcp").pop();
  const at = last?.meta && JSON.parse(last.meta).seek;
  if (at != null) seek(at);
  api("/api/assets").then((assets) => setProject(S.project, assets)).catch(() => {});
}
setInterval(follow, 2500);
export const restamp = () => (stamp = 0);

export async function stills(asset) {
  if (!asset || made.has(asset.id) || !["video", "image"].includes(asset.kind)) return;
  made.add(asset.id);
  if ((await fetch(`/api/assets/${asset.id}/frames/0`).catch(() => null))?.ok) return;
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  const fit = (w, hgt) => {
    const k = Math.min(1, 640 / w);
    canvas.width = Math.round(w * k);
    canvas.height = Math.round(hgt * k);
  };
  const put = (sec) => new Promise((ok) => canvas.toBlob((b) => fetch(`/api/assets/${asset.id}/frames/${sec}`, { method: "PUT", headers: { "content-type": "image/jpeg" }, body: b }).then(ok, ok), "image/jpeg", 0.72));
  const src = `/media/${asset.id}`;
  if (asset.kind === "image") {
    const img = new Image();
    img.src = src;
    await img.decode().catch(() => null);
    if (!img.naturalWidth) return;
    fit(img.naturalWidth, img.naturalHeight);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    return put(0);
  }
  const v = Object.assign(document.createElement("video"), { muted: true, preload: "auto", src });
  await new Promise((ok) => ((v.onloadedmetadata = ok), (v.onerror = ok)));
  if (!v.videoWidth) return;
  fit(v.videoWidth, v.videoHeight);
  const last = Math.min(Math.floor(v.duration || 0), 900);
  const note = toast(`Making frames for your agent: ${asset.name}`);
  for (let s = 0; s <= last; s++) {
    v.currentTime = Math.min(s + 0.04, Math.max(0, v.duration - 0.05));
    await new Promise((ok) => (v.onseeked = ok));
    ctx.drawImage(v, 0, 0, canvas.width, canvas.height);
    await put(s);
    if (note) note.textContent = `Making frames for your agent: ${asset.name} · ${s + 1}/${last + 1}`;
  }
  note?.remove();
}

export function menu(anchor, items) {
  document.querySelector(".menu")?.remove();
  const box = h("div", { className: "menu", id: "menu" }, items.map(([label, fn, cls]) => (label === "-" ? h("hr") : h("button", { className: cls || "", onclick: () => (box.remove(), fn()) }, label))));
  const r = anchor.getBoundingClientRect();
  Object.assign(box.style, { left: `${r.left}px`, top: `${r.bottom + 6}px` });
  document.body.append(box);
  setTimeout(() => document.addEventListener("pointerdown", (e) => !box.contains(e.target) && box.remove(), { once: true }));
}
