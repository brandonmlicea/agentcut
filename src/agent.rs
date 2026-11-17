use crate::media::Asset;
use crate::model::Project;
use crate::store;
use crate::util::secs;
use js_sys::{Function, Promise, Reflect, JSON};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use worker::{Bucket, D1Database, Env};

pub const MODEL: &str = "celeris/celeris-1";

const FALLBACK: &str = "baseten/zai-org/GLM-5.3-Flash";

pub const OPS_GUIDE: &str = r##"Times are seconds on the timeline. Tracks are layers: later tracks draw on top; a clip goes on the first track of its kind with room, else a new layer above.
Ops (JSON objects):
- {"op":"add_clip","asset":"<asset id>","start":0,"dur":5,"in":0} or {"op":"add_clip","text":"Title","style":{...}} or {"op":"add_clip","fill":"#hex"} (a colour card; "#hex>#hex" is a gradient). Optional "track":"<track id>".
- {"op":"update_clip","clip":"<id>", any of: start, dur, in (source seconds), speed (0.1-16, changes dur), volume (0-2), muted, fade_in, fade_out, x, y (0-1, centre of frame is 0.5), scale, rotation (degrees), opacity (0-1), name, text, fill, "track":"<id to move to>", "effects":{"brightness":1,"contrast":1,"saturate":1,"blur":0,"grayscale":0,"sepia":0,"hue":0,"vignette":0}, "style":{"font":"Google font","size":96,"weight":600,"color":"#fff","bg":null,"align":"center"}}
- {"op":"split","clip":"<id>","at":12.5}
- {"op":"remove_clip","clip":"<id>"}
- {"op":"keyframe","clip":"<id>","prop":"x|y|scale|rotation|opacity|volume|blur","at":12.5,"value":1.2,"ease":"linear|ease|hold"}
- {"op":"remove_keyframe","clip":"<id>","prop":"scale","at":12.5} (no "at" clears that prop)
- {"op":"effect","clip":"<id>","preset":"none|bw|vintage|vivid|cool|warm|dreamy|noir|fade"}
- {"op":"add_track","kind":"video|text|audio"} {"op":"update_track","track":"<id>","muted":true,"hidden":false,"name":"..."} {"op":"move_track","track":"<id>","index":0} (0 is the bottom layer) {"op":"remove_track","track":"<id>"}
- {"op":"set_project","name":"...","width":1920,"height":1080,"fps":30,"background":"#000","font":"Inter"}
- {"op":"import_url","url":"https://...","name":"...","start":0} downloads video, audio or an image into the media library and puts it on the timeline
- {"op":"seek","at":20} moves the playhead to show a moment"##;

const SYSTEM: &str = r#"You are AgentCut, a video editing agent beside a timeline editor. Answer with one JSON object and nothing else: {"reply":"<one or two short sentences to the person>","ops":[...]}.
"Here", "now" and "the playhead" mean the playhead time. "This clip" means the selected clip, otherwise the clip under the playhead on the top visible layer. Music is an audio asset; to fade music out set fade_out; to mute set muted true; to speed up set speed; to zoom or move over time add keyframes at two or more times. Titles are text clips; keep text readable (light text on dark footage, or a bg). Use only clip, track and asset ids that appear in the state. If the request needs a file that is not in the media library, ask for an upload and send no ops. If you only answer a question, ops is []. The reply says what you changed in plain words, never ids."#;

fn state(project: &Project, media: &[Asset], playhead: f64, selected: Option<&str>) -> String {
    let under: Vec<&str> = project.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| c.start <= playhead && playhead < c.end()).map(|c| c.id.as_str()).collect();
    let library: Vec<Value> = media.iter().map(|a| json!({ "id": a.id, "name": a.name, "kind": a.kind, "duration": a.duration })).collect();
    format!(
        "State\nProject: {}\nDuration: {}\nMedia library: {}\nPlayhead: {}\nSelected clip: {}\nClips under the playhead (bottom to top): {:?}",
        serde_json::to_string(project).unwrap_or_default(),
        secs(project.duration()),
        Value::from(library),
        secs(playhead),
        selected.unwrap_or("none"),
        under
    )
}

async fn bridge(env: &Env, model: &str, messages: &[Value]) -> Result<String, String> {
    let fail = |e: wasm_bindgen::JsValue| e.dyn_ref::<js_sys::Error>().map(|e| String::from(e.message())).or_else(|| JSON::stringify(&e).ok().and_then(|s| s.as_string())).unwrap_or_else(|| "bridge error".into());
    let star = Reflect::get(env, &"STAR".into()).map_err(fail)?;
    let chat: Function = Reflect::get(&star, &"chat".into()).map_err(fail)?.dyn_into().map_err(fail)?;
    let call = json!({ "model": model, "messages": messages, "maxTokens": 2400, "temperature": 0.2, "json": true });
    let arg = JSON::parse(&call.to_string()).map_err(fail)?;
    let answer = JsFuture::from(Promise::resolve(&Reflect::apply(&chat, &star, &js_sys::Array::of1(&arg)).map_err(fail)?)).await.map_err(fail)?;
    let text = JSON::stringify(&answer).map_err(fail)?.as_string().unwrap_or_default();
    let answer: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if answer["ok"].as_bool() != Some(true) {
        return Err(answer["error"].as_str().unwrap_or("no answer").to_string());
    }
    Ok(answer["text"].as_str().unwrap_or_default().to_string())
}

async fn think(env: &Env, messages: &[Value], notes: &mut Vec<String>) -> Option<(String, &'static str)> {
    for model in [MODEL, FALLBACK] {
        match bridge(env, model, messages).await {
            Ok(text) => return Some((text, model)),
            Err(e) => notes.push(format!("{model}: {e}")),
        }
    }
    None
}

#[derive(Deserialize, Default)]
struct Plan {
    #[serde(default)]
    reply: String,
    #[serde(default)]
    ops: Vec<Value>,
}

fn plan_of(text: &str) -> Plan {
    let body = match (text.find('{'), text.rfind('}')) {
        (Some(a), Some(b)) if b > a => &text[a..=b],
        _ => "",
    };
    serde_json::from_str(body).unwrap_or_else(|_| Plan { reply: text.trim().to_string(), ops: vec![] })
}

#[derive(Deserialize)]
pub struct Ask {
    pub message: String,
    #[serde(default)]
    pub playhead: f64,
    pub selected: Option<String>,
}

pub async fn chat(env: &Env, db: &D1Database, bucket: &Bucket, user: &str, pid: &str, ask: Ask) -> worker::Result<Option<Value>> {
    let Some(project) = store::load(db, user, pid).await? else { return Ok(None) };
    let media = crate::media::list(db, user).await?;
    let mut messages = vec![json!({ "role": "system", "content": format!("{SYSTEM}\n{OPS_GUIDE}") })];
    for m in store::messages(db, pid, 12).await? {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }
    messages.push(json!({ "role": "user", "content": format!("{}\n\nRequest: {}", state(&project, &media, ask.playhead, ask.selected.as_deref()), ask.message) }));
    store::add_message(db, pid, "user", &ask.message, None).await?;
    let mut reply = String::new();
    let mut done = Vec::new();
    let mut errors = Vec::new();
    let mut seek = None;
    let mut latest = project;
    let mut model = "";
    let mut notes = Vec::new();
    for round in 0..2 {
        let Some((text, by)) = think(env, &messages, &mut notes).await else { break };
        model = by;
        let plan = plan_of(&text);
        if !plan.reply.is_empty() {
            reply = plan.reply.clone();
        }
        if plan.ops.is_empty() {
            break;
        }
        let Some((p, outcome, media)) = store::edit(db, bucket, user, pid, plan.ops).await? else { break };
        latest = p;
        done.extend(outcome.done.clone());
        seek = outcome.seek.or(seek);
        if outcome.errors.is_empty() || round == 1 {
            errors.extend(outcome.errors);
            break;
        }
        messages.push(json!({ "role": "assistant", "content": text }));
        messages.push(json!({ "role": "user", "content": format!("Applied: {:?}\nFailed: {:?}\n{}\nSend only the ops that fix the failures, with a reply for the person.", outcome.done, outcome.errors, state(&latest, &media, ask.playhead, ask.selected.as_deref())) }));
    }
    if reply.is_empty() {
        reply = if model.is_empty() { "The editing model didn't answer. Try again in a moment.".into() } else if done.is_empty() { "I couldn't make that edit. Try saying it another way.".into() } else { "Done.".into() };
    }
    let meta = json!({ "done": done, "errors": errors, "model": model, "notes": notes }).to_string();
    store::add_message(db, pid, "assistant", &reply, Some(meta)).await?;
    Ok(Some(json!({ "reply": reply, "done": done, "errors": errors, "seek": seek, "model": model, "notes": notes, "project": latest })))
}
