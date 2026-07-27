//! Shapes scan results for the client.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::catalog;
use crate::scanner::{self, ProjectScan, MAX_OCCURRENCES_SENT};

pub struct OccurrenceReport {
    pub rel: String,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
    pub ignored: bool,
    pub tracked: bool,
}

pub struct ItemReport {
    pub rule_id: String,
    pub name: String,
    pub category: String,
    pub risk: String,
    pub restore: String,
    pub ignore_pattern: String,
    pub ignore_status: String,
    pub tracked_count: usize,
    pub size: u64,
    pub files: u64,
    pub count: usize,
    pub occurrences: Vec<OccurrenceReport>,
    pub truncated: usize,
}

pub struct ProjectReport {
    pub id: usize,
    pub name: String,
    pub path: PathBuf,
    pub is_git: bool,
    pub branch: String,
    pub git_size: u64,
    pub total_size: u64,
    pub total_files: u64,
    pub reclaimable_size: u64,
    pub reclaimable_files: u64,
    pub items: Vec<ItemReport>,
}

/// A project's `rule_id -> (rel -> absolute path)` table.
///
/// It acts as the source of authority: only the paths it holds can be deleted,
/// which makes a forged selection inert.
pub type ProjectIndex = HashMap<String, HashMap<String, PathBuf>>;

pub fn build(
    root: &Path,
    is_git: bool,
    mut scan: ProjectScan,
    id: usize,
) -> (ProjectReport, ProjectIndex) {
    let mut index: ProjectIndex = HashMap::new();
    for bucket in &scan.buckets {
        let entry = index.entry(bucket.rule.id.to_string()).or_default();
        for occurrence in &bucket.occurrences {
            entry.insert(occurrence.rel.clone(), occurrence.path.clone());
        }
    }

    // Largest first, so the interface leads with what actually costs space.
    scan.buckets
        .sort_by_key(|bucket| std::cmp::Reverse(bucket.size));

    let mut items = Vec::new();
    let mut reclaimable_size = 0u64;
    let mut reclaimable_files = 0u64;

    for bucket in &mut scan.buckets {
        bucket
            .occurrences
            .sort_by_key(|occurrence| std::cmp::Reverse(occurrence.size));
        reclaimable_size += bucket.size;
        reclaimable_files += bucket.files;

        let total = bucket.occurrences.len();
        let occurrences = bucket
            .occurrences
            .iter()
            .take(MAX_OCCURRENCES_SENT)
            .map(|occurrence| OccurrenceReport {
                rel: occurrence.rel.clone(),
                size: occurrence.size,
                files: occurrence.files,
                is_dir: occurrence.is_dir,
                ignored: occurrence.ignored,
                tracked: occurrence.tracked,
            })
            .collect();

        items.push(ItemReport {
            rule_id: bucket.rule.id.to_string(),
            name: catalog::display_name(bucket.rule),
            category: bucket.rule.cat.to_string(),
            risk: bucket.rule.risk.to_string(),
            restore: bucket.rule.restore.to_string(),
            ignore_pattern: bucket.rule.ignore.to_string(),
            ignore_status: if is_git {
                bucket.ignore_status.clone()
            } else {
                "na".to_string()
            },
            tracked_count: bucket.tracked_count,
            size: bucket.size,
            files: bucket.files,
            count: total,
            occurrences,
            truncated: total.saturating_sub(MAX_OCCURRENCES_SENT),
        });
    }

    let report = ProjectReport {
        id,
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        path: root.to_path_buf(),
        is_git,
        branch: if is_git {
            scanner::git_branch(root)
        } else {
            String::new()
        },
        git_size: scan.git_size,
        total_size: scan.total_size,
        total_files: scan.total_files,
        reclaimable_size,
        reclaimable_files,
        items,
    };

    (report, index)
}

impl ItemReport {
    pub fn to_json(&self) -> Value {
        json!({
            "rule_id": self.rule_id,
            "name": self.name,
            "category": self.category,
            "risk": self.risk,
            "restore": self.restore,
            "ignore_pattern": self.ignore_pattern,
            "ignore_status": self.ignore_status,
            "tracked_count": self.tracked_count,
            "size": self.size,
            "files": self.files,
            "count": self.count,
            "truncated": self.truncated,
            "occurrences": self.occurrences.iter().map(|occurrence| json!({
                "rel": occurrence.rel,
                "size": occurrence.size,
                "files": occurrence.files,
                "is_dir": occurrence.is_dir,
                "ignored": occurrence.ignored,
                "tracked": occurrence.tracked,
            })).collect::<Vec<_>>(),
        })
    }
}

impl ProjectReport {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "path": self.path.to_string_lossy(),
            "is_git": self.is_git,
            "branch": self.branch,
            "git_size": self.git_size,
            "total_size": self.total_size,
            "total_files": self.total_files,
            "reclaimable_size": self.reclaimable_size,
            "reclaimable_files": self.reclaimable_files,
            "items": self.items.iter().map(|item| item.to_json()).collect::<Vec<_>>(),
        })
    }

    /// Drops the occurrences that were actually removed, so the client stays
    /// in sync without running a full scan again.
    pub fn apply_removals(&mut self, removed: &[(String, String, u64)]) {
        let freed: u64 = removed.iter().map(|(_, _, size)| size).sum();

        for item in self.items.iter_mut() {
            let gone: Vec<&(String, String, u64)> = removed
                .iter()
                .filter(|(rule_id, _, _)| *rule_id == item.rule_id)
                .collect();
            if gone.is_empty() {
                continue;
            }
            let gone_rels: Vec<&String> = gone.iter().map(|(_, rel, _)| rel).collect();

            let removed_here: Vec<&OccurrenceReport> = item
                .occurrences
                .iter()
                .filter(|occurrence| gone_rels.contains(&&occurrence.rel))
                .collect();

            item.size = item
                .size
                .saturating_sub(removed_here.iter().map(|o| o.size).sum::<u64>());
            item.files = item
                .files
                .saturating_sub(removed_here.iter().map(|o| o.files).sum::<u64>());
            item.count = item.count.saturating_sub(gone.len());
            item.occurrences
                .retain(|occurrence| !gone_rels.contains(&&occurrence.rel));
        }

        self.items.retain(|item| item.count > 0);
        self.reclaimable_size = self.items.iter().map(|item| item.size).sum();
        self.reclaimable_files = self.items.iter().map(|item| item.files).sum();
        self.total_size = self.total_size.saturating_sub(freed);
    }
}
