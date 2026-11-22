//! Guidance: starting and stopping it, the locations of the traveller and the new routes when they leave the route.

use super::*;

impl Core {
    pub(super) fn start_navigation(&mut self) {
        let Some(d) = &self.directions else { return };
        let Some((router, route)) = d.routes.get(d.selected).cloned() else { return };
        if route.steps().next().is_none() {
            return;
        }
        let Some(request) = &d.request else { return };
        let targets = request.waypoints[1..].to_vec();
        let mode = d.mode;
        let avoid = d.avoid;
        let mut session = Session::new(route, mode);
        if let Some(location) = self.location {
            session.locate(&location);
        }
        let first = session.steps().first().map(|s| self.book().instruction(&StepWords::of(s, false)));
        self.navigation = Some(Navigation { session, router, targets, avoid, faster: None, last_check_ms: self.now, check: 0, arrived_ms: None });
        self.effects.push(Effect::Navigating { active: true });
        if self.settings.voice {
            let book = self.book();
            let text = match first {
                Some(f) => format!("{}。{}", book.message(Message::StartGuidance), f),
                None => book.message(Message::StartGuidance).to_string(),
            };
            self.speak(text);
        }
    }

    pub(super) fn stop_navigation(&mut self) {
        if self.navigation.take().is_some() {
            self.effects.push(Effect::Navigating { active: false });
        }
        if let Some(d) = &mut self.directions {
            d.reroute = false;
        }
    }

    pub(super) fn speak(&mut self, text: String) {
        if self.settings.voice {
            let language = self.book().tag().to_string();
            self.effects.push(Effect::Speak { text, language });
        }
    }

    pub(super) fn located(&mut self, location: Location) {
        if !location.coordinate.is_valid() {
            return;
        }
        if let Some(last) = self.location {
            // Two providers feed the platform: a fix older than the last one is stale, and a poor
            // fix is not taken over a good one of the last 10 s.
            let poor = location.accuracy_m.is_some_and(|a| a > Self::POOR_ACCURACY_M);
            let last_good = last.accuracy_m.is_none_or(|a| a <= Self::POOR_ACCURACY_M);
            if location.time_ms < last.time_ms || poor && last_good && location.time_ms - last.time_ms < 10_000 {
                return;
            }
        }
        self.location = Some(location);
        if let Some(d) = self.directions.as_ref().filter(|d| d.waiting_for_location) {
            let (origin, destination, stops, mode, avoid, choice) = (d.origin.clone(), d.destination.clone(), d.stops.clone(), d.mode, d.avoid, d.choice);
            self.plan(origin, destination, stops, mode, avoid, choice);
        }
        let Some(nav) = self.navigation.as_mut() else { return };
        // A stop is reached when the traveller comes within 40 m of it; it is not a target of
        // later routes.
        while nav.targets.len() > 1 && nav.targets[0].distance_m(&location.coordinate) <= Self::STOP_REACHED_M {
            nav.targets.remove(0);
        }
        let events = nav.session.locate(&location);
        self.announce(events);
        // Without a fix for a while (a tunnel), the position is estimated from this one.
        self.effects.push(Effect::Wake { at: self.now + Session::ESTIMATE_AFTER_MS + 500 });
        self.check_route();
        self.end_after_arrival();
    }

    /// A new route from where the traveller is, to what is left to reach, by the router of the
    /// current route first.
    pub(super) fn reroute(&mut self, location: Location) {
        let Some(nav) = self.navigation.as_ref() else { return };
        // `targets` holds only what is still to reach: stops are dropped as they are reached
        // (`located`), whatever router made the route and however many legs it has.
        let targets: Vec<Coordinate> = nav.targets.clone();
        let mut waypoints = vec![location.coordinate];
        waypoints.extend(targets.iter().copied());
        let (mode, avoid, first) = (nav.session.mode, nav.avoid, nav.router);
        let mut request = self.route_request(waypoints, mode, avoid);
        request.heading = location.bearing;
        let mut order = self.routers_for(&request, RouterChoice::Automatic);
        order.retain(|k| *k != first);
        order.insert(0, first);
        self.plan_generation += 1;
        let generation = self.plan_generation;
        let Some(d) = self.directions.as_mut() else { return };
        d.generation = generation;
        d.request = Some(request);
        d.remaining = order.into();
        d.failures.clear();
        d.loading = true;
        d.reroute = true;
        d.waiting_for_traffic = false;
        self.speak(self.book().message(Message::Rerouting).to_string());
        self.try_next_router();
    }
}
