use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::header::{ACCEPT, CONTENT_TYPE, RETRY_AFTER};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::error;

use gridforge_app::{
    AppContext, AppError, Metrics, RecentEvent, create_ingest_event, create_test_event, readiness,
    recent_events,
};
use gridforge_contracts::Envelope;
use prost::Message;

use crate::BUILD_SHA;

const BODY_LIMIT: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DASHBOARD_TIMEOUT: Duration = Duration::from_secs(2);

pub fn router(ctx: AppContext) -> Router {
    let metrics = ctx.ingest.metrics.clone();

    Router::new()
        .route("/", get(index))
        .route("/events/test", post(events_test))
        .route("/ingest", post(ingest))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics_handler))
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(middleware::from_fn_with_state(metrics, track_http))
        .layer(middleware::from_fn(map_layer_errors))
        .with_state(ctx)
}

async fn map_layer_errors(request: Request, next: Next) -> Response {
    let instance = request.uri().path().to_string();
    let response = next.run(request).await;
    let status = response.status();

    if status != StatusCode::PAYLOAD_TOO_LARGE && status != StatusCode::GATEWAY_TIMEOUT {
        return response;
    }

    let request_id = response.headers().get("x-request-id").cloned();
    let request_id_text = request_id
        .as_ref()
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");
    let (kind, title, detail) = if status == StatusCode::PAYLOAD_TOO_LARGE {
        (
            "body-too-large",
            "Payload too large",
            "request body exceeds 64 KiB",
        )
    } else {
        (
            "request-timeout",
            "Request timed out",
            "request exceeded 10 seconds",
        )
    };

    let mut mapped = problem_response(
        status,
        kind,
        title,
        detail,
        &instance,
        request_id_text,
        None,
    );
    if let Some(value) = request_id {
        mapped.headers_mut().insert("x-request-id", value);
    }
    mapped
}

async fn track_http(State(metrics): State<Arc<Metrics>>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = method_label(request.method().as_str());
    let route = route_label(request.uri().path());
    let response = next.run(request).await;
    metrics.observe_http(
        method,
        route,
        status_class(response.status()),
        started.elapsed(),
    );
    response
}

fn method_label(method: &str) -> &'static str {
    match method {
        "GET" => "GET",
        "POST" => "POST",
        _ => "other",
    }
}

fn route_label(path: &str) -> &'static str {
    match path {
        "/" => "/",
        "/events/test" => "/events/test",
        "/ingest" => "/ingest",
        "/healthz" => "/healthz",
        "/readyz" => "/readyz",
        "/metrics" => "/metrics",
        _ => "other",
    }
}

fn status_class(status: StatusCode) -> &'static str {
    match status.as_u16() {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

async fn index(State(ctx): State<AppContext>) -> Response {
    let (db_up, events) =
        match tokio::time::timeout(DASHBOARD_TIMEOUT, recent_events(&ctx.query, 20)).await {
            Ok(Ok(events)) => (true, events),
            Ok(Err(error)) => {
                error!(error = %error, "dashboard query failed");
                (false, Vec::new())
            }
            Err(_) => {
                error!("dashboard query timed out");
                (false, Vec::new())
            }
        };

    let body = render_dashboard(db_up, &events);
    ([(CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
}

async fn events_test(State(ctx): State<AppContext>, headers: HeaderMap) -> Response {
    match create_test_event(&ctx.ingest) {
        Ok(id) => {
            if wants_json(&headers) {
                (StatusCode::ACCEPTED, Json(serde_json::json!({ "id": id }))).into_response()
            } else {
                Redirect::to("/").into_response()
            }
        }
        Err(error) => problem(
            app_error_status(&error),
            error.slug(),
            error.title(),
            &error.detail(),
            "/events/test",
            &headers,
            error.retry_after(),
        ),
    }
}

fn app_error_status(error: &AppError) -> StatusCode {
    match error {
        AppError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
        AppError::Sink(_) | AppError::Store(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn ingest(State(ctx): State<AppContext>, headers: HeaderMap, body: Bytes) -> Response {
    if !is_protobuf(&headers) {
        return problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported-media-type",
            "Unsupported media type",
            "content type must be application/x-protobuf",
            "/ingest",
            &headers,
            None,
        );
    }

    let envelope = match Envelope::decode(body.as_ref()) {
        Ok(envelope) => envelope,
        Err(error) => {
            return problem(
                StatusCode::BAD_REQUEST,
                "malformed-protobuf",
                "Malformed protobuf",
                &error.to_string(),
                "/ingest",
                &headers,
                None,
            );
        }
    };

    match create_ingest_event(&ctx.ingest, &envelope) {
        Ok(id) => (StatusCode::ACCEPTED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(error) => problem(
            app_error_status(&error),
            error.slug(),
            error.title(),
            &error.detail(),
            "/ingest",
            &headers,
            error.retry_after(),
        ),
    }
}

async fn healthz() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/plain; charset=utf-8")], "ok")
}

async fn readyz(State(ctx): State<AppContext>) -> Response {
    let report = readiness(&ctx).await;
    let ready = report.db_up && !report.draining;

    let body = Readiness {
        status: if report.draining {
            "draining"
        } else if ready {
            "ready"
        } else {
            "not_ready"
        },
        db: if report.db_up { "up" } else { "down" },
        draining: report.draining,
    };

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status, Json(body)).into_response()
}

async fn metrics_handler(State(ctx): State<AppContext>) -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        ctx.ingest.metrics.render(),
    )
}

fn is_protobuf(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(';').next().is_some_and(|media_type| {
                media_type
                    .trim()
                    .eq_ignore_ascii_case("application/x-protobuf")
            })
        })
}

fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"))
}

fn problem(
    status: StatusCode,
    kind: &str,
    title: &str,
    detail: &str,
    instance: &str,
    headers: &HeaderMap,
    retry_after: Option<u64>,
) -> Response {
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");

    problem_response(
        status,
        kind,
        title,
        detail,
        instance,
        request_id,
        retry_after,
    )
}

fn problem_response(
    status: StatusCode,
    kind: &str,
    title: &str,
    detail: &str,
    instance: &str,
    request_id: &str,
    retry_after: Option<u64>,
) -> Response {
    let body = Problem {
        problem_type: format!("urn:gridforge:problem:{kind}"),
        title: title.to_string(),
        status: status.as_u16(),
        detail: detail.to_string(),
        instance: instance.to_string(),
        request_id: request_id.to_string(),
    };

    let body = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    let mut response = (status, body).into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    if let Some(seconds) = retry_after {
        response_headers.insert(RETRY_AFTER, HeaderValue::from(seconds));
    }
    response
}

#[derive(Serialize)]
struct Problem {
    #[serde(rename = "type")]
    problem_type: String,
    title: String,
    status: u16,
    detail: String,
    instance: String,
    request_id: String,
}

#[derive(Serialize)]
struct Readiness {
    status: &'static str,
    db: &'static str,
    draining: bool,
}

fn render_dashboard(db_up: bool, events: &[RecentEvent]) -> String {
    let mut html = String::new();
    let db = if db_up { "up" } else { "down" };

    let _ = write!(
        html,
        r#"<!doctype html>
<html lang="en" data-build-sha="{sha}">
<head>
<meta charset="utf-8">
<meta http-equiv="refresh" content="1">
<title>GridForge</title>
<style>
body {{ font-family: system-ui, sans-serif; margin: 2rem; color: #1f2933; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border-bottom: 1px solid #d9e2ec; padding: .45rem; text-align: left; }}
code {{ font-size: .85rem; }}
.muted {{ color: #627d98; }}
</style>
</head>
<body>
<main>
<h1>GridForge</h1>
<p>build <code>{sha}</code> &middot; db <strong>{db}</strong></p>
<form method="post" action="/events/test">
<button type="submit">send test event</button>
</form>
<h2>Recent events</h2>
"#,
        sha = html_escape(BUILD_SHA),
    );

    if events.is_empty() {
        html.push_str("<p class=\"muted\">No events yet.</p>\n");
    } else {
        html.push_str(
            "<table>\n<thead><tr><th>id</th><th>device</th><th>seq</th><th>ingest</th><th>payload</th></tr></thead>\n<tbody>\n",
        );
        for event in events {
            let preview: String = String::from_utf8_lossy(&event.payload)
                .chars()
                .take(80)
                .collect();
            let _ = writeln!(
                html,
                "<tr><td><code>{id}</code></td><td>{device}</td><td>{seq}</td><td>{ingest}</td><td><code>{payload}</code> <span class=\"muted\">{payload_type}</span></td></tr>",
                id = html_escape(&event.id.to_string()),
                device = html_escape(&event.device_id),
                seq = event.seq,
                ingest = html_escape(&event.ingest_ts.to_rfc3339()),
                payload = html_escape(&preview),
                payload_type = html_escape(&event.payload_type),
            );
        }
        html.push_str("</tbody>\n</table>\n");
    }

    html.push_str("</main>\n</body>\n</html>\n");
    html
}

fn html_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::header::CONTENT_LENGTH;
    use chrono::Utc;
    use std::sync::atomic::AtomicBool;
    use tokio::sync::mpsc;
    use tower::ServiceExt;
    use uuid::Uuid;

    use gridforge_app::{ChannelSink, Db, EventStamp};
    use gridforge_domain::{NewEvent, OrgScope};

    #[test]
    fn dashboard_has_build_sha_marker() {
        let html = render_dashboard(true, &[]);

        assert!(html.contains(&format!("data-build-sha=\"{BUILD_SHA}\"")));
        assert!(html.contains("db <strong>up</strong>"));
    }

    #[test]
    fn dashboard_escapes_event_fields() {
        let event = RecentEvent {
            id: Uuid::now_v7(),
            device_id: "<script>alert(1)</script>".to_string(),
            seq: 1,
            ingest_ts: Utc::now(),
            payload: b"<b>payload</b>".to_vec(),
            payload_type: "text/plain".to_string(),
        };

        let html = render_dashboard(true, &[event]);

        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;b&gt;payload&lt;/b&gt;"));
    }

    #[tokio::test]
    async fn oversized_body_returns_problem_json() {
        let (ctx, _receiver) = test_state().await;
        let request = Request::builder()
            .method("POST")
            .uri("/events/test")
            .header(CONTENT_LENGTH, (BODY_LIMIT + 1).to_string())
            .body(Body::empty())
            .unwrap();

        let response = router(ctx).oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["type"], "urn:gridforge:problem:body-too-large");
        assert_eq!(problem["status"], 413);
        assert_eq!(problem["instance"], "/events/test");
    }

    #[tokio::test]
    async fn timeout_returns_problem_json() {
        let app = Router::new()
            .route(
                "/slow",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    "done"
                }),
            )
            .layer(TimeoutLayer::with_status_code(
                StatusCode::GATEWAY_TIMEOUT,
                Duration::from_millis(1),
            ))
            .layer(middleware::from_fn(map_layer_errors));
        let request = Request::builder().uri("/slow").body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["type"], "urn:gridforge:problem:request-timeout");
        assert_eq!(problem["status"], 504);
    }

    async fn test_state() -> (AppContext, mpsc::Receiver<NewEvent>) {
        state_with_capacity(4).await
    }

    async fn state_with_capacity(capacity: usize) -> (AppContext, mpsc::Receiver<NewEvent>) {
        let db = Db::lazy("postgres://gridforge:gridforge@localhost:5432/gridforge")
            .await
            .unwrap();
        let (sender, receiver) = mpsc::channel(capacity);
        let ctx = AppContext::new(
            db,
            Arc::new(ChannelSink::new(sender)),
            Arc::new(Metrics::default()),
            OrgScope::new(Uuid::nil()),
            Arc::new(AtomicBool::new(false)),
        );

        (ctx, receiver)
    }

    fn valid_envelope() -> Envelope {
        Envelope {
            schema_ver: 1,
            org_id: Uuid::nil().to_string(),
            device_id: "device-1".to_string(),
            boot_count: 0,
            seq: 1,
            device_ts: Some(prost_types::Timestamp {
                seconds: Utc::now().timestamp(),
                nanos: 0,
            }),
            payload: b"hello".to_vec(),
            payload_type: "text/plain".to_string(),
        }
    }

    fn ingest_request(body: Vec<u8>) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/ingest")
            .header(CONTENT_TYPE, "application/x-protobuf")
            .body(Body::from(body))
            .unwrap()
    }

    async fn problem_body(response: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn ingest_accepts_valid_envelope() {
        let (ctx, _receiver) = test_state().await;
        let request = ingest_request(valid_envelope().encode_to_vec());

        let response = router(ctx).oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(Uuid::parse_str(json["id"].as_str().unwrap()).is_ok());
    }

    #[tokio::test]
    async fn ingest_rejects_wrong_content_type() {
        let (ctx, _receiver) = test_state().await;
        let request = Request::builder()
            .method("POST")
            .uri("/ingest")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = router(ctx).oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
        let problem = problem_body(response).await;
        assert_eq!(
            problem["type"],
            "urn:gridforge:problem:unsupported-media-type"
        );
        assert_eq!(problem["status"], 415);
    }

    #[tokio::test]
    async fn ingest_rejects_malformed_protobuf() {
        let (ctx, _receiver) = test_state().await;
        let request = ingest_request(vec![0xff]);

        let response = router(ctx).oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = problem_body(response).await;
        assert_eq!(problem["type"], "urn:gridforge:problem:malformed-protobuf");
        assert_eq!(problem["status"], 400);
    }

    #[tokio::test]
    async fn ingest_rejects_invalid_envelope() {
        let (ctx, _receiver) = test_state().await;
        let mut envelope = valid_envelope();
        envelope.schema_ver = 2;

        let response = router(ctx)
            .oneshot(ingest_request(envelope.encode_to_vec()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(response.headers().get(RETRY_AFTER).is_none());
        let problem = problem_body(response).await;
        assert_eq!(problem["type"], "urn:gridforge:problem:invalid-envelope");
        assert_eq!(problem["title"], "Invalid envelope");
        assert_eq!(problem["status"], 422);
        assert_eq!(problem["detail"], "schema_ver must be 1");
    }

    #[tokio::test]
    async fn ingest_returns_503_when_sink_is_full() {
        let (ctx, _receiver) = state_with_capacity(1).await;
        ctx.ingest
            .sink
            .try_send(EventStamp::new(Utc::now(), Uuid::now_v7(), Uuid::nil()).test_event())
            .unwrap();

        let response = router(ctx)
            .oneshot(ingest_request(valid_envelope().encode_to_vec()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
        let problem = problem_body(response).await;
        assert_eq!(problem["type"], "urn:gridforge:problem:sink-unavailable");
        assert_eq!(problem["status"], 503);
    }

    #[tokio::test]
    async fn ingest_returns_503_when_sink_is_closed() {
        let (ctx, receiver) = test_state().await;
        drop(receiver);

        let response = router(ctx)
            .oneshot(ingest_request(valid_envelope().encode_to_vec()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
        let problem = problem_body(response).await;
        assert_eq!(problem["type"], "urn:gridforge:problem:sink-unavailable");
    }

    #[tokio::test]
    async fn events_test_redirects_browsers() {
        let (ctx, _receiver) = test_state().await;

        let response = router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/events/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get("location").unwrap(), "/");
    }

    #[tokio::test]
    async fn events_test_returns_json_for_json_clients() {
        let (ctx, _receiver) = test_state().await;

        let response = router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/events/test")
                    .header(ACCEPT, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(Uuid::parse_str(json["id"].as_str().unwrap()).is_ok());
    }

    #[tokio::test]
    async fn events_test_returns_503_when_sink_is_full() {
        let (ctx, _receiver) = state_with_capacity(1).await;
        ctx.ingest
            .sink
            .try_send(EventStamp::new(Utc::now(), Uuid::now_v7(), Uuid::nil()).test_event())
            .unwrap();

        let response = router(ctx)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/events/test")
                    .header(ACCEPT, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
    }
}
