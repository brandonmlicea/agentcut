mod agent;
mod auth;
mod contrast;
mod mcp;
mod media;
mod model;
mod ops;
mod store;
mod util;
#[cfg(test)]
mod tests;

use serde::Deserialize;
use serde_json::{json, Value};
use worker::*;

const LOGIN: &str = include_str!("../web/login.html");
const APP: &str = include_str!("../web/app.html");
const STATIC: [(&str, &str, &str); 10] = [
    ("/app.css", "text/css", include_str!("../web/app.css")),
    ("/mark.svg", "image/svg+xml", include_str!("../web/mark.svg")),
    ("/js/main.js", "text/javascript", include_str!("../web/js/main.js")),
    ("/js/state.js", "text/javascript", include_str!("../web/js/state.js")),
    ("/js/look.js", "text/javascript", include_str!("../web/js/look.js")),
    ("/js/player.js", "text/javascript", include_str!("../web/js/player.js")),
    ("/js/timeline.js", "text/javascript", include_str!("../web/js/timeline.js")),
    ("/js/panels.js", "text/javascript", include_str!("../web/js/panels.js")),
    ("/js/inspector.js", "text/javascript", include_str!("../web/js/inspector.js")),
    ("/js/chat.js", "text/javascript", include_str!("../web/js/chat.js")),
];

fn page(body: &str, kind: &str) -> Result<Response> {
    let headers = Headers::new();
    headers.set("content-type", &format!("{kind}; charset=utf-8"))?;
    headers.set("cache-control", "no-cache")?;
    Ok(Response::ok(body)?.with_headers(headers))
}

fn go(url: &Url, to: &str) -> Result<Response> {
    Response::redirect(url.join(to)?)
}

fn bad(msg: impl Into<String>) -> Result<Response> {
    Response::from_json(&json!({ "error": msg.into() })).map(|r| r.with_status(400))
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let db = env.d1("DB")?;
    let url = req.url()?;
    let path = url.path().to_string();
    let method = req.method();
    let secure = url.scheme() == "https";
    if method == Method::Get {
        if let Some((_, kind, body)) = STATIC.iter().find(|(p, _, _)| *p == path) {
            return page(body, kind);
        }
    }
    match (method.clone(), path.as_str()) {
        (Method::Post, "/auth/options") => {
            #[derive(Deserialize)]
            struct Mode {
                register: bool,
            }
            let mode: Mode = req.json().await?;
            return Response::from_json(&auth::options(&db, &req, mode.register).await?);
        }
        (Method::Post, "/auth/register" | "/auth/login") => {
            let proof: auth::Proof = req.json().await?;
            return match auth::finish(&db, &req, proof, path == "/auth/register").await {
                Ok(user) => {
                    let token = auth::open_session(&db, &user).await?;
                    let mut res = Response::from_json(&json!({ "ok": true }))?;
                    res.headers_mut().set("set-cookie", &auth::cookie(&token, secure))?;
                    Ok(res)
                }
                Err(e) => bad(e),
            };
        }
        (Method::Post, "/auth/logout") => {
            auth::close_session(&db, &req).await?;
            let mut res = Response::from_json(&json!({ "ok": true }))?;
            res.headers_mut().set("set-cookie", &format!("{}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax", auth::COOKIE))?;
            return Ok(res);
        }
        _ => {}
    }
    let who = auth::who(&db, &req).await?;
    if path == "/login" {
        return if who.is_some() { go(&url, "/") } else { page(LOGIN, "text/html") };
    }
    let Some(who) = who else {
        let api = path.starts_with("/api/") || path.starts_with("/media/") || path == "/mcp";
        return if api { auth::unauthorized() } else { go(&url, "/login") };
    };
    let mut res = route(req, &env, &db, &who.user, &url, &path, method).await?;
    if let Some(token) = who.refresh {
        res.headers_mut().append("set-cookie", &auth::cookie(&token, secure))?;
    }
    Ok(res)
}

async fn route(mut req: Request, env: &Env, db: &D1Database, user: &str, url: &Url, path: &str, method: Method) -> Result<Response> {
    let bucket = env.bucket("MEDIA")?;
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    match (method, parts.as_slice()) {
        (Method::Get, [""] | ["app"]) => page(APP, "text/html"),
        (Method::Get, ["api", "me"]) => Response::from_json(&json!({ "projects": store::list(db, user).await?, "assets": media::list(db, user).await?, "model": agent::MODEL })),
        (Method::Post, ["api", "projects"]) => {
            let body: Value = req.json().await.unwrap_or_default();
            let id = store::create(db, user, body["name"].as_str().map(str::to_string)).await?;
            Response::from_json(&json!({ "id": id }))
        }
        (Method::Get, ["api", "projects", pid]) => match store::load(db, user, pid).await? {
            Some(project) => Response::from_json(&json!({ "id": pid, "project": project, "messages": store::messages(db, pid, 60).await? })),
            None => Response::error("Not found", 404),
        },
        (Method::Put, ["api", "projects", pid]) => {
            #[derive(Deserialize)]
            struct Body {
                project: model::Project,
            }
            let body: Body = req.json().await?;
            if store::load(db, user, pid).await?.is_none() {
                return Response::error("Not found", 404);
            }
            store::save(db, user, pid, &body.project).await?;
            Response::from_json(&json!({ "ok": true }))
        }
        (Method::Delete, ["api", "projects", pid]) => {
            store::remove(db, user, pid).await?;
            Response::from_json(&json!({ "ok": true }))
        }
        (Method::Post, ["api", "projects", pid, "ops"]) => {
            let body: Value = req.json().await?;
            let ops = body["ops"].as_array().cloned().unwrap_or_default();
            match store::edit(db, &bucket, user, pid, ops).await? {
                Some((project, outcome, assets)) => Response::from_json(&json!({ "project": project, "done": outcome.done, "errors": outcome.errors, "seek": outcome.seek, "assets": assets })),
                None => Response::error("Not found", 404),
            }
        }
        (Method::Post, ["api", "projects", pid, "chat"]) => {
            let ask: agent::Ask = req.json().await?;
            if ask.message.trim().is_empty() {
                return bad("Say what to change.");
            }
            match agent::chat(env, db, &bucket, user, pid, ask).await? {
                Some(out) => Response::from_json(&out),
                None => Response::error("Not found", 404),
            }
        }
        (Method::Get, ["api", "assets"]) => Response::from_json(&media::list(db, user).await?),
        (Method::Put, ["api", "assets"]) => media::upload(db, &bucket, user, &req).await,
        (Method::Patch, ["api", "assets", aid]) => {
            let body: Value = req.json().await?;
            media::measure(db, user, aid, body["duration"].as_f64(), body["width"].as_f64(), body["height"].as_f64()).await?;
            Response::from_json(&json!({ "ok": true }))
        }
        (Method::Delete, ["api", "assets", aid]) => {
            media::remove(db, &bucket, user, aid).await?;
            Response::from_json(&json!({ "ok": true }))
        }
        (Method::Post, ["api", "import"]) => {
            let body: Value = req.json().await?;
            match media::import_url(db, &bucket, user, body["url"].as_str().unwrap_or_default(), body["name"].as_str().map(str::to_string)).await {
                Ok(asset) => Response::from_json(&asset),
                Err(e) => bad(e),
            }
        }
        (Method::Post, ["api", "contrast"]) => {
            let b: Value = req.json().await?;
            let s = |k: &str, d: &'static str| b[k].as_str().unwrap_or(d).to_string();
            Response::from_json(&contrast::check(&s("foreground", "#000"), &s("background", "#fff"), &s("goal", "AA"), &s("adjust", "foreground")))
        }
        (Method::Post, ["api", "mcp-key"]) => Response::from_json(&json!({ "key": auth::mcp_key(db, user).await?, "url": url.join("/mcp")?.to_string() })),
        (Method::Get, ["media", aid]) => media::serve(db, &bucket, user, aid, &req).await,
        (Method::Post, ["mcp"]) => {
            let body: Value = req.json().await?;
            let replies: Vec<Value> = match body {
                Value::Array(batch) => {
                    let mut out = vec![];
                    for b in batch {
                        out.extend(mcp::handle(db, &bucket, user, b).await?);
                    }
                    out
                }
                one => mcp::handle(db, &bucket, user, one).await?.into_iter().collect(),
            };
            match replies.len() {
                0 => Ok(Response::empty()?.with_status(202)),
                1 => Response::from_json(&replies[0]),
                _ => Response::from_json(&replies),
            }
        }
        (Method::Get, ["mcp"]) => Ok(Response::error("Use POST", 405)?),
        _ => Response::error("Not found", 404),
    }
}
