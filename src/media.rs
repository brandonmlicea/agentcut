use crate::ops::AssetInfo;
use crate::util::{id, now, nullable};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::{Bucket, D1Database, Data, Fetch, Headers, HttpMetadata, Range, Request, Response, Result, Url};

#[derive(Serialize, Deserialize, Clone)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub mime: String,
    pub size: f64,
    pub duration: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub created: f64,
}

pub fn kind_of(mime: &str) -> Option<&'static str> {
    match mime.split('/').next() {
        Some("video") => Some("video"),
        Some("audio") => Some("audio"),
        Some("image") => Some("image"),
        _ => None,
    }
}

fn key(user: &str, asset: &str) -> String {
    format!("u/{user}/{asset}")
}

fn frames(user: &str, asset: &str) -> String {
    format!("f/{user}/{asset}/")
}

pub async fn frame(bucket: &Bucket, user: &str, asset: &str, second: u32) -> Result<Option<Vec<u8>>> {
    match bucket.get(format!("{}{second}", frames(user, asset))).execute().await? {
        Some(object) => Ok(Some(object.body().ok_or("empty frame")?.bytes().await?)),
        None => Ok(None),
    }
}

pub async fn put_frame(db: &D1Database, bucket: &Bucket, user: &str, asset: &str, second: u32, bytes: Vec<u8>) -> Result<Response> {
    let owned: Option<String> = db.prepare("SELECT id FROM assets WHERE id = ?1 AND user_id = ?2").bind(&[asset.into(), user.into()])?.first(Some("id")).await?;
    if owned.is_none() {
        return Response::error("Not found", 404);
    }
    if bytes.len() > 300_000 || !bytes.starts_with(&[0xff, 0xd8]) {
        return Response::error("A frame is one JPEG under 300 KB.", 400);
    }
    bucket.put(format!("{}{second}", frames(user, asset)), Data::Bytes(bytes)).http_metadata(HttpMetadata { content_type: Some("image/jpeg".into()), ..Default::default() }).execute().await?;
    Response::from_json(&serde_json::json!({ "ok": true }))
}

pub async fn list(db: &D1Database, user: &str) -> Result<Vec<Asset>> {
    db.prepare("SELECT id, name, kind, mime, size, duration, width, height, created FROM assets WHERE user_id = ?1 ORDER BY created DESC")
        .bind(&[user.into()])?
        .all()
        .await?
        .results()
}

pub async fn infos(db: &D1Database, user: &str) -> Result<HashMap<String, AssetInfo>> {
    Ok(list(db, user).await?.into_iter().map(|a| (a.id, AssetInfo { kind: a.kind, duration: a.duration, name: a.name })).collect())
}

async fn record(db: &D1Database, user: &str, asset: &Asset) -> Result<()> {
    db.prepare("INSERT INTO assets (id, user_id, name, kind, mime, size, duration, width, height, created) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")
        .bind(&[
            asset.id.clone().into(),
            user.into(),
            asset.name.clone().into(),
            asset.kind.clone().into(),
            asset.mime.clone().into(),
            asset.size.into(),
            nullable(asset.duration),
            nullable(asset.width),
            nullable(asset.height),
            asset.created.into(),
        ])?
        .run()
        .await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct UploadQuery {
    name: String,
    duration: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
}

pub async fn upload(db: &D1Database, bucket: &Bucket, user: &str, req: &Request) -> Result<Response> {
    let q: UploadQuery = req.query()?;
    let mime = req.headers().get("content-type")?.unwrap_or_default();
    let Some(kind) = kind_of(&mime) else { return Response::error("Upload a video, audio or image file.", 415) };
    let size: f64 = req.headers().get("content-length")?.and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let Some(body) = req.inner().body() else { return Response::error("Empty upload", 400) };
    let asset = Asset { id: format!("a{}", id(10)), name: q.name.chars().take(120).collect(), kind: kind.into(), mime: mime.clone(), size, duration: q.duration, width: q.width, height: q.height, created: now() as f64 };
    bucket.put(key(user, &asset.id), Data::ReadableStream(body)).http_metadata(HttpMetadata { content_type: Some(mime), ..Default::default() }).execute().await?;
    record(db, user, &asset).await?;
    Response::from_json(&asset)
}

pub async fn import_url(db: &D1Database, bucket: &Bucket, user: &str, url: &str, name: Option<String>) -> std::result::Result<Asset, String> {
    let parsed = Url::parse(url).map_err(|_| format!("{url} is not a web address"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Only http and https addresses can be imported.".into());
    }
    let mut res = Fetch::Url(parsed.clone()).send().await.map_err(|e| e.to_string())?;
    if res.status_code() >= 400 {
        return Err(format!("{url} answered {}", res.status_code()));
    }
    let mime = res.headers().get("content-type").ok().flatten().unwrap_or_default().split(';').next().unwrap_or_default().trim().to_string();
    let kind = kind_of(&mime).ok_or(format!("{url} is {mime}, not video, audio or an image"))?;
    let length: Option<f64> = res.headers().get("content-length").ok().flatten().and_then(|v| v.parse().ok());
    let file = parsed.path_segments().and_then(|mut s| s.next_back().map(str::to_string)).filter(|s| !s.is_empty()).unwrap_or_else(|| "Imported".into());
    let asset = Asset { id: format!("a{}", id(10)), name: name.unwrap_or(file).chars().take(120).collect(), kind: kind.into(), mime: mime.clone(), size: length.unwrap_or(0.0), duration: None, width: None, height: None, created: now() as f64 };
    let stream = match (length, res.body()) {
        (Some(_), worker::ResponseBody::Stream(s)) => Some(s.clone()),
        _ => None,
    };
    let data = match stream {
        Some(s) => Data::ReadableStream(s),
        _ => {
            let bytes = res.bytes().await.map_err(|e| e.to_string())?;
            if bytes.len() > 90_000_000 {
                return Err("That file is over 90 MB.".into());
            }
            Data::Bytes(bytes)
        }
    };
    bucket.put(key(user, &asset.id), data).http_metadata(HttpMetadata { content_type: Some(mime), ..Default::default() }).execute().await.map_err(|e| e.to_string())?;
    record(db, user, &asset).await.map_err(|e| e.to_string())?;
    Ok(asset)
}

pub async fn serve(db: &D1Database, bucket: &Bucket, user: &str, asset: &str, req: &Request) -> Result<Response> {
    let mime: Option<String> = db.prepare("SELECT mime FROM assets WHERE id = ?1 AND user_id = ?2").bind(&[asset.into(), user.into()])?.first(Some("mime")).await?;
    let Some(mime) = mime else { return Response::error("Not found", 404) };
    let Some(head) = bucket.head(key(user, asset)).await? else { return Response::error("Not found", 404) };
    let total = head.size();
    let span = req.headers().get("range")?.and_then(|r| {
        let (a, b) = r.strip_prefix("bytes=")?.split_once('-')?;
        match (a.trim().parse::<u64>().ok(), b.trim().parse::<u64>().ok()) {
            (Some(s), Some(e)) if s < total && e >= s => Some((s, e.min(total - 1))),
            (Some(s), None) if s < total => Some((s, total - 1)),
            (None, Some(n)) if n > 0 && total > 0 => Some((total - n.min(total), total - 1)),
            _ => None,
        }
    });
    let mut get = bucket.get(key(user, asset));
    if let Some((s, e)) = span {
        get = get.range(Range::OffsetWithLength { offset: s, length: e - s + 1 });
    }
    let Some(object) = get.execute().await? else { return Response::error("Not found", 404) };
    let headers = Headers::new();
    headers.set("content-type", &mime)?;
    headers.set("accept-ranges", "bytes")?;
    headers.set("cache-control", "private, max-age=31536000, immutable")?;
    let status = match span {
        Some((s, e)) => {
            headers.set("content-range", &format!("bytes {s}-{e}/{total}"))?;
            headers.set("content-length", &(e - s + 1).to_string())?;
            206
        }
        None => {
            headers.set("content-length", &total.to_string())?;
            200
        }
    };
    let body = object.body().ok_or("empty object")?.response_body()?;
    Ok(Response::from_body(body)?.with_headers(headers).with_status(status))
}

pub async fn remove(db: &D1Database, bucket: &Bucket, user: &str, asset: &str) -> Result<()> {
    bucket.delete(key(user, asset)).await?;
    for object in bucket.list().prefix(frames(user, asset)).execute().await?.objects() {
        bucket.delete(object.key()).await?;
    }
    db.prepare("DELETE FROM assets WHERE id = ?1 AND user_id = ?2").bind(&[asset.into(), user.into()])?.run().await?;
    Ok(())
}

pub async fn measure(db: &D1Database, user: &str, asset: &str, duration: Option<f64>, width: Option<f64>, height: Option<f64>) -> Result<()> {
    db.prepare("UPDATE assets SET duration = COALESCE(?1, duration), width = COALESCE(?2, width), height = COALESCE(?3, height) WHERE id = ?4 AND user_id = ?5")
        .bind(&[nullable(duration), nullable(width), nullable(height), asset.into(), user.into()])?
        .run()
        .await?;
    Ok(())
}
