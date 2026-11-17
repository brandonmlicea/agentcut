use crate::util::{b64, hash, id, now, random, sha256, unb64};
use js_sys::{Array, Function, Promise, Reflect, Uint8Array, JSON};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use worker::{D1Database, Request, Response, Result};

pub const COOKIE: &str = "ac";
pub const MAX_AGE: i64 = 34_560_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proof {
    id: String,
    client_data_json: String,
    authenticator_data: String,
    signature: Option<String>,
    public_key: Option<String>,
    alg: Option<i64>,
}

pub struct Who {
    pub user: String,
    pub refresh: Option<String>,
}

fn js(e: JsValue) -> String {
    e.as_string().or_else(|| JSON::stringify(&e).ok().and_then(|s| s.as_string())).unwrap_or_else(|| "crypto error".into())
}

fn der_to_raw(der: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let bad = || "bad signature".to_string();
    let int = |at: usize| -> std::result::Result<(Vec<u8>, usize), String> {
        if der.get(at) != Some(&2) {
            return Err(bad());
        }
        let len = *der.get(at + 1).ok_or_else(bad)? as usize;
        let mut v = der.get(at + 2..at + 2 + len).ok_or_else(bad)?.to_vec();
        while v.len() > 32 && v[0] == 0 {
            v.remove(0);
        }
        let mut out = vec![0u8; 32usize.saturating_sub(v.len())];
        out.extend(v);
        Ok((out, at + 2 + len))
    };
    let start = if der.get(1).map(|b| b & 0x80 != 0).unwrap_or(false) { 3 } else { 2 };
    let (r, next) = int(start)?;
    let (s, _) = int(next)?;
    Ok([r, s].concat())
}

async fn verify(spki: &[u8], alg: i64, sig: &[u8], data: &[u8]) -> std::result::Result<bool, String> {
    let (import, check, sig) = match alg {
        -7 => (r#"{"name":"ECDSA","namedCurve":"P-256"}"#, r#"{"name":"ECDSA","hash":"SHA-256"}"#, der_to_raw(sig)?),
        -257 => (r#"{"name":"RSASSA-PKCS1-v1_5","hash":"SHA-256"}"#, r#"{"name":"RSASSA-PKCS1-v1_5"}"#, sig.to_vec()),
        -8 => (r#"{"name":"Ed25519"}"#, r#"{"name":"Ed25519"}"#, sig.to_vec()),
        _ => return Err(format!("unsupported algorithm {alg}")),
    };
    let crypto = Reflect::get(&js_sys::global(), &"crypto".into()).map_err(js)?;
    let subtle = Reflect::get(&crypto, &"subtle".into()).map_err(js)?;
    let call = |name: &str, args: Array| -> std::result::Result<JsFuture, String> {
        let f: Function = Reflect::get(&subtle, &name.into()).map_err(js)?.dyn_into().map_err(js)?;
        let p: Promise = Reflect::apply(&f, &subtle, &args).map_err(js)?.dyn_into().map_err(js)?;
        Ok(JsFuture::from(p))
    };
    let usage = Array::of1(&"verify".into());
    let args = Array::of5(&"spki".into(), &Uint8Array::from(spki).into(), &JSON::parse(import).map_err(js)?, &false.into(), &usage.into());
    let key = call("importKey", args)?.await.map_err(js)?;
    let args = Array::of4(&JSON::parse(check).map_err(js)?, &key, &Uint8Array::from(&sig[..]).into(), &Uint8Array::from(data).into());
    Ok(call("verify", args)?.await.map_err(js)?.as_bool().unwrap_or(false))
}

pub fn origin_of(req: &Request) -> Result<(String, String)> {
    let url = req.url()?;
    let host = url.host_str().unwrap_or_default().to_string();
    Ok((url.origin().ascii_serialization(), host))
}

pub async fn options(db: &D1Database, req: &Request, register: bool) -> Result<Value> {
    let (_, host) = origin_of(req)?;
    let challenge = b64(&random(32));
    let user = register.then(|| format!("u{}", id(14)));
    db.prepare("DELETE FROM challenges WHERE created < ?1").bind(&[((now() - 600_000) as f64).into()])?.run().await?;
    db.prepare("INSERT INTO challenges (id, user_id, created) VALUES (?1, ?2, ?3)")
        .bind(&[challenge.clone().into(), user.clone().map(JsValue::from).unwrap_or(JsValue::NULL), (now() as f64).into()])?
        .run()
        .await?;
    Ok(match user {
        Some(u) => json!({ "challenge": challenge, "rpId": host, "user": { "id": b64(u.as_bytes()), "name": format!("AgentCut {}", &u[1..7]), "displayName": "AgentCut editor" } }),
        None => json!({ "challenge": challenge, "rpId": host }),
    })
}

pub async fn finish(db: &D1Database, req: &Request, proof: Proof, register: bool) -> std::result::Result<String, String> {
    let (origin, host) = origin_of(req).map_err(|e| e.to_string())?;
    let client_raw = unb64(&proof.client_data_json)?;
    let client: ClientData = serde_json::from_slice(&client_raw).map_err(|e| e.to_string())?;
    let want = if register { "webauthn.create" } else { "webauthn.get" };
    if client.kind != want || client.origin != origin {
        return Err("This passkey answer was made for another page.".into());
    }
    #[derive(Deserialize)]
    struct Row {
        user_id: Option<String>,
        created: f64,
    }
    let row: Option<Row> = db
        .prepare("DELETE FROM challenges WHERE id = ?1 RETURNING user_id, created")
        .bind(&[client.challenge.clone().into()])
        .map_err(|e| e.to_string())?
        .first(None)
        .await
        .map_err(|e| e.to_string())?;
    let row = row.filter(|r| (now() as f64) - r.created < 600_000.0).ok_or("That passkey request expired. Try again.")?;
    let auth = unb64(&proof.authenticator_data)?;
    if auth.len() < 37 || auth[..32] != sha256(host.as_bytes())[..] || auth[32] & 1 == 0 {
        return Err("The passkey answered for another site.".into());
    }
    let count = u32::from_be_bytes([auth[33], auth[34], auth[35], auth[36]]) as f64;
    if register {
        let user = row.user_id.ok_or("Start again.")?;
        if auth[32] & 0x40 == 0 || auth.len() < 55 {
            return Err("The passkey sent no key.".into());
        }
        let len = u16::from_be_bytes([auth[53], auth[54]]) as usize;
        let cred = auth.get(55..55 + len).ok_or("The passkey sent a broken key.")?;
        if b64(cred) != proof.id.trim_end_matches('=') {
            return Err("The passkey id does not match.".into());
        }
        let key = proof.public_key.ok_or("This browser did not share the passkey's public key.")?;
        let alg = proof.alg.ok_or("This browser did not share the passkey's algorithm.")?;
        let t = now() as f64;
        let stmts = vec![
            db.prepare("INSERT INTO users (id, created) VALUES (?1, ?2)").bind(&[user.clone().into(), t.into()]).map_err(|e| e.to_string())?,
            db.prepare("INSERT INTO credentials (id, user_id, public_key, alg, sign_count, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
                .bind(&[b64(cred).into(), user.clone().into(), key.into(), (alg as f64).into(), count.into(), t.into()])
                .map_err(|e| e.to_string())?,
        ];
        db.batch(stmts).await.map_err(|e| e.to_string())?;
        return Ok(user);
    }
    #[derive(Deserialize)]
    struct Cred {
        user_id: String,
        public_key: String,
        alg: f64,
    }
    let cred: Cred = db
        .prepare("SELECT user_id, public_key, alg FROM credentials WHERE id = ?1")
        .bind(&[proof.id.trim_end_matches('=').into()])
        .map_err(|e| e.to_string())?
        .first(None)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("This passkey isn't registered here. Create a new one.")?;
    let signed = [auth.clone(), sha256(&client_raw)].concat();
    let sig = unb64(proof.signature.as_deref().ok_or("No signature.")?)?;
    if !verify(&unb64(&cred.public_key)?, cred.alg as i64, &sig, &signed).await? {
        return Err("The passkey signature did not check out.".into());
    }
    db.prepare("UPDATE credentials SET sign_count = ?1 WHERE id = ?2").bind(&[count.into(), proof.id.trim_end_matches('=').into()]).map_err(|e| e.to_string())?.run().await.map_err(|e| e.to_string())?;
    Ok(cred.user_id)
}

pub async fn open_session(db: &D1Database, user: &str) -> Result<String> {
    let token = b64(&random(32));
    let t = now() as f64;
    db.prepare("INSERT INTO sessions (id, user_id, created, seen) VALUES (?1, ?2, ?3, ?3)").bind(&[hash(&token).into(), user.into(), t.into()])?.run().await?;
    Ok(token)
}

pub fn cookie(token: &str, secure: bool) -> String {
    format!("{COOKIE}={token}; Path=/; Max-Age={MAX_AGE}; HttpOnly; SameSite=Lax{}", if secure { "; Secure" } else { "" })
}

fn cookie_value(req: &Request) -> Option<String> {
    let header = req.headers().get("cookie").ok()??;
    header.split(';').find_map(|p| p.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

pub async fn who(db: &D1Database, req: &Request) -> Result<Option<Who>> {
    if let Some(key) = req.headers().get("authorization")?.and_then(|h| h.strip_prefix("Bearer ").map(str::to_string)) {
        let user: Option<String> = db.prepare("SELECT id FROM users WHERE mcp_key = ?1").bind(&[hash(key.trim()).into()])?.first(Some("id")).await?;
        return Ok(user.map(|user| Who { user, refresh: None }));
    }
    let Some(token) = cookie_value(req) else { return Ok(None) };
    #[derive(Deserialize)]
    struct Row {
        user_id: String,
        seen: f64,
    }
    let row: Option<Row> = db.prepare("SELECT user_id, seen FROM sessions WHERE id = ?1").bind(&[hash(&token).into()])?.first(None).await?;
    let Some(row) = row else { return Ok(None) };
    let stale = (now() as f64) - row.seen > DAY_MS as f64;
    if stale {
        db.prepare("UPDATE sessions SET seen = ?1 WHERE id = ?2").bind(&[(now() as f64).into(), hash(&token).into()])?.run().await?;
    }
    Ok(Some(Who { user: row.user_id, refresh: stale.then_some(token) }))
}

pub async fn close_session(db: &D1Database, req: &Request) -> Result<()> {
    if let Some(token) = cookie_value(req) {
        db.prepare("DELETE FROM sessions WHERE id = ?1").bind(&[hash(&token).into()])?.run().await?;
    }
    Ok(())
}

pub async fn mcp_key(db: &D1Database, user: &str) -> Result<String> {
    let key = format!("ack_{}", b64(&random(24)));
    db.prepare("UPDATE users SET mcp_key = ?1 WHERE id = ?2").bind(&[hash(&key).into(), user.into()])?.run().await?;
    Ok(key)
}

pub fn unauthorized() -> Result<Response> {
    Response::error("Sign in first", 401)
}
