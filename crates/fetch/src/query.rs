/// The query of a URL, built pair by pair with every name and value percent-encoded
/// (RFC 3986: everything but the unreserved characters).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    text: String,
}

impl Query {
    pub fn new() -> Query {
        Query::default()
    }

    pub fn pair(mut self, name: &str, value: impl AsRef<str>) -> Query {
        if !self.text.is_empty() {
            self.text.push('&');
        }
        Self::encode_into(name, &mut self.text);
        self.text.push('=');
        Self::encode_into(value.as_ref(), &mut self.text);
        self
    }

    /// Adds the pair when `value` is `Some`.
    pub fn optional(self, name: &str, value: Option<impl AsRef<str>>) -> Query {
        match value {
            Some(value) => self.pair(name, value),
            None => self,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// `base` with this query appended (after `?`, or `&` when `base` has a query already).
    pub fn append_to(&self, base: &str) -> String {
        if self.text.is_empty() {
            return base.to_string();
        }
        let separator = if base.contains('?') { '&' } else { '?' };
        format!("{base}{separator}{}", self.text)
    }

    /// `text` percent-encoded as a component of a URL.
    pub fn encode(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        Self::encode_into(text, &mut out);
        out
    }

    fn encode_into(text: &str, out: &mut String) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for &byte in text.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                out.push(byte as char);
            } else {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0xf) as usize] as char);
            }
        }
    }

    /// Decodes a percent-encoded component (`+` is left as is). Invalid escapes are kept as
    /// they are; invalid UTF-8 becomes U+FFFD.
    pub fn decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = |b: u8| (b as char).to_digit(16);
                if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}
