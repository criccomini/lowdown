use axum::{
    body::Body,
    http::{Response, StatusCode},
};
use serde::Serialize;
use tracing::error;

pub fn json_response<T: Serialize>(status: StatusCode, value: &T, trailer: &str) -> Response<Body> {
    match serde_json::to_string(value) {
        Ok(mut body) => {
            body.push_str(trailer);
            Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .expect("building response")
        }
        Err(err) => {
            error!("failed to serialize JSON response: {err}");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("content-type", "application/json")
                .body(Body::from("{\"error\":\"internal\"}"))
                .expect("building response")
        }
    }
}
