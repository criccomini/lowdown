use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::{
    Router,
    body::{self, Body},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode},
};
use bytes::Bytes;
use http::header::HeaderName;
use lowdown::{
    admin,
    http_client::{
        HttpClient, HttpClientError, OutgoingRequest, ProxiedResponse, SharedHttpClient,
    },
    proxy,
    settings::SettingsLayer,
    state::AppState,
};
use parking_lot::Mutex;
use serde_json::Value;
use tower::util::ServiceExt;

#[derive(Clone)]
struct RecordedRequest {
    url: String,
    headers: HeaderMap,
}

struct StubClient {
    responses: Mutex<VecDeque<ProxiedResponse>>,
    recorded: Mutex<Vec<RecordedRequest>>,
}

impl StubClient {
    fn new() -> Self {
        Self {
            responses: Mutex::new(VecDeque::new()),
            recorded: Mutex::new(Vec::new()),
        }
    }

    fn enqueue(&self, response: ProxiedResponse) {
        self.responses.lock().push_back(response);
    }

    fn recordings(&self) -> Vec<RecordedRequest> {
        self.recorded.lock().clone()
    }
}

#[async_trait]
impl HttpClient for StubClient {
    async fn execute(&self, request: OutgoingRequest) -> Result<ProxiedResponse, HttpClientError> {
        self.recorded.lock().push(RecordedRequest {
            url: request.url.clone(),
            headers: request.headers.clone(),
        });
        let response = self.responses.lock().pop_front().unwrap_or_else(|| {
            ProxiedResponse::new(StatusCode::OK, HeaderMap::new(), Bytes::from_static(b"ok"))
        });
        Ok(response)
    }
}

struct TestHarness {
    proxy: Router,
    admin: Router,
    client: Arc<StubClient>,
}

impl TestHarness {
    fn new() -> Self {
        let client = Arc::new(StubClient::new());
        let shared: SharedHttpClient = client.clone();
        let state = Arc::new(AppState::new(
            SettingsLayer::default(),
            "".to_string(),
            shared,
        ));
        Self {
            proxy: proxy::router(state.clone()),
            admin: admin::router(state),
            client,
        }
    }

    async fn proxy_call(&self, request: Request<Body>) -> ResponseParts {
        let response = self.proxy.clone().oneshot(request).await.unwrap();
        ResponseParts::from(response).await
    }

    async fn admin_call(&self, request: Request<Body>) -> ResponseParts {
        let response = self.admin.clone().oneshot(request).await.unwrap();
        ResponseParts::from(response).await
    }
}

struct ResponseParts {
    status: StatusCode,
    body: Bytes,
}

impl ResponseParts {
    async fn from(response: axum::http::Response<Body>) -> Self {
        let status = response.status();
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Self { status, body }
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

fn request_builder(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(uri)
}

fn json_ok() -> ProxiedResponse {
    ProxiedResponse::new(
        StatusCode::OK,
        HeaderMap::new(),
        Bytes::from_static(b"upstream"),
    )
}

fn destination_header() -> (HeaderName, HeaderValue) {
    (
        HeaderName::from_static("x-lowdown-destination-url"),
        HeaderValue::from_static("http://example.com"),
    )
}

#[tokio::test]
async fn basic_proxy_flow() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();

    let request = request_builder(Method::GET, "/")
        .header(header_name, header_value)
        .body(Body::empty())
        .unwrap();
    let response = harness.proxy_call(request).await;

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body, Bytes::from_static(b"upstream"));
    assert_eq!(harness.client.recordings().len(), 1);
}

#[tokio::test]
async fn forwarding_rewrites_destination() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let request = request_builder(Method::GET, "/lowdown-forward-http/example.org/api")
        .body(Body::empty())
        .unwrap();
    let response = harness.proxy_call(request).await;
    assert_eq!(response.status, StatusCode::OK);
    let recorded = harness.client.recordings();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].url, "http://example.org/api");
    assert_eq!(recorded[0].headers.get("host").unwrap(), "example.org");
}

#[tokio::test]
async fn fail_before_prevents_outbound_request() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();
    let request = request_builder(Method::GET, "/")
        .header(header_name.clone(), header_value.clone())
        .header("x-lowdown-fail-before-percentage", "100")
        .body(Body::empty())
        .unwrap();
    let response = harness.proxy_call(request).await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(harness.client.recordings().len(), 0);
}

#[tokio::test]
async fn fail_after_returns_custom_status() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();
    let request = request_builder(Method::GET, "/")
        .header(header_name.clone(), header_value.clone())
        .header("x-lowdown-fail-after-percentage", "100")
        .body(Body::empty())
        .unwrap();
    let response = harness.proxy_call(request).await;
    assert_eq!(response.status, StatusCode::BAD_GATEWAY);
    let json = response.json();
    assert_eq!(json["error"], "fail-after");
    assert_eq!(json["destination-response-code"], 200);
    assert_eq!(harness.client.recordings().len(), 1);
}

#[tokio::test]
async fn duplicate_requests_are_sent() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    harness.client.enqueue(ProxiedResponse::new(
        StatusCode::CREATED,
        HeaderMap::new(),
        Bytes::from_static(b"secondary"),
    ));
    let (header_name, header_value) = destination_header();
    let request = request_builder(Method::GET, "/")
        .header(header_name.clone(), header_value.clone())
        .header("x-lowdown-duplicate-percentage", "100")
        .body(Body::empty())
        .unwrap();
    let _ = harness.proxy_call(request).await;
    assert_eq!(harness.client.recordings().len(), 2);
}

#[tokio::test]
async fn admin_update_and_reset_affect_defaults() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    harness
        .admin_call(
            request_builder(Method::POST, "/api/v1/update")
                .header("x-lowdown-fail-before-percentage", "100")
                .header("x-lowdown-destination-url", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    let response = harness
        .proxy_call(
            request_builder(Method::GET, "/")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);

    harness
        .admin_call(
            request_builder(Method::POST, "/api/v1/reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();
    let response = harness
        .proxy_call(
            request_builder(Method::GET, "/")
                .header(header_name.clone(), header_value.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
}

#[tokio::test]
async fn one_off_is_consumed_once() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();
    harness
        .admin_call(
            request_builder(Method::POST, "/api/v1/one-off")
                .header("x-lowdown-fail-before-percentage", "100")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    let response = harness
        .proxy_call(
            request_builder(Method::GET, "/")
                .header(header_name.clone(), header_value.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    harness.client.enqueue(json_ok());
    let response = harness
        .proxy_call(
            request_builder(Method::GET, "/")
                .header(header_name.clone(), header_value.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
}

#[tokio::test]
async fn header_matching() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();
    let match_builder = || {
        request_builder(Method::GET, "/")
            .header(header_name.clone(), header_value.clone())
            .header("x-lowdown-match-header-name", "x-user-id")
            .header("x-lowdown-match-header-value", "abc")
            .header("x-lowdown-fail-before-percentage", "100")
    };
    let success = harness
        .proxy_call(match_builder().body(Body::empty()).unwrap())
        .await;
    assert_eq!(success.status, StatusCode::OK);
    let failure = harness
        .proxy_call(
            match_builder()
                .header("x-user-id", "abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(failure.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn delay_before_introduces_latency() {
    let harness = TestHarness::new();
    harness.client.enqueue(json_ok());
    let (header_name, header_value) = destination_header();
    let request = request_builder(Method::GET, "/")
        .header(header_name.clone(), header_value.clone())
        .header("x-lowdown-delay-before-percentage", "100")
        .header("x-lowdown-delay-before-ms", "75")
        .body(Body::empty())
        .unwrap();
    let start = Instant::now();
    harness.proxy_call(request).await;
    assert!(start.elapsed().as_millis() >= 60);
}
