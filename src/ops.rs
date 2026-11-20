use crate::model::{Clip, ClipPatch, Effects, Key, Project, Track, KEY_PROPS, PATCH_KEYS, PRESETS};
use crate::util::{id, secs};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub struct AssetInfo {
    pub kind: String,
    pub duration: Option<f64>,
    pub name: String,
}

#[derive(Default, serde::Serialize)]
pub struct Outcome {
    pub done: Vec<String>,
    pub errors: Vec<String>,
    pub seek: Option<f64>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    SetProject { name: Option<String>, width: Option<u32>, height: Option<u32>, fps: Option<u32>, background: Option<String>, font: Option<String> },
    AddTrack { kind: String, name: Option<String>, index: Option<usize> },
    UpdateTrack { track: String, name: Option<String>, muted: Option<bool>, hidden: Option<bool> },
    MoveTrack { track: String, index: usize },
    RemoveTrack { track: String },
    AddClip { track: Option<String>, #[serde(flatten)] patch: ClipPatch },
    UpdateClip { clip: String, track: Option<String>, #[serde(flatten)] patch: ClipPatch },
    RemoveClip { clip: String },
    Split { clip: String, at: f64 },
    Keyframe { clip: String, prop: String, at: f64, value: f64, ease: Option<String> },
    RemoveKeyframe { clip: String, prop: String, at: Option<f64> },
    Effect { clip: String, preset: String },
    Seek { at: f64 },
}

fn visual(kind: &str) -> bool {
    kind != "audio"
}

fn kind_of(clip: &Clip, assets: &HashMap<String, AssetInfo>) -> String {
    match (&clip.asset, &clip.text) {
        (Some(a), _) => match assets.get(a).map(|i| i.kind.as_str()) {
            Some("audio") => "audio".into(),
            _ => "video".into(),
        },
        (None, Some(_)) => "text".into(),
        _ => "video".into(),
    }
}

fn label(clip: &Clip) -> String {
    format!("“{}”", if clip.name.is_empty() { &clip.id } else { &clip.name })
}

fn num(v: Option<f64>, lo: f64, hi: f64) -> Option<f64> {
    v.filter(|x| x.is_finite()).map(|x| x.clamp(lo, hi))
}

fn patch(clip: &mut Clip, p: &ClipPatch, assets: &HashMap<String, AssetInfo>) -> Result<(), String> {
    if let Some(a) = &p.asset {
        if !assets.contains_key(a) {
            return Err(format!("no asset {a} in the media library"));
        }
        clip.asset = Some(a.clone());
    }
    if let Some(v) = &p.name { clip.name = v.chars().take(80).collect() }
    if let Some(v) = &p.text { clip.text = Some(v.chars().take(400).collect()) }
    if let Some(v) = &p.fill { clip.fill = Some(v.chars().take(40).collect()) }
    if let Some(s) = num(p.speed, 0.1, 16.0) {
        if p.dur.is_none() {
            clip.dur = (clip.dur * clip.speed / s).max(0.05);
        }
        clip.speed = s;
    }
    if let Some(v) = num(p.start, 0.0, 36000.0) { clip.start = v }
    if let Some(v) = num(p.dur, 0.05, 36000.0) { clip.dur = v }
    if let Some(v) = num(p.src_in, 0.0, 36000.0) { clip.src_in = v }
    if let Some(v) = num(p.volume, 0.0, 2.0) { clip.volume = v }
    if let Some(v) = p.muted { clip.muted = v }
    if let Some(v) = num(p.fade_in, 0.0, 60.0) { clip.fade_in = v }
    if let Some(v) = num(p.fade_out, 0.0, 60.0) { clip.fade_out = v }
    if let Some(v) = num(p.x, -1.0, 2.0) { clip.x = v }
    if let Some(v) = num(p.y, -1.0, 2.0) { clip.y = v }
    if let Some(v) = num(p.scale, 0.05, 10.0) { clip.scale = v }
    if let Some(v) = num(p.rotation, -3600.0, 3600.0) { clip.rotation = v }
    if let Some(v) = num(p.opacity, 0.0, 1.0) { clip.opacity = v }
    for (k, v) in p.effects.iter().flatten() {
        clip.effects.set(k, v.as_f64().ok_or(format!("effect {k} needs a number"))?)?;
    }
    for (k, v) in p.style.iter().flatten() {
        let s = &mut clip.style;
        match (k.as_str(), v) {
            ("font", Value::String(f)) => s.font = Some(f.chars().take(60).collect()),
            ("font", Value::Null) => s.font = None,
            ("bg", Value::String(b)) => s.bg = Some(b.chars().take(40).collect()),
            ("bg", Value::Null) => s.bg = None,
            ("color", Value::String(c)) => s.color = c.chars().take(40).collect(),
            ("align", Value::String(a)) if ["left", "center", "right"].contains(&a.as_str()) => s.align = a.clone(),
            ("size", Value::Number(n)) => s.size = n.as_f64().unwrap_or(96.0).clamp(8.0, 400.0),
            ("weight", Value::Number(n)) => s.weight = (n.as_f64().unwrap_or(600.0) as u32).clamp(100, 900),
            _ => return Err(format!("style {k} can't be {v}")),
        }
    }
    let span = clip.fade_in + clip.fade_out;
    if span > clip.dur {
        clip.fade_in *= clip.dur / span;
        clip.fade_out *= clip.dur / span;
    }
    if let Some(full) = clip.asset.as_ref().and_then(|a| assets.get(a)).filter(|i| i.kind != "image").and_then(|i| i.duration) {
        clip.src_in = clip.src_in.min((full - 0.05).max(0.0));
        clip.dur = clip.dur.min(((full - clip.src_in) / clip.speed).max(0.05));
    }
    Ok(())
}

fn place(project: &mut Project, ti: usize, clip: Clip) -> usize {
    let kind = project.tracks[ti].kind.clone();
    let fits = |t: &Track| visual(&t.kind) == visual(&kind) && !t.clips.iter().any(|c| c.overlaps(&clip));
    let target = std::iter::once(ti).chain(ti + 1..project.tracks.len()).find(|&i| fits(&project.tracks[i]));
    let at = target.unwrap_or_else(|| {
        let n = project.tracks.iter().filter(|t| t.kind == kind).count() + 1;
        let track = Track { id: format!("t{}", id(5)), name: format!("{} {}", title(&kind), n), kind: kind.clone(), ..Default::default() };
        let at = if visual(&kind) { ti + 1 } else { project.tracks.len() };
        project.tracks.insert(at, track);
        at
    });
    let clips = &mut project.tracks[at].clips;
    clips.push(clip);
    clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    at
}

fn title(kind: &str) -> String {
    let mut c = kind.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn new_track(project: &mut Project, kind: &str, name: Option<String>, index: Option<usize>) -> usize {
    let n = project.tracks.iter().filter(|t| t.kind == kind).count() + 1;
    let track = Track { id: format!("t{}", id(5)), name: name.unwrap_or_else(|| format!("{} {}", title(kind), n)), kind: kind.into(), ..Default::default() };
    let at = index.unwrap_or(project.tracks.len()).min(project.tracks.len());
    project.tracks.insert(at, track);
    at
}

fn take(project: &mut Project, clip: &str) -> Result<(usize, Clip), String> {
    let (ti, ci) = project.locate(clip).ok_or(format!("no clip {clip}"))?;
    Ok((ti, project.tracks[ti].clips.remove(ci)))
}

fn clip_mut<'a>(project: &'a mut Project, clip: &str) -> Result<&'a mut Clip, String> {
    let (ti, ci) = project.locate(clip).ok_or(format!("no clip {clip}"))?;
    Ok(&mut project.tracks[ti].clips[ci])
}

fn key_range(prop: &str) -> (f64, f64) {
    match prop {
        "opacity" => (0.0, 1.0),
        "scale" => (0.05, 10.0),
        "volume" => (0.0, 2.0),
        "blur" => (0.0, 40.0),
        "rotation" => (-3600.0, 3600.0),
        _ => (-1.0, 2.0),
    }
}

fn run(project: &mut Project, op: Op, assets: &HashMap<String, AssetInfo>, made: &mut HashSet<String>, out: &mut Outcome) -> Result<String, String> {
    match op {
        Op::SetProject { name, width, height, fps, background, font } => {
            if let Some(v) = name { project.name = v.chars().take(80).collect() }
            if let Some(v) = width { project.width = v.clamp(144, 3840) / 2 * 2 }
            if let Some(v) = height { project.height = v.clamp(144, 3840) / 2 * 2 }
            if let Some(v) = fps { project.fps = v.clamp(12, 60) }
            if let Some(v) = background { project.background = v.chars().take(40).collect() }
            if let Some(v) = font { project.font = v.chars().take(60).collect() }
            Ok("Updated the project".into())
        }
        Op::AddTrack { kind, name, index } => {
            if !["video", "text", "audio"].contains(&kind.as_str()) {
                return Err("track kind is video, text or audio".into());
            }
            let at = new_track(project, &kind, name, index);
            made.insert(project.tracks[at].id.clone());
            Ok(format!("Added track {}", project.tracks[at].id))
        }
        Op::UpdateTrack { track, name, muted, hidden } => {
            let t = project.track_index(&track).ok_or(format!("no track {track}"))?;
            let t = &mut project.tracks[t];
            if let Some(v) = name { t.name = v.chars().take(40).collect() }
            if let Some(v) = muted { t.muted = v }
            if let Some(v) = hidden { t.hidden = v }
            Ok(format!("Updated track {}", t.name))
        }
        Op::MoveTrack { track, index } => {
            let t = project.track_index(&track).ok_or(format!("no track {track}"))?;
            let moved = project.tracks.remove(t);
            let at = index.min(project.tracks.len());
            let name = moved.name.clone();
            project.tracks.insert(at, moved);
            Ok(format!("Moved {name} to layer {at}"))
        }
        Op::RemoveTrack { track } => {
            let t = project.track_index(&track).ok_or(format!("no track {track}"))?;
            Ok(format!("Removed track {}", project.tracks.remove(t).name))
        }
        Op::AddClip { track, patch: p } => {
            let sources = [p.asset.is_some(), p.text.is_some(), p.fill.is_some()].iter().filter(|x| **x).count();
            if sources != 1 {
                return Err("add_clip needs exactly one of asset, text or fill".into());
            }
            let mut clip = Clip { id: format!("c{}", id(6)), ..Default::default() };
            patch(&mut clip, &ClipPatch { start: None, dur: None, speed: None, ..p }, assets)?;
            let kind = kind_of(&clip, assets);
            let info = clip.asset.as_ref().and_then(|a| assets.get(a));
            clip.dur = match (&kind[..], info) {
                ("text", _) => 3.0,
                (_, Some(i)) if i.kind != "image" => i.duration.map(|d| (d - clip.src_in).max(0.05)).unwrap_or(5.0),
                _ => 5.0,
            };
            if clip.name.is_empty() {
                clip.name = info.map(|i| i.name.clone()).or(clip.text.clone()).unwrap_or_else(|| "Colour".into()).chars().take(40).collect();
            }
            let ti = match &track {
                Some(t) => project.track_index(t).ok_or(format!("no track {t}"))?,
                None => match project.tracks.iter().position(|t| t.kind == kind) {
                    Some(i) => i,
                    None => new_track(project, &kind, None, if kind == "video" { Some(0) } else { None }),
                },
            };
            if visual(&project.tracks[ti].kind) != visual(&kind) {
                return Err(format!("a {kind} clip can't go on a {} track", project.tracks[ti].kind));
            }
            clip.start = project.tracks[ti].clips.iter().map(Clip::end).fold(0.0, f64::max);
            let timing = ClipPatch { start: p.start, dur: p.dur, speed: p.speed, ..Default::default() };
            patch(&mut clip, &timing, assets)?;
            let msg = format!("Added {} at {}", label(&clip), secs(clip.start));
            let at = place(project, ti, clip);
            out.seek.get_or_insert(project.tracks[at].clips.iter().map(|c| c.start).fold(0.0, f64::max));
            Ok(msg)
        }
        Op::UpdateClip { clip, track, patch: p } => {
            let (ti, mut c) = take(project, &clip)?;
            let before = c.clone();
            if let Err(e) = patch(&mut c, &p, assets) {
                project.tracks[ti].clips.push(before);
                project.tracks[ti].clips.sort_by(|a, b| a.start.total_cmp(&b.start));
                return Err(e);
            }
            let target = match &track {
                Some(t) => project.track_index(t).filter(|&i| visual(&project.tracks[i].kind) == visual(&project.tracks[ti].kind)).ok_or(format!("no matching track {t}"))?,
                None => ti,
            };
            let msg = format!("Updated {}", label(&c));
            if p.start.is_some() {
                out.seek.get_or_insert(c.start);
            }
            place(project, target, c);
            Ok(msg)
        }
        Op::RemoveClip { clip } => take(project, &clip).map(|(_, c)| format!("Removed {}", label(&c))),
        Op::Split { clip, at } => {
            let (ti, mut left) = take(project, &clip)?;
            let cut = at - left.start;
            if cut <= 0.05 || cut >= left.dur - 0.05 {
                let msg = format!("{} runs {}–{}, so {} is not inside it", label(&left), secs(left.start), secs(left.end()), secs(at));
                place(project, ti, left);
                return Err(msg);
            }
            let mut right = Clip { id: format!("c{}", id(6)), start: at, dur: left.dur - cut, src_in: left.src_in + cut * left.speed, fade_in: 0.0, ..left.clone() };
            left.dur = cut;
            left.fade_out = 0.0;
            for (prop, keys) in left.keyframes.iter_mut() {
                let after: Vec<Key> = keys.iter().filter(|k| k.t >= cut).map(|k| Key { t: k.t - cut, ..k.clone() }).collect();
                keys.retain(|k| k.t < cut);
                right.keyframes.insert(prop.clone(), after);
            }
            let msg = format!("Split {} at {}", label(&left), secs(at));
            place(project, ti, left);
            place(project, ti, right);
            out.seek.get_or_insert(at);
            Ok(msg)
        }
        Op::Keyframe { clip, prop, at, value, ease } => {
            if !KEY_PROPS.contains(&prop.as_str()) {
                return Err(format!("keyframes animate {}", KEY_PROPS.join(", ")));
            }
            let c = clip_mut(project, &clip)?;
            let t = (at - c.start).clamp(0.0, c.dur);
            let (lo, hi) = key_range(&prop);
            let ease = ease.filter(|e| ["linear", "ease", "hold"].contains(&e.as_str())).unwrap_or_else(|| "ease".into());
            let keys = c.keyframes.entry(prop.clone()).or_default();
            keys.retain(|k| (k.t - t).abs() > 1.0 / 60.0);
            keys.push(Key { t, v: value.clamp(lo, hi), ease });
            keys.sort_by(|a, b| a.t.total_cmp(&b.t));
            out.seek.get_or_insert(c.start + t);
            Ok(format!("Keyframed {prop} of {} at {}", label(c), secs(c.start + t)))
        }
        Op::RemoveKeyframe { clip, prop, at } => {
            let c = clip_mut(project, &clip)?;
            let start = c.start;
            let keys = c.keyframes.entry(prop.clone()).or_default();
            match at {
                Some(at) => keys.retain(|k| (k.t + start - at).abs() > 0.05),
                None => keys.clear(),
            }
            c.keyframes.retain(|_, k| !k.is_empty());
            Ok(format!("Removed {prop} keyframes from {}", label(c)))
        }
        Op::Effect { clip, preset } => {
            let values = PRESETS.iter().find(|(n, _)| *n == preset).ok_or(format!("presets: {}", PRESETS.map(|p| p.0).join(", ")))?.1;
            let c = clip_mut(project, &clip)?;
            c.effects = Effects::preset(values);
            Ok(format!("Applied {preset} to {}", label(c)))
        }
        Op::Seek { at } => {
            out.seek = Some(at.max(0.0));
            Ok(format!("Moved the playhead to {}", secs(at.max(0.0))))
        }
    }
}

pub fn apply(project: &mut Project, ops: &[Value], assets: &HashMap<String, AssetInfo>) -> Outcome {
    let mut out = Outcome::default();
    let mut made = HashSet::new();
    for (i, given) in ops.iter().enumerate() {
        let expanded = match crate::moment::normalize(project, given.clone()) {
            Ok(v) => v,
            Err(e) => (out.errors.push(format!("op {i}: {e}")), vec![]).1,
        };
        for raw in &expanded {
            let name = raw.get("op").and_then(Value::as_str).unwrap_or("?").to_string();
            if name == "add_clip" || name == "update_clip" {
                let extra: Vec<&String> = raw.as_object().map(|o| o.keys().filter(|k| !PATCH_KEYS.contains(&k.as_str())).collect()).unwrap_or_default();
                if !extra.is_empty() {
                    out.errors.push(format!("op {i} {name}: ignored unknown fields {extra:?}"));
                }
            }
            match serde_json::from_value::<Op>(raw.clone()).map_err(|e| e.to_string()).and_then(|op| run(project, op, assets, &mut made, &mut out)) {
                Ok(msg) => out.done.push(msg),
                Err(e) => out.errors.push(format!("op {i} {name}: {e}")),
            }
        }
    }
    project.tracks.retain(|t| !t.clips.is_empty() || made.contains(&t.id));
    out
}
