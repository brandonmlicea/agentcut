use crate::media::{self, Asset};
use crate::model::Project;
use crate::ops::{apply, Outcome};
use crate::util::{id, now, nullable};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use worker::{Bucket, D1Database, Result};

#[derive(Serialize, Deserialize)]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub updated: f64,
}

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub meta: Option<String>,
    pub created: f64,
}

pub async fn list(db: &D1Database, user: &str) -> Result<Vec<Summary>> {
    db.prepare("SELECT id, name, updated FROM projects WHERE user_id = ?1 ORDER BY updated DESC").bind(&[user.into()])?.all().await?.results()
}

pub async fn load(db: &D1Database, user: &str, project: &str) -> Result<Option<Project>> {
    let data: Option<String> = db.prepare("SELECT data FROM projects WHERE id = ?1 AND user_id = ?2").bind(&[project.into(), user.into()])?.first(Some("data")).await?;
    Ok(data.map(|d| serde_json::from_str(&d).unwrap_or_default()))
}

pub async fn save(db: &D1Database, user: &str, project: &str, data: &Project) -> Result<()> {
    db.prepare("UPDATE projects SET name = ?1, data = ?2, updated = ?3 WHERE id = ?4 AND user_id = ?5")
        .bind(&[data.name.clone().into(), serde_json::to_string(data)?.into(), (now() as f64).into(), project.into(), user.into()])?
        .run()
        .await?;
    Ok(())
}

pub async fn create(db: &D1Database, user: &str, name: Option<String>) -> Result<String> {
    let project = Project { name: name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Untitled video".into()), ..Default::default() };
    let pid = format!("p{}", id(10));
    db.prepare("INSERT INTO projects (id, user_id, name, data, updated) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(&[pid.clone().into(), user.into(), project.name.clone().into(), serde_json::to_string(&project)?.into(), (now() as f64).into()])?
        .run()
        .await?;
    Ok(pid)
}

pub async fn remove(db: &D1Database, user: &str, project: &str) -> Result<()> {
    db.batch(vec![
        db.prepare("DELETE FROM messages WHERE project_id = (SELECT id FROM projects WHERE id = ?1 AND user_id = ?2)").bind(&[project.into(), user.into()])?,
        db.prepare("DELETE FROM projects WHERE id = ?1 AND user_id = ?2").bind(&[project.into(), user.into()])?,
    ])
    .await?;
    Ok(())
}

pub async fn stamp(db: &D1Database, user: &str, project: &str) -> Result<Option<f64>> {
    db.prepare("SELECT updated FROM projects WHERE id = ?1 AND user_id = ?2").bind(&[project.into(), user.into()])?.first(Some("updated")).await
}

pub async fn messages(db: &D1Database, project: &str, limit: u32) -> Result<Vec<Message>> {
    let mut rows: Vec<Message> = db
        .prepare("SELECT role, content, meta, created FROM messages WHERE project_id = ?1 ORDER BY id DESC LIMIT ?2")
        .bind(&[project.into(), (limit as f64).into()])?
        .all()
        .await?
        .results()?;
    rows.reverse();
    Ok(rows)
}

pub async fn add_message(db: &D1Database, project: &str, role: &str, content: &str, meta: Option<String>) -> Result<()> {
    db.prepare("INSERT INTO messages (project_id, role, content, meta, created) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(&[project.into(), role.into(), content.into(), nullable(meta), (now() as f64).into()])?
        .run()
        .await?;
    Ok(())
}

pub async fn edit(db: &D1Database, bucket: &Bucket, user: &str, pid: &str, ops: Vec<Value>) -> Result<Option<(Project, Outcome, Vec<Asset>)>> {
    let Some(mut project) = load(db, user, pid).await? else { return Ok(None) };
    let mut ready = Vec::new();
    let mut failed = Vec::new();
    for (i, op) in ops.into_iter().enumerate() {
        if op.get("op").and_then(Value::as_str) != Some("import_url") {
            ready.push(op);
            continue;
        }
        let url = op.get("url").and_then(Value::as_str).unwrap_or_default().to_string();
        let name = op.get("name").and_then(Value::as_str).map(str::to_string);
        match media::import_url(db, bucket, user, &url, name).await {
            Ok(asset) if op.get("add").and_then(Value::as_bool) != Some(false) => {
                let mut add = json!({ "op": "add_clip", "asset": asset.id });
                for k in ["start", "dur", "track"] {
                    if let Some(v) = op.get(k) {
                        add[k] = v.clone();
                    }
                }
                ready.push(add);
            }
            Ok(_) => {}
            Err(e) => failed.push(format!("op {i} import_url: {e}")),
        }
    }
    let assets = media::infos(db, user).await?;
    let mut outcome = apply(&mut project, &ready, &assets);
    outcome.errors.extend(failed);
    if !outcome.done.is_empty() {
        save(db, user, pid, &project).await?;
    }
    Ok(Some((project, outcome, media::list(db, user).await?)))
}
