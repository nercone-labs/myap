use serde_json::Value;

/// Reading the responses of Google Maps: nested JSON arrays without keys, where a field is known
/// only by its position, and any of them may be missing.
pub struct Positional;

impl Positional {
    /// The guards that come before the JSON of a response, to stop it being run as a script.
    pub const GUARDS: [&'static str; 6] = [")]}'\n", ")]}',\n", ")]}'", ")]}", "while(1);", "for(;;);"];

    /// Parses `body` after its guard.
    pub fn parse(body: &str) -> Option<Value> {
        let mut text = body.trim_start_matches('\u{feff}');
        for guard in Self::GUARDS {
            if let Some(rest) = text.strip_prefix(guard) {
                text = rest;
                break;
            }
        }
        serde_json::from_str(text.trim_start_matches(['\r', '\n'])).ok()
    }

    /// The value at `path` (indices into arrays), if every step exists and is not null.
    pub fn at<'a>(value: &'a Value, path: &[usize]) -> Option<&'a Value> {
        let mut v = value;
        for &i in path {
            v = v.as_array()?.get(i)?;
        }
        (!v.is_null()).then_some(v)
    }

    pub fn str<'a>(value: &'a Value, path: &[usize]) -> Option<&'a str> {
        Self::at(value, path)?.as_str().filter(|s| !s.is_empty())
    }

    pub fn f64(value: &Value, path: &[usize]) -> Option<f64> {
        Self::at(value, path)?.as_f64()
    }

    pub fn array<'a>(value: &'a Value, path: &[usize]) -> Option<&'a Vec<Value>> {
        Self::at(value, path)?.as_array()
    }

    /// Ends the first JSON object of `text` (matching braces, outside strings): the suggestions
    /// come as an object followed by other text.
    pub fn first_object(text: &str) -> Option<&str> {
        let start = text.find('{')?;
        let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
        for (i, c) in text[start..].char_indices() {
            if in_string {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[start..start + i + 1]);
                    }
                }
                _ => {}
            }
        }
        None
    }
}
