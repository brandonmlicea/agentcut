use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY_PROPS: [&str; 7] = ["x", "y", "scale", "rotation", "opacity", "volume", "blur"];

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Project {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub background: String,
    pub font: String,
    pub tracks: Vec<Track>,
}

impl Default for Project {
    fn default() -> Self {
        Project { name: "Untitled video".into(), width: 1920, height: 1080, fps: 30, background: "#0e0e0d".into(), font: "Inter".into(), tracks: vec![] }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Track {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub muted: bool,
    pub hidden: bool,
    pub clips: Vec<Clip>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Clip {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    pub start: f64,
    pub dur: f64,
    #[serde(rename = "in")]
    pub src_in: f64,
    pub speed: f64,
    pub volume: f64,
    pub muted: bool,
    pub fade_in: f64,
    pub fade_out: f64,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub effects: Effects,
    pub style: Style,
    pub keyframes: BTreeMap<String, Vec<Key>>,
}

impl Default for Clip {
    fn default() -> Self {
        Clip {
            id: String::new(),
            name: String::new(),
            asset: None,
            text: None,
            fill: None,
            start: 0.0,
            dur: 5.0,
            src_in: 0.0,
            speed: 1.0,
            volume: 1.0,
            muted: false,
            fade_in: 0.0,
            fade_out: 0.0,
            x: 0.5,
            y: 0.5,
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            effects: Effects::default(),
            style: Style::default(),
            keyframes: BTreeMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Effects {
    pub brightness: f64,
    pub contrast: f64,
    pub saturate: f64,
    pub blur: f64,
    pub grayscale: f64,
    pub sepia: f64,
    pub hue: f64,
    pub vignette: f64,
}

impl Default for Effects {
    fn default() -> Self {
        Effects { brightness: 1.0, contrast: 1.0, saturate: 1.0, blur: 0.0, grayscale: 0.0, sepia: 0.0, hue: 0.0, vignette: 0.0 }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Style {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    pub size: f64,
    pub weight: u32,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    pub align: String,
}

impl Default for Style {
    fn default() -> Self {
        Style { font: None, size: 96.0, weight: 600, color: "#ffffff".into(), bg: None, align: "center".into() }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Key {
    pub t: f64,
    pub v: f64,
    #[serde(default = "linear")]
    pub ease: String,
}

fn linear() -> String {
    "linear".into()
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ClipPatch {
    pub asset: Option<String>,
    pub text: Option<String>,
    pub fill: Option<String>,
    pub name: Option<String>,
    pub start: Option<f64>,
    pub dur: Option<f64>,
    #[serde(rename = "in")]
    pub src_in: Option<f64>,
    pub speed: Option<f64>,
    pub volume: Option<f64>,
    pub muted: Option<bool>,
    pub fade_in: Option<f64>,
    pub fade_out: Option<f64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub scale: Option<f64>,
    pub rotation: Option<f64>,
    pub opacity: Option<f64>,
    pub effects: Option<serde_json::Map<String, serde_json::Value>>,
    pub style: Option<serde_json::Map<String, serde_json::Value>>,
}

pub const PATCH_KEYS: [&str; 22] = [
    "op", "clip", "track", "asset", "text", "fill", "name", "start", "dur", "in", "speed", "volume", "muted", "fade_in", "fade_out", "x", "y", "scale", "rotation", "opacity", "effects", "style",
];

pub const PRESETS: [(&str, [f64; 8]); 9] = [
    ("none", [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("bw", [1.05, 1.15, 1.0, 0.0, 1.0, 0.0, 0.0, 0.2]),
    ("vintage", [1.05, 0.9, 0.8, 0.0, 0.0, 0.45, -8.0, 0.35]),
    ("vivid", [1.05, 1.15, 1.45, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("cool", [1.0, 1.05, 0.95, 0.0, 0.0, 0.0, 15.0, 0.1]),
    ("warm", [1.04, 1.02, 1.1, 0.0, 0.0, 0.18, -6.0, 0.1]),
    ("dreamy", [1.12, 0.88, 1.1, 1.5, 0.0, 0.1, 0.0, 0.25]),
    ("noir", [0.9, 1.45, 1.0, 0.0, 1.0, 0.0, 0.0, 0.55]),
    ("fade", [1.1, 0.8, 0.85, 0.0, 0.0, 0.08, 0.0, 0.0]),
];

impl Effects {
    pub fn preset(values: [f64; 8]) -> Effects {
        let [brightness, contrast, saturate, blur, grayscale, sepia, hue, vignette] = values;
        Effects { brightness, contrast, saturate, blur, grayscale, sepia, hue, vignette }
    }

    pub fn set(&mut self, key: &str, v: f64) -> Result<(), String> {
        let slot = match key {
            "brightness" => (&mut self.brightness, 0.0, 3.0),
            "contrast" => (&mut self.contrast, 0.0, 3.0),
            "saturate" => (&mut self.saturate, 0.0, 3.0),
            "blur" => (&mut self.blur, 0.0, 40.0),
            "grayscale" => (&mut self.grayscale, 0.0, 1.0),
            "sepia" => (&mut self.sepia, 0.0, 1.0),
            "hue" => (&mut self.hue, -180.0, 180.0),
            "vignette" => (&mut self.vignette, 0.0, 1.0),
            _ => return Err(format!("unknown effect {key}")),
        };
        *slot.0 = v.clamp(slot.1, slot.2);
        Ok(())
    }
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.dur
    }

    pub fn overlaps(&self, other: &Clip) -> bool {
        self.id != other.id && self.start < other.end() - 1e-6 && other.start < self.end() - 1e-6
    }
}

impl Project {
    pub fn duration(&self) -> f64 {
        self.tracks.iter().flat_map(|t| t.clips.iter()).map(Clip::end).fold(0.0, f64::max)
    }

    pub fn locate(&self, clip: &str) -> Option<(usize, usize)> {
        self.tracks.iter().enumerate().find_map(|(ti, t)| t.clips.iter().position(|c| c.id == clip).map(|ci| (ti, ci)))
    }

    pub fn track_index(&self, track: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.id == track || t.name.eq_ignore_ascii_case(track))
    }
}
