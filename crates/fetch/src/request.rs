use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
}

/// What to fetch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: Method,
    pub url: String,
    /// Header names and values, sent in this order.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// The body of a POST, as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The longest the host is to wait for the whole response, in milliseconds.
    pub timeout_ms: u64,
    /// When set, the host writes the body to this file instead of returning it (for large
    /// downloads), and the [`Response`] carries no body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_to: Option<String>,
}

impl Request {
    pub const DEFAULT_TIMEOUT_MS: u64 = 20_000;

    pub fn get(url: impl Into<String>) -> Request {
        Request { method: Method::Get, url: url.into(), headers: Vec::new(), body: None, timeout_ms: Self::DEFAULT_TIMEOUT_MS, save_to: None }
    }

    pub fn post(url: impl Into<String>, content_type: &str, body: impl Into<String>) -> Request {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: vec![("Content-Type".into(), content_type.into())],
            body: Some(body.into()),
            timeout_ms: Self::DEFAULT_TIMEOUT_MS,
            save_to: None,
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Request {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn timeout_ms(mut self, timeout_ms: u64) -> Request {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn save_to(mut self, path: impl Into<String>) -> Request {
        self.save_to = Some(path.into());
        self
    }

    /// The value of the header `name` (case-insensitive), if set.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The host of the URL (`example.org` of `https://example.org:8080/path`), lowercased.
    pub fn host(&self) -> Option<String> {
        let rest = self.url.split_once("://")?.1;
        let authority = rest.split(['/', '?', '#']).next()?;
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        let host = if host.starts_with('[') { host.split_once(']').map(|(h, _)| &h[1..])? } else { host.split(':').next()? };
        (!host.is_empty()).then(|| host.to_ascii_lowercase())
    }
}

/// What came back. `status` 0 means that no response arrived (no network, a timeout, a TLS
/// failure), and `error` says why.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    #[serde(default)]
    pub body: String,
    /// The headers of the response, when the requester needs them (cookies); a host may leave
    /// them out otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(body: impl Into<String>) -> Response {
        Response { status: 200, body: body.into(), headers: Vec::new(), error: None }
    }

    pub fn failed(error: impl Into<String>) -> Response {
        Response { status: 0, body: String::new(), headers: Vec::new(), error: Some(error.into()) }
    }

    /// The values of the header `name` (case-insensitive), in order.
    pub fn header_values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers.iter().filter(move |(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}
