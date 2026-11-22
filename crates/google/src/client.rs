use crate::{Browser, Positional};
use fetch::{Query, Request, Response};
use geo::{Bounds, Coordinate};
use serde_json::Value;
use std::fmt;

/// The shapes of the requests, as data: when Google changes them, these change, not the code.
/// Placeholders in braces are filled in by [`Google`].
#[derive(Debug, Clone, PartialEq)]
pub struct Templates {
    /// The `pb` of suggestions: `{SPAN}` metres, `{LNG}`, `{LAT}`, `{W}`×`{H}` window.
    pub suggest: String,
    /// The `pb` of search: `{QUERY}`, `{LNG}`, `{LAT}`; the span is the first `!1d`.
    pub search: String,
    /// The `pb` of directions: the origin and destination `{OLAT}` `{OLNG}` `{DLAT}` `{DLNG}`, and
    /// `{MODE}` (0 drive, 1 bicycle, 2 walk).
    pub directions: String,
    /// The window of the browser the requests claim, in pixels.
    pub window: (u32, u32),
}

impl Default for Templates {
    fn default() -> Templates {
        Templates {
            suggest: "!2i5!4m12!1m3!1d{SPAN}!2d{LNG}!3d{LAT}!2m3!1f0!2f0!3f0!3m2!1i{W}!2i{H}!4f13.1!7i20!10b1!12m6!1m2!18b1!30b1!2m2!1i203!2i100!19m4!1m3!1i1!2i1!3i1!20m1!1e1".into(),
            search: "!1s{QUERY}!4m8!1m3!1d25229.167291701906!2d{LNG}!3d{LAT}!3m2!1i{W}!2i{H}!4f13.1!7i20!10b1!12m52!1m5!18b1!30b1!31m1!1b1!34e1!2m4!5m1!6e2!20e3!39b1!6m25!32i1!49b1!63m0!66b1!85b1!114b1!149b1!206b1!209b1!212b1!216b1!222b1!223b1!232b1!234b1!235b1!244b1!246b1!250b1!253b1!260b1!266b1!273b1!281b1!291m0!10b1!12b1!13b1!14b1!16b1!17m1!3e1!20m3!5e2!6b1!14b1!46m1!1b0!96b1!99b1!19m4!2m3!1i360!2i120!4i8!20m57!2m2!1i203!2i100!3m2!2i4!5b1!6m6!1m2!1i86!2i86!1m2!1i408!2i240!7m33!1m3!1e1!2b0!3e3!1m3!1e2!2b1!3e2!1m3!1e2!2b0!3e3!1m3!1e8!2b0!3e3!1m3!1e10!2b0!3e3!1m3!1e10!2b1!3e2!1m3!1e10!2b0!3e4!1m3!1e9!2b1!3e2!2b1!9b0!15m8!1m7!1m2!1m1!1e2!2m2!1i195!2i195!3i20!15i9937!24m107!1m25!13m9!2b1!3b1!4b1!6i1!8b1!9b1!14b1!20b1!25b1!18m14!3b1!4b1!5b1!6b1!13b1!14b1!17b1!21b1!22b1!32b1!33m1!1b1!34b1!36e2!10m1!8e3!11m1!3e1!17b1!20m2!1e3!1e6!24b1!25b1!26b1!27b1!29b1!30m1!2b1!36b1!37b1!39m3!2m2!2i1!3i1!43b1!52b1!54m1!1b1!55b1!56m1!1b1!61m2!1m1!1e1!65m5!3m4!1m3!1m2!1i224!2i298!72m22!1m8!2b1!5b1!7b1!12m4!1b1!2b1!4m1!1e1!4b1!8m10!1m6!4m1!1e1!4m1!1e3!4m1!1e4!3sother_user_google_review_posts__and__hotel_and_vr_partner_review_posts!6m1!1e1!9b1!89b1!90m2!1m1!1e2!98m3!1b1!2b1!3b1!103b1!113b1!114m3!1b1!2m1!1b1!117b1!122m1!1b1!126b1!127b1!128m1!1b0!26m4!2m3!1i80!2i92!4i8!30m28!1m6!1m2!1i0!2i0!2m2!1i530!2i{H}!1m6!1m2!1i{W-50}!2i0!2m2!1i{W}!2i{H}!1m6!1m2!1i0!2i0!2m2!1i{W}!2i20!1m6!1m2!1i0!2i{H-20}!2m2!1i{W}!2i{H}!34m19!2b1!3b1!4b1!6b1!8m6!1b1!3b1!4b1!5b1!6b1!7b1!9b1!12b1!14b1!20b1!23b1!25b1!26b1!31b1!37m1!1e81!42b1!49m10!3b1!6m2!1b1!2b1!7m2!1e3!2b1!8b1!9b1!10e2!50m3!2e2!3m1!3b1!61b1!67m5!7b1!10b1!14b1!15m1!1b0!69i782!77b1".into(),
            directions: "!1m4!3m2!3d{OLAT}!4d{OLNG}!6e2!1m4!3m2!3d{DLAT}!4d{DLNG}!6e2!3m12!1m3!1d{SPAN}!2d{CLNG}!3d{CLAT}!2m3!1f0.0!2f0.0!3f0.0!3m2!1i{W}!2i{H}!4f13.1!6m56!1m5!18b1!30b1!31m1!1b1!34e1!2m4!5m1!6e2!20e3!39b1!6m27!32i1!49b1!63m0!66b1!85b1!114b1!149b1!206b1!209b1!212b1!216b1!222b1!223b1!232b1!234b1!235b1!244b1!246b1!250b1!253b1!260b1!266b1!270b1!273b1!279b1!281b1!291m0!10b1!12b1!13b1!14b1!16b1!17m1!3e1!20m5!1e{MODE}!2e3!5e2!6b1!14b1!46m1!1b0!96b1!99b1!15i10142!20m28!1m6!1m2!1i0!2i0!2m2!1i530!2i{H}!1m6!1m2!1i{W-50}!2i0!2m2!1i{W}!2i{H}!1m6!1m2!1i0!2i0!2m2!1i{W}!2i20!1m6!1m2!1i0!2i{H-20}!2m2!1i{W}!2i{H}!27b1!40i783!47m2!8b1!10e2".into(),
            window: (1920, 945),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GoogleError {
    /// No response arrived.
    Network(String),
    /// Google refused: a status of 403 or 429, its "unusual traffic" page, or its consent page.
    Blocked(String),
    /// Another HTTP error.
    Server(u16),
    /// The response is not in the shape this client knows: Google changed it.
    Unreadable(String),
}

impl fmt::Display for GoogleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoogleError::Network(s) => write!(f, "network: {s}"),
            GoogleError::Blocked(s) => write!(f, "refused by Google: {s}"),
            GoogleError::Server(code) => write!(f, "HTTP {code}"),
            GoogleError::Unreadable(s) => write!(f, "unreadable response: {s}"),
        }
    }
}

impl std::error::Error for GoogleError {}

/// The client: what to ask Google, in which language, and how to read the answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Google {
    pub browser: Browser,
    pub templates: Templates,
    /// The language of the texts (`hl`), a BCP 47 tag.
    pub language: String,
    /// The region (`gl`), an ISO 3166 code.
    pub region: String,
}

impl Google {
    pub const ORIGIN: &'static str = "https://www.google.com";

    pub fn new(language: &str, region: &str) -> Google {
        Google { browser: Browser::default(), templates: Templates::default(), language: language.into(), region: region.into() }
    }

    /// The `hl` value: the primary language, with the script region for Chinese.
    pub fn hl(&self) -> String {
        let lower = self.language.to_ascii_lowercase().replace('_', "-");
        let primary = lower.split('-').next().unwrap_or("en");
        match primary {
            "zh" if lower.contains("tw") || lower.contains("hk") || lower.contains("hant") => "zh-TW".into(),
            "zh" => "zh-CN".into(),
            "" => "en".into(),
            p => p.chars().filter(char::is_ascii_alphabetic).collect(),
        }
    }

    pub fn gl(&self) -> String {
        let code: String = self.region.chars().filter(char::is_ascii_alphabetic).take(2).collect();
        if code.len() == 2 { code.to_ascii_lowercase() } else { "us".into() }
    }

    /// Opening the page of Google Maps, before the first data request, as the page itself would
    /// be opened: it gives the ordinary cookies of a visitor without an account, which the host
    /// keeps in memory for Google's hosts only.
    pub fn session_request(&self) -> Request {
        let url = format!("{}/maps?hl={}&gl={}", Self::ORIGIN, self.hl(), self.gl());
        let mut request = Request::get(url).timeout_ms(12_000);
        request.headers = self.browser.page_headers(&self.language);
        request
    }

    pub(crate) fn data_request(&self, url: String) -> Request {
        let mut request = Request::get(url).timeout_ms(12_000);
        request.headers = self.browser.data_headers(&self.language);
        request
    }

    /// The span in metres of `bounds`, or `default` without bounds, within `min..max`.
    pub(crate) fn span(bounds: Option<&Bounds>, default: f64, min: f64, max: f64) -> f64 {
        let span = bounds.map_or(default, |b| {
            let height = Coordinate::new(b.south, b.west).distance_m(&Coordinate::new(b.north, b.west));
            let middle = (b.south + b.north) / 2.0;
            let width = Coordinate::new(middle, b.west).distance_m(&Coordinate::new(middle, b.east));
            height.max(width)
        });
        span.clamp(min, max)
    }

    /// `template` with the window and the given placeholders filled in.
    pub(crate) fn fill(&self, template: &str, values: &[(&str, String)]) -> String {
        let (w, h) = self.templates.window;
        let mut text = template
            .replace("{W-50}", &w.saturating_sub(50).to_string())
            .replace("{H-20}", &h.saturating_sub(20).to_string())
            .replace("{W}", &w.to_string())
            .replace("{H}", &h.to_string());
        for (name, value) in values {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }

    pub(crate) fn query() -> Query {
        Query::new()
    }

    /// The body of a response as JSON, or the error the response stands for.
    pub(crate) fn json(response: &Response) -> Result<Value, GoogleError> {
        Self::check(response)?;
        Positional::parse(&response.body).ok_or_else(|| GoogleError::Unreadable(response.body.chars().take(80).collect()))
    }

    /// Whether the response of [`session_request`](Self::session_request) is the page, not a
    /// refusal.
    pub fn session_check(response: &Response) -> Result<(), GoogleError> {
        Self::check(response)
    }

    pub(crate) fn check(response: &Response) -> Result<(), GoogleError> {
        if response.status == 0 {
            return Err(GoogleError::Network(response.error.clone().unwrap_or_default()));
        }
        if matches!(response.status, 403 | 429) {
            return Err(GoogleError::Blocked(format!("HTTP {}", response.status)));
        }
        if !response.is_success() {
            return Err(GoogleError::Server(response.status));
        }
        let head: String = response.body.chars().take(4000).collect();
        if head.contains("/sorry/") || head.contains("consent.google.") {
            return Err(GoogleError::Blocked("verification or consent page".into()));
        }
        Ok(())
    }
}
