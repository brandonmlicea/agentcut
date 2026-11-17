# AgentCut

A video editor you talk to. Chat on the left, the editor on the right: media, a layered timeline, keyframes, music, effects, speed and mute. One Rust Worker serves everything: https://agentcut.brandonlicea61.workers.dev

## How it fits together

- `src/` is the whole server, in Rust (`worker` 0.8, built by `worker-build` 0.8.5):
  - `ops.rs` is the one edit engine. The editor's own buttons, the chat agent and MCP all send the same JSON ops (`add_clip`, `update_clip`, `split`, `keyframe`, `effect`, `move_track`…), so every edit is checked and saved in one place. `model.rs` is the timeline.
  - `agent.rs` asks Celeris (`celeris/celeris-1`) through Star's `ScaffoldBridge` for `{reply, ops}`, applies them and gives failures back once. If Celeris doesn't answer, GLM 5.3 Flash on Baseten does, and the reply names the model.
  - `mcp.rs` is `POST /mcp`: `edit_video` (read with `ops: []`, or edit) and `check_contrast`. Keys come from the MCP tab and are sent as `Authorization: Bearer`.
  - `auth.rs` is passkey-only sign-in (WebAuthn, ES256, RS256 and Ed25519 checked with the runtime's WebCrypto) and a 400-day session cookie that renews itself daily.
  - `contrast.rs` ports the Design capability's `design_contrast`: WCAG 2 and APCA, with the nearest passing colour at the same hue.
  - `media.rs` streams uploads to R2 and serves byte ranges so video can seek.
- `web/` is embedded in the binary with `include_str!`: plain ES modules with no build step. `player.js` draws the frame on a canvas, mixes sound through WebAudio, and exports with MediaRecorder (MP4 where the browser can, otherwise WebM).

## Work

- Deploy: `wrangler deploy` here (D1 `agentcut`, R2 `agentcut`, service `star#ScaffoldBridge`). New migration: a new file in `migrations/`, then `wrangler d1 migrations apply agentcut --remote`.
- Tests: `cargo test --lib` covers the edit engine and the contrast math.
