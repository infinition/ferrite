//! Catalogues de traduction embarques dans le binaire.
//!
//! Les memes fichiers alimentent le backend, pour les messages d'erreur, et le
//! frontend, servi par `/api/i18n/<lang>`. Il n'y a donc qu'une source de
//! verite par langue.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

pub const DEFAULT_LANG: &str = "fr";
const FALLBACK_LANG: &str = "en";

static RAW: &[(&str, &str)] = &[
    ("fr", include_str!("../assets/locales/fr.json")),
    ("en", include_str!("../assets/locales/en.json")),
];

fn catalogs() -> &'static HashMap<&'static str, Value> {
    static CACHE: OnceLock<HashMap<&'static str, Value>> = OnceLock::new();
    CACHE.get_or_init(|| {
        RAW.iter()
            .filter_map(|(code, body)| serde_json::from_str(body).ok().map(|value| (*code, value)))
            .collect()
    })
}

pub fn resolve(lang: Option<&str>) -> &'static str {
    let requested = lang.unwrap_or(DEFAULT_LANG);
    let base = requested.split(['-', ',']).next().unwrap_or(DEFAULT_LANG).trim();
    catalogs().keys().find(|code| **code == base).copied().unwrap_or(DEFAULT_LANG)
}

pub fn catalog(lang: &str) -> Value {
    catalogs().get(lang).cloned().unwrap_or(Value::Null)
}

pub fn languages() -> Vec<(&'static str, String)> {
    let mut list: Vec<(&'static str, String)> = catalogs()
        .iter()
        .map(|(code, value)| {
            let label = value
                .get("meta")
                .and_then(|meta| meta.get("label"))
                .and_then(|label| label.as_str())
                .unwrap_or(code)
                .to_string();
            (*code, label)
        })
        .collect();
    list.sort_by(|a, b| a.0.cmp(b.0));
    list
}

/// Traduit une cle pointee, avec repli sur l'anglais puis sur la cle brute.
pub fn t(lang: &str, key: &str) -> String {
    for candidate in [lang, FALLBACK_LANG] {
        if let Some(value) = catalogs().get(candidate).and_then(|root| lookup(root, key)) {
            return value;
        }
    }
    key.to_string()
}

fn lookup(root: &Value, key: &str) -> Option<String> {
    let mut node = root;
    for part in key.split('.') {
        node = node.get(part)?;
    }
    node.as_str().map(|text| text.to_string())
}
