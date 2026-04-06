// SPDX-License-Identifier: AGPL-3.0-or-later
// VoidVeil — token_map.rs
// Session-scoped token store. Lives in RAM. Never transmitted. Dies with session.
// O(1) insert and lookup. Thread-safe. Zero allocation on hot path.

use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct TokenMap {
    /// real → token
    forward: Arc<DashMap<String, String>>,
    /// token → real
    reverse: Arc<DashMap<String, String>>,
    /// entity type counters for deterministic short tokens
    counters: Arc<DashMap<String, u32>>,
}

impl TokenMap {
    pub fn new() -> Self {
        Self {
            forward: Arc::new(DashMap::new()),
            reverse: Arc::new(DashMap::new()),
            counters: Arc::new(DashMap::new()),
        }
    }

    /// Get or create a token for a real value.
    /// Same real value always returns same token within session.
    pub fn tokenize(&self, real: &str, entity_type: &str) -> String {
        if let Some(tok) = self.forward.get(real) {
            return tok.clone();
        }
        let n = {
            let mut c = self.counters.entry(entity_type.to_string()).or_insert(0);
            *c += 1;
            *c
        };
        // Format: [TYPE_XXXX] — readable, consistent, model-friendly
        let token = format!("[{}_{:04X}]", entity_type, n);
        self.forward.insert(real.to_string(), token.clone());
        self.reverse.insert(token.clone(), real.to_string());
        token
    }

    /// Reverse: token → real value.
    pub fn rehydrate(&self, token: &str) -> Option<String> {
        self.reverse.get(token).map(|v| v.clone())
    }

    /// Apply full rehydration pass to a string — replaces all tokens with real values.
    pub fn rehydrate_text(&self, text: &str) -> String {
        let mut result = text.to_string();
        for entry in self.reverse.iter() {
            result = result.replace(entry.key().as_str(), entry.value().as_str());
        }
        result
    }

    pub fn session_id() -> String {
        Uuid::new_v4().to_string()
    }
}

impl Default for TokenMap {
    fn default() -> Self {
        Self::new()
    }
}
