//! Surface Groq rate-limit headroom as SteamVR notifications.
//!
//! Every Groq response carries `x-ratelimit-remaining-*` headers. Workers watch
//! them and queue a warning when a threshold is crossed. The daemon's main thread
//! (which owns the OpenVR runtime) delivers it through `IVRNotifications`, so it
//! appears in the headset / Steam UI rather than the desktop notifier. The
//! cooldown is only armed after a delivery succeeds, so a failed attempt retries.

use crate::config::Config;
use std::{
    sync::{
        atomic::{AtomicI64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use ureq::http::HeaderMap;

/// A queued warning: (summary, body).
static PENDING: Mutex<Option<(String, String)>> = Mutex::new(None);
/// Unix seconds of the last *successful* delivery.
static LAST_OK: AtomicI64 = AtomicI64::new(0);

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse().ok())
}

fn queue(c: &Config, summary: &str, body: &str) {
    let legacy = c.boolean("VOICE_NOTIFY", true).unwrap_or(true);
    if !c.boolean("GROQ_USAGE_NOTIFY", legacy).unwrap_or(legacy) {
        return;
    }
    let cooldown: i64 = c
        .get("VOICE_WARN_COOLDOWN_SEC", "3600")
        .parse()
        .unwrap_or(3600);
    let last = LAST_OK.load(Ordering::SeqCst);
    if last != 0 && cooldown > 0 && now().saturating_sub(last) < cooldown {
        return;
    }
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some((summary.to_string(), body.to_string()));
    }
}

/// Inspect a Groq response's rate-limit headers and queue a warning when headroom
/// is low. `VOICE_WARN_REQUESTS`/`VOICE_WARN_TOKENS` are absolute remaining
/// counts; 0 disables that check.
pub fn observe(c: &Config, headers: &HeaderMap) {
    let warn_requests: i64 = c.get("VOICE_WARN_REQUESTS", "50").parse().unwrap_or(50);
    let warn_tokens: i64 = c.get("VOICE_WARN_TOKENS", "500").parse().unwrap_or(500);
    let requests = header_i64(headers, "x-ratelimit-remaining-requests");
    let tokens = header_i64(headers, "x-ratelimit-remaining-tokens");
    let mut parts = Vec::new();
    if warn_requests > 0 {
        if let Some(r) = requests.filter(|r| *r <= warn_requests) {
            parts.push(format!("{r} API requests left today"));
        }
    }
    if warn_tokens > 0 {
        if let Some(t) = tokens.filter(|t| *t <= warn_tokens) {
            parts.push(format!("{t} tokens left this minute"));
        }
    }
    if !parts.is_empty() {
        queue(c, "Groq usage low", &parts.join("; "));
    }
}

/// Queue a warning that a request was rejected with HTTP 429.
pub fn rate_limited(c: &Config) {
    queue(
        c,
        "Groq rate limited",
        "A request was rejected (429). Dictation may fail until the limit resets.",
    );
}

/// Hand the pending warning to the main thread, if any.
pub fn take_pending() -> Option<(String, String)> {
    PENDING.lock().ok().and_then(|mut pending| pending.take())
}

/// Record the outcome so the cooldown only applies after real delivery.
pub fn delivered(ok: bool) {
    if ok {
        LAST_OK.store(now(), Ordering::SeqCst);
    }
}
