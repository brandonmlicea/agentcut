use crate::contrast;
use crate::model::Project;
use crate::ops::{apply, AssetInfo};
use serde_json::json;
use std::collections::HashMap;

fn library() -> HashMap<String, AssetInfo> {
    let mut a = HashMap::new();
    a.insert("v1".into(), AssetInfo { kind: "video".into(), duration: Some(20.0), name: "beach.mp4".into() });
    a.insert("m1".into(), AssetInfo { kind: "audio".into(), duration: Some(60.0), name: "song.mp3".into() });
    a.insert("i1".into(), AssetInfo { kind: "image".into(), duration: None, name: "logo.png".into() });
    a
}

fn ops(p: &mut Project, list: serde_json::Value) -> crate::ops::Outcome {
    apply(p, list.as_array().unwrap(), &library())
}

#[test]
fn builds_a_timeline_with_layers_music_and_text() {
    let mut p = Project::default();
    let out = ops(&mut p, json!([
        { "op": "add_clip", "asset": "v1" },
        { "op": "add_clip", "asset": "v1" },
        { "op": "add_clip", "asset": "m1" },
        { "op": "add_clip", "text": "Hello", "start": 1 },
        { "op": "add_clip", "asset": "i1", "start": 2 }
    ]));
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let kinds: Vec<&str> = p.tracks.iter().map(|t| t.kind.as_str()).filter(|k| *k != "audio").collect();
    assert_eq!(kinds, ["video", "video", "text"]);
    assert_eq!(p.tracks.iter().filter(|t| t.kind == "audio").count(), 1);
    assert_eq!(p.tracks[0].clips.len(), 2);
    assert_eq!(p.tracks[0].clips[1].start, 20.0);
    assert_eq!(p.tracks[1].clips[0].start, 2.0);
    assert_eq!(p.tracks.iter().find(|t| t.kind == "audio").unwrap().clips[0].dur, 60.0);
    assert_eq!(p.duration(), 60.0);
}

#[test]
fn splits_speeds_mutes_and_keyframes() {
    let mut p = Project::default();
    ops(&mut p, json!([{ "op": "add_clip", "asset": "v1" }]));
    let id = p.tracks[0].clips[0].id.clone();
    let out = ops(&mut p, json!([
        { "op": "keyframe", "clip": id, "prop": "scale", "at": 2, "value": 1 },
        { "op": "keyframe", "clip": id, "prop": "scale", "at": 12, "value": 1.5 },
        { "op": "split", "clip": id, "at": 10 }
    ]));
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let clips = &p.tracks[0].clips;
    assert_eq!(clips.len(), 2);
    assert_eq!((clips[0].dur, clips[1].start, clips[1].src_in), (10.0, 10.0, 10.0));
    assert_eq!(clips[0].keyframes["scale"].len(), 1);
    assert_eq!(clips[1].keyframes["scale"][0].t, 2.0);
    let right = clips[1].id.clone();
    let out = ops(&mut p, json!([{ "op": "update_clip", "clip": right, "speed": 2, "muted": true }, { "op": "effect", "clip": right, "preset": "noir" }]));
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let c = &p.tracks[0].clips[1];
    assert_eq!((c.speed, c.dur, c.muted, c.effects.grayscale), (2.0, 5.0, true, 1.0));
}

#[test]
fn overlapping_moves_open_a_new_layer_and_bad_ops_report() {
    let mut p = Project::default();
    ops(&mut p, json!([{ "op": "add_clip", "asset": "v1" }, { "op": "add_clip", "asset": "v1" }]));
    let second = p.tracks[0].clips[1].id.clone();
    ops(&mut p, json!([{ "op": "update_clip", "clip": second, "start": 5 }]));
    assert_eq!(p.tracks.len(), 2);
    let out = ops(&mut p, json!([{ "op": "split", "clip": "nope", "at": 1 }, { "op": "add_clip", "asset": "zzz" }, { "op": "update_clip", "clip": second, "sped": 2 }]));
    assert_eq!(out.errors.len(), 3, "{:?}", out.errors);
}

#[test]
fn contrast_matches_known_pairs() {
    let r = contrast::check("#ffffff", "#000000", "AA", "foreground");
    assert_eq!(r["wcag"], 21.0);
    assert!(r["apca"].as_f64().unwrap() < -100.0);
    let r = contrast::check("#999999", "#ffffff", "AA", "foreground");
    assert_eq!(r["passes"], false);
    let fix = r["fix"]["foreground"].as_str().unwrap();
    let again = contrast::check(fix, "#ffffff", "AA", "foreground");
    assert_eq!(again["passes"], true, "{again}");
    assert_eq!(contrast::check("oklch(0.5 0.1 250)", "rgb(255 255 255)", "AA", "foreground")["AA"], true);
}
