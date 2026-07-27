//! Application state, scan jobs and HTTP routing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tiny_http::{Header, Request, Response};

use crate::catalog;
use crate::i18n;
use crate::report::{self, ProjectIndex, ProjectReport};
use crate::scanner;

const JOB_TTL_SECONDS: u64 = 3600;

// =====================================================================
// State
// =====================================================================

pub struct Job {
    pub id: String,
    pub workspace: PathBuf,
    pub status: String,
    pub total: usize,
    pub done: usize,
    pub current: String,
    pub started: Instant,
    pub elapsed: f64,
    pub projects: Vec<ProjectReport>,
    pub index: HashMap<usize, ProjectIndex>,
    pub error: String,
    pub cancel: Arc<AtomicBool>,
}

impl Job {
    fn status_json(&self, include_projects: bool) -> Value {
        let elapsed = if self.elapsed > 0.0 {
            self.elapsed
        } else {
            self.started.elapsed().as_secs_f64()
        };

        let mut payload = json!({
            "id": self.id,
            "status": self.status,
            "workspace": self.workspace.to_string_lossy(),
            "total": self.total,
            "done": self.done,
            "current": self.current,
            "elapsed": (elapsed * 10.0).round() / 10.0,
            "error": self.error,
        });

        if include_projects {
            payload["projects"] = Value::Array(
                self.projects
                    .iter()
                    .map(|project| project.to_json())
                    .collect(),
            );
        }
        payload
    }

    fn finished(&self) -> bool {
        matches!(self.status.as_str(), "done" | "cancelled" | "error")
    }

    fn project_mut(&mut self, id: usize) -> Option<&mut ProjectReport> {
        self.projects.iter_mut().find(|project| project.id == id)
    }
}

pub struct AppState {
    jobs: Mutex<HashMap<String, Arc<Mutex<Job>>>>,
    counter: AtomicU64,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            jobs: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 + d.as_secs())
            .unwrap_or(0);
        let seq = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{nanos:x}{seq:x}")
    }

    fn get(&self, id: &str) -> Option<Arc<Mutex<Job>>> {
        self.jobs.lock().ok()?.get(id).cloned()
    }

    fn purge(&self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.retain(|_, job| match job.lock() {
                Ok(job) => !(job.finished() && job.started.elapsed().as_secs() > JOB_TTL_SECONDS),
                Err(_) => false,
            });
        }
    }
}

// =====================================================================
// Scan
// =====================================================================

fn start_scan(state: &Arc<AppState>, workspace: PathBuf, depth: usize) -> Value {
    state.purge();

    let cancel = Arc::new(AtomicBool::new(false));
    let job = Arc::new(Mutex::new(Job {
        id: state.next_id(),
        workspace: workspace.clone(),
        status: "discovering".to_string(),
        total: 0,
        done: 0,
        current: String::new(),
        started: Instant::now(),
        elapsed: 0.0,
        projects: Vec::new(),
        index: HashMap::new(),
        error: String::new(),
        cancel: cancel.clone(),
    }));

    let payload = job
        .lock()
        .map(|job| job.status_json(false))
        .unwrap_or(Value::Null);
    let id = payload["id"].as_str().unwrap_or_default().to_string();

    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.insert(id, job.clone());
    }

    std::thread::spawn(move || run_scan(job, workspace, depth, cancel));
    payload
}

fn run_scan(job: Arc<Mutex<Job>>, workspace: PathBuf, depth: usize, cancel: Arc<AtomicBool>) {
    let projects = scanner::discover_projects(&workspace, depth);

    {
        let mut guard = match job.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        guard.total = projects.len();
        guard.status = "scanning".to_string();
    }

    for (position, path) in projects.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            finish(&job, "cancelled");
            return;
        }

        if let Ok(mut guard) = job.lock() {
            guard.current = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
        }

        // The scan itself runs outside the lock: it takes time, and the
        // interface has to keep reading progress meanwhile.
        let scan = match scanner::scan_project(path, &cancel) {
            Some(scan) => scan,
            None => {
                finish(&job, "cancelled");
                return;
            }
        };

        let is_git = scanner::is_git_repo(path);
        let mut scan = scan;
        if is_git {
            scanner::annotate_git(path, &mut scan.buckets);
        }

        let (project, index) = report::build(path, is_git, scan, position);

        if let Ok(mut guard) = job.lock() {
            guard.projects.push(project);
            guard.index.insert(position, index);
            guard.done = position + 1;
        }
    }

    if let Ok(mut guard) = job.lock() {
        guard.current = String::new();
    }
    finish(&job, "done");
}

fn finish(job: &Arc<Mutex<Job>>, status: &str) {
    if let Ok(mut guard) = job.lock() {
        guard.status = status.to_string();
        guard.elapsed = guard.started.elapsed().as_secs_f64();
    }
}

// =====================================================================
// Selections
// =====================================================================

struct Target {
    project: usize,
    rule_id: String,
    rel: String,
    path: PathBuf,
}

/// Turns the client selection into verified absolute paths.
///
/// Only paths produced by the current scan are accepted, so a forged selection
/// cannot reach an arbitrary file on disk.
fn resolve_selection(job: &Job, selections: &Value) -> Vec<Target> {
    let mut targets = Vec::new();
    let entries = match selections.as_array() {
        Some(entries) => entries,
        None => return targets,
    };

    for entry in entries {
        let project_id = match entry.get("project").and_then(|v| v.as_u64()) {
            Some(id) => id as usize,
            None => continue,
        };
        let rule_id = match entry.get("rule_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let known = match job
            .index
            .get(&project_id)
            .and_then(|index| index.get(&rule_id))
        {
            Some(known) => known,
            None => continue,
        };
        let project = match job.projects.iter().find(|p| p.id == project_id) {
            Some(project) => project,
            None => continue,
        };

        let wanted: Option<Vec<String>> = entry.get("rels").and_then(|value| {
            value.as_array().map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
        });

        let rels: Vec<String> = match wanted {
            Some(list) => list
                .into_iter()
                .filter(|rel| known.contains_key(rel))
                .collect(),
            None => known.keys().cloned().collect(),
        };

        for rel in rels {
            if let Some(path) = known.get(&rel) {
                if is_safe_target(path, &job.workspace, &project.path) {
                    targets.push(Target {
                        project: project_id,
                        rule_id: rule_id.clone(),
                        rel,
                        path: path.clone(),
                    });
                }
            }
        }
    }
    targets
}

fn is_safe_target(target: &Path, workspace: &Path, project: &Path) -> bool {
    let normalize = |path: &Path| path.to_string_lossy().to_lowercase().replace('/', "\\");
    let target_text = normalize(target);
    let workspace_text = normalize(workspace);
    let project_text = normalize(project);

    if !target_text.starts_with(&format!("{workspace_text}\\")) {
        return false;
    }
    if target_text == workspace_text || target_text == project_text {
        return false;
    }
    target_text.split('\\').all(|part| part != ".git")
}

fn keep_patterns(body: &Value) -> Vec<String> {
    body.get("keep")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// =====================================================================
// Handlers
// =====================================================================

fn handle_scan(state: &Arc<AppState>, body: &Value) -> (u16, Value) {
    let lang = i18n::resolve(body.get("lang").and_then(|v| v.as_str()));
    let raw = body
        .get("workspace")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    if raw.is_empty() {
        return (
            400,
            json!({ "error": i18n::t(lang, "error.missing_workspace") }),
        );
    }

    let workspace = scanner::normalize_path(raw);
    if !workspace.is_dir() {
        return (
            400,
            json!({ "error": i18n::t(lang, "error.not_a_directory") }),
        );
    }

    let depth = body
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(2)
        .clamp(1, 6) as usize;

    (200, start_scan(state, workspace, depth))
}

fn handle_clean(state: &Arc<AppState>, body: &Value) -> (u16, Value) {
    let lang = i18n::resolve(body.get("lang").and_then(|v| v.as_str()));
    let job_handle = match body
        .get("job")
        .and_then(|v| v.as_str())
        .and_then(|id| state.get(id))
    {
        Some(handle) => handle,
        None => {
            return (
                404,
                json!({ "error": i18n::t(lang, "error.job_not_found") }),
            )
        }
    };

    let keep = keep_patterns(body);
    let empty = Value::Array(Vec::new());
    let selections = body.get("selections").unwrap_or(&empty);

    let targets = {
        let job = match job_handle.lock() {
            Ok(job) => job,
            Err(_) => {
                return (
                    500,
                    json!({ "error": i18n::t(lang, "error.invalid_selection") }),
                )
            }
        };
        resolve_selection(&job, selections)
    };

    if targets.is_empty() {
        return (
            400,
            json!({ "error": i18n::t(lang, "error.invalid_selection") }),
        );
    }

    // Deletion runs outside the lock: it can span several gigabytes.
    let mut removed = Vec::new();
    let mut failures = Vec::new();
    let mut freed = 0u64;
    let mut kept_size = 0u64;
    let mut kept_files = 0u64;

    for target in &targets {
        let outcome = scanner::delete_path(&target.path, &keep);
        freed += outcome.freed;
        kept_size += outcome.kept_size;
        kept_files += outcome.kept_files;

        if outcome.ok {
            removed.push((
                target.project,
                target.rule_id.clone(),
                target.rel.clone(),
                outcome.freed,
            ));
        } else {
            failures.push(json!({
                "project": target.project,
                "rel": target.rel,
                "error": outcome.error,
            }));
        }
    }

    // An occurrence that kept files is still on disk: it must stay in the
    // report, otherwise it could never be reviewed or emptied again.
    let mut fully_removed: Vec<(usize, String, String, u64)> = Vec::new();
    for entry in &removed {
        let still_there = targets
            .iter()
            .find(|t| t.project == entry.0 && t.rule_id == entry.1 && t.rel == entry.2)
            .map(|t| t.path.exists())
            .unwrap_or(false);
        if !still_there {
            fully_removed.push(entry.clone());
        }
    }

    if let Ok(mut job) = job_handle.lock() {
        let mut by_project: HashMap<usize, Vec<(String, String, u64)>> = HashMap::new();
        for (project, rule_id, rel, size) in &fully_removed {
            by_project
                .entry(*project)
                .or_default()
                .push((rule_id.clone(), rel.clone(), *size));
        }
        for (project_id, entries) in by_project {
            if let Some(index) = job.index.get_mut(&project_id) {
                for (rule_id, rel, _) in &entries {
                    if let Some(known) = index.get_mut(rule_id) {
                        known.remove(rel);
                    }
                }
            }
            if let Some(project) = job.project_mut(project_id) {
                project.apply_removals(&entries);
            }
        }
    }

    (
        200,
        json!({
            "ok": removed.len(),
            "failed": failures.len(),
            "freed": freed,
            "kept_size": kept_size,
            "kept_files": kept_files,
            "failures": failures,
        }),
    )
}

fn handle_gitignore(state: &Arc<AppState>, body: &Value) -> (u16, Value) {
    let lang = i18n::resolve(body.get("lang").and_then(|v| v.as_str()));
    let job_handle = match body
        .get("job")
        .and_then(|v| v.as_str())
        .and_then(|id| state.get(id))
    {
        Some(handle) => handle,
        None => {
            return (
                404,
                json!({ "error": i18n::t(lang, "error.job_not_found") }),
            )
        }
    };

    let mut wanted: HashMap<usize, Vec<String>> = HashMap::new();
    if let Some(entries) = body.get("selections").and_then(|v| v.as_array()) {
        for entry in entries {
            let project_id = match entry.get("project").and_then(|v| v.as_u64()) {
                Some(id) => id as usize,
                None => continue,
            };
            let rule = entry
                .get("rule_id")
                .and_then(|v| v.as_str())
                .and_then(catalog::rule_by_id);
            if let Some(rule) = rule {
                let patterns = wanted.entry(project_id).or_default();
                if !patterns.iter().any(|p| p == rule.ignore) {
                    patterns.push(rule.ignore.to_string());
                }
            }
        }
    }

    let mut job = match job_handle.lock() {
        Ok(job) => job,
        Err(_) => {
            return (
                500,
                json!({ "error": i18n::t(lang, "error.invalid_selection") }),
            )
        }
    };

    let mut added_total = 0usize;
    let mut touched = 0usize;
    let mut skipped = 0usize;
    let mut updates = Vec::new();

    let project_ids: Vec<usize> = wanted.keys().copied().collect();
    for project_id in project_ids {
        let mut patterns = wanted.remove(&project_id).unwrap_or_default();
        patterns.sort();

        let (path, is_git) = match job.projects.iter().find(|p| p.id == project_id) {
            Some(project) => (project.path.clone(), project.is_git),
            None => continue,
        };
        if !is_git {
            skipped += 1;
            continue;
        }

        let added = scanner::append_gitignore(&path, &patterns);
        if !added.is_empty() {
            added_total += added.len();
            touched += 1;
        }

        let statuses = refresh_ignore_status(&mut job, project_id, &path);
        updates.push(json!({ "project": project_id, "added": added, "statuses": statuses }));
    }

    (
        200,
        json!({
            "added": added_total,
            "repos": touched,
            "skipped": skipped,
            "updates": updates,
        }),
    )
}

/// Recomputes a project's .gitignore coverage after writing to the file.
fn refresh_ignore_status(job: &mut Job, project_id: usize, path: &Path) -> Vec<Value> {
    let rel_paths: Vec<String> = job
        .index
        .get(&project_id)
        .map(|index| {
            index
                .values()
                .flat_map(|known| known.keys().cloned())
                .collect()
        })
        .unwrap_or_default();

    if rel_paths.is_empty() {
        return Vec::new();
    }

    let ignored = scanner::git_ignored_set(path, &rel_paths);
    let known_by_rule: HashMap<String, Vec<String>> = job
        .index
        .get(&project_id)
        .map(|index| {
            index
                .iter()
                .map(|(rule, known)| (rule.clone(), known.keys().cloned().collect()))
                .collect()
        })
        .unwrap_or_default();

    let mut statuses = Vec::new();
    if let Some(project) = job.project_mut(project_id) {
        for item in project.items.iter_mut() {
            let rels = known_by_rule
                .get(&item.rule_id)
                .cloned()
                .unwrap_or_default();
            let hits = rels.iter().filter(|rel| ignored.contains(*rel)).count();
            let status = scanner::ignore_status(hits, item.tracked_count, rels.len());
            item.ignore_status = status.to_string();

            for occurrence in item.occurrences.iter_mut() {
                occurrence.ignored = ignored.contains(&occurrence.rel);
            }
            statuses.push(json!({ "rule_id": item.rule_id, "ignore_status": status }));
        }
    }
    statuses
}

fn handle_gitgc(state: &Arc<AppState>, body: &Value) -> (u16, Value) {
    let lang = i18n::resolve(body.get("lang").and_then(|v| v.as_str()));
    let job_handle = match body
        .get("job")
        .and_then(|v| v.as_str())
        .and_then(|id| state.get(id))
    {
        Some(handle) => handle,
        None => {
            return (
                404,
                json!({ "error": i18n::t(lang, "error.job_not_found") }),
            )
        }
    };

    let project_id = match body.get("project").and_then(|v| v.as_u64()) {
        Some(id) => id as usize,
        None => {
            return (
                400,
                json!({ "error": i18n::t(lang, "error.invalid_selection") }),
            )
        }
    };

    let (path, is_git) = {
        let job = match job_handle.lock() {
            Ok(job) => job,
            Err(_) => {
                return (
                    500,
                    json!({ "error": i18n::t(lang, "error.invalid_selection") }),
                )
            }
        };
        match job.projects.iter().find(|p| p.id == project_id) {
            Some(project) => (project.path.clone(), project.is_git),
            None => {
                return (
                    400,
                    json!({ "error": i18n::t(lang, "error.invalid_selection") }),
                )
            }
        }
    };

    if !is_git {
        return (400, json!({ "error": i18n::t(lang, "error.not_git") }));
    }

    let outcome = scanner::git_gc(&path);

    if outcome.ok {
        if let Ok(mut job) = job_handle.lock() {
            if let Some(project) = job.project_mut(project_id) {
                project.git_size = outcome.after;
                project.total_size = project.total_size.saturating_sub(outcome.freed);
            }
        }
    }

    (
        200,
        json!({
            "ok": outcome.ok,
            "before": outcome.before,
            "after": outcome.after,
            "freed": outcome.freed,
            "error": outcome.error,
        }),
    )
}

// =====================================================================
// Embedded assets
// =====================================================================

const INDEX_HTML: &str = include_str!("../assets/index.html");
const STYLE_CSS: &str = include_str!("../assets/style.css");
const APP_JS: &str = include_str!("../assets/app.js");
const ICON_32: &[u8] = include_bytes!("../assets/icon-32.png");
const ICON_180: &[u8] = include_bytes!("../assets/icon-180.png");

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn respond(request: Request, status: u16, body: Vec<u8>, content_type: &str) {
    let response = Response::from_data(body)
        .with_status_code(status)
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"));
    let _ = request.respond(response);
}

fn respond_json(request: Request, status: u16, value: &Value) {
    respond(
        request,
        status,
        value.to_string().into_bytes(),
        "application/json; charset=utf-8",
    );
}

// =====================================================================
// Routing
// =====================================================================

pub fn handle_request(state: Arc<AppState>, mut request: Request) {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let query = url
        .split_once('?')
        .map(|(_, q)| q.to_string())
        .unwrap_or_default();
    let is_post = request.method().as_str() == "POST";

    // The body is read first: tiny_http needs an exclusive borrow for it.
    let mut body_text = String::new();
    if is_post {
        let _ = request.as_reader().read_to_string(&mut body_text);
    }
    let body: Value = serde_json::from_str(&body_text).unwrap_or(Value::Null);

    match (is_post, path.as_str()) {
        (false, "/") => respond(
            request,
            200,
            INDEX_HTML.as_bytes().to_vec(),
            "text/html; charset=utf-8",
        ),
        (false, "/static/style.css") => respond(
            request,
            200,
            STYLE_CSS.as_bytes().to_vec(),
            "text/css; charset=utf-8",
        ),
        (false, "/static/app.js") => respond(
            request,
            200,
            APP_JS.as_bytes().to_vec(),
            "text/javascript; charset=utf-8",
        ),
        (false, "/favicon.ico") | (false, "/static/icon-32.png") => {
            respond(request, 200, ICON_32.to_vec(), "image/png")
        }
        (false, "/static/icon-180.png") => respond(request, 200, ICON_180.to_vec(), "image/png"),

        (false, "/api/languages") => {
            let languages: Vec<Value> = i18n::languages()
                .into_iter()
                .map(|(code, label)| json!({ "code": code, "label": label }))
                .collect();
            respond_json(
                request,
                200,
                &json!({
                    "languages": languages,
                    "default": i18n::DEFAULT_LANG,
                }),
            );
        }

        (false, "/api/pick-folder") => {
            let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.FolderBrowserDialog
$f.Description = "Select a workspace directory"
if ($f.ShowDialog() -eq "OK") { Write-Output $f.SelectedPath }
"#;
            let path = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", script])
                .output()
                .ok()
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                });
            respond_json(request, 200, &json!({ "path": path }));
        }

        (false, _) if path.starts_with("/api/i18n/") => {
            let lang = i18n::resolve(path.strip_prefix("/api/i18n/"));
            respond_json(request, 200, &i18n::catalog(lang));
        }

        (true, "/api/scan") => {
            let (status, payload) = handle_scan(&state, &body);
            respond_json(request, status, &payload);
        }

        (true, _) if path.starts_with("/api/scan/") && path.ends_with("/cancel") => {
            let id = path
                .trim_start_matches("/api/scan/")
                .trim_end_matches("/cancel");
            match state.get(id) {
                Some(job) => {
                    if let Ok(job) = job.lock() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    respond_json(request, 200, &json!({ "ok": true }));
                }
                None => {
                    let lang = i18n::resolve(query_lang(&query));
                    respond_json(
                        request,
                        404,
                        &json!({
                            "error": i18n::t(lang, "error.job_not_found")
                        }),
                    );
                }
            }
        }

        (false, _) if path.starts_with("/api/scan/") => {
            let id = path.trim_start_matches("/api/scan/");
            let lang = i18n::resolve(query_lang(&query));
            match state.get(id) {
                Some(job) => match job.lock() {
                    Ok(job) => {
                        let payload = job.status_json(job.finished());
                        respond_json(request, 200, &payload);
                    }
                    Err(_) => respond_json(request, 500, &json!({ "error": "lock" })),
                },
                None => respond_json(
                    request,
                    404,
                    &json!({
                        "error": i18n::t(lang, "error.job_not_found")
                    }),
                ),
            }
        }

        (true, "/api/clean") => {
            let (status, payload) = handle_clean(&state, &body);
            respond_json(request, status, &payload);
        }
        (true, "/api/gitignore") => {
            let (status, payload) = handle_gitignore(&state, &body);
            respond_json(request, status, &payload);
        }
        (true, "/api/gitgc") => {
            let (status, payload) = handle_gitgc(&state, &body);
            respond_json(request, status, &payload);
        }

        _ => respond_json(request, 404, &json!({ "error": "not found" })),
    }
}

fn query_lang(query: &str) -> Option<&str> {
    query.split('&').find_map(|pair| pair.strip_prefix("lang="))
}
