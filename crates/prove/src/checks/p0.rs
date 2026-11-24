use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::Value;

use super::check;
use crate::model::{Check, CheckError, CheckOutcome, Ctx};

const DASHBOARD_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

static DATA_BUILD_SHA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"data-build-sha="([^"]*)""#).expect("data-build-sha pattern is valid")
});

pub(super) fn register(checks: &mut Vec<Check>) {
    checks.push(check!("P0-A1", p0_send_test_event_appears_in_dashboard));
    checks.push(check!("P0-A2", p0_page_build_sha_matches_running_digest));
    checks.push(check!("P0-A3", p0_readyz_tracks_database_availability));
    checks.push(check!("P0-A4", p0_staging_and_prod_run_same_digest));
    checks.push(check!("P0-A5", p0_pr_pipeline_median_under_ten_minutes));
    checks.push(check!("P0-A6", p0_merge_to_staging_lead_time_recorded));
    checks.push(check!("P0-S1", p0_channel_full_drops_are_counted));
    checks.push(check!("P0-S2", p0_page_build_sha_is_current));
    checks.push(check!("P0-S3", p0_deployed_app_passes_smoke_test));
    checks.push(check!("P0-S4", p0_running_image_is_signed_and_scanned));
    checks.push(check!("P0-S5", p0_worker_retry_exhaustion_is_counted));
    checks.push(check!("P0-S6", p0_readyz_is_not_ready_while_draining));
    checks.push(check!("P0-R1", p0_rollback_to_previous_digest_is_recorded));
}

fn p0_send_test_event_appears_in_dashboard(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    send_test_event_and_wait(ctx)?;
    Ok(CheckOutcome::Pass)
}

fn p0_page_build_sha_matches_running_digest(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    let sha = page_build_sha(ctx)?;
    let Some(expected) = expected_build_sha(ctx) else {
        return Ok(CheckOutcome::Pending(
            "PROVE_EXPECTED_BUILD_SHA is not set; cannot verify the page build SHA".to_string(),
        ));
    };
    if sha != expected {
        return Err(CheckError::new(format!(
            "page build SHA '{sha}' does not match PROVE_EXPECTED_BUILD_SHA '{expected}'"
        )));
    }
    Ok(CheckOutcome::Pass)
}

fn p0_readyz_tracks_database_availability(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    let Some(container) = ctx
        .get("PROVE_DB_CONTAINER")
        .filter(|name| !name.is_empty())
    else {
        return Ok(CheckOutcome::Pending(
            "set PROVE_DB_CONTAINER to the database container name to run this check".to_string(),
        ));
    };

    let mut guard = ContainerRestartGuard::new(container);
    docker(container, "stop")?;

    let detection = wait_for_readyz(ctx, false, Duration::from_secs(30));
    if !detection.ok {
        return Err(CheckError::new(format!(
            "/readyz never returned non-200 within 30 s while the database was stopped ({})",
            detection.last
        )));
    }

    docker(container, "start")?;
    guard.disarm();

    let recovery = wait_for_readyz(ctx, true, Duration::from_secs(60));
    if !recovery.ok {
        return Err(CheckError::new(format!(
            "/readyz never returned 200 within 60 s after the database restarted ({})",
            recovery.last
        )));
    }

    Ok(CheckOutcome::Pass)
}

fn p0_staging_and_prod_run_same_digest(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Pending(
        "needs staging and prod clusters so both running image digests can be compared".to_string(),
    ))
}

fn p0_pr_pipeline_median_under_ten_minutes(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Pending(
        "needs GitHub Actions history (the last 10 completed PR runs) to compute the median"
            .to_string(),
    ))
}

fn p0_merge_to_staging_lead_time_recorded(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Pending(
        "needs a recorded merge-to-staging Argo CD sync for the current main revision".to_string(),
    ))
}

fn p0_channel_full_drops_are_counted(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    run_self_test(ctx, "channel-full")
}

fn p0_page_build_sha_is_current(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Pending(
        "requires deployed metadata: deployment annotation, committed tag, running digest, registry revision"
            .to_string(),
    ))
}

fn p0_deployed_app_passes_smoke_test(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    let health = ctx.get_path("/healthz")?;
    if health.status != 200 {
        return Err(CheckError::new(format!(
            "deploy is green but the app is broken: GET /healthz returned {} ({})",
            health.status,
            health.preview()
        )));
    }

    let ready = ctx.get_path("/readyz")?;
    if ready.status != 200 {
        return Err(CheckError::new(format!(
            "deploy is green but the app is not ready: GET /readyz returned {} ({})",
            ready.status,
            ready.preview()
        )));
    }

    send_test_event_and_wait(ctx)?;
    Ok(CheckOutcome::Pass)
}

fn p0_running_image_is_signed_and_scanned(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Pending(
        "needs a cluster plus cosign/crane to verify the signature and vulnerability attestation of the running digest"
            .to_string(),
    ))
}

fn p0_worker_retry_exhaustion_is_counted(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    run_self_test(ctx, "db-retry")
}

fn p0_readyz_is_not_ready_while_draining(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    run_self_test(ctx, "drain")
}

fn run_self_test(ctx: &Ctx, self_test: &str) -> Result<CheckOutcome, CheckError> {
    let root = repo_root();
    let (program, arguments) = match ctx.get("GRIDFORGE_BIN").filter(|value| !value.is_empty()) {
        Some(binary) => (
            PathBuf::from(binary),
            vec!["self-test".to_string(), self_test.to_string()],
        ),
        None => (
            PathBuf::from(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())),
            vec![
                "run".to_string(),
                "-p".to_string(),
                "grid-forge".to_string(),
                "--quiet".to_string(),
                "--".to_string(),
                "self-test".to_string(),
                self_test.to_string(),
            ],
        ),
    };

    let output = Command::new(&program)
        .args(&arguments)
        .current_dir(&root)
        .output()
        .map_err(|error| {
            CheckError::new(format!(
                "failed to run `{} {}`: {error}",
                program.display(),
                arguments.join(" ")
            ))
        })?;

    if !output.status.success() {
        let mut reason = format!(
            "`{} {}` exited with {}",
            program.display(),
            arguments.join(" "),
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            reason.push_str(&format!("\nstdout:\n{}", stdout.trim()));
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            reason.push_str(&format!("\nstderr:\n{}", stderr.trim()));
        }
        return Err(CheckError::new(reason));
    }

    Ok(CheckOutcome::Pass)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .to_path_buf()
}

fn p0_rollback_to_previous_digest_is_recorded(_ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    Ok(CheckOutcome::Pending(
        "needs two deployed digests and a completed rollback drill".to_string(),
    ))
}

fn expected_build_sha(ctx: &Ctx) -> Option<&str> {
    ctx.get("PROVE_EXPECTED_BUILD_SHA")
        .filter(|value| !value.trim().is_empty())
}

fn page_build_sha(ctx: &Ctx) -> Result<String, CheckError> {
    let page = ctx.get_path("/")?;
    if page.status != 200 {
        return Err(CheckError::new(format!(
            "GET / returned {} instead of 200 ({})",
            page.status,
            page.preview()
        )));
    }
    let captures = DATA_BUILD_SHA
        .captures(&page.body)
        .ok_or_else(|| CheckError::new("dashboard page is missing the data-build-sha attribute"))?;
    let sha = captures
        .get(1)
        .map(|value| value.as_str())
        .unwrap_or_default();
    if sha.is_empty() {
        return Err(CheckError::new(
            "dashboard page has an empty data-build-sha attribute",
        ));
    }
    Ok(sha.to_string())
}

fn send_test_event_and_wait(ctx: &Ctx) -> Result<(), CheckError> {
    let response = ctx.post_path("/events/test", &[("Accept", "application/json")])?;
    if response.status != 202 {
        return Err(CheckError::new(format!(
            "POST /events/test: expected 202, got {} ({})",
            response.status,
            response.preview()
        )));
    }

    let payload: Value = serde_json::from_str(&response.body).map_err(|error| {
        CheckError::new(format!(
            "POST /events/test: response is not JSON ({error}): {}",
            response.preview()
        ))
    })?;
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            CheckError::new(format!(
                "POST /events/test: response has no id: {}",
                response.preview()
            ))
        })?;

    let deadline = Instant::now() + DASHBOARD_TIMEOUT;
    let mut last: String;
    loop {
        match ctx.get_path("/") {
            Ok(page) => {
                if page.status == 200 && page.body.contains(id) {
                    return Ok(());
                }
                last = format!("GET / returned {}", page.status);
            }
            Err(error) => last = error.to_string(),
        }

        if Instant::now() >= deadline {
            return Err(CheckError::new(format!(
                "event {id} did not appear on the dashboard within 5 s ({last})"
            )));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

struct ReadyzWait {
    ok: bool,
    last: String,
}

fn wait_for_readyz(ctx: &Ctx, want_200: bool, timeout: Duration) -> ReadyzWait {
    let deadline = Instant::now() + timeout;
    let mut last: String;
    loop {
        match ctx.get_path("/readyz") {
            Ok(response) => {
                last = format!("/readyz returned {}", response.status);
                if (response.status == 200) == want_200 {
                    return ReadyzWait { ok: true, last };
                }
            }
            Err(error) => last = error.to_string(),
        }

        if Instant::now() >= deadline {
            return ReadyzWait { ok: false, last };
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

struct ContainerRestartGuard {
    name: String,
    armed: bool,
}

impl ContainerRestartGuard {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ContainerRestartGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = Command::new("docker")
                .args(["start", &self.name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

fn docker(container: &str, action: &str) -> Result<(), CheckError> {
    let output = Command::new("docker")
        .args([action, container])
        .output()
        .map_err(|error| {
            CheckError::new(format!(
                "failed to run `docker {action} {container}`: {error}"
            ))
        })?;
    if !output.status.success() {
        return Err(CheckError::new(format!(
            "`docker {action} {container}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
