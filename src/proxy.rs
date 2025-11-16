use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    Router,
    body::{self, Body},
    http::{
        Request, Response, StatusCode, Uri,
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, HOST, HeaderName, HeaderValue, ORIGIN},
    },
};
use bytes::Bytes;
use http::{HeaderMap, Method};
use rand::Rng;
use serde_json::json;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use url::Url;

use crate::http_client::{HttpClientError, OutgoingRequest, ProxiedResponse};
use crate::response::json_response;
use crate::settings::{
    Settings, SettingsLayer, from_parts as request_context_from_parts, matches_request,
};
use crate::state::AppState;
use tower::Service;

const DESTINATION_HEADER: &str = "x-lowdown-destination-url";

pub fn router(state: Arc<AppState>) -> Router {
    Router::new().fallback_service(ProxyService { state })
}

async fn proxy_entry(state: Arc<AppState>, req: Request<Body>) -> Response<Body> {
    let req = rewrite_forwarding(req);
    match handle_proxy(state, req).await {
        Ok(response) => response,
        Err(response) => response,
    }
}

async fn handle_proxy(
    state: Arc<AppState>,
    req: Request<Body>,
) -> Result<Response<Body>, Response<Body>> {
    let (parts, body) = req.into_parts();
    let body_bytes = body::to_bytes(body, usize::MAX).await.map_err(|err| {
        warn!("Failed to read request body: {err}");
        json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &json!({"error":"invalid-request"}),
            state.body_trailer(),
        )
    })?;

    let request_layer = SettingsLayer::from_headers(&parts.headers);
    let mut settings = state.effective_settings(&request_layer);
    let ctx = request_context_from_parts(&parts.method, &parts.uri, &parts.headers);
    settings = state.apply_one_off(&ctx, settings);

    let destination = match settings.destination_url.clone() {
        Some(url) => match Destination::parse(&url, state.body_trailer()) {
            Ok(dest) => dest,
            Err(response) => return Err(response),
        },
        None => {
            return Err(json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &json!({"error":"missing-destination-url"}),
                state.body_trailer(),
            ));
        }
    };

    let matches = matches_request(&ctx, &settings);

    if should_trigger(settings.delay_before_percentage, matches) && settings.delay_before_ms > 0 {
        info!("before-delay {} ms", settings.delay_before_ms);
        sleep(Duration::from_millis(settings.delay_before_ms)).await;
    }

    if should_trigger(settings.fail_before_percentage, matches) {
        info!("HTTP {} {} fail-before", settings.fail_before_code, ctx.uri);
        return Err(json_response(
            status_from_code(settings.fail_before_code),
            &json!({"error":"fail-before"}),
            state.body_trailer(),
        ));
    }

    let outgoing_headers =
        build_destination_headers(&parts.headers, &destination, state.body_trailer())?;
    let original_origin = parts.headers.get(ORIGIN).cloned();

    let outgoing = OutgoingRequest {
        method: parts.method.clone(),
        url: format!("{}{}", destination.raw, ctx.uri),
        headers: outgoing_headers,
        body: body_bytes,
    };

    let duplicate = should_trigger(settings.duplicate_percentage, matches);

    let client = state.client();
    let first = client.execute(outgoing.clone());
    let second = if duplicate {
        Some(client.execute(outgoing.clone()))
    } else {
        None
    };

    let first_response = map_client_response(
        first.await,
        &outgoing.url,
        &outgoing.method,
        state.body_trailer(),
    );
    let second_response = match second {
        Some(call) => Some(map_client_response(
            call.await,
            &outgoing.url,
            &outgoing.method,
            state.body_trailer(),
        )),
        None => None,
    };

    log_duplicate_status(
        &outgoing.method,
        &outgoing.url,
        duplicate,
        &first_response,
        second_response.as_ref(),
    );

    let mut proxied = select_response(first_response, second_response);

    if should_trigger(settings.delay_after_percentage, matches) && settings.delay_after_ms > 0 {
        info!("delay-after {} ms", settings.delay_after_ms);
        sleep(Duration::from_millis(settings.delay_after_ms)).await;
    }

    if should_trigger(settings.fail_after_percentage, matches) {
        info!(
            "HTTP {} {} fail-after. Destination response code: {}",
            settings.fail_after_code, ctx.uri, proxied.status
        );
        return Err(json_response(
            status_from_code(settings.fail_after_code),
            &json!({
                "error":"fail-after",
                "destination-response-code": proxied.status.as_u16()
            }),
            state.body_trailer(),
        ));
    }

    rewrite_response_headers(&mut proxied, original_origin);

    log_result(
        matches,
        &settings,
        &outgoing.method,
        &ctx.uri,
        proxied.status,
    );

    Ok(build_response(proxied, state.body_trailer()))
}

fn rewrite_forwarding(mut req: Request<Body>) -> Request<Body> {
    let uri_str = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    if let Some((scheme, host, new_path)) = parse_forward_target(&uri_str) {
        let destination = format!("{scheme}://{host}");
        if let Ok(value) = HeaderValue::from_str(&destination) {
            req.headers_mut()
                .insert(HeaderName::from_static(DESTINATION_HEADER), value);
        }
        if let Ok(parsed) = new_path.parse::<Uri>() {
            *req.uri_mut() = parsed;
        } else {
            *req.uri_mut() = Uri::from_static("/");
        }
    }
    req
}

fn parse_forward_target(uri: &str) -> Option<(String, String, String)> {
    for prefix in ["/lowdown-fwd-", "/lowdown-forward-"] {
        if let Some(rest) = uri.strip_prefix(prefix) {
            for scheme in ["http", "https"] {
                let marker = format!("{scheme}/");
                if let Some(after_scheme) = rest.strip_prefix(&marker) {
                    let mut parts = after_scheme.splitn(2, '/');
                    let host = parts.next()?.to_string();
                    if host.is_empty() {
                        return None;
                    }
                    let path = parts
                        .next()
                        .map(|segment| format!("/{segment}"))
                        .unwrap_or_else(|| "/".to_string());
                    return Some((scheme.to_string(), host, path));
                }
            }
        }
    }
    None
}

fn build_destination_headers(
    headers: &HeaderMap,
    destination: &Destination,
    trailer: &str,
) -> Result<HeaderMap, Response<Body>> {
    let mut map = headers.clone();
    map.insert(
        HOST,
        HeaderValue::from_str(&destination.authority).map_err(|_| invalid_destination(trailer))?,
    );
    if headers.get(ORIGIN).is_some() {
        map.insert(
            ORIGIN,
            HeaderValue::from_str(&destination.origin())
                .map_err(|_| invalid_destination(trailer))?,
        );
    }
    Ok(map)
}

fn rewrite_response_headers(response: &mut ProxiedResponse, client_origin: Option<HeaderValue>) {
    if let Some(origin) = client_origin {
        if response.headers.contains_key(ACCESS_CONTROL_ALLOW_ORIGIN) {
            if let Ok(value) = HeaderValue::from_bytes(origin.as_bytes()) {
                response.headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, value);
                debug!("Rewriting access-control-allow-origin for proxied response");
            }
        }
    }
}

fn select_response(first: ProxiedResponse, second: Option<ProxiedResponse>) -> ProxiedResponse {
    match second {
        Some(second) => {
            if rand::thread_rng().gen_bool(0.5) {
                first
            } else {
                second
            }
        }
        None => first,
    }
}

fn log_duplicate_status(
    method: &Method,
    url: &str,
    duplicate: bool,
    first: &ProxiedResponse,
    second: Option<&ProxiedResponse>,
) {
    if !duplicate {
        debug!("No duplicate request for {} {}", method, url);
        return;
    }
    if let Some(second) = second {
        if first.status != second.status {
            info!(
                "Duplicate request returned different HTTP status codes {} vs {} for {} {}",
                first.status.as_u16(),
                second.status.as_u16(),
                method,
                url
            );
        } else {
            info!(
                "Duplicate request returned identical HTTP status code {} for {} {}",
                first.status.as_u16(),
                method,
                url
            );
        }
    }
}

fn log_result(matches: bool, settings: &Settings, method: &Method, uri: &str, status: StatusCode) {
    let all_zero = settings.fail_before_percentage == 0
        && settings.fail_after_percentage == 0
        && settings.duplicate_percentage == 0
        && settings.delay_before_percentage == 0
        && settings.delay_after_percentage == 0;
    if all_zero || !matches {
        info!(
            "HTTP {} {} {}. No match / all percentages were zero.",
            status.as_u16(),
            method,
            uri
        );
    } else {
        info!("HTTP {} {} {}", status.as_u16(), method, uri);
    }
}

fn invalid_destination(trailer: &str) -> Response<Body> {
    json_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        &json!({"error":"invalid-destination-url"}),
        trailer,
    )
}

fn should_trigger(percentage: u8, matches: bool) -> bool {
    matches && percentage > rand::thread_rng().gen_range(0..100)
}

fn map_client_response(
    result: Result<ProxiedResponse, HttpClientError>,
    url: &str,
    method: &Method,
    trailer: &str,
) -> ProxiedResponse {
    match result {
        Ok(response) => response,
        Err(err) => {
            warn!("Unexpected error when {} {}: {err}", method, url);
            proxied_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error":"unexpected-error","url":url}),
                trailer,
            )
        }
    }
}

fn status_from_code(code: u16) -> StatusCode {
    StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn proxied_json(status: StatusCode, value: serde_json::Value, trailer: &str) -> ProxiedResponse {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    let mut body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    body.push_str(trailer);
    ProxiedResponse::new(status, headers, Bytes::from(body))
}

fn build_response(proxied: ProxiedResponse, trailer: &str) -> Response<Body> {
    Response::builder()
        .status(proxied.status)
        .body(Body::from(proxied.body))
        .map(|mut response| {
            *response.headers_mut() = proxied.headers;
            response
        })
        .unwrap_or_else(|_| {
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &json!({"error":"internal"}),
                trailer,
            )
        })
}

struct Destination {
    raw: String,
    scheme: String,
    authority: String,
}

impl Destination {
    fn parse(url: &str, trailer: &str) -> Result<Self, Response<Body>> {
        match Url::parse(url) {
            Ok(parsed) => {
                let host = parsed
                    .host_str()
                    .map(|h| h.to_string())
                    .ok_or_else(|| invalid_destination(trailer))?;
                let authority = match parsed.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                };
                Ok(Self {
                    raw: url.to_string(),
                    scheme: parsed.scheme().to_string(),
                    authority,
                })
            }
            Err(_) => Err(invalid_destination(trailer)),
        }
    }

    fn origin(&self) -> String {
        format!("{}://{}", self.scheme, self.authority)
    }
}

#[derive(Clone)]
struct ProxyService {
    state: Arc<AppState>,
}

impl Service<Request<Body>> for ProxyService {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future =
        Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let state = self.state.clone();
        Box::pin(async move { Ok(proxy_entry(state, req).await) })
    }
}
