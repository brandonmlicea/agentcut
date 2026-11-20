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

#[test]
fn reads_times_the_way_people_say_them() {
    use crate::moment::{span, time};
    assert_eq!(time("0:15"), Some(15.0));
    assert_eq!(time("1:02.5"), Some(62.5));
    assert_eq!(time("2m 3s"), Some(123.0));
    assert_eq!(time("15 seconds"), Some(15.0));
    assert_eq!(span("I really liked 0:15 to 0:17"), Some((15.0, 17.0)));
    assert_eq!(span("from 15s to 17s"), Some((15.0, 17.0)));
    assert_eq!(span("0:15-0:17"), Some((15.0, 17.0)));
    assert_eq!(span("between 15 and 17 seconds please"), Some((15.0, 17.0)));
    assert_eq!(span("the bit at 1:02"), Some((62.0, 62.0)));
    assert_eq!(span("no time here"), None);
}

#[test]
fn looks_at_a_moment_and_carries_its_style() {
    let mut p = Project::default();
    let made = ops(&mut p, json!([
        { "op": "add_clip", "asset": "v1", "dur": 10 },
        { "op": "add_clip", "asset": "v1", "start": 10, "dur": 10 },
        { "op": "effect", "clip": "@0:05", "preset": "warm" },
        { "op": "keyframe", "clip": "@0:05", "prop": "scale", "at": "0:04", "value": 1.0 },
        { "op": "keyframe", "clip": "@0:05", "prop": "scale", "at": "0:06", "value": 1.2 },
    ]));
    assert!(made.errors.is_empty(), "{:?}", made.errors);
    let normal = crate::moment::normalize(&p, json!({ "op": "split", "clip": "@0:12", "at": "0:13" })).unwrap();
    assert_eq!(normal[0]["at"], json!(13.0));
    assert_eq!(normal[0]["clip"], json!(p.tracks[0].clips[1].id));
    let (report, stills) = crate::moment::look(&p, &library(), 4.0, 6.0);
    let text = report.to_string();
    assert!(text.contains("warm look"), "{text}");
    assert!(text.contains("animates scale"), "{text}");
    assert_eq!(stills.iter().map(|s| s.second).collect::<Vec<_>>(), vec![4, 5]);
    let out = ops(&mut p, json!([{ "op": "match_style", "from": "0:04 to 0:06", "to": "0:14 to 0:18" }]));
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    let second = &p.tracks[0].clips[1];
    assert!((second.effects.sepia - 0.18).abs() < 1e-9);
    let keys = &second.keyframes["scale"];
    assert_eq!(keys.len(), 2);
    assert!((keys[0].t - 4.0).abs() < 1e-6 && (keys[1].t - 8.0).abs() < 1e-6);
    assert!((keys[1].v - 1.2).abs() < 1e-9);
}
