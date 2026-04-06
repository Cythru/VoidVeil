// SPDX-License-Identifier: AGPL-3.0-or-later
// VoidVeil — proxy.rs
// Forward anonymised requests. Strip telemetry. Handle streaming SSE.
// Re-hydrate all responses before they reach the caller.
// Provider sees tokens only. Always.

use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use crate::token_map::TokenMap;
use crate::anonymise::{anonymise, anonymise_messages};

// ── Telemetry headers to strip before forwarding ─────────────────────────────
// These fingerprint the SDK, client version, and runtime — metadata leaks.
const STRIP_HEADERS: &[&str] = &[
    "x-stainless-lang",
    "x-stainless-package-version",
    "x-stainless-os",
    "x-stainless-arch",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
    "x-stainless-async",
    "anthropic-client-name",
    "anthropic-client-sha1",
    "anthropic-client-version",
    "x-forwarded-for",
    "x-real-ip",
    "x-request-id",
    "cf-connecting-ip",
    "cf-ray",
];

// Generic UA — indistinguishable from any browser
const GENERIC_UA: &str = "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 Chrome/124.0";

#[derive(Clone)]
pub struct Proxy {
    client: Client,
    upstream: String,
}

impl Proxy {
    pub fn new(upstream: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .use_rustls_tls()
                .user_agent(GENERIC_UA)
                .build()
                .expect("reqwest client"),
            upstream: upstream.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        }
    }

    /// Non-streaming: anonymise → forward → rehydrate.
    pub async fn forward(
        &self,
        path: &str,
        mut body: Value,
        headers: HashMap<String, String>,
        map: &TokenMap,
    ) -> Result<Value, String> {
        // Anonymise content
        if let Some(msgs) = body.get_mut("messages") {
            if let Value::Array(msgs) = msgs {
                anonymise_messages(msgs, map);
            }
        }
        if let Some(system) = body.get_mut("system") {
            if let Value::String(s) = system {
                *s = anonymise(s, map);
            }
        }

        let url = format!("{}{}", self.upstream, path);
        let mut req = self.client.post(&url).json(&body);
        req = apply_headers(req, &headers);

        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let mut resp_body: Value = resp.json().await.map_err(|e| e.to_string())?;

        if !status.is_success() {
            return Err(format!("upstream {}: {}", status, resp_body));
        }

        rehydrate_value(&mut resp_body, map);
        Ok(resp_body)
    }

    /// Streaming: anonymise request, stream SSE chunks, rehydrate each delta.
    /// Returns collected full text after stream completes.
    pub async fn forward_stream(
        &self,
        path: &str,
        mut body: Value,
        headers: HashMap<String, String>,
        map: &TokenMap,
    ) -> Result<Vec<String>, String> {
        // Anonymise
        if let Some(msgs) = body.get_mut("messages") {
            if let Value::Array(msgs) = msgs {
                anonymise_messages(msgs, map);
            }
        }
        if let Some(system) = body.get_mut("system") {
            if let Value::String(s) = system {
                *s = anonymise(s, map);
            }
        }

        let url = format!("{}{}", self.upstream, path);
        let mut req = self.client.post(&url).json(&body);
        req = apply_headers(req, &headers);

        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("upstream {}", resp.status()));
        }

        // Parse SSE stream — each chunk: "data: {...}\n\n"
        let text = resp.text().await.map_err(|e| e.to_string())?;
        let mut chunks: Vec<String> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if !line.starts_with("data: ") { continue; }
            let data = &line["data: ".len()..];
            if data == "[DONE]" { break; }

            if let Ok(mut chunk) = serde_json::from_str::<Value>(data) {
                // Rehydrate delta content
                rehydrate_value(&mut chunk, map);
                chunks.push(chunk.to_string());
            }
        }

        Ok(chunks)
    }
}

fn apply_headers(
    mut req: reqwest::RequestBuilder,
    headers: &HashMap<String, String>,
) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        let k_lower = k.to_lowercase();
        // Skip telemetry — strip completely
        if STRIP_HEADERS.iter().any(|s| *s == k_lower) {
            continue;
        }
        // Replace user-agent with generic
        if k_lower == "user-agent" {
            req = req.header("user-agent", GENERIC_UA);
            continue;
        }
        req = req.header(k.as_str(), v.as_str());
    }
    req
}

fn rehydrate_value(val: &mut Value, map: &TokenMap) {
    match val {
        Value::String(s) => *s = map.rehydrate_text(s),
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                rehydrate_value(item, map);
            }
        }
        Value::Object(obj) => {
            for key in &["content", "text", "delta", "value"] {
                if let Some(v) = obj.get_mut(*key) {
                    rehydrate_value(v, map);
                }
            }
        }
        _ => {}
    }
}
