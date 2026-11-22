use crate::{Place, PlaceSource};
use serde::{Deserialize, Serialize};

/// A part of a [`Place`] that one source may know and another not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceField {
    Address,
    Category,
    /// The rating and the number of reviews, which go together.
    Rating,
    Phone,
    Website,
    Hours,
    Status,
    Wheelchair,
    Bounds,
}

impl PlaceField {
    pub const ALL: [PlaceField; 9] = [
        PlaceField::Address,
        PlaceField::Category,
        PlaceField::Rating,
        PlaceField::Phone,
        PlaceField::Website,
        PlaceField::Hours,
        PlaceField::Status,
        PlaceField::Wheelchair,
        PlaceField::Bounds,
    ];
}

/// A field of a place filled from a source other than the place's own, so that what is shown can
/// say where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filled {
    pub field: PlaceField,
    pub source: PlaceSource,
}

/// Whether a wheelchair can get in, as OpenStreetMap's `wheelchair` tag says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wheelchair {
    Yes,
    Limited,
    No,
}

impl Wheelchair {
    /// The value of the `wheelchair` tag; `None` for any other value (`designated` is `Yes`, as the
    /// wiki describes it).
    pub fn of_tag(value: &str) -> Option<Wheelchair> {
        match value.trim() {
            "yes" | "designated" => Some(Wheelchair::Yes),
            "limited" => Some(Wheelchair::Limited),
            "no" => Some(Wheelchair::No),
            _ => None,
        }
    }
}

impl Place {
    /// Whether `field` is known.
    pub fn has(&self, field: PlaceField) -> bool {
        match field {
            PlaceField::Address => self.address.is_some(),
            PlaceField::Category => self.category.is_some(),
            PlaceField::Rating => self.rating.is_some(),
            PlaceField::Phone => self.phone.is_some(),
            PlaceField::Website => self.website.is_some(),
            PlaceField::Hours => !self.hours.is_empty(),
            PlaceField::Status => self.status.is_some(),
            PlaceField::Wheelchair => self.wheelchair.is_some(),
            PlaceField::Bounds => self.bounds.is_some(),
        }
    }

    /// The source `field` came from: the source that filled it, else the place's own.
    pub fn source_of(&self, field: PlaceField) -> PlaceSource {
        self.filled.iter().find(|f| f.field == field).map_or(self.source, |f| f.source)
    }

    /// Whether any part of the place came from `source`, or the place is linked to one of its
    /// ids.
    pub fn knows(&self, source: PlaceSource) -> bool {
        self.source == source || self.filled.iter().any(|f| f.source == source) || self.links.iter().any(|l| PlaceSource::of_id(l) == Some(source))
    }

    /// Fills what this place does not know with what `other`, the same place from another source
    /// or another lookup, knows. What this place knows is never replaced; its name, position and
    /// id stay. The fields filled are recorded with their source, and `other`'s id is linked.
    /// Returns the fields filled.
    pub fn fill_from(&mut self, other: &Place) -> Vec<PlaceField> {
        for id in std::iter::once(&other.id).chain(other.links.iter()) {
            if *id != self.id && !self.links.contains(id) {
                self.links.push(id.clone());
            }
        }
        let mut filled = Vec::new();
        for field in PlaceField::ALL {
            if self.has(field) || !other.has(field) {
                continue;
            }
            match field {
                PlaceField::Address => self.address = other.address.clone(),
                PlaceField::Category => self.category = other.category.clone(),
                PlaceField::Rating => {
                    self.rating = other.rating;
                    self.review_count = other.review_count;
                }
                PlaceField::Phone => self.phone = other.phone.clone(),
                PlaceField::Website => self.website = other.website.clone(),
                PlaceField::Hours => self.hours = other.hours.clone(),
                PlaceField::Status => self.status = other.status.clone(),
                PlaceField::Wheelchair => self.wheelchair = other.wheelchair,
                PlaceField::Bounds => self.bounds = other.bounds,
            }
            let source = other.source_of(field);
            if source != self.source {
                self.filled.retain(|f| f.field != field);
                self.filled.push(Filled { field, source });
            }
            filled.push(field);
        }
        filled
    }

    /// Refreshes this place with `newer`, the same place looked up again in its own source:
    /// every field `newer` knows replaces this one's, and is no longer said to come from another
    /// source; the other fields stay. Its name, position and id stay. Returns the fields replaced.
    pub fn update_from(&mut self, newer: &Place) -> Vec<PlaceField> {
        for id in std::iter::once(&newer.id).chain(newer.links.iter()) {
            if *id != self.id && !self.links.contains(id) {
                self.links.push(id.clone());
            }
        }
        let mut replaced = Vec::new();
        for field in PlaceField::ALL {
            if !newer.has(field) {
                continue;
            }
            match field {
                PlaceField::Address => self.address = newer.address.clone(),
                PlaceField::Category => self.category = newer.category.clone(),
                PlaceField::Rating => {
                    self.rating = newer.rating;
                    self.review_count = newer.review_count;
                }
                PlaceField::Phone => self.phone = newer.phone.clone(),
                PlaceField::Website => self.website = newer.website.clone(),
                PlaceField::Hours => self.hours = newer.hours.clone(),
                PlaceField::Status => self.status = newer.status.clone(),
                PlaceField::Wheelchair => self.wheelchair = newer.wheelchair,
                PlaceField::Bounds => self.bounds = newer.bounds,
            }
            self.filled.retain(|f| f.field != field);
            let source = newer.source_of(field);
            if source != self.source {
                self.filled.push(Filled { field, source });
            }
            replaced.push(field);
        }
        replaced
    }

    /// The first id of the place in `source`: its own, or a link.
    pub fn id_in(&self, source: PlaceSource) -> Option<&str> {
        std::iter::once(&self.id).chain(self.links.iter()).find(|id| PlaceSource::of_id(id) == Some(source)).map(String::as_str)
    }
}

impl PlaceSource {
    /// The source of an id (`osm:…`, `google:…`, `geo:…`), or `None` for an id of no known form.
    pub fn of_id(id: &str) -> Option<PlaceSource> {
        match id.split_once(':')?.0 {
            "osm" => Some(PlaceSource::OpenStreetMap),
            "google" => Some(PlaceSource::Google),
            "geo" => Some(PlaceSource::Coordinate),
            _ => None,
        }
    }
}

/// Decides whether two places, usually of two sources, are the same place: by their names and how
/// far apart they are. Used to fill what one source lacks with what another knows.
///
/// Names are compared after folding case, full-width forms, hiragana into katakana, spaces and
/// punctuation; their likeness is the Dice coefficient of their pairs of characters, or at least
/// `contained_similarity` when one holds the other ("スターバックス" in "スターバックス 渋谷店").
/// Names in different scripts ("Tokyo Tower", "東京タワー") are not the same: no transliteration is
/// attempted, so that a match is never a guess.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceMatch {
    /// How far apart two points may be to be the same place, in metres.
    pub max_distance_m: f64,
    /// How far apart they may be when one is an area whose bounds hold the other, in metres.
    pub max_area_distance_m: f64,
    /// The least likeness of the names.
    pub min_similarity: f64,
    /// The likeness given to a name held whole in the other.
    pub contained_similarity: f64,
}

impl Default for PlaceMatch {
    fn default() -> PlaceMatch {
        PlaceMatch { max_distance_m: 150.0, max_area_distance_m: 2_000.0, min_similarity: 0.5, contained_similarity: 0.75 }
    }
}

impl PlaceMatch {
    /// `name` folded for comparison: lower case, half-width, katakana, without spaces and
    /// punctuation.
    pub fn normalize(name: &str) -> String {
        name.chars()
            .flat_map(char::to_lowercase)
            .map(|c| match c {
                // Full-width ASCII (！ to ～) to ASCII, and the ideographic space to a space.
                '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
                '\u{3000}' => ' ',
                // Hiragana to katakana (ぁ to ゖ).
                '\u{3041}'..='\u{3096}' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
                _ => c,
            })
            .filter(|c| c.is_alphanumeric())
            .collect()
    }

    /// The likeness of two names, from 0 (nothing alike) to 1 (the same once folded).
    pub fn similarity(&self, a: &str, b: &str) -> f64 {
        let (a, b) = (Self::normalize(a), Self::normalize(b));
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }
        if a == b {
            return 1.0;
        }
        let dice = Self::dice(&a, &b);
        let (short, long) = if a.chars().count() <= b.chars().count() { (&a, &b) } else { (&b, &a) };
        if short.chars().count() >= 2 && long.contains(short.as_str()) { dice.max(self.contained_similarity) } else { dice }
    }

    /// The Dice coefficient of the pairs of characters of two folded names.
    fn dice(a: &str, b: &str) -> f64 {
        let pairs = |s: &str| -> Vec<(char, char)> {
            let chars: Vec<char> = s.chars().collect();
            chars.windows(2).map(|w| (w[0], w[1])).collect()
        };
        let (pa, mut pb) = (pairs(a), pairs(b));
        if pa.is_empty() || pb.is_empty() {
            return 0.0;
        }
        let total = pa.len() + pb.len();
        let mut common = 0;
        for pair in &pa {
            if let Some(i) = pb.iter().position(|p| p == pair) {
                pb.swap_remove(i);
                common += 1;
            }
        }
        2.0 * common as f64 / total as f64
    }

    /// How sure it is that `a` and `b` are the same place (above 0), or `None` when they are not.
    pub fn score(&self, a: &Place, b: &Place) -> Option<f64> {
        let distance = a.coordinate.distance_m(&b.coordinate);
        let within_area = |area: &Place, point: &Place| area.bounds.is_some_and(|bounds| bounds.expanded_m(100.0).contains(&point.coordinate));
        let limit = if within_area(a, b) || within_area(b, a) { self.max_area_distance_m } else { self.max_distance_m };
        if !distance.is_finite() || distance > limit {
            return None;
        }
        let similarity = self.similarity(&a.name, &b.name);
        if similarity < self.min_similarity {
            return None;
        }
        // Nearer is surer, by at most a third.
        Some(similarity * (1.0 - distance / limit / 3.0))
    }

    /// The candidate most surely the same place as `place`, if any is.
    pub fn best<'a>(&self, place: &Place, candidates: &'a [Place]) -> Option<&'a Place> {
        candidates.iter().filter_map(|c| self.score(place, c).map(|s| (s, c))).max_by(|x, y| x.0.total_cmp(&y.0)).map(|(_, c)| c)
    }

    /// Combines the results of two searches of the same thing: each place of `primary` is filled
    /// from the place of `secondary` that is surely the same (each used once, the surest pairs
    /// first). Returns `primary`, filled and in its order, and the places of `secondary` that
    /// matched none.
    pub fn combine(&self, mut primary: Vec<Place>, secondary: Vec<Place>) -> (Vec<Place>, Vec<Place>) {
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for (i, p) in primary.iter().enumerate() {
            for (j, s) in secondary.iter().enumerate() {
                if let Some(score) = self.score(p, s) {
                    pairs.push((score, i, j));
                }
            }
        }
        pairs.sort_by(|x, y| y.0.total_cmp(&x.0));
        let (mut used_primary, mut used_secondary) = (vec![false; primary.len()], vec![false; secondary.len()]);
        for (_, i, j) in pairs {
            if used_primary[i] || used_secondary[j] {
                continue;
            }
            used_primary[i] = true;
            used_secondary[j] = true;
            primary[i].fill_from(&secondary[j]);
        }
        let rest = secondary.into_iter().zip(used_secondary).filter(|(_, used)| !used).map(|(p, _)| p).collect();
        (primary, rest)
    }
}
