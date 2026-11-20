use crate::{contrast, media, moment, store};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use worker::{Bucket, D1Database, Result};

pub const OPS_GUIDE: &str = r##"Times are seconds on the timeline, or words: "0:15", "1:02.5", "15s", "2m 3s". Tracks are layers: later tracks draw on top; a clip goes on the first track of its kind with room, else a new layer above.
A clip is its id, or "@<time>" for the clip on top of the picture at that moment ("@0:15"), or "@<time> audio" for the music there.
Ops (JSON objects):
- {"op":"add_clip","asset":"<asset id>","start":0,"dur":5,"in":0} or {"op":"add_clip","text":"Title","style":{...}} or {"op":"add_clip","fill":"#hex"} (a colour card; "#hex>#hex" is a gradient). Optional "track":"<track id>".
- {"op":"update_clip","clip":"<id>", any of: start, dur, in (source seconds), speed (0.1-16, changes dur), volume (0-2), muted, fade_in, fade_out, x, y (0-1, centre of frame is 0.5), scale, rotation (degrees), opacity (0-1), name, text, fill, "track":"<id to move to>", "effects":{"brightness":1,"contrast":1,"saturate":1,"blur":0,"grayscale":0,"sepia":0,"hue":0,"vignette":0}, "style":{"font":"Google font","size":96,"weight":600,"color":"#fff","bg":null,"align":"center"}}
- {"op":"split","clip":"<id>","at":12.5}
- {"op":"remove_clip","clip":"<id>"}
- {"op":"keyframe","clip":"<id>","prop":"x|y|scale|rotation|opacity|volume|blur","at":12.5,"value":1.2,"ease":"linear|ease|hold"}
- {"op":"remove_keyframe","clip":"<id>","prop":"scale","at":12.5} (no "at" clears that prop)
- {"op":"effect","clip":"<id>","preset":"none|bw|vintage|vivid|cool|warm|dreamy|noir|fade"}
- {"op":"match_style","from":"0:15 to 0:17","to":"0:30 to 0:34"} (or "clip":"<id>" instead of "to"; "speed":true also copies speed): gives what is on screen in "to" the look, transform and motion of the clip on top in "from", its keyframes stretched to fit
- {"op":"add_track","kind":"video|text|audio"} {"op":"update_track","track":"<id>","muted":true,"hidden":false,"name":"..."} {"op":"move_track","track":"<id>","index":0} (0 is the bottom layer) {"op":"remove_track","track":"<id>"}
- {"op":"set_project","name":"...","width":1920,"height":1080,"fps":30,"background":"#000","font":"Inter"}
- {"op":"import_url","url":"https://...","name":"...","start":0} downloads video, audio or an image into the media library and puts it on the timeline
- {"op":"seek","at":20} moves the playhead to show a moment"##;

const INSTRUCTIONS: &str = "AgentCut is a video editor the person drives by hand; you are their agent over MCP. When they name a moment (\"I like 0:15 to 0:17\", \"the bit at 1:02\"), call look_at with their words first: it returns the frames there, every layer, transform, effect, keyframe, cut and fade, so you edit from what is on screen instead of guessing. Then call edit_video. Pass their times as they said them; ops accept \"0:15\" and \"@0:15\" for the clip on screen then, and match_style carries one moment's look onto another.";

fn tools() -> Value {
    let project = json!({ "type": "string", "description": "Project id or name. Defaults to the most recently edited one." });
    json!([
        {
            "name": "look_at",
            "description": "Look at a moment of the video before editing it. when is the person's own words or times: \"0:15 to 0:17\", \"from 15s to 17s\", \"1:02\". Returns the frames at its start, middle and end as pictures (the source footage on top), and for every sampled frame each layer bottom to top with its clip id, source second, position, scale, rotation, opacity after fades, blur, look and effects, text and style, the sound playing, plus the cuts, keyframes, speed changes and fades inside.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "when": { "type": "string", "description": "A moment or span, as said: \"0:15 to 0:17\"." },
                    "pictures": { "type": "boolean", "description": "Include the frames as pictures (default true)." }
                },
                "required": ["when"]
            }
        },
        {
            "name": "edit_video",
            "description": format!("Edit an AgentCut video timeline and read it back. Send ops [] to read the project, its tracks and clips, and the media library with asset ids. A new project name creates a project.\n{OPS_GUIDE}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "ops": { "type": "array", "items": { "type": "object" }, "description": "Edits to apply in order." }
                },
                "required": ["ops"]
            }
        },
        {
            "name": "check_contrast",
            "description": "Contrast of text on a background: WCAG 2 ratio and APCA Lc, against AA (4.5:1 and Lc 60) or AAA (7:1 and Lc 75), with the nearest passing colour at the same hue when it falls short. Hex, rgb() or oklch().",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "foreground": { "type": "string" },
                    "background": { "type": "string" },
                    "goal": { "type": "string", "enum": ["AA", "AAA"] },
                    "adjust": { "type": "string", "enum": ["foreground", "background"] }
                },
                "required": ["foreground", "background"]
            }
        }
    ])
}

async fn project_id(db: &D1Database, user: &str, args: &Value) -> Result<String> {
    let wanted = args["project"].as_str().map(str::trim).filter(|s| !s.is_empty());
    let projects = store::list(db, user).await?;
    Ok(match wanted {
        Some(w) => match projects.iter().find(|p| p.id == w || p.name.eq_ignore_ascii_case(w)) {
            Some(p) => p.id.clone(),
            None => store::create(db, user, Some(w.to_string())).await?,
        },
        None => match projects.first() {
            Some(p) => p.id.clone(),
            None => store::create(db, user, None).await?,
        },
    })
}

async fn edit_video(db: &D1Database, bucket: &Bucket, user: &str, args: &Value) -> Result<Value> {
    let pid = project_id(db, user, args).await?;
    let ops = args["ops"].as_array().cloned().unwrap_or_default();
    let asked = ops.len();
    let Some((project, outcome, library)) = store::edit(db, bucket, user, &pid, ops).await? else {
        return Ok(json!({ "error": "project not found" }));
    };
    if asked > 0 {
        let meta = json!({ "done": outcome.done, "errors": outcome.errors, "seek": outcome.seek }).to_string();
        store::add_message(db, &pid, "mcp", &format!("{} edit{}", outcome.done.len(), if outcome.done.len() == 1 { "" } else { "s" }), Some(meta)).await?;
    }
    let media: Vec<Value> = library.iter().map(|a| json!({ "id": a.id, "name": a.name, "kind": a.kind, "duration": a.duration })).collect();
    Ok(json!({ "project_id": pid, "done": outcome.done, "errors": outcome.errors, "seek": outcome.seek, "duration": project.duration(), "project": project, "media": media }))
}

async fn look_at(db: &D1Database, bucket: &Bucket, user: &str, args: &Value) -> Result<Vec<Value>> {
    let said = args["when"].as_str().unwrap_or_default();
    let Some((a, b)) = moment::span(said) else {
        return Ok(vec![json!({ "type": "text", "text": json!({ "error": format!("\"{said}\" is not a time; say it like \"0:15 to 0:17\"") }).to_string() })]);
    };
    let pid = project_id(db, user, args).await?;
    let Some(project) = store::load(db, user, &pid).await? else {
        return Ok(vec![]);
    };
    let assets = media::infos(db, user).await?;
    let (report, stills) = moment::look(&project, &assets, a, b);
    let mut out = vec![json!({ "type": "text", "text": json!({ "project_id": pid, "duration": project.duration(), "moment": report }).to_string() })];
    if args["pictures"].as_bool() != Some(false) {
        for s in stills {
            match media::frame(bucket, user, &s.asset, s.second).await? {
                Some(bytes) => {
                    out.push(json!({ "type": "text", "text": format!("Frame at {:.2}s: source second {} of {}", s.at, s.second, s.asset) }));
                    out.push(json!({ "type": "image", "data": STANDARD.encode(bytes), "mimeType": "image/jpeg" }));
                }
                None => out.push(json!({ "type": "text", "text": format!("No picture yet for {:.2}s: open the project in AgentCut once and it makes them", s.at) })),
            }
        }
    }
    Ok(out)
}

pub async fn handle(db: &D1Database, bucket: &Bucket, user: &str, body: Value) -> Result<Option<Value>> {
    let id = body.get("id").cloned();
    let method = body["method"].as_str().unwrap_or_default();
    let Some(id) = id else { return Ok(None) };
    let result = match method {
        "initialize" => json!({
            "protocolVersion": body["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "agentcut", "version": "0.2.0" },
            "instructions": INSTRUCTIONS
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let args = &body["params"]["arguments"];
            let content = match body["params"]["name"].as_str() {
                Some("look_at") => look_at(db, bucket, user, args).await?,
                Some("edit_video") => vec![json!({ "type": "text", "text": edit_video(db, bucket, user, args).await?.to_string() })],
                Some("check_contrast") => vec![json!({ "type": "text", "text": contrast::check(
                    args["foreground"].as_str().unwrap_or_default(),
                    args["background"].as_str().unwrap_or_default(),
                    args["goal"].as_str().unwrap_or("AA"),
                    args["adjust"].as_str().unwrap_or("foreground"),
                ).to_string() })],
                other => return Ok(Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": format!("unknown tool {other:?}") } }))),
            };
            let failed = content.first().and_then(|c| c["text"].as_str()).is_some_and(|t| t.starts_with("{\"error\""));
            json!({ "content": content, "isError": failed })
        }
        _ => return Ok(Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("no method {method}") } }))),
    };
    Ok(Some(json!({ "jsonrpc": "2.0", "id": id, "result": result })))
}
