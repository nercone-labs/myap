use crate::Browser;
use fetch::{Request, Response};
use std::collections::BTreeMap;

/// The cookies Google gives a visitor without an account, kept in memory only: a new session
/// starts with every launch, and [`clear`](Self::clear) starts one at any time.
///
/// Only Google's own cookies are kept (it is used for requests to `www.google.com` alone). The
/// consent cookies of a response (`CONSENT`, `SOCS`) are never taken, so that the consent sent
/// stays the one of [`Browser::CONSENT_COOKIES`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Session {
    cookies: BTreeMap<String, String>,
    started: bool,
}

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    /// Whether the page has been opened in this session (`Google::session_request`).
    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
        self.started = false;
    }

    /// Keeps the cookies set by `response`.
    pub fn absorb(&mut self, response: &Response) {
        self.started = true;
        for header in response.header_values("set-cookie") {
            let Some((pair, attributes)) = header.split_once(';').or(Some((header, ""))) else { continue };
            let Some((name, value)) = pair.split_once('=') else { continue };
            let (name, value) = (name.trim(), value.trim());
            if name.is_empty() || matches!(name, "CONSENT" | "SOCS") {
                continue;
            }
            let lower = attributes.to_ascii_lowercase();
            let expired = lower.contains("max-age=0") || lower.contains("expires=thu, 01 jan 1970") || lower.contains("max-age=-");
            if expired || value.is_empty() {
                self.cookies.remove(name);
            } else if value.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b';' && b != b',')
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                self.cookies.insert(name.to_string(), value.to_string());
            }
        }
    }

    /// `request` with the cookies of the session after the consent cookies.
    pub fn apply(&self, mut request: Request) -> Request {
        let mut cookie = Browser::CONSENT_COOKIES.to_string();
        for (name, value) in &self.cookies {
            cookie.push_str("; ");
            cookie.push_str(name);
            cookie.push('=');
            cookie.push_str(value);
        }
        request.headers.retain(|(n, _)| !n.eq_ignore_ascii_case("cookie"));
        request.headers.push(("Cookie".into(), cookie));
        request
    }
}
