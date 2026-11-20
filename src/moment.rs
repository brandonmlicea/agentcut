use crate::model::{Clip, Effects, Project, KEY_PROPS, PRESETS};
use crate::ops::AssetInfo;
use crate::util::secs;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub fn time(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    let t = t.trim_start_matches("at").trim();
    if t.contains(':') {
        return t.split(':').try_fold(0.0, |acc, p| p.trim().trim_end_matches('s').parse::<f64>().ok().map(|v| acc * 60.0 + v));
    }
    let words = [("minutes", "m"), ("minute", "m"), ("mins", "m"), ("min", "m"), ("seconds", "s"), ("second", "s"), ("secs", "s"), ("sec", "s")];
    let t = words.iter().fold(t.to_string(), |acc, (w, u)| acc.replace(w, u));
    let (mut total, mut num, mut any) = (0.0, String::new(), false);
    for ch in t.chars() {
        match ch {
            '0'..='9' | '.' => num.push(ch),
            'm' | 's' => {
                let v: f64 = num.parse().ok()?;
                total += if ch == 'm' { v * 60.0 } else { v };
                num.clear();
                any = true;
            }
            ' ' => {}
            _ => return None,
        }
    }
    if !num.is_empty() {
        total += num.parse::<f64>().ok()?;
        any = true;
    }
    any.then_some(total).filter(|v| v.is_finite())
}

fn loose(text: &str) -> Option<f64> {
    time(text).or_else(|| {
        let end = text.rfind(|c: char| c.is_ascii_digit())?;
        let unit = text[end + 1..].trim_start().chars().next().filter(|c| *c == 'm' || *c == 's');
        time(&format!("{}{}", &text[..=end], unit.map(String::from).unwrap_or_default()))
    })
}

pub fn span(text: &str) -> Option<(f64, f64)> {
    let lower = text.to_lowercase();
    let from = lower.find(|c: char| c.is_ascii_digit())?;
    let t = &lower[from..];
    for sep in [" to ", " through ", " thru ", " until ", " till ", " and ", "...", "..", "–", "—", "-"] {
        if let Some((a, b)) = t.split_once(sep) {
            if let (Some(a), Some(b)) = (loose(a), loose(b)) {
                return Some((a.min(b), a.max(b)));
            }
        }
    }
    loose(t).map(|v| (v, v))
}

fn visible(kind: &str) -> bool {
    kind != "audio"
}

pub fn under(project: &Project, t: f64, audio: bool) -> Vec<(usize, &Clip)> {
    let mut out: Vec<(usize, &Clip)> = project
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, tr)| visible(&tr.kind) != audio && !(tr.hidden && !audio))
        .flat_map(|(i, tr)| tr.clips.iter().filter(move |c| c.start <= t + 1e-6 && t < c.end() - 1e-6).map(move |c| (i, c)))
        .collect();
    out.sort_by_key(|(i, _)| *i);
    out
}

fn resolve(project: &Project, reference: &str) -> Option<String> {
    let rest = reference.strip_prefix('@')?;
    let audio = rest.contains("audio") || rest.contains("music");
    let t = span(rest)?.0;
    under(project, t, audio).last().map(|(_, c)| c.id.clone())
}

pub fn value_at(clip: &Clip, prop: &str, local: f64) -> f64 {
    let base = match prop {
        "x" => clip.x,
        "y" => clip.y,
        "scale" => clip.scale,
        "rotation" => clip.rotation,
        "opacity" => clip.opacity,
        "volume" => clip.volume,
        _ => clip.effects.blur,
    };
    let Some(keys) = clip.keyframes.get(prop).filter(|k| !k.is_empty()) else {
        return base;
    };
    if local <= keys[0].t {
        return keys[0].v;
    }
    for w in keys.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if local > b.t {
            continue;
        }
        if a.ease == "hold" {
            return a.v;
        }
        let mut p = (local - a.t) / if b.t > a.t { b.t - a.t } else { 1.0 };
        if a.ease == "ease" {
            p = if p < 0.5 { 2.0 * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(2) / 2.0 };
        }
        return a.v + (b.v - a.v) * p;
    }
    keys[keys.len() - 1].v
}

pub fn fade_at(clip: &Clip, local: f64) -> f64 {
    let a = if clip.fade_in > 0.0 { local / clip.fade_in } else { 1.0 };
    let b = if clip.fade_out > 0.0 { (clip.dur - local) / clip.fade_out } else { 1.0 };
    a.min(b).clamp(0.0, 1.0)
}

fn look_name(e: &Effects) -> String {
    let v = [e.brightness, e.contrast, e.saturate, e.blur, e.grayscale, e.sepia, e.hue, e.vignette];
    let (name, d) = PRESETS.iter().map(|(n, p)| (*n, p.iter().zip(v).map(|(a, b)| (a - b).abs()).sum::<f64>())).fold(("none", f64::MAX), |m, x| if x.1 < m.1 { x } else { m });
    if d < 0.05 {
        name.into()
    } else {
        format!("custom (closest: {name})")
    }
}

fn kind(clip: &Clip, assets: &HashMap<String, AssetInfo>) -> String {
    match (&clip.asset, &clip.text, &clip.fill) {
        (Some(a), _, _) => assets.get(a).map(|i| i.kind.clone()).unwrap_or_else(|| "video".into()),
        (_, Some(_), _) => "text".into(),
        _ => "colour".into(),
    }
}

fn layer(project: &Project, ti: usize, c: &Clip, t: f64, assets: &HashMap<String, AssetInfo>) -> Value {
    let local = t - c.start;
    let mut out = json!({ "clip": c.id, "name": c.name, "track": project.tracks[ti].name, "kind": kind(c, assets) });
    let o = out.as_object_mut().unwrap();
    if let Some(a) = &c.asset {
        o.insert("asset".into(), json!({ "id": a, "name": assets.get(a).map(|i| i.name.as_str()), "second": (c.src_in + local * c.speed * 100.0).round() / 100.0 }));
    }
    for p in KEY_PROPS {
        o.insert(p.into(), json!((value_at(c, p, local) * 1000.0).round() / 1000.0));
    }
    o.insert("opacity".into(), json!(((value_at(c, "opacity", local) * fade_at(c, local)) * 1000.0).round() / 1000.0));
    o.insert("speed".into(), json!(c.speed));
    o.insert("look".into(), json!(look_name(&c.effects)));
    o.insert("effects".into(), json!(c.effects));
    if let Some(text) = &c.text {
        o.insert("text".into(), json!(text));
        o.insert("style".into(), json!(c.style));
    }
    if let Some(fill) = &c.fill {
        o.insert("fill".into(), json!(fill));
    }
    out
}

pub struct Still {
    pub at: f64,
    pub asset: String,
    pub second: u32,
}

pub fn look(project: &Project, assets: &HashMap<String, AssetInfo>, a: f64, b: f64) -> (Value, Vec<Still>) {
    let clips: Vec<(usize, &Clip)> = project
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(i, t)| t.clips.iter().map(move |c| (i, c)))
        .filter(|(_, c)| c.start < b + 1e-6 && a < c.end() - 1e-6 || (a == b && c.start <= a && a < c.end()))
        .collect();
    let mut times = vec![a, (a + b) / 2.0, b];
    for (_, c) in &clips {
        times.extend([c.start, c.end()].into_iter().filter(|t| *t > a && *t < b));
        times.extend(c.keyframes.values().flatten().map(|k| c.start + k.t).filter(|t| *t > a && *t < b));
    }
    times.sort_by(f64::total_cmp);
    times.dedup_by(|x, y| (*x - *y).abs() < 1.0 / 30.0);
    times.truncate(12);
    let frames: Vec<Value> = times
        .iter()
        .map(|&t| {
            let t = t.min(b.max(a + 1e-3) - 1e-3).max(a);
            let video: Vec<Value> = under(project, t, false).into_iter().map(|(ti, c)| layer(project, ti, c, t, assets)).collect();
            let sound: Vec<Value> = under(project, t, true)
                .into_iter()
                .filter(|(ti, c)| !c.muted && !project.tracks[*ti].muted)
                .map(|(_, c)| json!({ "clip": c.id, "name": c.name, "volume": (value_at(c, "volume", t - c.start) * fade_at(c, t - c.start) * 100.0).round() / 100.0 }))
                .collect();
            json!({ "at": secs(t), "layers_bottom_to_top": video, "sound": sound })
        })
        .collect();
    let mut notes = vec![];
    for (_, c) in &clips {
        let inside = |t: f64| t > a + 1e-3 && t < b - 1e-3;
        if inside(c.start) {
            notes.push(format!("cut in: \"{}\" starts at {}", c.name, secs(c.start)));
        }
        if inside(c.end()) {
            notes.push(format!("cut out: \"{}\" ends at {}", c.name, secs(c.end())));
        }
        for (prop, keys) in &c.keyframes {
            let moving: Vec<String> =
                keys.iter().filter(|k| c.start + k.t >= a - 0.5 && c.start + k.t <= b + 0.5).map(|k| format!("{} at {} ({})", (k.v * 1000.0).round() / 1000.0, secs(c.start + k.t), k.ease)).collect();
            if !moving.is_empty() {
                notes.push(format!("\"{}\" animates {prop}: {}", c.name, moving.join(" → ")));
            }
        }
        if c.speed != 1.0 {
            notes.push(format!("\"{}\" plays at {}x", c.name, c.speed));
        }
        let look = look_name(&c.effects);
        if look != "none" {
            notes.push(format!("\"{}\" has the {look} look", c.name));
        }
        if c.fade_in > 0.0 && inside(c.start + c.fade_in / 2.0) || c.fade_out > 0.0 && inside(c.end() - c.fade_out / 2.0) {
            notes.push(format!("\"{}\" fades (in {}, out {})", c.name, secs(c.fade_in), secs(c.fade_out)));
        }
    }
    let stills = [a, (a + b) / 2.0, b]
        .iter()
        .filter_map(|&t| {
            let t = t.min(b.max(a + 1e-3) - 1e-3).max(a);
            under(project, t, false).into_iter().rev().find_map(|(_, c)| {
                let asset = c.asset.as_ref().filter(|x| assets.get(*x).is_some_and(|i| i.kind != "audio"))?;
                let second = if assets.get(asset).is_some_and(|i| i.kind == "image") { 0 } else { (c.src_in + (t - c.start) * c.speed).floor().max(0.0) as u32 };
                Some(Still { at: t, asset: asset.clone(), second })
            })
        })
        .fold(Vec::<Still>::new(), |mut v, s| {
            if !v.iter().any(|x| x.asset == s.asset && x.second == s.second) {
                v.push(s);
            }
            v
        });
    (json!({ "from": secs(a), "to": secs(b), "style": notes, "frames": frames }), stills)
}

fn copy_style(project: &Project, from: (f64, f64), clip: &Clip, to: (f64, f64), speed: bool) -> Result<Vec<Value>, String> {
    let mid = (from.0 + from.1) / 2.0;
    let (_, src) = under(project, mid, false).last().copied().ok_or(format!("nothing is on screen at {}", secs(mid)))?;
    let mut fields = Map::new();
    fields.insert("op".into(), json!("update_clip"));
    fields.insert("clip".into(), json!(clip.id));
    fields.insert("effects".into(), json!(src.effects));
    if src.text.is_some() && clip.text.is_some() {
        fields.insert("style".into(), json!(src.style));
    }
    if speed {
        fields.insert("speed".into(), json!(src.speed));
    }
    for p in ["x", "y", "scale", "rotation", "opacity"] {
        if !src.keyframes.contains_key(p) {
            fields.insert(p.into(), json!(value_at(src, p, mid - src.start)));
        }
    }
    let mut ops = vec![Value::Object(fields)];
    let (fa, fb) = (from.0.max(src.start), from.1.min(src.end()));
    let (ta, tb) = (to.0.max(clip.start), to.1.min(clip.end()));
    for (prop, keys) in src.keyframes.iter().filter(|(p, _)| p.as_str() != "volume") {
        let mut at: Vec<(f64, String)> = vec![(fa, "ease".into()), (fb, "ease".into())];
        at.extend(keys.iter().map(|k| (src.start + k.t, k.ease.clone())).filter(|(t, _)| *t > fa && *t < fb));
        at.sort_by(|x, y| x.0.total_cmp(&y.0));
        for (t, ease) in at {
            let u = if fb > fa { (t - fa) / (fb - fa) } else { 0.0 };
            ops.push(json!({ "op": "keyframe", "clip": clip.id, "prop": prop, "at": ta + u * (tb - ta), "value": value_at(src, prop, t - src.start), "ease": ease }));
        }
    }
    Ok(ops)
}

fn when(v: Option<&Value>) -> Option<(f64, f64)> {
    match v? {
        Value::String(s) => span(s),
        Value::Number(n) => n.as_f64().map(|t| (t, t)),
        Value::Array(a) if a.len() == 2 => Some((a[0].as_f64()?, a[1].as_f64()?)),
        _ => None,
    }
}

pub fn match_style(project: &Project, op: &Value) -> Result<Vec<Value>, String> {
    let from = when(op.get("from")).ok_or("match_style needs \"from\", like \"0:15 to 0:17\"")?;
    let speed = op.get("speed").and_then(Value::as_bool).unwrap_or(false);
    let targets: Vec<(&Clip, (f64, f64))> = match (op.get("clip").and_then(Value::as_str), when(op.get("to"))) {
        (Some(id), to) => {
            let (ti, ci) = project.locate(id).ok_or(format!("no clip {id}"))?;
            let c = &project.tracks[ti].clips[ci];
            vec![(c, to.unwrap_or((c.start, c.end())))]
        }
        (None, Some(to)) => {
            let found: Vec<&Clip> =
                project.tracks.iter().filter(|t| visible(&t.kind) && !t.hidden).flat_map(|t| t.clips.iter()).filter(|c| c.start < to.1.max(to.0 + 1e-3) && to.0 < c.end()).collect();
            found.into_iter().map(|c| (c, if to.1 > to.0 { to } else { (c.start, c.end()) })).collect()
        }
        _ => return Err("match_style needs \"to\" (a time span) or \"clip\"".into()),
    };
    if targets.is_empty() {
        return Err("no clip is on screen in that \"to\" span".into());
    }
    let mut ops = vec![];
    for (c, to) in targets {
        ops.extend(copy_style(project, from, c, to, speed)?);
    }
    Ok(ops)
}

pub fn normalize(project: &Project, mut op: Value) -> Result<Vec<Value>, String> {
    let Some(o) = op.as_object_mut() else { return Ok(vec![op]) };
    for k in ["at", "start", "dur", "in", "fade_in", "fade_out"] {
        if let Some(v) = o.get(k).and_then(Value::as_str).and_then(time) {
            o.insert(k.into(), json!(v));
        }
    }
    if let Some(r) = o.get("clip").and_then(Value::as_str).filter(|s| s.starts_with('@')) {
        let id = resolve(project, r).ok_or(format!("nothing is on the timeline at {r}"))?;
        o.insert("clip".into(), json!(id));
    }
    if o.get("op").and_then(Value::as_str) == Some("match_style") {
        return match_style(project, &op).map_err(|e| format!("match_style: {e}"));
    }
    Ok(vec![op])
}
