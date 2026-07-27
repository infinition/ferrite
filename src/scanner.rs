//! Workspace traversal, size measurement, git queries and selective deletion.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::catalog::{self, Rule};

pub const MAX_OCCURRENCES_SENT: usize = 150;
const GIT_BATCH: usize = 1500;

// =====================================================================
// Paths
// =====================================================================

pub fn normalize_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    let expanded = if let Some(rest) = trimmed.strip_prefix('~') {
        match std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            Ok(home) => format!("{home}{rest}"),
            Err(_) => trimmed.to_string(),
        }
    } else {
        trimmed.to_string()
    };

    let path = PathBuf::from(expanded);
    fs::canonicalize(&path).map(strip_verbatim).unwrap_or(path)
}

/// Strips the `\\?\` prefix returned by `canonicalize` on Windows: it reads
/// badly in the interface and some tools reject it.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

/// Windows prefix that lifts the historical 260 character path limit.
#[cfg(windows)]
fn long_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy().to_string();
    if text.starts_with(r"\\?\") {
        return path.to_path_buf();
    }
    if let Some(share) = text.strip_prefix(r"\\") {
        return PathBuf::from(format!(r"\\?\UNC\{share}"));
    }
    PathBuf::from(format!(r"\\?\{text}"))
}

#[cfg(not(windows))]
fn long_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Total size and file count of a directory tree.
pub fn dir_stats(root: &Path) -> (u64, u64) {
    let mut total = 0u64;
    let mut count = 0u64;
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
                count += 1;
            }
        }
    }
    (total, count)
}

// =====================================================================
// Git
// =====================================================================

fn git_command(repo: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(repo);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Keeps a console window from flashing on every call.
        command.creation_flags(0x0800_0000);
    }
    command
}

pub fn run_git(repo: &Path, args: &[&str], stdin_data: Option<Vec<u8>>) -> (bool, Vec<u8>, String) {
    let mut command = git_command(repo);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.stdin(if stdin_data.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return (false, Vec::new(), error.to_string()),
    };

    if let Some(payload) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            // Write from a thread: on a large batch git can fill its output
            // buffer before it has read everything, which would deadlock.
            std::thread::spawn(move || {
                let _ = stdin.write_all(&payload);
            });
        }
    }

    match child.wait_with_output() {
        Ok(output) => (
            output.status.success(),
            output.stdout,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ),
        Err(error) => (false, Vec::new(), error.to_string()),
    }
}

pub fn is_git_repo(path: &Path) -> bool {
    let marker = path.join(".git");
    marker.is_dir() || marker.is_file()
}

pub fn git_branch(repo: &Path) -> String {
    let (ok, out, _) = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"], None);
    if ok {
        String::from_utf8_lossy(&out).trim().to_string()
    } else {
        String::new()
    }
}

/// Subset of the given paths covered by a .gitignore.
pub fn git_ignored_set(repo: &Path, rel_paths: &[String]) -> HashSet<String> {
    let mut ignored = HashSet::new();

    for chunk in rel_paths.chunks(GIT_BATCH) {
        let mut payload = Vec::new();
        for rel in chunk {
            payload.extend_from_slice(rel.as_bytes());
            payload.push(0);
        }
        let (_, out, _) = run_git(repo, &["check-ignore", "-z", "--stdin"], Some(payload));
        for raw in out.split(|byte| *byte == 0) {
            if !raw.is_empty() {
                ignored.insert(String::from_utf8_lossy(raw).to_string());
            }
        }
    }
    ignored
}

/// Paths holding at least one file tracked by git.
pub fn git_tracked_set(repo: &Path, rel_paths: &[String]) -> HashSet<String> {
    let mut tracked = HashSet::new();

    for chunk in rel_paths.chunks(GIT_BATCH) {
        let mut args: Vec<&str> = vec!["ls-files", "-z", "--"];
        args.extend(chunk.iter().map(|rel| rel.as_str()));

        let (ok, out, _) = run_git(repo, &args, None);
        if !ok {
            continue;
        }
        let found: Vec<String> = out
            .split(|byte| *byte == 0)
            .filter(|raw| !raw.is_empty())
            .map(|raw| String::from_utf8_lossy(raw).to_string())
            .collect();
        if found.is_empty() {
            continue;
        }
        for candidate in chunk {
            let prefix = format!("{}/", candidate.trim_end_matches('/'));
            if found
                .iter()
                .any(|f| f == candidate || f.starts_with(&prefix))
            {
                tracked.insert(candidate.clone());
            }
        }
    }
    tracked
}

pub struct GcOutcome {
    pub ok: bool,
    pub before: u64,
    pub after: u64,
    pub freed: u64,
    pub error: String,
}

pub fn git_gc(repo: &Path) -> GcOutcome {
    let (before, _) = dir_stats(&repo.join(".git"));
    let (ok, _, error) = run_git(repo, &["gc", "--prune=now", "--quiet"], None);
    let (after, _) = dir_stats(&repo.join(".git"));

    GcOutcome {
        ok,
        before,
        after,
        freed: before.saturating_sub(after),
        error: if ok { String::new() } else { error },
    }
}

// =====================================================================
// Project discovery
// =====================================================================

fn looks_like_project(names: &HashSet<String>) -> bool {
    catalog::PROJECT_MARKERS.iter().any(|marker| {
        if marker.contains('*') {
            names
                .iter()
                .any(|name| catalog::wildcard_match(marker, name))
        } else {
            names.contains(*marker)
        }
    })
}

fn child_names(path: &Path) -> Option<HashSet<String>> {
    let entries = fs::read_dir(path).ok()?;
    Some(
        entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
    )
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "$RECYCLE.BIN",
    "System Volume Information",
];

/// Directories to analyse. A directory recognised as a project is not searched
/// any deeper for nested projects.
pub fn discover_projects(workspace: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut projects = Vec::new();
    let mut queue = vec![(workspace.to_path_buf(), 0usize)];

    while let Some((current, depth)) = queue.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }

            let names = match child_names(&entry.path()) {
                Some(names) => names,
                None => continue,
            };

            if looks_like_project(&names) {
                projects.push(entry.path());
            } else if depth + 1 < max_depth {
                queue.push((entry.path(), depth + 1));
            }
        }
    }

    projects.sort();
    projects
}

// =====================================================================
// Project scan
// =====================================================================

pub struct Occurrence {
    pub path: PathBuf,
    pub rel: String,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
    pub ignored: bool,
    pub tracked: bool,
}

pub struct Bucket {
    pub rule: &'static Rule,
    pub size: u64,
    pub files: u64,
    pub occurrences: Vec<Occurrence>,
    pub ignore_status: String,
    pub tracked_count: usize,
}

pub struct ProjectScan {
    pub buckets: Vec<Bucket>,
    pub total_size: u64,
    pub total_files: u64,
    pub git_size: u64,
}

pub fn scan_project(root: &Path, cancel: &AtomicBool) -> Option<ProjectScan> {
    let mut index: HashMap<&'static str, usize> = HashMap::new();
    let mut buckets: Vec<Bucket> = Vec::new();
    let mut total_size = 0u64;
    let mut total_files = 0u64;
    let mut git_size = 0u64;

    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut stack = vec![(root.to_path_buf(), root_name)];

    while let Some((current, current_name)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }

        let entries: Vec<fs::DirEntry> = match fs::read_dir(&current) {
            Ok(entries) => entries.flatten().collect(),
            Err(_) => continue,
        };

        let siblings: HashSet<String> = entries
            .iter()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();

        for entry in &entries {
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_symlink() {
                continue;
            }
            let is_dir = file_type.is_dir();
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();

            if is_dir && name == ".git" {
                let (size, count) = dir_stats(&path);
                total_size += size;
                total_files += count;
                if current == root {
                    git_size = size;
                }
                continue;
            }

            match catalog::find_rule(&name, is_dir, &current_name, &siblings) {
                Some(rule) => {
                    let (size, count) = if is_dir {
                        dir_stats(&path)
                    } else {
                        (entry.metadata().map(|m| m.len()).unwrap_or(0), 1)
                    };
                    total_size += size;
                    total_files += count;

                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");

                    let position = *index.entry(rule.id).or_insert_with(|| {
                        buckets.push(Bucket {
                            rule,
                            size: 0,
                            files: 0,
                            occurrences: Vec::new(),
                            ignore_status: "na".to_string(),
                            tracked_count: 0,
                        });
                        buckets.len() - 1
                    });

                    let bucket = &mut buckets[position];
                    bucket.size += size;
                    bucket.files += count;
                    bucket.occurrences.push(Occurrence {
                        path,
                        rel,
                        size,
                        files: count,
                        is_dir,
                        ignored: false,
                        tracked: false,
                    });

                    // A recognised artifact is not descended into: that avoids
                    // double counting and speeds the walk up considerably.
                }
                None => {
                    if is_dir {
                        stack.push((path, name));
                    } else {
                        total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
                        total_files += 1;
                    }
                }
            }
        }
    }

    Some(ProjectScan {
        buckets,
        total_size,
        total_files,
        git_size,
    })
}

/// .gitignore coverage state of an artifact.
///
/// Git never reports a tracked file as ignored: while it sits in the index, a
/// pattern added to .gitignore has no effect. That case deserves its own
/// state, otherwise adding the pattern looks like it did nothing.
pub fn ignore_status(ignored: usize, tracked: usize, total: usize) -> &'static str {
    if total == 0 {
        "na"
    } else if ignored == 0 && tracked > 0 {
        "tracked"
    } else if ignored == 0 {
        "none"
    } else if ignored == total {
        "all"
    } else {
        "partial"
    }
}

pub fn annotate_git(root: &Path, buckets: &mut [Bucket]) {
    let rel_paths: Vec<String> = buckets
        .iter()
        .flat_map(|bucket| bucket.occurrences.iter().map(|occ| occ.rel.clone()))
        .collect();
    if rel_paths.is_empty() {
        return;
    }

    let ignored = git_ignored_set(root, &rel_paths);
    let tracked = git_tracked_set(root, &rel_paths);

    for bucket in buckets.iter_mut() {
        let mut ignored_count = 0usize;
        let mut tracked_count = 0usize;

        for occurrence in bucket.occurrences.iter_mut() {
            occurrence.ignored = ignored.contains(&occurrence.rel);
            occurrence.tracked = tracked.contains(&occurrence.rel);
            ignored_count += usize::from(occurrence.ignored);
            tracked_count += usize::from(occurrence.tracked);
        }

        let total = bucket.occurrences.len();
        bucket.ignore_status = ignore_status(ignored_count, tracked_count, total).to_string();
        bucket.tracked_count = tracked_count;
    }
}

// =====================================================================
// Deletion
// =====================================================================

pub struct DeleteOutcome {
    pub ok: bool,
    pub freed: u64,
    pub kept_size: u64,
    pub kept_files: u64,
    pub error: String,
}

impl DeleteOutcome {
    fn success() -> Self {
        DeleteOutcome {
            ok: true,
            freed: 0,
            kept_size: 0,
            kept_files: 0,
            error: String::new(),
        }
    }

    fn failure(error: String) -> Self {
        DeleteOutcome {
            ok: false,
            freed: 0,
            kept_size: 0,
            kept_files: 0,
            error,
        }
    }
}

fn matches_keep(name: &str, keep: &[String]) -> bool {
    keep.iter()
        .any(|pattern| catalog::wildcard_match(pattern, name))
}

fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        let mut permissions = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        let _ = fs::set_permissions(path, permissions);
    }
}

fn remove_file(path: &Path) -> bool {
    if fs::remove_file(long_path(path)).is_ok() {
        return true;
    }
    clear_readonly(path);
    fs::remove_file(long_path(path)).is_ok()
}

fn remove_dir_all(path: &Path) -> Result<(), String> {
    if fs::remove_dir_all(long_path(path)).is_ok() {
        return Ok(());
    }
    // Second pass: some files carry the read only attribute, which
    // `remove_dir_all` does not clear by itself on Windows.
    let (freed, _, _) = selective_rmtree(path, &[]);
    let _ = freed;
    if path.exists() {
        Err("incomplete deletion".to_string())
    } else {
        Ok(())
    }
}

/// Empties a directory tree while preserving the files to keep.
///
/// The walk is bottom up: a directory is removed only after its contents, and
/// only if it ends up empty. A directory that keeps a file survives holding
/// just that file.
fn selective_rmtree(root: &Path, keep: &[String]) -> (u64, u64, u64) {
    let mut freed = 0u64;
    let mut kept_size = 0u64;
    let mut kept_files = 0u64;

    // Collect directories breadth first, then process them in reverse.
    let mut directories = vec![root.to_path_buf()];
    let mut cursor = 0usize;
    while cursor < directories.len() {
        let current = directories[cursor].clone();
        cursor += 1;
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_dir() && !file_type.is_symlink() {
                        directories.push(entry.path());
                    }
                }
            }
        }
    }

    for directory in directories.iter().rev() {
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(_) => continue,
                };
                if file_type.is_dir() {
                    continue;
                }
                let path = entry.path();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let name = entry.file_name().to_string_lossy().to_string();

                if matches_keep(&name, keep) {
                    kept_size += size;
                    kept_files += 1;
                } else if remove_file(&path) {
                    freed += size;
                }
            }
        }
        let _ = fs::remove_dir(long_path(directory));
    }

    (freed, kept_size, kept_files)
}

/// Deletes a file or a directory.
///
/// `keep` preserves files whose name matches one of the patterns, along with
/// the directories needed to reach them.
pub fn delete_path(path: &Path, keep: &[String]) -> DeleteOutcome {
    if !path.exists() {
        return DeleteOutcome::success();
    }

    let is_dir = path.is_dir();

    if !is_dir {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        if !keep.is_empty() && matches_keep(&name, keep) {
            let mut outcome = DeleteOutcome::success();
            outcome.kept_size = size;
            outcome.kept_files = 1;
            return outcome;
        }
        return if remove_file(path) {
            let mut outcome = DeleteOutcome::success();
            outcome.freed = size;
            outcome
        } else {
            DeleteOutcome::failure("deletion refused".to_string())
        };
    }

    if !keep.is_empty() {
        let (freed, kept_size, kept_files) = selective_rmtree(path, keep);
        let mut outcome = DeleteOutcome::success();
        outcome.freed = freed;
        outcome.kept_size = kept_size;
        outcome.kept_files = kept_files;
        if path.exists() && kept_files == 0 {
            outcome.ok = false;
            outcome.error = "incomplete deletion".to_string();
        }
        return outcome;
    }

    let (size, _) = dir_stats(path);
    match remove_dir_all(path) {
        Ok(()) => {
            let mut outcome = DeleteOutcome::success();
            outcome.freed = size;
            outcome
        }
        Err(error) => DeleteOutcome::failure(error),
    }
}

// =====================================================================
// .gitignore
// =====================================================================

const MANAGED_HEADER: &str = "# ferrite";

/// Appends the missing patterns under a dedicated section. Returns what was
/// actually added.
pub fn append_gitignore(repo: &Path, patterns: &[String]) -> Vec<String> {
    let target = repo.join(".gitignore");
    let existing: HashSet<String> = fs::read_to_string(&target)
        .unwrap_or_default()
        .lines()
        .map(|line| line.trim().to_string())
        .collect();

    let missing: Vec<String> = patterns
        .iter()
        .filter(|pattern| !pattern.trim().is_empty() && !existing.contains(pattern.trim()))
        .cloned()
        .collect();

    if missing.is_empty() {
        return missing;
    }

    let current = fs::read_to_string(&target).unwrap_or_default();
    let mut block = String::new();
    if !current.is_empty() && !current.ends_with('\n') {
        block.push('\n');
    }
    block.push('\n');
    block.push_str(MANAGED_HEADER);
    block.push_str(&format!(" {}\n", today()));
    for pattern in &missing {
        block.push_str(pattern);
        block.push('\n');
    }

    match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
    {
        Ok(mut file) => {
            if file.write_all(block.as_bytes()).is_err() {
                return Vec::new();
            }
        }
        Err(_) => return Vec::new(),
    }

    missing
}

/// Today's date in ISO form, computed without an external dependency.
fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;

    let days = seconds / 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since the epoch to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}
