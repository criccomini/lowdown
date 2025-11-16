use std::collections::HashMap;

use http::{HeaderMap, Method, Uri};
use regex::Regex;
use serde::Serialize;
use tracing::warn;

pub const HEADER_PREFIX: &str = "x-lowdown-";

#[derive(Debug, Clone, Serialize)]
pub struct Settings {
    #[serde(rename = "fail-before-code")]
    pub fail_before_code: u16,
    #[serde(rename = "fail-before-percentage")]
    pub fail_before_percentage: u8,
    #[serde(rename = "fail-after-percentage")]
    pub fail_after_percentage: u8,
    #[serde(rename = "fail-after-code")]
    pub fail_after_code: u16,
    #[serde(rename = "duplicate-percentage")]
    pub duplicate_percentage: u8,
    #[serde(rename = "delay-before-percentage")]
    pub delay_before_percentage: u8,
    #[serde(rename = "delay-before-ms")]
    pub delay_before_ms: u64,
    #[serde(rename = "delay-after-percentage")]
    pub delay_after_percentage: u8,
    #[serde(rename = "delay-after-ms")]
    pub delay_after_ms: u64,
    #[serde(rename = "match-uri")]
    pub match_uri: String,
    #[serde(rename = "match-uri-regex")]
    pub match_uri_regex: String,
    #[serde(rename = "match-method")]
    pub match_method: String,
    #[serde(rename = "match-uri-starts-with")]
    pub match_uri_starts_with: String,
    #[serde(rename = "match-host")]
    pub match_host: String,
    #[serde(rename = "match-header-name")]
    pub match_header_name: String,
    #[serde(rename = "match-header-value")]
    pub match_header_value: String,
    #[serde(rename = "destination-url")]
    pub destination_url: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fail_before_code: 503,
            fail_before_percentage: 0,
            fail_after_percentage: 0,
            fail_after_code: 502,
            duplicate_percentage: 0,
            delay_before_percentage: 0,
            delay_before_ms: 0,
            delay_after_percentage: 0,
            delay_after_ms: 0,
            match_uri: "*".to_string(),
            match_uri_regex: "*".to_string(),
            match_method: "*".to_string(),
            match_uri_starts_with: "*".to_string(),
            match_host: "*".to_string(),
            match_header_name: "*".to_string(),
            match_header_value: "*".to_string(),
            destination_url: None,
        }
    }
}

impl Settings {
    pub fn apply_layer(&mut self, layer: &SettingsLayer) {
        if let Some(value) = layer.fail_before_code {
            self.fail_before_code = value;
        }
        if let Some(value) = layer.fail_before_percentage {
            self.fail_before_percentage = value;
        }
        if let Some(value) = layer.fail_after_percentage {
            self.fail_after_percentage = value;
        }
        if let Some(value) = layer.fail_after_code {
            self.fail_after_code = value;
        }
        if let Some(value) = layer.duplicate_percentage {
            self.duplicate_percentage = value;
        }
        if let Some(value) = layer.delay_before_percentage {
            self.delay_before_percentage = value;
        }
        if let Some(value) = layer.delay_before_ms {
            self.delay_before_ms = value;
        }
        if let Some(value) = layer.delay_after_percentage {
            self.delay_after_percentage = value;
        }
        if let Some(value) = layer.delay_after_ms {
            self.delay_after_ms = value;
        }
        if let Some(value) = &layer.match_uri {
            self.match_uri = value.clone();
        }
        if let Some(value) = &layer.match_uri_regex {
            self.match_uri_regex = value.clone();
        }
        if let Some(value) = &layer.match_method {
            self.match_method = value.clone();
        }
        if let Some(value) = &layer.match_uri_starts_with {
            self.match_uri_starts_with = value.clone();
        }
        if let Some(value) = &layer.match_host {
            self.match_host = value.clone();
        }
        if let Some(value) = &layer.match_header_name {
            self.match_header_name = value.clone();
        }
        if let Some(value) = &layer.match_header_value {
            self.match_header_value = value.clone();
        }
        if let Some(value) = &layer.destination_url {
            self.destination_url = if value.is_empty() {
                None
            } else {
                Some(value.clone())
            };
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SettingsLayer {
    pub fail_before_code: Option<u16>,
    pub fail_before_percentage: Option<u8>,
    pub fail_after_percentage: Option<u8>,
    pub fail_after_code: Option<u16>,
    pub duplicate_percentage: Option<u8>,
    pub delay_before_percentage: Option<u8>,
    pub delay_before_ms: Option<u64>,
    pub delay_after_percentage: Option<u8>,
    pub delay_after_ms: Option<u64>,
    pub match_uri: Option<String>,
    pub match_uri_regex: Option<String>,
    pub match_method: Option<String>,
    pub match_uri_starts_with: Option<String>,
    pub match_host: Option<String>,
    pub match_header_name: Option<String>,
    pub match_header_value: Option<String>,
    pub destination_url: Option<String>,
}

impl SettingsLayer {
    pub fn merge(&mut self, other: &SettingsLayer) {
        if other.fail_before_code.is_some() {
            self.fail_before_code = other.fail_before_code;
        }
        if other.fail_before_percentage.is_some() {
            self.fail_before_percentage = other.fail_before_percentage;
        }
        if other.fail_after_percentage.is_some() {
            self.fail_after_percentage = other.fail_after_percentage;
        }
        if other.fail_after_code.is_some() {
            self.fail_after_code = other.fail_after_code;
        }
        if other.duplicate_percentage.is_some() {
            self.duplicate_percentage = other.duplicate_percentage;
        }
        if other.delay_before_percentage.is_some() {
            self.delay_before_percentage = other.delay_before_percentage;
        }
        if other.delay_before_ms.is_some() {
            self.delay_before_ms = other.delay_before_ms;
        }
        if other.delay_after_percentage.is_some() {
            self.delay_after_percentage = other.delay_after_percentage;
        }
        if other.delay_after_ms.is_some() {
            self.delay_after_ms = other.delay_after_ms;
        }
        if other.match_uri.is_some() {
            self.match_uri = other.match_uri.clone();
        }
        if other.match_uri_regex.is_some() {
            self.match_uri_regex = other.match_uri_regex.clone();
        }
        if other.match_method.is_some() {
            self.match_method = other.match_method.clone();
        }
        if other.match_uri_starts_with.is_some() {
            self.match_uri_starts_with = other.match_uri_starts_with.clone();
        }
        if other.match_host.is_some() {
            self.match_host = other.match_host.clone();
        }
        if other.match_header_name.is_some() {
            self.match_header_name = other.match_header_name.clone();
        }
        if other.match_header_value.is_some() {
            self.match_header_value = other.match_header_value.clone();
        }
        if other.destination_url.is_some() {
            self.destination_url = other.destination_url.clone();
        }
    }

    pub fn from_env() -> Self {
        let mut layer = SettingsLayer::default();
        layer.fail_before_code = parse_env_u16("FAIL_BEFORE_CODE");
        layer.fail_before_percentage = parse_env_u8("FAIL_BEFORE_PERCENTAGE");
        layer.fail_after_percentage = parse_env_u8("FAIL_AFTER_PERCENTAGE");
        layer.fail_after_code = parse_env_u16("FAIL_AFTER_CODE");
        layer.duplicate_percentage = parse_env_u8("DUPLICATE_PERCENTAGE");
        layer.delay_before_percentage = parse_env_u8("DELAY_BEFORE_PERCENTAGE");
        layer.delay_before_ms = parse_env_u64("DELAY_BEFORE_MS");
        layer.delay_after_percentage = parse_env_u8("DELAY_AFTER_PERCENTAGE");
        layer.delay_after_ms = parse_env_u64("DELAY_AFTER_MS");
        layer.match_uri = env_string("MATCH_URI");
        layer.match_uri_regex = env_string("MATCH_URI_REGEX");
        layer.match_method = env_string("MATCH_METHOD");
        layer.match_uri_starts_with = env_string("MATCH_URI_STARTS_WITH");
        layer.match_host = env_string("MATCH_HOST");
        layer.match_header_name = env_string("MATCH_HEADER_NAME").map(|v| v.to_ascii_lowercase());
        layer.match_header_value = env_string("MATCH_HEADER_VALUE");
        layer.destination_url = env_string("DESTINATION_URL");
        layer
    }

    pub fn from_headers(headers: &HeaderMap) -> Self {
        let mut layer = SettingsLayer::default();
        for (name, value) in headers.iter() {
            let key = name.as_str().to_ascii_lowercase();
            if let Some(stripped) = key.strip_prefix(HEADER_PREFIX)
                && let Ok(text) = value.to_str() {
                    match stripped {
                        "fail-before-code" => layer.fail_before_code = text.parse().ok(),
                        "fail-before-percentage" => {
                            layer.fail_before_percentage = text.parse().ok()
                        }
                        "fail-after-percentage" => layer.fail_after_percentage = text.parse().ok(),
                        "fail-after-code" => layer.fail_after_code = text.parse().ok(),
                        "duplicate-percentage" => layer.duplicate_percentage = text.parse().ok(),
                        "delay-before-percentage" => {
                            layer.delay_before_percentage = text.parse().ok()
                        }
                        "delay-before-ms" => layer.delay_before_ms = text.parse().ok(),
                        "delay-after-percentage" => {
                            layer.delay_after_percentage = text.parse().ok()
                        }
                        "delay-after-ms" => layer.delay_after_ms = text.parse().ok(),
                        "match-uri" => layer.match_uri = Some(text.to_string()),
                        "match-uri-regex" => layer.match_uri_regex = Some(text.to_string()),
                        "match-method" => layer.match_method = Some(text.to_string()),
                        "match-uri-starts-with" => {
                            layer.match_uri_starts_with = Some(text.to_string())
                        }
                        "match-host" => layer.match_host = Some(text.to_string()),
                        "match-header-name" => {
                            layer.match_header_name = Some(text.to_ascii_lowercase())
                        }
                        "match-header-value" => layer.match_header_value = Some(text.to_string()),
                        "destination-url" => layer.destination_url = Some(text.to_string()),
                        _ => {}
                    }
                }
        }
        layer
    }

    pub fn entries(&self) -> Vec<(&'static str, String)> {
        let mut values = Vec::new();
        macro_rules! push_entry {
            ($field:expr, $name:expr) => {
                if let Some(value) = $field {
                    values.push(($name, value.to_string()));
                }
            };
        }
        push_entry!(self.fail_before_code, "fail-before-code");
        push_entry!(self.fail_before_percentage, "fail-before-percentage");
        push_entry!(self.fail_after_percentage, "fail-after-percentage");
        push_entry!(self.fail_after_code, "fail-after-code");
        push_entry!(self.duplicate_percentage, "duplicate-percentage");
        push_entry!(self.delay_before_percentage, "delay-before-percentage");
        push_entry!(self.delay_before_ms, "delay-before-ms");
        push_entry!(self.delay_after_percentage, "delay-after-percentage");
        push_entry!(self.delay_after_ms, "delay-after-ms");
        if let Some(value) = &self.match_uri {
            values.push(("match-uri", value.clone()));
        }
        if let Some(value) = &self.match_uri_regex {
            values.push(("match-uri-regex", value.clone()));
        }
        if let Some(value) = &self.match_method {
            values.push(("match-method", value.clone()));
        }
        if let Some(value) = &self.match_uri_starts_with {
            values.push(("match-uri-starts-with", value.clone()));
        }
        if let Some(value) = &self.match_host {
            values.push(("match-host", value.clone()));
        }
        if let Some(value) = &self.match_header_name {
            values.push(("match-header-name", value.clone()));
        }
        if let Some(value) = &self.match_header_value {
            values.push(("match-header-value", value.clone()));
        }
        if let Some(value) = &self.destination_url {
            values.push(("destination-url", value.clone()));
        }
        values
    }
}

fn parse_env_u8(key: &str) -> Option<u8> {
    std::env::var(key).ok()?.parse().ok()
}

fn parse_env_u16(key: &str) -> Option<u16> {
    std::env::var(key).ok()?.parse().ok()
}

fn parse_env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub method: Method,
    pub uri: String,
    pub headers: HashMap<String, String>,
}

impl RequestContext {
    pub fn new(method: Method, uri: String, headers: HashMap<String, String>) -> Self {
        Self {
            method,
            uri,
            headers,
        }
    }
}

pub fn from_parts(method: &Method, uri: &Uri, headers: &HeaderMap) -> RequestContext {
    RequestContext {
        method: method.clone(),
        uri: uri
            .path_and_query()
            .map(|pq| pq.as_str().to_string())
            .unwrap_or_else(|| uri.path().to_string()),
        headers: headers_to_map(headers),
    }
}

fn headers_to_map(headers: &HeaderMap) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(text) = value.to_str() {
            map.insert(name.as_str().to_ascii_lowercase(), text.to_string());
        }
    }
    map
}

pub fn matches_request(ctx: &RequestContext, settings: &Settings) -> bool {
    matches_uri(&settings.match_uri, &ctx.uri)
        && matches_uri_regex(&settings.match_uri_regex, &ctx.uri)
        && matches_host(&settings.match_host, settings.destination_url.as_deref())
        && matches_uri_starts_with(&settings.match_uri_starts_with, &ctx.uri)
        && matches_method(&settings.match_method, &ctx.method)
        && match_header(
            &ctx.headers,
            &settings.match_header_name,
            &settings.match_header_value,
        )
}

fn matches_uri(pattern: &str, uri: &str) -> bool {
    pattern == "*" || pattern == uri
}

fn matches_uri_regex(pattern: &str, uri: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match Regex::new(pattern) {
        Ok(regex) => regex
            .find(uri)
            .map(|m| m.start() == 0 && m.end() == uri.len())
            .unwrap_or(false),
        Err(err) => {
            warn!("Invalid match-uri-regex pattern {pattern:?}: {err}");
            false
        }
    }
}

fn matches_uri_starts_with(prefix: &str, uri: &str) -> bool {
    prefix == "*" || uri.starts_with(prefix)
}

fn matches_method(pattern: &str, method: &Method) -> bool {
    pattern == "*" || pattern.eq_ignore_ascii_case(method.as_str())
}

fn match_header(headers: &HashMap<String, String>, name: &str, value: &str) -> bool {
    if name == "*" || value == "*" {
        return true;
    }
    headers
        .get(&name.to_ascii_lowercase())
        .map(|v| v == value)
        .unwrap_or(false)
}

fn matches_host(pattern: &str, destination: Option<&str>) -> bool {
    if pattern == "*" {
        return true;
    }
    destination
        .and_then(destination_host_fragment)
        .map(|host| host == pattern)
        .unwrap_or(false)
}

pub fn destination_host_fragment(url: &str) -> Option<String> {
    url.split_once("://").map(|(_, host)| host.to_string())
}
