/// The headers of the desktop browser that the web endpoints of Google Maps are made for.
///
/// The endpoints answer in the shape the parsers know only to a desktop browser; a mobile one gets
/// other pages. The values are held here, not built, so that they can follow the browser's
/// releases as data.
#[derive(Debug, Clone, PartialEq)]
pub struct Browser {
    pub user_agent: String,
    /// `Sec-CH-UA`, which must agree with the version in `user_agent`.
    pub client_hints: String,
    pub platform: String,
}

impl Default for Browser {
    fn default() -> Browser {
        Browser {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36".into(),
            client_hints: "\"Not:A-Brand\";v=\"8\", \"Chromium\";v=\"156\", \"Google Chrome\";v=\"156\"".into(),
            platform: "\"Windows\"".into(),
        }
    }
}

impl Browser {
    /// The consent cookie that Vela documents for skipping Google's consent page in the European
    /// Union (`SOCS`); this value is widely reported to be "reject all", but Google does not
    /// document it. The older `CONSENT=YES+` that Vela also sends is a grant, and is not sent.
    pub const CONSENT_COOKIES: &'static str = "SOCS=CAESHAgBEhIaAB";

    /// The `Accept-Language` of a browser set to `language` (a BCP 47 tag): `ja-JP,ja;q=0.9`.
    pub fn accept_language(language: &str) -> String {
        let tag = language.trim();
        let primary = tag.split(['-', '_']).next().unwrap_or("en");
        if tag.is_empty() {
            "en-US,en;q=0.9".into()
        } else if primary == tag {
            tag.to_string()
        } else {
            format!("{tag},{primary};q=0.9")
        }
    }

    /// The headers of a data request made by the page of Google Maps.
    pub fn data_headers(&self, language: &str) -> Vec<(String, String)> {
        vec![
            ("User-Agent".into(), self.user_agent.clone()),
            ("Accept".into(), "*/*".into()),
            ("Accept-Language".into(), Self::accept_language(language)),
            ("Sec-CH-UA".into(), self.client_hints.clone()),
            ("Sec-CH-UA-Mobile".into(), "?0".into()),
            ("Sec-CH-UA-Platform".into(), self.platform.clone()),
            ("Sec-Fetch-Dest".into(), "empty".into()),
            ("Sec-Fetch-Mode".into(), "cors".into()),
            ("Sec-Fetch-Site".into(), "same-origin".into()),
            ("Referer".into(), "https://www.google.com/maps/".into()),
            ("Cookie".into(), Self::CONSENT_COOKIES.into()),
        ]
    }

    /// The headers of opening the page of Google Maps, which gives the ordinary cookies of a
    /// visitor without an account.
    pub fn page_headers(&self, language: &str) -> Vec<(String, String)> {
        vec![
            ("User-Agent".into(), self.user_agent.clone()),
            ("Accept".into(), "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8".into()),
            ("Accept-Language".into(), Self::accept_language(language)),
            ("Sec-CH-UA".into(), self.client_hints.clone()),
            ("Sec-CH-UA-Mobile".into(), "?0".into()),
            ("Sec-CH-UA-Platform".into(), self.platform.clone()),
            ("Sec-Fetch-Dest".into(), "document".into()),
            ("Sec-Fetch-Mode".into(), "navigate".into()),
            ("Sec-Fetch-Site".into(), "none".into()),
            ("Cookie".into(), Self::CONSENT_COOKIES.into()),
        ]
    }
}
