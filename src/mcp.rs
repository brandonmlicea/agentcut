use crate::agent::OPS_GUIDE;
use crate::{contrast, store};
use serde_json::{json, Value};
use worker::{Bucket, D1Database, Result};

fn tools() -> Value {
    json!([
        {
            "name": "edit_video",
            "description": format!("Edit an AgentCut video timeline and read it back. Send ops [] to read the project, its tracks and clips, and the media library with asset ids. project is a project id or name; it defaults to the most recently edited one, and a new name creates a project.\n{OPS_GUIDE}"),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project id or name." },
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

async fn edit_video(db: &D1Database, bucket: &Bucket, user: &str, args: &Value) -> Result<Value> {
    let wanted = args["project"].as_str().map(str::trim).filter(|s| !s.is_empty());
    let projects = store::list(db, user).await?;
    let pid = match wanted {
        Some(w) => match projects.iter().find(|p| p.id == w || p.name.eq_ignore_ascii_case(w)) {
            Some(p) => p.id.clone(),
            None => store::create(db, user, Some(w.to_string())).await?,
        },
        None => match projects.first() {
            Some(p) => p.id.clone(),
            None => store::create(db, user, None).await?,
        },
    };
    let ops = args["ops"].as_array().cloned().unwrap_or_default();
    let Some((project, outcome, library)) = store::edit(db, bucket, user, &pid, ops).await? else {
        return Ok(json!({ "error": "project not found" }));
    };
    let media: Vec<Value> = library.iter().map(|a| json!({ "id": a.id, "name": a.name, "kind": a.kind, "duration": a.duration })).collect();
    Ok(json!({ "project_id": pid, "done": outcome.done, "errors": outcome.errors, "seek": outcome.seek, "duration": project.duration(), "project": project, "media": media }))
}

pub async fn handle(db: &D1Database, bucket: &Bucket, user: &str, body: Value) -> Result<Option<Value>> {
    let id = body.get("id").cloned();
    let method = body["method"].as_str().unwrap_or_default();
    let Some(id) = id else { return Ok(None) };
    let result = match method {
        "initialize" => json!({
            "protocolVersion": body["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "agentcut", "version": "0.1.0" },
            "instructions": "AgentCut edits video timelines. Call edit_video with ops [] first to see the project and media ids."
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let args = &body["params"]["arguments"];
            let out = match body["params"]["name"].as_str() {
                Some("edit_video") => edit_video(db, bucket, user, args).await?,
                Some("check_contrast") => contrast::check(
                    args["foreground"].as_str().unwrap_or_default(),
                    args["background"].as_str().unwrap_or_default(),
                    args["goal"].as_str().unwrap_or("AA"),
                    args["adjust"].as_str().unwrap_or("foreground"),
                ),
                other => return Ok(Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": format!("unknown tool {other:?}") } }))),
            };
            let failed = out.get("error").is_some();
            json!({ "content": [{ "type": "text", "text": out.to_string() }], "isError": failed })
        }
        _ => return Ok(Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("no method {method}") } }))),
    };
    Ok(Some(json!({ "jsonrpc": "2.0", "id": id, "result": result })))
}
