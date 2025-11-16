use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use reqwest::Client;
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct OutgoingRequest {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[derive(Clone, Debug)]
pub struct ProxiedResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl ProxiedResponse {
    pub fn new(status: StatusCode, headers: HeaderMap, body: Bytes) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }
}

#[derive(Debug, Error)]
pub enum HttpClientError {
    #[error("request failed: {0}")]
    Transport(String),
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn execute(&self, request: OutgoingRequest) -> Result<ProxiedResponse, HttpClientError>;
}

pub struct ReqwestHttpClient {
    client: Client,
}

impl ReqwestHttpClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: Client::builder().build()?,
        })
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn execute(&self, request: OutgoingRequest) -> Result<ProxiedResponse, HttpClientError> {
        let builder = self
            .client
            .request(
                reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                    .unwrap_or(reqwest::Method::GET),
                &request.url,
            )
            .headers(request.headers.clone())
            .body(request.body.clone());

        match builder.send().await {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                let body = response
                    .bytes()
                    .await
                    .map_err(|err| HttpClientError::Transport(err.to_string()))?;
                Ok(ProxiedResponse::new(
                    StatusCode::from_u16(status.as_u16()).unwrap_or(status),
                    headers,
                    body,
                ))
            }
            Err(err) => Err(HttpClientError::Transport(err.to_string())),
        }
    }
}

pub type SharedHttpClient = Arc<dyn HttpClient>;
