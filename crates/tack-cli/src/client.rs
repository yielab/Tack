use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use serde::Serialize;

use crate::config::Config;

pub struct TackClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl TackClient {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            token: config.token.clone(),
        })
    }

    pub fn get(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self.request(reqwest::Method::GET, path, &[]).send()?;
        extract(resp)
    }

    /// GET returning the parsed body plus the response's `ETag` header, when
    /// the server sent one. Plain `get` throws headers away entirely, which
    /// is fine for read-only callers — but the MCP write path (`mcp.rs`)
    /// needs the ETag from a fresh read to send back as `If-Match` on the
    /// write that follows. Losing it here would silently reopen the exact
    /// read-then-write race `If-Match` exists to close, one layer up from
    /// where it looks closed. Returns `None` for the ETag (never an error)
    /// when the server doesn't send one — today's server, and any provider
    /// route this client hits before optimistic concurrency lands there —
    /// so a caller threading it into a later write degrades to sending no
    /// `If-Match`, i.e. today's unconditional-write behavior, exactly.
    pub fn get_with_etag(&self, path: &str) -> anyhow::Result<(serde_json::Value, Option<String>)> {
        let resp = self.request(reqwest::Method::GET, path, &[]).send()?;
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let value = extract(resp)?;
        Ok((value, etag))
    }

    pub fn post<T: Serialize>(&self, path: &str, body: &T) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::POST, path, &[])
            .json(body)
            .send()?;
        extract(resp)
    }

    /// POST with extra request headers on top of the ordinary auth header —
    /// for a route gated on its own privileged token
    /// (`orch dispatch`'s `X-Tack-Dispatch-Token`), which is never the same
    /// value as `token`/`TACK_API_TOKEN` and must be sent alongside it, not
    /// instead of it.
    pub fn post_with_headers<T: Serialize>(
        &self,
        path: &str,
        body: &T,
        extra_headers: &[(&str, &str)],
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::POST, path, extra_headers)
            .json(body)
            .send()?;
        extract(resp)
    }

    pub fn patch<T: Serialize>(&self, path: &str, body: &T) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PATCH, path, &[])
            .json(body)
            .send()?;
        extract(resp)
    }

    /// PATCH with an optional `If-Match` precondition. `if_match` is
    /// normally the ETag `get_with_etag` returned for the same resource a
    /// moment earlier; passing `None` sends no header at all and behaves
    /// exactly like plain `patch` — an absent precondition must preserve
    /// today's unconditional-write behavior, not fail closed, so an older
    /// server (or any route that doesn't version its rows yet) keeps
    /// working unchanged.
    ///
    /// A `412 Precondition Failed` is reported as a distinct, actionable
    /// error rather than falling through to the generic `{status}:
    /// {message}` shape `extract` produces for every other status: the
    /// caller raced a concurrent write to the same row (a human in the UI,
    /// another agent, or the reconciler) and needs to be told to re-read,
    /// not handed a message it can't distinguish from "the server broke."
    /// An MCP tool that can't tell those apart retries blindly and clobbers
    /// whatever won the race.
    pub fn patch_if_match<T: Serialize>(
        &self,
        path: &str,
        body: &T,
        if_match: Option<&str>,
    ) -> anyhow::Result<serde_json::Value> {
        let headers: [(&str, &str); 1] = match if_match {
            Some(tag) => [("If-Match", tag)],
            None => return self.patch(path, body),
        };
        let resp = self
            .request(reqwest::Method::PATCH, path, &headers)
            .json(body)
            .send()?;
        if resp.status() == StatusCode::PRECONDITION_FAILED {
            anyhow::bail!(
                "412 Precondition Failed: this item changed since it was last read — \
                 re-read it (get_item) and retry with the current data instead of \
                 resending the same change"
            );
        }
        extract(resp)
    }

    /// GET a binary response (e.g. a file download).
    pub fn get_bytes(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let resp = self
            .request(reqwest::Method::GET, path, &[])
            .timeout(std::time::Duration::from_secs(120))
            .send()?;
        let status = resp.status();
        if status.is_success() {
            Ok(resp.bytes()?.to_vec())
        } else {
            let body: serde_json::Value = resp.json().unwrap_or_default();
            anyhow::bail!("{}: {}", status, error_msg(&body))
        }
    }

    /// POST raw bytes and return the JSON response.
    pub fn post_bytes(&self, path: &str, data: Vec<u8>) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::POST, path, &[])
            .header("Content-Type", "application/octet-stream")
            .body(data)
            .send()?;
        extract(resp)
    }

    pub fn delete(&self, path: &str) -> anyhow::Result<()> {
        let resp = self.request(reqwest::Method::DELETE, path, &[]).send()?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let body: serde_json::Value = resp.json().unwrap_or_default();
        anyhow::bail!("{}: {}", status, error_msg(&body))
    }

    pub fn delete_json(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self.request(reqwest::Method::DELETE, path, &[]).send()?;
        extract(resp)
    }

    pub fn put_empty(&self, path: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PUT, path, &[])
            .header("Content-Length", "0")
            .send()?;
        extract(resp)
    }

    pub fn put_json<T: Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(reqwest::Method::PUT, path, &[])
            .json(body)
            .send()?;
        extract(resp)
    }

    /// Build a request with the auth header (if configured) plus any extra
    /// headers the caller supplies. Without `extra_headers`, every MCP write
    /// would go out with no `If-Match` and be unconditionally
    /// last-write-wins — exactly the agent-versus-human race `If-Match`
    /// exists to catch, on exactly the path that couldn't send it.
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        extra_headers: &[(&str, &str)],
    ) -> reqwest::blocking::RequestBuilder {
        let url = format!("{}/api{}", self.base_url, path);
        let mut req = self.client.request(method, url);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        for (name, value) in extra_headers {
            req = req.header(*name, *value);
        }
        req
    }
}

fn extract(resp: Response) -> anyhow::Result<serde_json::Value> {
    let status = resp.status();
    // Parse body regardless of status so we can show the server's error message
    let body: serde_json::Value = resp.json().unwrap_or_default();
    if status.is_success() {
        Ok(body)
    } else {
        anyhow::bail!("{}: {}", status, error_msg(&body))
    }
}

/// The operator execution/fleet/runner/profile routes (`/api/executions`,
/// `/api/runner-fleets`, `/api/runners/*`, `/api/agent-profiles`,
/// `/api/model-profiles`) answer errors with the stable runner-v1 protocol
/// envelope, `{"error": {"code": "...", "message": "...", "details": {...},
/// "retryable": bool}}` — `error` is an *object* there, not a string. Every
/// other route in this API answers with `{"error": "text"}` or
/// `{"message": "text"}`. Try the plain-string shapes first (unchanged
/// behavior for every existing command), and only when `error` turns out to
/// be an object, surface its `code` alongside `message` — e.g.
/// `"409: idempotency_conflict: The idempotency key was used with a
/// different request"` instead of the generic "server error" that
/// `.as_str()` on an object silently produces. This is what makes
/// `idempotency_conflict`/`invalid_transition`/`stale_lease`/`conflict`
/// read as distinct, actionable outcomes for `execution`/`fleet`/`runner`/
/// `*-profile` commands instead of collapsing into one opaque line — the
/// stable `code` is there for a script to grep on; `message` is the
/// human-readable half.
fn error_msg(body: &serde_json::Value) -> String {
    if let Some(s) = body
        .get("error")
        .or_else(|| body.get("message"))
        .and_then(|v| v.as_str())
    {
        return s.to_string();
    }
    if let Some(obj) = body.get("error").and_then(|v| v.as_object()) {
        let code = obj.get("code").and_then(|v| v.as_str());
        let message = obj.get("message").and_then(|v| v.as_str());
        if let (Some(code), Some(message)) = (code, message) {
            return format!("{code}: {message}");
        }
        if let Some(message) = message {
            return message.to_string();
        }
    }
    "server error".to_string()
}

// ── Connection check ──────────────────────────────────────────────────────────

pub fn check_connection(config: &Config) -> anyhow::Result<()> {
    let client = TackClient::new(config)?;
    client.get("/health").map_err(|_| {
        anyhow::anyhow!(
            "Cannot reach Tack API at {}\n\
             Make sure the server is running: cargo run -p tack-cli -- serve",
            config.base_url
        )
    })?;
    Ok(())
}

// ── Response helpers ──────────────────────────────────────────────────────────

/// Format a status code for user-facing output
pub fn status_label(code: StatusCode) -> &'static str {
    match code.as_u16() {
        400 => "Bad request",
        401 => "Unauthorized (check TACK_API_TOKEN)",
        403 => "Forbidden",
        404 => "Not found",
        409 => "Conflict",
        412 => "Precondition failed (item changed — re-read and retry)",
        422 => "Validation failed",
        _ => "Error",
    }
}

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;
