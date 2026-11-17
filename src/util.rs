use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

pub fn random(n: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; n];
    getrandom::getrandom(&mut bytes).expect("random");
    bytes
}

pub fn id(n: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    random(n).iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect()
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(text: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD.decode(text.trim_end_matches('=').replace('+', "-").replace('/', "_")).map_err(|e| e.to_string())
}

pub fn sha256(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

pub fn hash(text: &str) -> String {
    b64(&sha256(text.as_bytes()))
}

pub fn now() -> i64 {
    js_sys::Date::now() as i64
}

pub fn secs(v: f64) -> String {
    format!("{:.2}s", v)
}

pub fn nullable<T: Into<wasm_bindgen::JsValue>>(v: Option<T>) -> wasm_bindgen::JsValue {
    v.map(Into::into).unwrap_or(wasm_bindgen::JsValue::NULL)
}
