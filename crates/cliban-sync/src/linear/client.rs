//! The Linear GraphQL transport.
//!
//! The whole remote surface is one method — post a document and some variables,
//! get `data` back — and every typed operation in [`super::ops`] is written
//! against that trait rather than against reqwest. Tests then substitute a fake
//! that returns canned JSON, so the response-parsing and mapping logic is
//! exercised without a network, a token, or a mock HTTP server.

use std::time::Duration;

use serde_json::Value;

use crate::error::{Error, Result};

/// Linear's production endpoint.
pub const ENDPOINT: &str = "https://api.linear.app/graphql";

/// Overrides [`ENDPOINT`]. Exists for pointing integration tests at a local
/// stub; there is no reason to set it otherwise.
pub const ENDPOINT_ENV: &str = "CLIBAN_LINEAR_ENDPOINT";

/// A GraphQL endpoint that can answer a document. One method, so a test double
/// is a few lines rather than a mock framework.
#[allow(async_fn_in_trait)]
pub trait GraphQl {
    /// Execute `doc` with `vars` and return the `data` object.
    async fn query(&self, doc: &str, vars: Value) -> Result<Value>;
}

pub struct Client {
    http: reqwest::Client,
    token: String,
    endpoint: String,
}

impl Client {
    /// Build a client from a token. Reads [`ENDPOINT_ENV`] for the endpoint.
    pub fn new(token: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("cliban/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()?;
        let endpoint = std::env::var(ENDPOINT_ENV)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ENDPOINT.to_string());
        Ok(Self {
            http,
            token,
            endpoint,
        })
    }

    /// Build a client reading the token from `$LINEAR_API_KEY`.
    pub fn from_env() -> Result<Self> {
        Self::new(crate::config::token()?)
    }

    async fn post(&self, body: &Value) -> Result<reqwest::Response> {
        Ok(self
            .http
            .post(&self.endpoint)
            // Linear personal API keys go in Authorization *without* a
            // "Bearer " prefix; OAuth tokens use the prefix. Sending the key
            // bare is what works for the keys users mint in settings.
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(body)
            .send()
            .await?)
    }
}

impl GraphQl for Client {
    async fn query(&self, doc: &str, vars: Value) -> Result<Value> {
        let body = serde_json::json!({ "query": doc, "variables": vars });

        let mut response = self.post(&body).await?;

        // One retry on rate limiting. Linear's limits are generous and these
        // are single-issue commands, so a second 429 means something is wrong
        // that waiting longer will not fix.
        if response.status().as_u16() == 429 {
            let wait = retry_after(&response).unwrap_or(Duration::from_secs(2));
            tokio::time::sleep(wait).await;
            response = self.post(&body).await?;
        }

        let status = response.status().as_u16();
        if matches!(status, 401 | 403) {
            return Err(Error::Unauthorized(status));
        }

        let text = response.text().await?;
        if !(200..300).contains(&status) {
            return Err(Error::Http(format!("HTTP {status}: {}", truncate(&text))));
        }

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| Error::Unexpected(format!("{e}: {}", truncate(&text))))?;
        extract_data(parsed)
    }
}

/// Pull `data` out of a GraphQL envelope, turning `errors` into [`Error::Api`].
///
/// Linear can return both `data` and `errors` for a partially-successful query.
/// We treat any error as fatal: a partial result would mean importing an issue
/// with fields silently missing, which is worse than failing.
fn extract_data(mut envelope: Value) -> Result<Value> {
    if let Some(errors) = envelope.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            let messages = errors
                .iter()
                .map(|e| {
                    e.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("(no message)")
                        .to_string()
                })
                .collect();
            return Err(Error::Api(messages));
        }
    }
    match envelope.get_mut("data") {
        Some(data) => Ok(data.take()),
        None => Err(Error::Unexpected("response had no `data` field".into())),
    }
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        // Cap it: a huge Retry-After should surface as an error the user sees,
        // not a CLI that appears to hang.
        .map(|secs| Duration::from_secs(secs.min(30)))
}

/// Keep error messages readable when the remote returns an HTML error page.
fn truncate(s: &str) -> String {
    const MAX: usize = 300;
    if s.len() <= MAX {
        return s.to_string();
    }
    let cut = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|i| *i <= MAX)
        .last()
        .unwrap_or(0);
    format!("{}…", &s[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_data_unwraps_the_envelope() {
        let env = serde_json::json!({"data": {"issue": {"id": "x"}}});
        let data = extract_data(env).unwrap();
        assert_eq!(data["issue"]["id"], "x");
    }

    #[test]
    fn graphql_errors_beat_partial_data() {
        // Linear returns both on a partial failure; importing the partial
        // result would write an issue with fields silently missing.
        let env = serde_json::json!({
            "data": {"issue": null},
            "errors": [{"message": "Entity not found"}, {"message": "and another"}]
        });
        let err = extract_data(env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Entity not found"), "{msg}");
        assert!(msg.contains("and another"), "{msg}");
    }

    #[test]
    fn an_empty_errors_array_is_not_an_error() {
        let env = serde_json::json!({"data": {"ok": true}, "errors": []});
        assert!(extract_data(env).is_ok());
    }

    #[test]
    fn a_missing_data_field_is_reported_as_unexpected() {
        let err = extract_data(serde_json::json!({"nonsense": 1})).unwrap_err();
        assert!(err.to_string().contains("no `data` field"));
    }

    #[test]
    fn errors_without_a_message_still_produce_output() {
        let env = serde_json::json!({"errors": [{"code": 500}]});
        let err = extract_data(env).unwrap_err();
        assert!(err.to_string().contains("no message"));
    }

    #[test]
    fn truncate_keeps_long_html_error_pages_readable() {
        let long = "x".repeat(1000);
        let out = truncate(&long);
        assert!(out.len() < 400);
        assert!(out.ends_with('…'));
        assert_eq!(truncate("short"), "short");
    }

    #[test]
    fn truncate_does_not_split_a_multibyte_character() {
        let s = "é".repeat(400);
        let out = truncate(&s);
        assert!(out.ends_with('…'));
    }
}
