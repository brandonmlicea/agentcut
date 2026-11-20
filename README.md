# AgentCut

A video editor where you do everything yourself, MCP first. The editor fills the right: media, a layered timeline, keyframes, music, effects, speed and mute. The left is your own agent's seat: connect it over MCP, and what it changes shows up there while the editor follows it live. There is no model inside AgentCut. One Rust Worker serves everything: https://agentcut.brandonlicea61.workers.dev

Open source under the MIT license (`LICENSE`): https://github.com/brandonmlicea/agentcut. To run your own, make a D1 database and an R2 bucket named `agentcut`, put the database id in `wrangler.toml`, apply `migrations/` and `wrangler deploy`. [Paper Mache](https://github.com/brandonmlicea/papermache) renders product CGI in Rust and sends its clips straight onto an AgentCut timeline.

## How it fits together

- `src/` is the whole server, in Rust (`worker` 0.8, built by `worker-build` 0.8.5):
  - `ops.rs` is the one edit engine. The editor's own buttons and MCP send the same JSON ops (`add_clip`, `update_clip`, `split`, `keyframe`, `effect`, `match_style`, `move_track`…), so every edit is checked and saved in one place. `model.rs` is the timeline.
  - `moment.rs` reads moments the way people say them. `time` and `span` turn "0:15", "1:02.5", "2m 3s" and "I liked 0:15 to 0:17" into seconds; every op accepts those words, and `"clip": "@0:15"` is the clip on top of the picture then (`"@0:15 audio"` the music). `look` samples a span (its ends, middle, every cut and keyframe inside) and reports each layer bottom to top with its source second, position, scale, rotation, opacity after fades, blur, look and effects, the sound playing, and the cuts, keyframes, speed and fades in it. `match_style` gives another span the look, transform and motion of the clip on top in the first, its keyframes stretched to fit.
  - `mcp.rs` is `POST /mcp`: `look_at` (a span in the person's words: the report and the frames there as pictures), `edit_video` (read with `ops: []`, or edit) and `check_contrast`. Its instructions tell an agent to look before it edits. Keys come from the left panel and are sent as `Authorization: Bearer`. Each MCP edit is logged in `messages` as `mcp`.
  - `auth.rs` is passkey-only sign-in (WebAuthn, ES256, RS256 and Ed25519 checked with the runtime's WebCrypto) and a 400-day session cookie that renews itself daily.
  - `contrast.rs` ports the Design capability's `design_contrast`: WCAG 2 and APCA, with the nearest passing colour at the same hue.
  - `media.rs` streams uploads to R2, serves byte ranges so video can seek, and keeps one 640 px JPEG a second of every video and image (`f/<user>/<asset>/<second>`) for `look_at`.
- `web/` is embedded in the binary with `include_str!`: plain ES modules with no build step. `player.js` draws the frame on a canvas, mixes sound through WebAudio, and exports with MediaRecorder (MP4 where the browser can, otherwise WebM). `agent.js` is the left panel: the MCP key, the agent's edits, a poll of `GET /api/projects/:id/stamp` that reloads the timeline when an agent changes it, and the per-second frames, made in the browser on upload and for any clip that has none.

## Work

- Deploy: `wrangler deploy` here (D1 `agentcut`, R2 `agentcut`). New migration: a new file in `migrations/`, then `wrangler d1 migrations apply agentcut --remote`.
- Tests: `cargo test --lib` covers the edit engine, the time words, `look` and `match_style`, and the contrast math.
