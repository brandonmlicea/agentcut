use serde_json::{json, Value};

type Rgb = [f64; 3];

const USES: [(f64, &str); 6] = [
    (90.0, "Any text, even thin weights"),
    (75.0, "Body text"),
    (60.0, "Content text, AA"),
    (45.0, "Large text and headings"),
    (30.0, "Big bold titles and UI"),
    (15.0, "Shapes and dividers only"),
];

fn decode(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(c: f64) -> f64 {
    if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn oklch_to_srgb(l: f64, c: f64, h: f64) -> Rgb {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let lm = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let mm = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let sm = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
    [
        4.0767416621 * lm - 3.3077115913 * mm + 0.2309699292 * sm,
        -1.2684380046 * lm + 2.6097574011 * mm - 0.3413193965 * sm,
        -0.0041960863 * lm - 0.7034186147 * mm + 1.707614701 * sm,
    ]
    .map(encode)
}

fn srgb_to_oklch(rgb: Rgb) -> Rgb {
    let [r, g, b] = rgb.map(decode);
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let lab = [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ];
    let c = lab[1].hypot(lab[2]);
    [lab[0], c, if c < 1e-6 { 0.0 } else { lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0) }]
}

fn channel(part: &str, scale: f64) -> Option<f64> {
    let p = part.trim();
    match p.strip_suffix('%') {
        Some(n) => n.trim().parse::<f64>().ok().map(|v| v / 100.0),
        None => p.parse::<f64>().ok().map(|v| v / scale),
    }
}

pub fn parse(text: &str) -> Option<(Rgb, f64)> {
    let t = text.trim().to_lowercase();
    if let Some(hex) = t.strip_prefix('#') {
        let full: String = if hex.len() == 3 || hex.len() == 4 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.into() };
        let byte = |i: usize| u8::from_str_radix(full.get(i..i + 2)?, 16).ok().map(|v| v as f64 / 255.0);
        return match full.len() {
            6 => Some(([byte(0)?, byte(2)?, byte(4)?], 1.0)),
            8 => Some(([byte(0)?, byte(2)?, byte(4)?], byte(6)?)),
            _ => None,
        };
    }
    let (name, body) = t.split_once('(')?;
    let body = body.strip_suffix(')')?.replace(',', " ");
    let (main, alpha) = match body.split_once('/') {
        Some((m, a)) => (m.to_string(), channel(a, 1.0)?),
        None => (body.clone(), 1.0),
    };
    let parts: Vec<&str> = main.split_whitespace().collect();
    match (name.trim(), parts.as_slice()) {
        ("rgb" | "rgba", [r, g, b, rest @ ..]) => Some(([channel(r, 255.0)?, channel(g, 255.0)?, channel(b, 255.0)?], rest.first().and_then(|a| channel(a, 1.0)).unwrap_or(alpha))),
        ("oklch", [l, c, h]) => {
            let rgb = oklch_to_srgb(channel(l, 1.0)?, c.parse().ok()?, h.trim_end_matches("deg").parse().ok()?);
            Some((rgb.map(|v| v.clamp(0.0, 1.0)), alpha))
        }
        _ => None,
    }
}

pub fn hex(rgb: Rgb) -> String {
    rgb.iter().fold("#".to_string(), |s, v| s + &format!("{:02x}", (v.clamp(0.0, 1.0) * 255.0).round() as u8))
}

fn over(top: (Rgb, f64), bottom: Rgb) -> Rgb {
    [0, 1, 2].map(|i| top.0[i] * top.1 + bottom[i] * (1.0 - top.1))
}

pub fn wcag(a: Rgb, b: Rgb) -> f64 {
    let y = |c: Rgb| { let [r, g, b] = c.map(decode); 0.2126 * r + 0.7152 * g + 0.0722 * b };
    let (hi, lo) = (y(a).max(y(b)), y(a).min(y(b)));
    (hi + 0.05) / (lo + 0.05)
}

pub fn apca(text: Rgb, bg: Rgb) -> f64 {
    let y = |c: Rgb| {
        let v = 0.2126729 * c[0].powf(2.4) + 0.7151522 * c[1].powf(2.4) + 0.072175 * c[2].powf(2.4);
        if v < 0.022 { v + (0.022 - v).powf(1.414) } else { v }
    };
    let (yt, yb) = (y(text), y(bg));
    if (yb - yt).abs() < 0.0005 {
        return 0.0;
    }
    let s = if yb > yt { (yb.powf(0.56) - yt.powf(0.57)) * 1.14 } else { (yb.powf(0.65) - yt.powf(0.62)) * 1.14 };
    if s.abs() < 0.1 { 0.0 } else if s > 0.0 { (s - 0.027) * 100.0 } else { (s + 0.027) * 100.0 }
}

fn in_gamut(l: f64, c: f64, h: f64) -> bool {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let lm = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let mm = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let sm = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
    [4.0767416621 * lm - 3.3077115913 * mm + 0.2309699292 * sm, -1.2684380046 * lm + 2.6097574011 * mm - 0.3413193965 * sm, -0.0041960863 * lm - 0.7034186147 * mm + 1.707614701 * sm]
        .iter()
        .all(|v| (-1e-4..=1.0001).contains(v))
}

fn at(l: f64, c: f64, h: f64) -> Rgb {
    let (mut lo, mut hi) = (0.0, c);
    if !in_gamut(l, c, h) {
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if in_gamut(l, mid, h) { lo = mid } else { hi = mid }
        }
        return oklch_to_srgb(l, lo, h).map(|v| v.clamp(0.0, 1.0));
    }
    oklch_to_srgb(l, c, h).map(|v| v.clamp(0.0, 1.0))
}

fn fit(moving: Rgb, fixed: Rgb, moving_is_text: bool, goal: (f64, f64)) -> Option<Rgb> {
    let [l, c, h] = srgb_to_oklch(moving);
    let passes = |rgb: Rgb| {
        let (t, b) = if moving_is_text { (rgb, fixed) } else { (fixed, rgb) };
        wcag(t, b) >= goal.0 && apca(t, b).abs() >= goal.1
    };
    let darker = srgb_to_oklch(fixed)[0] >= l;
    if !passes(at(if darker { 0.0 } else { 1.0 }, c, h)) {
        let other = at(if darker { 1.0 } else { 0.0 }, c, h);
        return passes(other).then_some(other);
    }
    let (mut lo, mut hi) = if darker { (0.0, l) } else { (l, 1.0) };
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if passes(at(mid, c, h)) == darker { lo = mid } else { hi = mid }
    }
    let mut l = if darker { lo } else { hi };
    for _ in 0..200 {
        let snapped = parse(&hex(at(l, c, h)))?.0;
        if passes(snapped) || !(0.0..=1.0).contains(&l) {
            return Some(snapped);
        }
        l += if darker { -0.002 } else { 0.002 };
    }
    None
}

fn round(v: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (v * p).round() / p
}

pub fn check(foreground: &str, background: &str, goal: &str, adjust: &str) -> Value {
    let (Some(fg), Some(bg)) = (parse(foreground), parse(background)) else {
        return json!({ "error": "Use hex, rgb() or oklch() colours." });
    };
    let base = over(bg, [1.0, 1.0, 1.0]);
    let text = over(fg, base);
    let target = if goal == "AAA" { (7.0, 75.0) } else { (4.5, 60.0) };
    let (ratio, lc) = (wcag(text, base), apca(text, base));
    let met = ratio >= target.0 && lc.abs() >= target.1;
    let use_ = USES.iter().find(|(floor, _)| lc.abs() >= *floor).map(|u| u.1).unwrap_or("Not readable as text");
    let fix = (!met).then(|| {
        let moved = if adjust == "background" { fit(base, text, false, target) } else { fit(text, base, true, target) };
        match moved {
            Some(m) => {
                let (t, b) = if adjust == "background" { (text, m) } else { (m, base) };
                json!({ adjust: hex(m), "wcag": round(wcag(t, b), 2), "apca": round(apca(t, b), 1) })
            }
            None => json!(format!("No lightness at this hue reaches {goal}.")),
        }
    });
    json!({
        "foreground": hex(text), "background": hex(base), "goal": goal,
        "wcag": round(ratio, 2), "apca": round(lc, 1), "AA": ratio >= 4.5, "AAA": ratio >= 7.0, "largeText": ratio >= 3.0,
        "passes": met, "use": use_, "fix": fix,
    })
}
