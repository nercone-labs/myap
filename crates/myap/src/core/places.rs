//! Search and places: suggestions, searches of Google and of OpenStreetMap combined, and the
//! details of the place selected, looked up in each source and combined.

use super::{Core, Pending};
use crate::protocol::{LogLevel, PlaceView};
use crate::settings::GoogleUse;
use crate::style::Style;
use crate::words::{Message, Words};
use fetch::Response as HttpResponse;
use geo::{Bounds, Coordinate};
use geocoding::{Geocoder, Place, PlaceField, PlaceMatch, PlaceSource, SearchRequest, Suggestion};

/// Who answers a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SearchSource {
    Google,
    Photon,
    Nominatim,
}

/// The answer of one side of a search.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Part {
    /// Not asked.
    Off,
    Waiting,
    /// Answered, and waiting for the branches of its places.
    Naming(Vec<Place>),
    Done(Vec<Place>),
    Failed,
}

impl Part {
    fn places(&mut self) -> Vec<Place> {
        match std::mem::replace(self, Part::Off) {
            Part::Done(places) => places,
            _ => Vec::new(),
        }
    }
}

/// A search submitted: Google's side and the side of OpenStreetMap (Photon, then Nominatim when
/// Photon fails), combined when both have answered. Photon does not give the branches of places
/// ("渋谷店" of "スターバックス"), so they are looked up in Nominatim before its places count as an
/// answer.
#[derive(Debug, Clone)]
pub(super) struct SearchRun {
    generation: u64,
    query: String,
    google: Part,
    open: Part,
    nominatim_tried: bool,
}

impl Core {
    /// The most results of a search shown.
    const MAX_RESULTS: usize = 30;
    /// How far around the places of Google those of OpenStreetMap that matched none are shown.
    const OPEN_RESULTS_AROUND_M: f64 = 1_000.0;

    pub(super) fn rounded(c: &Coordinate) -> String {
        format!("{:.4}, {:.4}", c.latitude, c.longitude)
    }

    // ---- Suggestions ----

    /// The saved and recent places whose name or address holds `query`.
    fn local_matches(&self, query: &str) -> Vec<Suggestion> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<Suggestion> = Vec::new();
        let places = self.places.saved.iter().map(|s| &s.place).chain(self.places.recent.iter());
        for place in places {
            let hit = place.name.to_lowercase().contains(&q) || place.address.as_deref().is_some_and(|a| a.to_lowercase().contains(&q));
            if hit && !out.iter().any(|s| s.place.as_ref().is_some_and(|p| p.id == place.id)) {
                out.push(Suggestion { text: place.name.clone(), secondary: place.address.clone(), place: Some(place.clone()) });
            }
            if out.len() >= 3 {
                break;
            }
        }
        out
    }

    pub(super) fn suggest(&mut self, query: String) {
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search_run = None;
        self.search.query = query.clone();
        self.search.results.clear();
        self.search.error = None;
        self.search.sources.clear();
        self.search.suggestions = self.local_matches(&query);
        if let Some(c) = Place::parse_coordinate(&query) {
            self.search.suggestions.insert(0, Suggestion { text: query.trim().into(), secondary: None, place: Some(Place::at(c)) });
            self.search.loading = false;
            return;
        }
        if query.trim().chars().count() < 2 {
            self.search.loading = false;
            return;
        }
        self.search.loading = true;
        if self.settings.google.allows(GoogleUse::Suggestions) {
            self.suggest_counter += 1;
            let request = self.google().suggest_request(&query, &self.near(), self.viewport.map(|v| v.1).as_ref(), self.suggest_counter);
            let sent = format!("{} @ {}", query.trim(), Self::rounded(&self.near()));
            self.fetch_google(GoogleUse::Suggestions, sent, request, Pending::Suggest { generation, google: true });
        } else {
            match self.photon().search_request(&self.search_request(&query, 6)) {
                Ok(request) => {
                    self.fetch(request, Pending::Suggest { generation, google: false });
                }
                Err(_) => self.search.loading = false,
            }
        }
    }

    pub(super) fn suggestions_arrived(&mut self, generation: u64, google: bool, response: &HttpResponse) {
        if generation != self.search_generation {
            return;
        }
        self.search.loading = false;
        let found: Result<Vec<Suggestion>, String> = if google {
            self.google().parse_suggest(response).map_err(|e| {
                self.note_google_error(&e);
                e.to_string()
            })
        } else {
            self.photon()
                .parse_search(response)
                .map(|places| places.into_iter().map(|p| Suggestion { text: p.name.clone(), secondary: p.address.clone(), place: Some(p) }).collect())
                .map_err(|e| e.to_string())
        };
        match found {
            Ok(list) => {
                for s in list {
                    let duplicate = self.search.suggestions.iter().any(|o| o.text == s.text && o.secondary == s.secondary);
                    if !duplicate {
                        self.search.suggestions.push(s);
                    }
                }
            }
            Err(e) => self.log(LogLevel::Warning, format!("suggestions: {e}")),
        }
    }

    // ---- Search ----

    /// Searches `query`: in Google when allowed, and in OpenStreetMap when Google is not or the
    /// sources are combined; the results are shown once every side asked has answered.
    pub(super) fn search(&mut self, query: String) {
        self.search_generation += 1;
        let generation = self.search_generation;
        self.search = crate::protocol::SearchView { query: query.clone(), loading: true, ..Default::default() };
        self.deselect();
        self.search_run = None;
        if let Some(c) = Place::parse_coordinate(&query) {
            self.search.loading = false;
            self.select(Place::at(c));
            return;
        }
        self.places.remember_search(&query);
        self.save_places();
        let google = self.settings.google.allows(GoogleUse::Search);
        self.search_run = Some(SearchRun { generation, query: query.clone(), google: Part::Off, open: Part::Off, nominatim_tried: false });
        if google {
            if let Some(run) = self.search_run.as_mut() {
                run.google = Part::Waiting;
            }
            let request = self.google().search_request(&query, &self.near(), self.viewport.map(|v| v.1).as_ref(), 0);
            let sent = format!("{} @ {}", query.trim(), Self::rounded(&self.near()));
            self.fetch_google(GoogleUse::Search, sent, request, Pending::Search { generation, source: SearchSource::Google });
        }
        if !google || self.settings.places.combine {
            self.ask_open(generation, SearchSource::Photon);
        }
        self.finish_search(generation);
    }

    /// Asks the open side of search `generation` of Photon or Nominatim.
    fn ask_open(&mut self, generation: u64, source: SearchSource) {
        let Some(run) = self.search_run.as_mut().filter(|r| r.generation == generation) else { return };
        let query = run.query.clone();
        if source == SearchSource::Nominatim {
            run.nominatim_tried = true;
        }
        let request = self.search_request(&query, 20);
        let built = match source {
            SearchSource::Nominatim => self.nominatim().search_request(&request),
            _ => self.photon().search_request(&request),
        };
        let pending = Pending::Search { generation, source };
        let waiting = match built {
            Ok(r) if source == SearchSource::Nominatim => {
                self.fetch_nominatim(r, pending);
                true
            }
            Ok(r) => {
                self.fetch(r, pending);
                true
            }
            Err(_) => false,
        };
        if let Some(run) = self.search_run.as_mut() {
            run.open = if waiting { Part::Waiting } else { Part::Failed };
        }
    }

    pub(super) fn results_arrived(&mut self, generation: u64, source: SearchSource, response: &HttpResponse) {
        if generation != self.search_generation || self.search_run.as_ref().is_none_or(|r| r.generation != generation) {
            return;
        }
        let parsed: Result<Vec<Place>, String> = match source {
            SearchSource::Google => self.google().parse_search(response).map_err(|e| {
                self.note_google_error(&e);
                e.to_string()
            }),
            SearchSource::Photon => self.photon().parse_search(response).map_err(|e| e.to_string()),
            SearchSource::Nominatim => self.nominatim().parse_search(response).map_err(|e| e.to_string()),
        };
        let label = match source {
            SearchSource::Google => "Google",
            SearchSource::Photon => "OpenStreetMap",
            SearchSource::Nominatim => "Nominatim",
        };
        match parsed {
            Ok(places) => {
                self.search.sources.push(label.into());
                match source {
                    SearchSource::Google => {
                        if let Some(run) = self.search_run.as_mut() {
                            run.google = Part::Done(places);
                        }
                    }
                    SearchSource::Photon => self.ask_branches(generation, places),
                    SearchSource::Nominatim => {
                        if let Some(run) = self.search_run.as_mut() {
                            run.open = Part::Done(places);
                        }
                    }
                }
            }
            Err(e) => {
                self.log(LogLevel::Warning, format!("search ({label}): {e}"));
                self.search.sources.push(format!("{label} ✕"));
                let Some(run) = self.search_run.as_mut() else { return };
                match source {
                    SearchSource::Google => {
                        run.google = Part::Failed;
                        // Google failed: the open data answers instead.
                        if run.open == Part::Off {
                            self.ask_open(generation, SearchSource::Photon);
                        }
                    }
                    _ => {
                        run.open = Part::Failed;
                        let google_answered = matches!(&run.google, Part::Done(p) if !p.is_empty());
                        if !run.nominatim_tried && !google_answered {
                            self.ask_open(generation, SearchSource::Nominatim);
                        }
                    }
                }
            }
        }
        self.finish_search(generation);
    }

    /// Asks Nominatim for the branches of `places`, Photon's answer to search `generation`, while
    /// *Look up details in OpenStreetMap* is on (it sends their ids); else they are the answer as
    /// they are.
    fn ask_branches(&mut self, generation: u64, places: Vec<Place>) {
        let ids: Vec<&str> = places.iter().filter_map(|p| p.id_in(PlaceSource::OpenStreetMap)).collect();
        let request = if self.settings.places.details { self.nominatim().lookup_request(&ids, self.book().tag()).ok() } else { None };
        let Some(run) = self.search_run.as_mut().filter(|r| r.generation == generation) else { return };
        match request {
            Some(request) => {
                run.open = Part::Naming(places);
                self.fetch_nominatim(request, Pending::Branches { generation });
            }
            None => run.open = Part::Done(places),
        }
    }

    /// The branches of the places of search `generation`: each place takes the branch of its
    /// element. When the lookup fails, the places are shown without them.
    pub(super) fn branches_arrived(&mut self, generation: u64, response: &HttpResponse) {
        let found = self.nominatim().parse_lookup(response);
        let Some(run) = self.search_run.as_mut().filter(|r| r.generation == generation && generation == self.search_generation) else { return };
        let Part::Naming(mut places) = std::mem::replace(&mut run.open, Part::Off) else { return };
        let failed = match found {
            Ok(elements) => {
                for place in places.iter_mut() {
                    let element = place.id_in(PlaceSource::OpenStreetMap).and_then(|id| elements.iter().find(|e| e.id == id));
                    if let Some(branch) = element.and_then(|e| e.branch.clone()) {
                        place.set_branch(&branch);
                    }
                }
                None
            }
            Err(e) => Some(e),
        };
        run.open = Part::Done(places);
        if let Some(e) = failed {
            self.log(LogLevel::Info, format!("branches (OpenStreetMap): {e}"));
        }
        self.finish_search(generation);
    }

    /// Shows the results of search `generation` once no side is waiting: Google's places filled
    /// from those of OpenStreetMap that are the same, then those of OpenStreetMap around them
    /// that matched none; or the places of the side that answered.
    fn finish_search(&mut self, generation: u64) {
        let Some(run) = self.search_run.as_mut().filter(|r| r.generation == generation) else { return };
        let waiting = |part: &Part| matches!(part, Part::Waiting | Part::Naming(_));
        if waiting(&run.google) || waiting(&run.open) {
            return;
        }
        let failed = matches!(run.google, Part::Failed | Part::Off) && matches!(run.open, Part::Failed | Part::Off);
        let (google, open) = (run.google.places(), run.open.places());
        self.search_run = None;
        self.search.loading = false;
        let places = if google.is_empty() {
            open
        } else {
            let (mut merged, rest) = PlaceMatch::default().combine(google, open);
            if let Some(area) = Bounds::of(merged.iter().map(|p| &p.coordinate)).map(|b| b.expanded_m(Self::OPEN_RESULTS_AROUND_M)) {
                merged.extend(rest.into_iter().filter(|p| area.contains(&p.coordinate)));
            }
            merged.truncate(Self::MAX_RESULTS);
            merged
        };
        if places.is_empty() {
            let message = if failed { Message::SearchFailed } else { Message::NoResults };
            self.search.error = Some(self.book().message(message).into());
        } else if places.len() == 1 {
            self.search.results = places.clone();
            self.select(places[0].clone());
        } else {
            let points: Vec<Coordinate> = places.iter().map(|p| p.coordinate).collect();
            self.search.results = places;
            self.move_camera(Style::frame(&points, 300.0), None, None);
        }
    }

    // ---- The place selected ----

    pub(super) fn deselect(&mut self) {
        self.selected = None;
        self.selection += 1;
        self.selection_waiting = 0;
    }

    /// Shows `place`, and looks up what its sources know of it.
    pub(super) fn select(&mut self, place: Place) {
        self.places.remember(&place);
        self.save_places();
        let saved = self.places.is_saved(&place.id);
        let distance = self.location.map(|l| Words::distance(l.coordinate.distance_m(&place.coordinate), self.imperial()));
        self.move_camera(None, Some(place.coordinate), Some(16.0));
        self.deselect();
        self.selected = Some(PlaceView { place: place.clone(), saved, distance, loading: false });
        self.look_up(&place);
    }

    /// Looks up `place` in each source the settings allow:
    ///
    /// - in Google, a place of Google without its details, or (combining) a place of
    ///   OpenStreetMap Google has not been asked about, while *Place details* is allowed;
    /// - in OpenStreetMap, the tags of its element (its own, or the one linked), or (combining)
    ///   for a place of Google, the element that is the same place.
    fn look_up(&mut self, place: &Place) {
        let selection = self.selection;
        let combine = self.settings.places.combine;
        let wants_google = match place.source {
            // Unless Google itself gave its hours or phone: those filled from OpenStreetMap (a search
            // combined) are no reason not to ask, since Google's own win over them.
            PlaceSource::Google => [PlaceField::Hours, PlaceField::Phone].iter().all(|f| !place.has(*f) || place.source_of(*f) != PlaceSource::Google),
            PlaceSource::OpenStreetMap => combine && !place.knows(PlaceSource::Google),
            PlaceSource::Coordinate => false,
        };
        if wants_google && self.settings.google.allows(GoogleUse::PlaceDetails) {
            let request = self.google().details_request(place);
            let sent = format!("{} {} @ {}", place.name, place.address.clone().unwrap_or_default(), Self::rounded(&place.coordinate));
            self.fetch_google(GoogleUse::PlaceDetails, sent, request, Pending::Details { selection });
            self.waiting(1);
        }
        match place.id_in(PlaceSource::OpenStreetMap).map(str::to_string) {
            Some(id) => {
                if self.settings.places.details {
                    self.look_up_osm(&id);
                }
            }
            None if combine && place.source == PlaceSource::Google => {
                // The element of OpenStreetMap by its name, within reach of the place.
                let reach = PlaceMatch::default().max_distance_m * 2.0;
                let c = place.coordinate;
                let (dlat, dlon) = (reach / 111_320.0, reach / (111_320.0 * c.latitude.to_radians().cos().max(0.01)));
                let request = SearchRequest {
                    query: place.name.clone(),
                    near: Some(c),
                    bounds: Some(Bounds { south: c.latitude - dlat, west: c.longitude - dlon, north: c.latitude + dlat, east: c.longitude + dlon }),
                    bounded: true,
                    language: self.book().tag().into(),
                    limit: 10,
                };
                if let Ok(r) = self.photon().search_request(&request) {
                    self.fetch(r, Pending::Counterpart { selection });
                    self.waiting(1);
                }
            }
            None => {}
        }
    }

    /// Asks Nominatim for the tags of element `id` of the place selected.
    fn look_up_osm(&mut self, id: &str) {
        let selection = self.selection;
        if let Ok(request) = self.nominatim().lookup_request(&[id], self.book().tag()) {
            self.fetch_nominatim(request, Pending::OsmDetails { selection });
            self.waiting(1);
        }
    }

    /// `n` more lookups of the place selected are waiting.
    fn waiting(&mut self, n: u32) {
        self.selection_waiting += n;
        if let Some(view) = self.selected.as_mut() {
            view.loading = true;
        }
    }

    /// The place selected, when `selection` is still its generation.
    fn selected_place(&mut self, selection: u64) -> Option<&mut Place> {
        if selection != self.selection {
            return None;
        }
        self.selected.as_mut().map(|v| &mut v.place)
    }

    /// One lookup of selection `selection` answered: what the place became is kept in the recent
    /// and saved places, and it stops loading when none is left.
    fn answered(&mut self, selection: u64, changed: bool) {
        if selection != self.selection {
            return;
        }
        self.selection_waiting = self.selection_waiting.saturating_sub(1);
        let Some(view) = self.selected.as_mut() else { return };
        view.loading = self.selection_waiting > 0;
        if !changed {
            return;
        }
        let place = view.place.clone();
        for p in self.places.recent.iter_mut().filter(|p| p.id == place.id) {
            *p = place.clone();
        }
        for s in self.places.saved.iter_mut().filter(|s| s.place.id == place.id) {
            s.place = place.clone();
        }
        self.save_places();
    }

    pub(super) fn pin(&mut self, coordinate: Coordinate) {
        let mut place = Place::at(coordinate);
        place.name = self.book().message(Message::DroppedPin).into();
        place.address = Some(Self::rounded(&coordinate));
        self.deselect();
        let distance = self.location.map(|l| Words::distance(l.coordinate.distance_m(&coordinate), self.imperial()));
        self.selected = Some(PlaceView { place, saved: false, distance, loading: false });
        let selection = self.selection;
        if let Ok(request) = self.nominatim().reverse_request(&coordinate, self.book().tag()) {
            self.fetch_nominatim(request, Pending::Reverse { selection });
            self.waiting(1);
        }
    }

    pub(super) fn reverse_arrived(&mut self, selection: u64, response: &HttpResponse) {
        let result = self.nominatim().parse_reverse(response);
        if let (Some(place), Ok(Some(found))) = (self.selected_place(selection), result) {
            // The point stays where it was dropped; the lookup names it.
            place.name = found.name;
            place.address = found.address.or(place.address.take());
            place.category = found.category;
        }
        // A dropped pin is not kept among the recent places: nothing to update.
        self.answered(selection, false);
    }

    /// Google's details of the place selected: they refresh a place of Google, and fill a place
    /// of OpenStreetMap.
    pub(super) fn details_arrived(&mut self, selection: u64, response: &HttpResponse) {
        let google = self.google();
        let Some(place) = self.selected_place(selection) else { return };
        let mut changed = false;
        match google.parse_details(place, response, &PlaceMatch::default()) {
            Ok(Some(details)) => {
                if place.source == PlaceSource::Google {
                    place.update_from(&details);
                } else {
                    place.fill_from(&details);
                }
                changed = true;
            }
            Ok(None) => {}
            Err(e) => {
                self.note_google_error(&e);
                self.log(LogLevel::Warning, format!("details: {e}"));
            }
        }
        self.answered(selection, changed);
    }

    /// The element of OpenStreetMap that is the place of Google selected: linked and filled from,
    /// and its tags looked up.
    pub(super) fn counterpart_arrived(&mut self, selection: u64, response: &HttpResponse) {
        let found = self.photon().parse_search(response);
        let details = self.settings.places.details;
        let Some(place) = self.selected_place(selection) else { return };
        let mut changed = false;
        let mut look_up = None;
        match found {
            Ok(candidates) => {
                if let Some(same) = PlaceMatch::default().best(place, &candidates) {
                    place.fill_from(same);
                    changed = true;
                    look_up = Some(same.id.clone()).filter(|id| details && PlaceSource::of_id(id) == Some(PlaceSource::OpenStreetMap));
                }
            }
            Err(e) => self.log(LogLevel::Info, format!("counterpart: {e}")),
        }
        if let Some(id) = look_up {
            self.look_up_osm(&id);
        }
        self.answered(selection, changed);
    }

    /// The tags of the element of OpenStreetMap of the place selected: they refresh a place of
    /// OpenStreetMap, and fill a place of Google.
    pub(super) fn osm_details_arrived(&mut self, selection: u64, response: &HttpResponse) {
        let found = self.nominatim().parse_lookup(response);
        let Some(place) = self.selected_place(selection) else { return };
        let mut changed = false;
        match found {
            Ok(elements) => {
                let wanted = place.id_in(PlaceSource::OpenStreetMap).map(str::to_string);
                if let Some(element) = elements.iter().find(|e| Some(&e.id) == wanted.as_ref()).or(elements.first()) {
                    let mut element = element.clone();
                    // The name and position stay those of the place; only what is known of it is taken,
                    // and the branch a place of Photon lacked.
                    if place.source == PlaceSource::OpenStreetMap {
                        element.address = None;
                        place.update_from(&element);
                        if let Some(branch) = &element.branch {
                            place.set_branch(branch);
                        }
                    } else {
                        place.fill_from(&element);
                    }
                    changed = true;
                }
            }
            Err(e) => self.log(LogLevel::Info, format!("details (OpenStreetMap): {e}")),
        }
        self.answered(selection, changed);
    }
}
