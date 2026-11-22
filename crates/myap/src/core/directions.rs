//! The directions: asking the routers in turn, with fallback, and the routes found.

use super::*;

impl Core {
    /// Closes the directions, as a new search or place does; never during guidance, which needs
    /// them to find a new route.
    pub(super) fn close_directions(&mut self) {
        if self.navigation.is_none() {
            self.directions = None;
        }
    }

    pub(super) fn resolve(&self, endpoint: &Endpoint) -> Option<Coordinate> {
        match endpoint {
            Endpoint::MyLocation => self.location.map(|l| l.coordinate),
            Endpoint::Place { place } => Some(place.coordinate),
        }
    }

    pub(super) fn endpoint_name(&self, endpoint: &Endpoint) -> String {
        match endpoint {
            Endpoint::MyLocation => self.book().message(Message::MyLocation).into(),
            Endpoint::Place { place } => place.name.clone(),
        }
    }

    /// The routers that can answer `request`, in the order to try them.
    pub(super) fn routers_for(&self, request: &RouteRequest, choice: RouterChoice) -> Vec<RouterKind> {
        let available = |kind: RouterKind| match kind {
            RouterKind::Google => self.settings.google.allows(GoogleUse::Directions) && request.mode != TravelMode::Transit,
            RouterKind::Valhalla => !self.settings.servers.valhalla.is_empty(),
            RouterKind::Osrm => match request.mode {
                TravelMode::Car => !self.settings.servers.osrm_car.is_empty(),
                TravelMode::Bicycle => !self.settings.servers.osrm_bicycle.is_empty(),
                TravelMode::Foot => !self.settings.servers.osrm_foot.is_empty(),
                TravelMode::Transit => false,
            },
            RouterKind::Myap => request.mode != TravelMode::Transit && self.regions.covering(&request.waypoints).is_some(),
        };
        match choice {
            RouterChoice::Only(kind) => vec![kind],
            RouterChoice::Automatic => {
                let mut order: Vec<RouterKind> = self.settings.routing.order.iter().copied().filter(|k| available(*k)).collect();
                if self.settings.routing.prefer_offline
                    && let Some(i) = order.iter().position(|k| *k == RouterKind::Myap)
                {
                    let myap = order.remove(i);
                    order.insert(0, myap);
                }
                order
            }
        }
    }

    pub(super) fn plan(&mut self, origin: Endpoint, destination: Endpoint, stops: Vec<Endpoint>, mode: TravelMode, avoid: routing::Avoid, choice: RouterChoice) {
        self.settings.routing.mode = mode;
        self.settings.routing.avoid = avoid;
        self.save_settings();
        self.plan_generation += 1;
        let generation = self.plan_generation;
        let points: Option<Vec<Coordinate>> =
            std::iter::once(&origin).chain(stops.iter()).chain(std::iter::once(&destination)).map(|e| self.resolve(e)).collect();
        let mut directions = Directions {
            origin,
            destination,
            stops,
            mode,
            avoid,
            choice,
            request: None,
            routes: Vec::new(),
            selected: 0,
            loading: true,
            error: None,
            notice: None,
            remaining: VecDeque::new(),
            failures: Vec::new(),
            generation,
            reroute: false,
            waiting_for_traffic: false,
            waiting_for_location: false,
        };
        let Some(points) = points else {
            // The location is not known yet: planned when it comes (the screen says it waits).
            directions.waiting_for_location = true;
            directions.notice = Some(self.book().message(Message::Locating).into());
            self.directions = Some(directions);
            return;
        };
        let mut request = self.route_request(points, mode, avoid);
        request.alternatives = self.settings.routing.alternatives;
        directions.remaining = self.routers_for(&request, choice).into();
        if directions.remaining.contains(&RouterKind::Myap) && self.fetch_traffic(&request.waypoints, generation) {
            directions.waiting_for_traffic = true;
        }
        directions.request = Some(request);
        if let Some(Endpoint::Place { place }) = Some(&directions.destination) {
            self.places.remember(place);
            self.save_places();
        }
        self.directions = Some(directions);
        self.try_next_router();
    }

    /// A request through `points` by `mode`, with what the settings ask to avoid and choose.
    pub(super) fn route_request(&self, points: Vec<Coordinate>, mode: TravelMode, avoid: routing::Avoid) -> RouteRequest {
        let routing = &self.settings.routing;
        let mut request = RouteRequest::new(points, mode);
        request.avoid = avoid;
        request.optimize = routing.optimize;
        request.driving_side = routing.side(&self.region_code());
        request.avoid_areas = routing.areas_at(self.now);
        request.language = self.book().tag().into();
        request
    }

    /// Asks the next router of the directions, until one is asked over the network or answers.
    pub(super) fn try_next_router(&mut self) {
        loop {
            let Some(d) = self.directions.as_mut() else { return };
            let Some(request) = d.request.clone() else { return };
            let generation = d.generation;
            if d.waiting_for_traffic && d.remaining.front() == Some(&RouterKind::Myap) {
                // Myap Router is asked once the traffic of now came (or failed).
                return;
            }
            let Some(router) = d.remaining.pop_front() else {
                d.loading = false;
                let error = Self::worst_error(&d.failures);
                let text = self.error_text(error.as_ref());
                if let Some(d) = self.directions.as_mut() {
                    d.error = Some(text.clone());
                }
                if let Some(nav) = self.navigation.as_mut() {
                    // The reroute failed: guidance stays on the old route.
                    nav.session.state = NavigationState::Navigating;
                }
                return;
            };
            let outcome: Result<Option<Vec<Route>>, RouteError> = match router {
                RouterKind::Myap => self.route_offline(&request).map(Some),
                RouterKind::Osrm => {
                    let s = &self.settings.servers;
                    let server = |base: &str| (!base.is_empty()).then(|| osrm::Server { base: base.to_string(), profile: s.osrm_profile.clone() });
                    let client = osrm::Osrm {
                        car: server(&s.osrm_car),
                        bicycle: server(&s.osrm_bicycle),
                        foot: server(&s.osrm_foot),
                        user_agent: Self::USER_AGENT.into(),
                    };
                    client.request(&request).map(|r| {
                        self.fetch(r, Pending::Route { generation, router, request: request.clone() });
                        None
                    })
                }
                RouterKind::Valhalla => {
                    let client = valhalla::Valhalla { base: self.settings.servers.valhalla.clone(), user_agent: Self::USER_AGENT.into() };
                    client.request(&request).map(|r| {
                        self.fetch(r, Pending::Route { generation, router, request: request.clone() });
                        None
                    })
                }
                RouterKind::Google => {
                    if !self.settings.google.allows(GoogleUse::Directions) {
                        Err(RouteError::UnsupportedMode)
                    } else {
                        self.google().directions_request(&request).map(|r| {
                            let sent = request.waypoints.iter().map(Self::rounded).collect::<Vec<_>>().join(" → ");
                            self.fetch_google(GoogleUse::Directions, sent, r, Pending::Route { generation, router, request: request.clone() });
                            None
                        })
                    }
                }
            };
            match outcome {
                Ok(None) => return,
                Ok(Some(routes)) => {
                    self.routes_found(router, routes);
                    return;
                }
                Err(e) => {
                    self.log(LogLevel::Info, format!("{router:?}: {e}"));
                    if let Some(d) = self.directions.as_mut() {
                        d.failures.push((router, e));
                    }
                }
            }
        }
    }

    /// The error to tell of failures: the most telling one (no route over a network error).
    pub(super) fn worst_error(failures: &[(RouterKind, RouteError)]) -> Option<RouteError> {
        let rank = |e: &RouteError| match e {
            RouteError::NoRoute => 0,
            RouteError::NoRoad => 1,
            RouteError::UnsupportedMode => 2,
            RouteError::OutOfCoverage => 3,
            RouteError::Server(_) | RouteError::Parse(_) | RouteError::InvalidRequest(_) => 4,
            RouteError::Network(_) => 5,
        };
        failures.iter().map(|(_, e)| e).min_by_key(|e| rank(e)).cloned()
    }

    pub(super) fn error_text(&self, error: Option<&RouteError>) -> String {
        let book = self.book();
        book.message(match error {
            None => Message::NoRouter,
            Some(RouteError::NoRoute) => Message::NoRoute,
            Some(RouteError::NoRoad) => Message::NoRoad,
            Some(RouteError::OutOfCoverage) => Message::OutOfCoverage,
            Some(RouteError::UnsupportedMode) => Message::UnsupportedMode,
            Some(RouteError::Network(_)) => Message::NetworkError,
            Some(_) => Message::ServerError,
        })
        .into()
    }

    /// Myap Router on the region that holds every one of `points`, its graph read when needed.
    pub(super) fn offline_router(&mut self, points: &[Coordinate]) -> Result<&mut Router, RouteError> {
        let region = self.regions.covering(points).cloned().ok_or(RouteError::OutOfCoverage)?;
        if self.router.as_ref().is_none_or(|(id, _)| *id != region.id) {
            let path = self.region_path(&region.id).ok_or(RouteError::OutOfCoverage)?;
            // The graph of another region is let go before this one is read: a phone holds one.
            self.router = None;
            let mut file = std::fs::File::open(&path).map_err(|e| RouteError::Server(e.to_string()))?;
            let graph = match Graph::read(&mut file) {
                Ok(graph) => graph,
                // An error of reading (the storage, not the file) is not the region's fault.
                Err(GraphError::Io(e)) => return Err(RouteError::Server(e.to_string())),
                Err(e) => {
                    // A graph of an older version, or damaged: the region is to be downloaded again.
                    if let Some(r) = self.regions.regions.iter_mut().find(|r| r.id == region.id) {
                        r.status = RegionStatus::Failed;
                        r.error = Some(e.to_string());
                    }
                    self.save_regions();
                    return Err(RouteError::OutOfCoverage);
                }
            };
            self.router = Some((region.id.clone(), Router::new(graph)));
            self.apply_traffic();
        }
        self.router.as_mut().map(|(_, router)| router).ok_or(RouteError::OutOfCoverage)
    }

    pub(super) fn route_offline(&mut self, request: &RouteRequest) -> Result<Vec<Route>, RouteError> {
        self.offline_router(&request.waypoints)?.route(request)
    }

    /// Adds to the routes of `router` (not Myap Router's own) what a downloaded region holding
    /// them knows and they lack, when the settings say so.
    pub(super) fn annotate(&mut self, router: RouterKind, routes: &mut [Route], mode: TravelMode) {
        if router == RouterKind::Myap || !self.settings.routing.annotate {
            return;
        }
        for route in routes {
            let (Some(first), Some(last)) = (route.geometry.first().copied(), route.geometry.last().copied()) else { continue };
            let Ok(offline) = self.offline_router(&[first, last]) else { return };
            let added = offline.annotate(route, mode);
            self.log(
                LogLevel::Info,
                format!(
                    "annotated a route of {router:?}: {:.0}% matched, speed limits {}, {} alerts, {} junctions",
                    added.matched_fraction * 100.0,
                    added.speed_limits,
                    added.alerts,
                    added.junctions
                ),
            );
        }
    }

    pub(super) fn route_arrived(&mut self, generation: u64, router: RouterKind, request: &RouteRequest, response: &HttpResponse) {
        if self.directions.as_ref().is_none_or(|d| d.generation != generation) {
            return;
        }
        let parsed = match router {
            RouterKind::Osrm => {
                let s = &self.settings.servers;
                let server = |base: &str| (!base.is_empty()).then(|| osrm::Server { base: base.to_string(), profile: s.osrm_profile.clone() });
                osrm::Osrm { car: server(&s.osrm_car), bicycle: server(&s.osrm_bicycle), foot: server(&s.osrm_foot), user_agent: Self::USER_AGENT.into() }
                    .parse(request, response)
            }
            RouterKind::Valhalla => {
                valhalla::Valhalla { base: self.settings.servers.valhalla.clone(), user_agent: Self::USER_AGENT.into() }.parse(request, response)
            }
            RouterKind::Google => {
                let parsed = self.google().parse(request, response);
                if let Err(RouteError::Server(e)) = &parsed {
                    self.google_error = Some(e.clone());
                }
                parsed
            }
            RouterKind::Myap => Err(RouteError::Parse("not a remote router".into())),
        };
        match parsed {
            Ok(routes) => self.routes_found(router, routes),
            Err(e) => {
                self.log(LogLevel::Info, format!("{router:?}: {e}"));
                if let Some(d) = self.directions.as_mut() {
                    d.failures.push((router, e));
                }
                self.try_next_router();
            }
        }
    }

    pub(super) fn router_name(kind: RouterKind) -> &'static str {
        match kind {
            RouterKind::Myap => "Myap Router",
            RouterKind::Osrm => "OSRM",
            RouterKind::Valhalla => "Valhalla",
            RouterKind::Google => "Google",
        }
    }

    pub(super) fn routes_found(&mut self, router: RouterKind, mut routes: Vec<Route>) {
        if let Some(mode) = self.directions.as_ref().map(|d| d.mode) {
            self.annotate(router, &mut routes, mode);
        }
        let book = self.book();
        let areas: Vec<Polygon> =
            self.directions.as_ref().and_then(|d| d.request.as_ref()).map(|r| r.effective_avoid_areas().into_iter().cloned().collect()).unwrap_or_default();
        for route in &mut routes {
            route.enters_avoided_area |= areas.iter().any(|a| a.touches_line(&route.geometry));
        }
        let Some(d) = self.directions.as_mut() else { return };
        let notice = d.failures.first().map(|(first, e)| {
            let reason = format!("{}: {}", Self::router_name(*first), e);
            book.fallback(Self::router_name(router), &reason)
        });
        d.loading = false;
        d.error = None;
        d.notice = notice;
        d.routes = routes.into_iter().map(|r| (router, r)).collect();
        d.selected = 0;
        if d.reroute {
            if let (Some(nav), Some((kind, route))) = (self.navigation.as_mut(), d.routes.first().cloned()) {
                nav.session.replace(route);
                nav.router = kind;
                // An offer made from where the traveller was is stale now.
                nav.faster = None;
            }
            return;
        }
        let points: Vec<Coordinate> = d.routes.iter().flat_map(|(_, r)| r.geometry.iter().copied()).collect();
        self.move_camera(Style::frame(&points, 200.0), None, None);
    }
}
