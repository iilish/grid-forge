use std::time::Duration;

use serde_json::{Value, json};

use crate::model::Ctx;
use crate::runner::RunReport;

const MAX_DESCRIPTION: usize = 140;

pub struct CommitStatusClient {
    agent: ureq::Agent,
    api_url: String,
    repo: String,
    token: String,
    head_sha: String,
    context: String,
}

impl CommitStatusClient {
    pub fn from_ctx(ctx: &Ctx) -> Result<Option<Self>, String> {
        let Some(context) = ctx.get("CHECK_NAME").filter(|value| !value.is_empty()) else {
            return Ok(None);
        };
        let head_sha = ctx
            .get("HEAD_SHA")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "CHECK_NAME is set but HEAD_SHA is missing".to_string())?;
        let repo = ctx
            .get("GH_REPO")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "CHECK_NAME is set but GH_REPO is missing".to_string())?;
        let token = ctx
            .get("GH_TOKEN")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "CHECK_NAME is set but GH_TOKEN is missing".to_string())?;
        let api_url = ctx
            .get("GITHUB_API_URL")
            .filter(|value| !value.is_empty())
            .unwrap_or("https://api.github.com");

        Self::start(context, head_sha, repo, token, api_url).map(Some)
    }

    pub fn start(
        context: &str,
        head_sha: &str,
        repo: &str,
        token: &str,
        api_url: &str,
    ) -> Result<Self, String> {
        let client = Self {
            agent: build_agent(),
            api_url: api_url.to_string(),
            repo: repo.to_string(),
            token: token.to_string(),
            head_sha: head_sha.to_string(),
            context: context.to_string(),
        };
        client.post("pending", "GridForge proofs running")?;
        Ok(client)
    }

    pub fn complete(&self, code: i32, description: &str) -> Result<(), String> {
        self.post(state_for(code), description)
    }

    fn post(&self, state: &str, description: &str) -> Result<(), String> {
        let url = format!(
            "{}/repos/{}/statuses/{}",
            self.api_url.trim_end_matches('/'),
            self.repo,
            self.head_sha
        );
        send(
            &self.agent,
            &url,
            &self.token,
            &payload(state, &self.context, description),
        )
    }
}

pub fn state_for(code: i32) -> &'static str {
    if code == 0 { "success" } else { "failure" }
}

pub fn description(report: &RunReport) -> String {
    let mut text = format!(
        "{} pass, {} fail, {} pending",
        report.pass, report.fail, report.pending
    );
    if !report.failed_ids.is_empty() {
        text.push_str(&format!("; failed {}", report.failed_ids.join(", ")));
    }
    if !report.pending_ids.is_empty() {
        text.push_str(&format!("; pending {}", report.pending_ids.join(", ")));
    }
    truncate(&text, MAX_DESCRIPTION)
}

fn truncate(value: &str, max: usize) -> String {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        return text;
    }
    let mut truncated = text.chars().take(max.saturating_sub(3)).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn payload(state: &str, context: &str, description: &str) -> Value {
    json!({
        "state": state,
        "context": context,
        "description": truncate(description, MAX_DESCRIPTION)
    })
}

fn send(agent: &ureq::Agent, url: &str, token: &str, payload: &Value) -> Result<(), String> {
    let authorization = format!("Bearer {token}");
    let mut response = agent
        .post(url)
        .header("Authorization", &authorization)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "gridforge-prove")
        .header("Content-Type", "application/json")
        .send(payload.to_string())
        .map_err(|error| format!("POST {url}: {error}"))?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("POST {url}: reading body: {error}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("POST {url}: HTTP {status}: {body}"));
    }
    Ok(())
}

fn build_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build();
    ureq::Agent::new_with_config(config)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn state_maps_success_and_failure() {
        assert_eq!(state_for(0), "success");
        assert_eq!(state_for(1), "failure");
        assert_eq!(state_for(2), "failure");
    }

    #[test]
    fn payload_contains_required_fields() {
        let payload = payload("pending", "prove/staging", "GridForge proofs running");
        assert_eq!(payload["state"], "pending");
        assert_eq!(payload["context"], "prove/staging");
        assert_eq!(payload["description"], "GridForge proofs running");
    }

    #[test]
    fn description_truncates_to_140_chars() {
        let long = "x".repeat(200);
        assert_eq!(truncate(&long, MAX_DESCRIPTION).chars().count(), 140);
        assert!(truncate(&long, MAX_DESCRIPTION).ends_with("..."));
    }

    #[test]
    fn description_includes_counts_and_ids() {
        let report = RunReport {
            code: 1,
            pass: 2,
            fail: 1,
            pending: 3,
            results_path: PathBuf::from("prove/results/x.tsv"),
            failed_ids: vec!["P0-A1".to_string()],
            pending_ids: vec!["P0-A2".to_string()],
        };
        let description = description(&report);
        assert!(description.contains("2 pass, 1 fail, 3 pending"));
        assert!(description.contains("failed P0-A1"));
        assert!(description.contains("pending P0-A2"));
    }
}
