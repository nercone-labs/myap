//! The snapshot the platform draws: the views of routes and guidance, and the overlay of the map.

use super::*;

impl Core {
    pub(super) fn step_view(&self, step: &routing::Step, last: bool, distance_m: f64) -> StepView {
        let book = self.book();
        StepView {
            instruction: book.instruction(&StepWords::of(step, last)),
            distance: if distance_m > 0.0 { Words::distance(distance_m, self.imperial()) } else { String::new() },
            icon: Words::icon(step.maneuver.kind, step.maneuver.modifier),
            road: step.name.clone().or(step.road_ref.clone()),
            lanes: step.lanes.clone(),
        }
    }

    /// The notes of what a route could not avoid.
    pub(super) fn notes(&self, route: &Route) -> Vec<String> {
        let book = self.book();
        let a = &route.ignored_avoid;
        let mut notes = Vec::new();
        for (on, message) in [
            (a.tolls, Message::TollsNotAvoided),
            (a.highways, Message::HighwaysNotAvoided),
            (a.ferries, Message::FerriesNotAvoided),
            (a.unpaved, Message::UnpavedNotAvoided),
            (a.steps, Message::StepsNotAvoided),
            (a.level_crossings, Message::LevelCrossingsNotAvoided),
            (route.enters_avoided_area, Message::AvoidedAreaEntered),
        ] {
            if on {
                notes.push(book.message(message).to_string());
            }
        }
        notes
    }

    /// The elevations of a route at `count` points evenly along it.
    pub(super) fn profile(route: &Route, count: usize) -> Vec<f32> {
        if route.elevations.len() != route.geometry.len() || route.geometry.len() < 2 {
            return Vec::new();
        }
        let line = route.line();
        let length = line.length_m();
        (0..count)
            .map(|k| {
                let d = length * k as f64 / (count - 1).max(1) as f64;
                let s = line.segment_at(d);
                let (a, b) = (route.elevations[s], route.elevations[(s + 1).min(route.elevations.len() - 1)]);
                let (start, end) = (line.along_m(s), line.along_m(s + 1));
                let t = if end > start { ((d - start) / (end - start)).clamp(0.0, 1.0) as f32 } else { 0.0 };
                ((a + (b - a) * t) * 10.0).round() / 10.0
            })
            .collect()
    }

    pub(super) fn route_view(&self, router: RouterKind, route: &Route) -> RouteView {
        let book = self.book();
        let imperial = self.imperial();
        let expected = route.expected_duration_s();
        let steps: Vec<&routing::Step> = route.steps().collect();
        let last = steps.len().saturating_sub(1);
        let climb = route.climb_m(3.0);
        // How many of each alert, in the order of their first.
        let mut alerts: Vec<(AlertKind, usize)> = Vec::new();
        for a in &route.alerts {
            match alerts.iter_mut().find(|(k, _)| *k == a.kind) {
                Some((_, n)) => *n += 1,
                None => alerts.push((a.kind, 1)),
            }
        }
        RouteView {
            router,
            duration: book.duration(expected),
            distance: Words::distance(route.distance_m, imperial),
            arrival: Words::clock(self.now + (expected * 1000.0) as i64, self.utc_offset_minutes),
            summary: if route.summary.is_empty() { String::new() } else { book.via(&route.summary) },
            traffic: route.duration_in_traffic_s.is_some(),
            delay_minutes: route.duration_in_traffic_s.map(|t| ((t - route.duration_s) / 60.0).max(0.0).round() as u32),
            has_tolls: route.has_tolls,
            has_ferries: route.has_ferries,
            notes: self.notes(route),
            steps: steps.iter().enumerate().map(|(i, s)| self.step_view(s, i == last, s.distance_m)).collect(),
            distance_m: route.distance_m,
            duration_s: expected,
            climb: climb.map(|(up, _)| format!("↑ {}", Words::distance(up, imperial))),
            descent: climb.map(|(_, down)| format!("↓ {}", Words::distance(down, imperial))),
            profile: Self::profile(route, Self::PROFILE_POINTS),
            alerts: alerts
                .into_iter()
                .map(|(kind, n)| AlertView {
                    icon: Words::alert_icon(kind).into(),
                    name: book.alert_name(kind).into(),
                    distance: format!("×{n}"),
                    distance_m: 0.0,
                })
                .collect(),
        }
    }

    pub(super) fn navigation_view(&self, nav: &Navigation) -> NavigationView {
        let book = self.book();
        let imperial = self.imperial();
        let session = &nav.session;
        let steps = session.steps();
        let last = steps.len().saturating_sub(1);
        let next = session.next_step().unwrap_or(last);
        let distance_m = session.distance_to_step_m(next);
        let remaining_s = session.remaining_s();
        let speed_mps = self.location.and_then(|l| l.speed_mps).filter(|s| s.is_finite() && *s >= 0.0);
        let to_unit = |kmh: f64| if imperial { (kmh / 1.609_344).round() as u16 } else { kmh.round() as u16 };
        let limit = session.speed_limit_kmh().filter(|_| self.settings.guidance.speed_limit);
        let speeding = matches!((limit, speed_mps), (Some(l), Some(v)) if v * 3.6 > l as f64 + 3.0);
        let alert_range = match session.mode {
            TravelMode::Car => 1_000.0,
            _ => 200.0,
        };
        let alerts = if self.settings.guidance.alerts {
            session
                .alerts_ahead(alert_range)
                .into_iter()
                .filter_map(|(i, d)| session.route.alerts.get(i).map(|a| (a.kind, d)))
                .take(3)
                .map(|(kind, d)| AlertView {
                    icon: Words::alert_icon(kind).into(),
                    name: book.alert_name(kind).into(),
                    distance: Words::distance(d.max(0.0), imperial),
                    distance_m: d,
                })
                .collect()
        } else {
            Vec::new()
        };
        let upcoming = (next + 1..steps.len()).take(4).map(|i| self.step_view(steps[i], i == last, session.distance_to_step_m(i))).collect();
        let congestion =
            session.congestion_ahead(5_000.0).map(|(d, length, _)| book.congestion(&Words::distance(d, imperial), &Words::distance(length, imperial)));
        let delay_minutes = session.route.duration_in_traffic_s.map(|t| ((t - session.route.duration_s) / 60.0).max(0.0).round() as u32);
        let faster_route = nav.faster.as_ref().map(|(router, route, saving)| FasterRouteView {
            text: book.faster_route(&book.duration(*saving)),
            saving_minutes: (saving / 60.0).round() as u32,
            summary: if route.summary.is_empty() { String::new() } else { book.via(&route.summary) },
            router: *router,
        });
        let (zoom, tilt) = session.camera(speed_mps);
        let points = session.ahead(250.0, 4.0);
        let maneuver = Some(distance_m).filter(|d| *d <= 250.0).map(|d| ((d / 4.0).round() as usize).min(points.len().saturating_sub(1)));
        let eye_height_m = if session.mode == TravelMode::Car { 1.3 } else { 1.5 };
        let step = steps.get(next).copied();
        NavigationView {
            state: session.state,
            instruction: match (session.state, step) {
                (NavigationState::Arrived, _) => book.message(Message::Arrived).into(),
                (_, Some(step)) => book.instruction(&StepWords::of(step, next == last)),
                (_, None) => String::new(),
            },
            icon: step.map_or_else(|| Words::icon(routing::ManeuverKind::Continue, None), |s| Words::icon(s.maneuver.kind, s.maneuver.modifier)),
            road: step.and_then(|s| s.name.clone().or(s.road_ref.clone())),
            distance: Words::distance(distance_m, imperial),
            distance_m,
            then_icon: steps
                .get(next + 1)
                .filter(|_| session.distance_to_step_m(next + 1) - distance_m < 150.0)
                .map(|s| Words::icon(s.maneuver.kind, s.maneuver.modifier)),
            remaining_distance: Words::distance(session.remaining_m(), imperial),
            remaining_time: book.duration(remaining_s),
            arrival: Words::clock(self.now + (remaining_s * 1000.0) as i64, self.utc_offset_minutes),
            remaining_m: session.remaining_m(),
            remaining_s,
            arrival_ms: self.now + (remaining_s * 1000.0) as i64,
            driving_side: self.settings.routing.side(&self.region_code()),
            progress: if session.length_m() > 0.0 { session.progress_m / session.length_m() } else { 1.0 },
            router: nav.router,
            position: session.snapped,
            bearing: session.bearing,
            mode: session.mode,
            lanes: step.map(|s| s.lanes.clone()).unwrap_or_default(),
            junction: step.and_then(|s| s.junction.clone()),
            destinations: step.and_then(|s| s.destinations.as_deref().map(|d| book.destinations(d))),
            exit_number: step.and_then(|s| s.exit_number.clone()),
            speed_limit: limit.map(|l| to_unit(l as f64)),
            speed: speed_mps.map(|v| to_unit(v * 3.6)),
            speeding,
            alerts,
            upcoming,
            congestion,
            delay_minutes,
            faster_route,
            estimated: session.estimated,
            zoom,
            tilt,
            ar: ArView { points, maneuver, eye_height_m },
        }
    }

    /// Whether the map is drawn dark: by the system, or by the Sun where the device is.
    pub(super) fn night(&self) -> bool {
        match self.settings.guidance.night_mode {
            NightMode::System => self.dark,
            NightMode::Always => true,
            NightMode::Never => false,
            NightMode::Sun => match self.location.map(|l| l.coordinate).or(self.viewport.map(|v| v.0)) {
                Some(here) => Sun::is_down(&here, self.now),
                None => self.dark,
            },
        }
    }

    /// The parts of `route` by their congestion, and its alerts, for the map.
    pub(super) fn route_extras(route: &Route, from_index: usize, features: &mut Vec<serde_json::Value>) {
        for span in route.congestion.iter().filter(|s| s.end > from_index) {
            if matches!(span.value, routing::Congestion::Free | routing::Congestion::Unknown) {
                continue;
            }
            let start = span.start.max(from_index);
            if let Some(points) = route.geometry.get(start..=span.end.min(route.geometry.len() - 1))
                && points.len() >= 2
            {
                features.push(Style::line(FeatureKind::RouteTraffic, points, json!({ "congestion": span.value })));
            }
        }
        for alert in route.alerts.iter().filter(|a| a.index >= from_index) {
            features.push(Style::point(FeatureKind::Alert, &alert.location, json!({ "icon": Words::alert_icon(alert.kind) })));
        }
    }

    pub(super) fn overlay(&self) -> String {
        let mut features = Vec::new();
        for area in self.settings.routing.avoid_areas.iter().filter(|a| a.until_ms.is_none_or(|u| self.now < u)) {
            features.push(Style::polygon(FeatureKind::AvoidArea, &area.area.ring, json!({ "id": area.id, "name": area.name })));
        }
        for saved in &self.places.saved {
            features.push(Style::point(FeatureKind::Saved, &saved.place.coordinate, json!({"name": saved.place.name, "label": saved.label})));
        }
        if let Some(nav) = &self.navigation {
            let (passed, ahead) = nav.session.split();
            if passed.len() >= 2 {
                features.push(Style::line(FeatureKind::RoutePassed, &passed, json!({})));
            }
            if ahead.len() >= 2 {
                features.push(Style::line(FeatureKind::Route, &ahead, json!({})));
            }
            Self::route_extras(&nav.session.route, nav.session.index(), &mut features);
            if let Some((_, route, _)) = &nav.faster {
                features.push(Style::line(FeatureKind::RouteAlternative, &route.geometry, json!({ "index": -1 })));
            }
            if let Some(end) = nav.targets.last() {
                features.push(Style::point(FeatureKind::Destination, end, json!({})));
            }
            for stop in &nav.targets[..nav.targets.len().saturating_sub(1)] {
                features.push(Style::point(FeatureKind::Stop, stop, json!({})));
            }
            return Style::collection(features);
        }
        if let Some(d) = &self.directions {
            for (i, (_, route)) in d.routes.iter().enumerate() {
                if i != d.selected {
                    features.push(Style::line(FeatureKind::RouteAlternative, &route.geometry, json!({"index": i})));
                }
            }
            if let Some((_, route)) = d.routes.get(d.selected) {
                features.push(Style::line(FeatureKind::Route, &route.geometry, json!({"index": d.selected})));
                Self::route_extras(route, 0, &mut features);
            }
            if let Some(request) = &d.request {
                let n = request.waypoints.len();
                for (i, p) in request.waypoints.iter().enumerate() {
                    let kind = if i == 0 {
                        FeatureKind::Origin
                    } else if i + 1 == n {
                        FeatureKind::Destination
                    } else {
                        FeatureKind::Stop
                    };
                    features.push(Style::point(kind, p, json!({})));
                }
            }
            return Style::collection(features);
        }
        for (i, place) in self.search.results.iter().enumerate() {
            features.push(Style::point(FeatureKind::Result, &place.coordinate, json!({"index": i, "name": place.name, "id": place.id})));
        }
        if let Some(s) = &self.selected {
            features.push(Style::point(FeatureKind::Selected, &s.place.coordinate, json!({"name": s.place.name, "id": s.place.id})));
        }
        Style::collection(features)
    }

    pub fn snapshot(&self) -> Snapshot {
        let book = self.book();
        let directions = self.directions.as_ref().map(|d| {
            let available = d.request.as_ref().map(|r| self.routers_for(r, RouterChoice::Automatic)).unwrap_or_default();
            DirectionsView {
                endpoints: std::iter::once(d.origin.clone()).chain(d.stops.iter().cloned()).chain(std::iter::once(d.destination.clone())).collect(),
                origin: self.endpoint_name(&d.origin),
                destination: self.endpoint_name(&d.destination),
                stops: d.stops.iter().map(|s| self.endpoint_name(s)).collect(),
                mode: d.mode,
                avoid: d.avoid,
                optimize: self.settings.routing.optimize,
                router: d.choice,
                routes: d.routes.iter().map(|(k, r)| self.route_view(*k, r)).collect(),
                selected: d.selected,
                loading: d.loading,
                error: d.error.clone(),
                notice: d.notice.clone(),
                available,
            }
        });
        let mut counts: Vec<GoogleCount> = Vec::new();
        for record in &self.google_log.records {
            match counts.iter_mut().find(|c| c.purpose == record.purpose) {
                Some(c) => c.count += 1,
                None => counts.push(GoogleCount { purpose: record.purpose, count: 1 }),
            }
        }
        let mut selected = self.selected.clone();
        if let Some(s) = &mut selected {
            s.saved = self.places.is_saved(&s.place.id);
        }
        Snapshot {
            settings: self.settings.clone(),
            language: book.tag().into(),
            imperial: self.imperial(),
            location: self.location,
            search: {
                let mut search = self.search.clone();
                if let Some(here) = self.location.map(|l| l.coordinate) {
                    search.distances = search.results.iter().map(|p| Words::distance(here.distance_m(&p.coordinate), self.imperial())).collect();
                }
                search
            },
            selected,
            directions,
            navigation: self.navigation.as_ref().map(|n| self.navigation_view(n)),
            saved: self.places.saved.clone(),
            recent: self.places.recent.iter().take(10).cloned().collect(),
            recent_searches: self.places.searches.iter().take(10).cloned().collect(),
            regions: self.regions.regions.clone(),
            google: GoogleView { records: self.google_log.records.iter().take(100).cloned().collect(), counts, last_error: self.google_error.clone() },
            style: Style::map(&self.settings, self.night(), book.tag(), &self.region_code()),
            overlay: self.overlay(),
            camera: self.camera.clone(),
            message: self.message.clone(),
        }
    }

    /// The label of a saved place, for screens.
    pub fn label_name(&self, label: SavedLabel) -> &'static str {
        match label {
            SavedLabel::Home => self.book().message(Message::Home),
            SavedLabel::Work => self.book().message(Message::Work),
            SavedLabel::Favorite => "★",
        }
    }
}
