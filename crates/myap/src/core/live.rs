//! What the core does with live data during planning and guidance: the traffic of now for Myap
//! Router, the checks for a faster route, the position estimated without fixes, and the areas to
//! avoid.

use super::{Core, Pending, TrafficData};
use crate::navigation::{Event, NavigationState, Session};
use crate::protocol::{Effect, LogLevel};
use crate::settings::{AvoidArea, GoogleUse, RouterKind};
use crate::words::Message;
use fetch::Response as HttpResponse;
use geo::{Bounds, Coordinate, Line, Polygon};
use myap_router::TrafficSpeed;
use routing::{RemoteRouter, Route, RouteError, RouteRequest};
use traffic::TrafficProvider;

impl Core {
    /// The HERE client of the settings, when a key is set.
    fn here(&self) -> Option<traffic::here::Here> {
        self.settings.traffic.enabled().then(|| traffic::here::Here::new(self.settings.traffic.here_api_key.trim().to_string(), Self::USER_AGENT.to_string()))
    }

    /// Whether the traffic held covers `area` and is recent.
    fn traffic_covers(&self, area: &Bounds) -> bool {
        self.traffic.as_ref().is_some_and(|t| {
            self.now - t.at_ms < Self::TRAFFIC_MAX_AGE_MS
                && t.areas.iter().any(|a| a.contains(&Coordinate::new(area.south, area.west)) && a.contains(&Coordinate::new(area.north, area.east)))
        })
    }

    /// Fetches the traffic of the area of `points` (with a margin) for `generation`, unless it is
    /// held or too large. Returns whether a fetch was sent and is to be waited for.
    pub(super) fn fetch_traffic(&mut self, points: &[Coordinate], generation: u64) -> bool {
        let Some(here) = self.here() else { return false };
        let Some(area) = Bounds::of(points).map(|b| b.expanded_m(3_000.0)) else { return false };
        if self.traffic_covers(&area) {
            return false;
        }
        let pieces = match here.pieces(&area, Self::TRAFFIC_MAX_PIECES) {
            Ok(p) => p,
            Err(e) => {
                self.log(LogLevel::Info, format!("traffic: {e}"));
                return false;
            }
        };
        // A new area replaces the old one once it comes.
        self.traffic = Some(TrafficData { at_ms: self.now, areas: Vec::new(), data: traffic::Traffic::default() });
        for piece in pieces {
            for (incidents, request) in [(false, here.flow_request(&piece)), (true, here.incidents_request(&piece))] {
                match request {
                    Ok(request) => {
                        let id = self.fetch(request, Pending::Traffic { generation, incidents });
                        self.traffic_waiting.insert(id, generation);
                        if !incidents {
                            self.traffic_pieces.insert(id, piece);
                        }
                    }
                    Err(e) => self.log(LogLevel::Info, format!("traffic: {}", here.redact(&e.to_string()))),
                }
            }
        }
        self.traffic_waiting.values().any(|g| *g == generation)
    }

    pub(super) fn traffic_arrived(&mut self, id: u64, generation: u64, incidents: bool, response: &HttpResponse) {
        self.traffic_waiting.remove(&id);
        let piece = self.traffic_pieces.remove(&id);
        if let Some(here) = self.here() {
            let parsed = if incidents { here.parse_incidents(response) } else { here.parse_flow(response) };
            match parsed {
                Ok(data) => {
                    if let Some(t) = self.traffic.as_mut() {
                        t.data.merge(data);
                        // An area is held only once its flow came: a failed fetch is tried again.
                        t.areas.extend(piece);
                    }
                }
                Err(e) => {
                    self.log(LogLevel::Warning, format!("traffic: {}", here.redact(&e.to_string())));
                    if self.traffic.as_ref().is_some_and(|t| t.data.flow.is_empty()) {
                        self.tell(self.book().message(Message::TrafficFailed));
                    }
                }
            }
        }
        if self.traffic_waiting.values().any(|g| *g == generation) {
            return;
        }
        self.apply_traffic();
        // The directions waiting for it ask Myap Router now.
        // The directions waiting for it go on, unless another router of theirs is still being
        // asked (Myap Router is then asked if that one fails).
        let asking = self.pending.values().any(|p| matches!(p, Pending::Route { generation: g, .. } if *g == generation));
        if let Some(d) = self.directions.as_mut()
            && d.generation == generation
            && d.waiting_for_traffic
        {
            d.waiting_for_traffic = false;
            if !asking && d.remaining.front() == Some(&RouterKind::Myap) {
                self.try_next_router();
            }
        }
        if self.navigation.as_ref().is_some_and(|n| n.check == generation) {
            self.check_offline(generation);
        }
    }

    /// Gives Myap Router the traffic held: the speed of each line of flow, and the closures of
    /// incidents. Old traffic is taken away.
    pub(super) fn apply_traffic(&mut self) {
        let now = self.now;
        let fresh = self.traffic.as_ref().filter(|t| now - t.at_ms < Self::TRAFFIC_MAX_AGE_MS);
        let speeds: Option<Vec<TrafficSpeed>> = fresh.map(|t| {
            let mut out: Vec<TrafficSpeed> =
                t.data.flow.iter().map(|f| TrafficSpeed { shape: f.shape.clone(), factor: f.speed_factor(), closed: f.closed }).collect();
            for incident in t.data.incidents.iter().filter(|i| i.road_closed && i.is_active_at(now)) {
                out.extend(incident.shape.iter().map(|line| TrafficSpeed { shape: line.clone(), factor: None, closed: true }));
            }
            out
        });
        if let Some((_, router)) = self.router.as_mut() {
            let matched = router.set_traffic(speeds.as_deref());
            if matched > 0 {
                self.log(LogLevel::Info, format!("traffic on {matched} edges"));
            }
        }
    }

    /// Time passed: the position is estimated while fixes are missing (a tunnel), and the route is
    /// compared with the traffic of now when due.
    pub(super) fn tick(&mut self) {
        let now = self.now;
        if self.end_after_arrival() {
            return;
        }
        let Some(nav) = self.navigation.as_mut() else { return };
        let events = nav.session.estimate(now);
        if nav.session.estimated {
            self.effects.push(Effect::Wake { at: now + 1_000 });
        } else if let Some(fix) = nav.session.last_fix_ms {
            // Woken early (for another reason): wake again when an estimate may be due.
            let due = fix + Session::ESTIMATE_AFTER_MS + 500;
            if due > now {
                self.effects.push(Effect::Wake { at: due });
            }
        }
        self.announce(events);
    }

    /// Notes when the destination was reached, and ends the guidance the minutes of the settings
    /// after (waking for it). Returns whether it ended.
    pub(super) fn end_after_arrival(&mut self) -> bool {
        let minutes = self.settings.guidance.end_after_arrival_minutes;
        let now = self.now;
        let Some(nav) = self.navigation.as_mut() else { return false };
        if nav.session.state != NavigationState::Arrived {
            nav.arrived_ms = None;
            return false;
        }
        let arrived = *nav.arrived_ms.get_or_insert(now);
        if minutes == 0 {
            return false;
        }
        let end = arrived + minutes as i64 * 60_000;
        if now >= end {
            self.stop_navigation();
            return true;
        }
        self.effects.push(Effect::Wake { at: end });
        false
    }

    /// Says what the events of the guidance ask, as a fix or an estimate made them.
    pub(super) fn announce(&mut self, events: Vec<Event>) {
        let Some(nav) = self.navigation.as_ref() else { return };
        let steps: Vec<routing::Step> = nav.session.route.steps().cloned().collect();
        let alerts = nav.session.route.alerts.clone();
        let last = steps.len().saturating_sub(1);
        let book = self.book();
        let imperial = self.imperial();
        let units = self.settings.units;
        let words = |i: usize| crate::words::StepWords::of(&steps[i], i == last);
        let mut location = None;
        for event in events {
            match event {
                Event::Prepare { step, distance_m } => {
                    let distance = book.spoken_distance(distance_m, units, imperial);
                    self.speak(book.prepare(&distance, &book.instruction(&words(step))));
                }
                Event::Now { step, then } => {
                    let then = then.map(|t| book.instruction(&words(t)));
                    self.speak(book.now(&book.instruction(&words(step)), then.as_deref()));
                }
                Event::Alert { index, distance_m } => {
                    if self.settings.guidance.alerts
                        && let Some(alert) = alerts.get(index)
                    {
                        self.speak(book.alert(alert.kind, &book.spoken_distance(distance_m, units, imperial)));
                    }
                }
                Event::Arrived => self.speak(book.message(Message::Arrived).to_string()),
                Event::OffRoute => location = self.location,
            }
        }
        if let Some(location) = location {
            self.reroute(location);
        }
    }

    /// During guidance, when due: asks for the route left with the traffic of now, to offer a
    /// faster one. By Myap Router with live traffic when it can, else by Google when its use
    /// during guidance is allowed.
    pub(super) fn check_route(&mut self) {
        let interval = self.settings.traffic.refresh_minutes as i64 * 60_000;
        let Some(location) = self.location else { return };
        let Some(nav) = self.navigation.as_ref() else { return };
        if !self.settings.guidance.faster_routes || nav.session.state != NavigationState::Navigating || self.now - nav.last_check_ms < interval {
            return;
        }
        self.plan_generation += 1;
        let generation = self.plan_generation;
        let mut waypoints = vec![location.coordinate];
        waypoints.extend(nav.targets.iter().copied());
        let mode = nav.session.mode;
        let avoid = nav.avoid;
        let mut request = self.route_request(waypoints, mode, avoid);
        request.heading = location.bearing;
        if let Some(nav) = self.navigation.as_mut() {
            nav.last_check_ms = self.now;
            nav.check = generation;
        }
        let offline = self.regions.covering(&request.waypoints).is_some();
        if offline && self.settings.traffic.enabled() {
            // The traffic of the route left first; Myap Router when it came.
            let mut points = request.waypoints.clone();
            if let Some(nav) = self.navigation.as_ref() {
                points.extend(nav.session.split().1.iter().step_by(20).copied());
            }
            self.traffic = None;
            if !self.fetch_traffic(&points, generation) {
                self.check_offline(generation);
            }
            return;
        }
        if self.settings.google.allows(GoogleUse::NavigationTraffic)
            && let Ok(r) = self.google().directions_request(&request)
        {
            let sent = request.waypoints.iter().map(Self::rounded).collect::<Vec<_>>().join(" → ");
            self.fetch_google(GoogleUse::NavigationTraffic, sent, r, Pending::Check { generation, router: RouterKind::Google, request });
        }
    }

    /// The check of the guidance by Myap Router, with the traffic held.
    pub(super) fn check_offline(&mut self, generation: u64) {
        let Some(location) = self.location else { return };
        let Some(nav) = self.navigation.as_ref() else { return };
        let mut waypoints = vec![location.coordinate];
        waypoints.extend(nav.targets.iter().copied());
        let (mode, avoid) = (nav.session.mode, nav.avoid);
        let mut request = self.route_request(waypoints, mode, avoid);
        request.heading = location.bearing;
        if let Ok(routes) = self.route_offline(&request)
            && let Some(route) = routes.into_iter().next()
        {
            self.compare(generation, RouterKind::Myap, route);
        }
    }

    pub(super) fn check_arrived(&mut self, generation: u64, router: RouterKind, request: &RouteRequest, response: &HttpResponse) {
        let parsed: Result<Vec<Route>, RouteError> = match router {
            RouterKind::Google => self.google().parse(request, response),
            _ => return,
        };
        match parsed {
            Ok(routes) => {
                if let Some(mut route) = routes.into_iter().next() {
                    if let Some(mode) = self.navigation.as_ref().map(|n| n.session.mode) {
                        self.annotate(router, std::slice::from_mut(&mut route), mode);
                    }
                    self.compare(generation, router, route);
                }
            }
            Err(e) => self.log(LogLevel::Info, format!("check: {e}")),
        }
    }

    /// The fraction of `candidate` that runs along the rest of the route followed (within 25 m).
    fn shared_with(session: &Session, candidate: &Route) -> f64 {
        let (_, ahead) = session.split();
        if ahead.len() < 2 {
            return 0.0;
        }
        let line = Line::new(ahead);
        let points = Line::new(candidate.geometry.clone());
        let length = points.length_m();
        if length <= 0.0 {
            return 1.0;
        }
        let samples = ((length / 50.0).ceil() as usize).clamp(2, 400);
        let near = (0..samples)
            .filter_map(|k| points.point_at(length * k as f64 / (samples - 1) as f64))
            .filter(|p| line.project(p, 0.0, f64::INFINITY).is_some_and(|q| q.distance_m <= 25.0))
            .count();
        near as f64 / samples as f64
    }

    /// The time left on the route followed with the live traffic held: the rest of the route at
    /// its own average speed, slowed where a line of traffic runs along it (within 20 m, the same
    /// way). Any router's route can be timed so, and compared with a route found now.
    fn remaining_with_traffic(&self) -> Option<f64> {
        let nav = self.navigation.as_ref()?;
        let usual = nav.session.remaining_s();
        let Some(t) = self.traffic.as_ref().filter(|t| self.now - t.at_ms < Self::TRAFFIC_MAX_AGE_MS && !t.data.flow.is_empty()) else { return Some(usual) };
        let (_, ahead) = nav.session.split();
        let line = Line::new(ahead);
        let length = line.length_m();
        if length <= 0.0 || usual <= 0.0 {
            return Some(usual);
        }
        let now = self.now;
        let mut lines: Vec<(Bounds, Line, f64)> = t
            .data
            .flow
            .iter()
            .filter_map(|f| {
                let factor = if f.closed { 0.05 } else { f.speed_factor()? };
                Some((Bounds::of(&f.shape)?.expanded_m(25.0), Line::new(f.shape.clone()), factor))
            })
            .collect();
        // Closures of incidents, as Myap Router is given them.
        for incident in t.data.incidents.iter().filter(|i| i.road_closed && i.is_active_at(now)) {
            for shape in &incident.shape {
                if let Some(b) = Bounds::of(shape) {
                    lines.push((b.expanded_m(25.0), Line::new(shape.clone()), 0.05));
                }
            }
        }
        let samples = ((length / 100.0).ceil() as usize).clamp(1, 2_000);
        let piece = length / samples as f64;
        let mut seconds = 0.0;
        for k in 0..samples {
            let at = (k as f64 + 0.5) * piece;
            let (Some(p), Some(bearing)) = (line.point_at(at), line.bearing_at(at)) else { continue };
            let factor = lines
                .iter()
                .filter(|(b, _, _)| b.contains(&p))
                .filter_map(|(_, l, f)| {
                    l.project(&p, 0.0, f64::INFINITY)
                        .filter(|q| q.distance_m <= 20.0 && Coordinate::bearing_difference(q.bearing, bearing).abs() <= 35.0)
                        .map(|_| *f)
                })
                .fold(f64::NAN, |a, b| if a.is_nan() { b } else { a.min(b) });
            seconds += usual / samples as f64 / if factor.is_nan() { 1.0 } else { factor.clamp(0.05, 1.5) };
        }
        Some(seconds)
    }

    /// Compares a route found now with the one followed: the same way takes its new times; another
    /// way that saves enough is offered (or taken, when the settings say so).
    fn compare(&mut self, generation: u64, router: RouterKind, candidate: Route) {
        let threshold = self.settings.guidance.faster_route_minutes as f64 * 60.0;
        let accept = self.settings.guidance.accept_faster_routes;
        let current = self.remaining_with_traffic();
        let Some(nav) = self.navigation.as_mut() else { return };
        let current = current.unwrap_or_else(|| nav.session.remaining_s());
        if nav.check != generation || nav.session.state != NavigationState::Navigating || candidate.validate().is_err() {
            return;
        }
        let shared = Self::shared_with(&nav.session, &candidate);
        if shared >= 0.9 {
            // The same way: only the times of now are new (the traffic).
            if candidate.duration_in_traffic_s.is_some() {
                nav.session.replace(candidate);
                nav.router = router;
            }
            nav.faster = None;
            return;
        }
        let saving = current - candidate.expected_duration_s();
        if saving < threshold.max(current * 0.08) {
            return;
        }
        nav.faster = Some((router, candidate, saving));
        if accept {
            self.accept_faster_route();
        } else {
            let text = self.book().faster_route(&self.book().duration(saving));
            self.speak(text);
        }
    }

    pub(super) fn accept_faster_route(&mut self) {
        let Some(nav) = self.navigation.as_mut() else { return };
        let Some((router, route, _)) = nav.faster.take() else { return };
        nav.session.replace(route);
        nav.router = router;
        self.speak(self.book().message(Message::FasterRouteTaken).to_string());
    }

    pub(super) fn add_avoid_area(&mut self, name: String, area: Polygon, until_ms: Option<i64>) {
        if !area.is_valid() {
            return;
        }
        let areas = &mut self.settings.routing.avoid_areas;
        // Areas whose time is up are forgotten; the oldest gives way to a new one past the limit.
        let now = self.now;
        areas.retain(|a| a.until_ms.is_none_or(|u| u > now));
        if areas.len() >= RouteRequest::MAX_AVOID_AREAS {
            areas.remove(0);
        }
        let id = format!("a{:x}", self.now.max(0) as u64 + areas.len() as u64);
        areas.push(AvoidArea { id, name, area, until_ms });
        self.save_settings();
        self.replan();
    }

    /// Plans the directions shown again (the areas to avoid changed); during guidance, finds a new
    /// route from where the traveller is.
    pub(super) fn replan(&mut self) {
        if self.navigation.is_some() {
            if let Some(location) = self.location {
                self.reroute(location);
            }
            return;
        }
        if let Some(d) = self.directions.as_ref() {
            let (origin, destination, stops, mode, avoid, choice) = (d.origin.clone(), d.destination.clone(), d.stops.clone(), d.mode, d.avoid, d.choice);
            self.plan(origin, destination, stops, mode, avoid, choice);
        }
    }

    /// Avoids the road just ahead (from 30 m to 250 m ahead, its width) for three hours, and
    /// finds a new route: for a closure or a jam the traveller sees.
    pub(super) fn avoid_ahead(&mut self) {
        let Some(nav) = self.navigation.as_ref() else { return };
        let (_, ahead) = nav.session.split();
        let line = Line::new(ahead);
        let length = line.length_m();
        if length < 60.0 {
            return;
        }
        let (Some(a), Some(b)) = (line.point_at(30.0), line.point_at(length.min(250.0))) else { return };
        let center = a.lerp(&b, 0.5);
        let radius = a.distance_m(&b) / 2.0 + 15.0;
        let name = self.book().alert_name(routing::AlertKind::Incident).to_string();
        self.add_avoid_area(name, Polygon::circle(&center, radius, 16), Some(self.now + 3 * 3_600_000));
    }
}
