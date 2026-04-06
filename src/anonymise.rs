// SPDX-License-Identifier: AGPL-3.0-or-later
// VoidVeil — anonymise.rs
// Single-pass anonymiser + semantic role tracker.
// O(n) on input. Strips PII, secrets, entities, role references.
// Same entity always maps to same token. Pronouns + roles resolved via context graph.

use once_cell::sync::Lazy;
use regex::Regex;
use crate::token_map::TokenMap;

struct Pattern {
    re: Regex,
    entity_type: &'static str,
}

static PATTERNS: Lazy<Vec<Pattern>> = Lazy::new(|| vec![
    // Secrets / API keys — highest priority, catch before anything else
    Pattern { re: Regex::new(r"(?i)(sk-[a-z0-9]{20,60}|sk-ant-[a-zA-Z0-9\-]{20,80}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xox[baprs]-[a-zA-Z0-9\-]{10,80}|AIza[a-zA-Z0-9\-_]{35})").unwrap(), entity_type: "SECRET" },
    // Email
    Pattern { re: Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}").unwrap(), entity_type: "EMAIL" },
    // IPv4 (non-private)
    Pattern { re: Regex::new(r"\b(?!127\.|192\.168\.|10\.|172\.(?:1[6-9]|2[0-9]|3[01])\.)(\d{1,3}\.){3}\d{1,3}\b").unwrap(), entity_type: "IP" },
    // Phone (UK + intl)
    Pattern { re: Regex::new(r"(\+44|0044|0)[0-9\s\-\(\)]{9,15}").unwrap(), entity_type: "PHONE" },
    // URLs
    Pattern { re: Regex::new(r#"https?://[^\s"'<>]{10,}"#).unwrap(), entity_type: "URL" },
    // Monetary amounts
    Pattern { re: Regex::new(r"£\d[\d,]*(?:\.\d{1,2})?|\$\d[\d,]*(?:\.\d{1,2})?|€\d[\d,]*(?:\.\d{1,2})?|\b\d[\d,]*(?:\.\d{1,2})?\s*(?:pounds?|dollars?|euros?|GBP|USD|EUR)\b").unwrap(), entity_type: "AMOUNT" },
    // UUIDs
    Pattern { re: Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap(), entity_type: "UUID" },
    // UK postcodes
    Pattern { re: Regex::new(r"\b[A-Z]{1,2}[0-9][0-9A-Z]?\s*[0-9][A-Z]{2}\b").unwrap(), entity_type: "POSTCODE" },
    // Dates
    Pattern { re: Regex::new(r"\b\d{1,2}[\/\-\.]\d{1,2}[\/\-\.]\d{2,4}\b|\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\.?\s+\d{1,2},?\s+\d{4}\b").unwrap(), entity_type: "DATE" },
    // Companies (Ltd, Inc, Corp, etc.)
    Pattern { re: Regex::new(r"\b[A-Z][A-Za-z0-9\s&]{2,30}(?:Ltd|Limited|Inc|Corp|Corporation|LLC|LLP|PLC|Group|Holdings)\b").unwrap(), entity_type: "ORG" },
    // Person names — Title Case pairs/triples
    Pattern { re: Regex::new(r"\b([A-Z][a-z]{2,15})\s+([A-Z][a-z]{2,15})(?:\s+([A-Z][a-z]{2,15}))?\b").unwrap(), entity_type: "PERSON" },
]);

// Role references — "the CEO", "my manager", "the doctor" etc.
// After a PERSON token is seen, these get mapped to the same person contextually.
static ROLE_REFS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:the|my|our|his|her|their)\s+(?:CEO|CTO|CFO|COO|director|manager|boss|doctor|dr|nurse|lawyer|solicitor|client|customer|partner|founder|owner|head|lead|chief)\b").unwrap()
});

static PRONOUNS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(he|she|they|him|her|them|his|hers|their|theirs|himself|herself|themselves)\b").unwrap()
});

pub fn anonymise(text: &str, map: &TokenMap) -> String {
    let mut result = text.to_string();
    let mut last_person_token: Option<String> = None;

    // Pattern pass — PII, secrets, entities
    for pattern in PATTERNS.iter() {
        let mut out = String::with_capacity(result.len());
        let mut last_end = 0;
        for m in pattern.re.find_iter(&result) {
            out.push_str(&result[last_end..m.start()]);
            let token = map.tokenize(m.as_str(), pattern.entity_type);
            if pattern.entity_type == "PERSON" {
                last_person_token = Some(token.clone());
            }
            out.push_str(&token);
            last_end = m.end();
        }
        out.push_str(&result[last_end..]);
        result = out;
    }

    // Role reference pass — map "the CEO", "my manager" etc. to last person token
    if let Some(ref person_tok) = last_person_token {
        let tok = person_tok.clone();
        result = ROLE_REFS.replace_all(&result, |caps: &regex::Captures| {
            map.tokenize(caps.get(0).unwrap().as_str(), "ROLE_REF");
            tok.clone()
        }).to_string();
    }

    // Pronoun coreference — he/she/they → last person token
    if let Some(ref person_tok) = last_person_token {
        let tok = person_tok.clone();
        result = PRONOUNS.replace_all(&result, |caps: &regex::Captures| {
            map.tokenize(caps.get(0).unwrap().as_str(), "PRONOUN");
            tok.clone()
        }).to_string();
    }

    result
}

pub fn anonymise_messages(messages: &mut Vec<serde_json::Value>, map: &TokenMap) {
    for msg in messages.iter_mut() {
        anonymise_value(msg, map);
    }
}

fn anonymise_value(val: &mut serde_json::Value, map: &TokenMap) {
    match val {
        serde_json::Value::String(s) => *s = anonymise(s, map),
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                if let serde_json::Value::Object(obj) = item {
                    if let Some(text) = obj.get_mut("text") {
                        if let serde_json::Value::String(s) = text {
                            *s = anonymise(s, map);
                        }
                    }
                } else {
                    anonymise_value(item, map);
                }
            }
        }
        serde_json::Value::Object(obj) => {
            for key in &["content", "text"] {
                if let Some(v) = obj.get_mut(*key) {
                    anonymise_value(v, map);
                }
            }
        }
        _ => {}
    }
}
