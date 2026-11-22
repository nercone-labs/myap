/// The parts of an address, in whatever detail a geocoder gives, and how to write them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Address {
    pub country_code: Option<String>,
    pub country: Option<String>,
    pub postcode: Option<String>,
    /// Prefecture, state, province.
    pub state: Option<String>,
    pub city: Option<String>,
    /// Ward, district, suburb.
    pub district: Option<String>,
    /// Neighbourhood, quarter, 丁目.
    pub locality: Option<String>,
    pub street: Option<String>,
    pub house_number: Option<String>,
}

impl Address {
    /// Whether `text` has Chinese characters, kana or hangul.
    pub fn is_cjk(text: &str) -> bool {
        text.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF))
    }

    /// The address as it is written where it is: in Japan, China, Korea and Taiwan from the
    /// largest part to the smallest without separators (東京都千代田区丸の内1丁目); elsewhere from
    /// the smallest, comma-separated (10 Downing Street, London). The country is left out.
    pub fn format(&self) -> Option<String> {
        // Written in Chinese characters, kana or hangul, an address of these countries runs from the
        // largest part without separators; in the Latin alphabet ("Shibakōen, Minato, Tokyo") it
        // is written as elsewhere.
        let cjk_country = matches!(self.country_code.as_deref().map(str::to_ascii_lowercase).as_deref(), Some("jp" | "cn" | "kr" | "tw"));
        // Every part with letters must be so: parts of two scripts run together without separators
        // ("東京都TokyoSenju") cannot be read, and are written as elsewhere instead.
        let parts_all = [&self.state, &self.city, &self.district, &self.locality, &self.street];
        let lettered: Vec<&str> = parts_all.iter().filter_map(|p| p.as_deref()).filter(|p| p.chars().any(char::is_alphabetic)).collect();
        let cjk_script = !lettered.is_empty() && lettered.iter().all(|p| Self::is_cjk(p));
        let largest_first = cjk_country && cjk_script;
        let street = match (&self.house_number, &self.street) {
            (Some(n), Some(s)) => Some(format!("{n} {s}")),
            (None, s) => s.clone(),
            (n, None) => n.clone(),
        };
        let order: [&Option<String>; 6] = if largest_first {
            [&self.state, &self.city, &self.district, &self.locality, &self.street, &self.house_number]
        } else {
            [&street, &self.locality, &self.district, &self.city, &self.state, &None]
        };
        let mut parts: Vec<&str> = Vec::new();
        for part in order {
            if let Some(p) = part.as_deref().map(str::trim).filter(|p| !p.is_empty())
                && !parts.contains(&p)
            {
                parts.push(p);
            }
        }
        if parts.is_empty() {
            None
        } else if largest_first {
            Some(parts.concat())
        } else {
            Some(parts.join(", "))
        }
    }
}
