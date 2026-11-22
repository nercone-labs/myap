use geo::{Coordinate, Line};
use routing::{AlertKind, Congestion, ManeuverKind, Route, Span, TravelMode};
use serde::{Deserialize, Serialize};

/// A location of the device.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub coordinate: Coordinate,
    /// The radius of 68 % confidence, in metres.
    #[serde(default)]
    pub accuracy_m: Option<f64>,
    /// The direction of travel, in degrees.
    #[serde(default)]
    pub bearing: Option<f64>,
    #[serde(default)]
    pub speed_mps: Option<f64>,
    /// When it was measured, in milliseconds since the epoch.
    pub time_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationState {
    Navigating,
    /// Off the route: a new one is being found.
    Rerouting,
    Arrived,
}

/// The announcements of a maneuver, each made once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Stage {
    Prepare,
    Now,
}

/// What a location changed in the navigation.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Announce the maneuver of step `step`: before it, or at it.
    Prepare {
        step: usize,
        distance_m: f64,
    },
    Now {
        step: usize,
        then: Option<usize>,
    },
    /// Announce alert `index` of the route (a level crossing, a speed camera…) ahead.
    Alert {
        index: usize,
        distance_m: f64,
    },
    /// The traveller left the route: find a new one from here.
    OffRoute,
    Arrived,
}

/// A point of the route ahead, around the traveller: metres east, north and up of them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalPoint {
    pub east: f64,
    pub north: f64,
    pub up: f64,
}

/// The guidance along one route: where the traveller is on it, what comes next, and when to say
/// what.
#[derive(Debug, Clone)]
pub struct Session {
    pub route: Route,
    pub mode: TravelMode,
    line: Line,
    /// The distance along the route to the start of each step (all legs, in order).
    step_starts: Vec<f64>,
    /// Whether each step is the arrival at the last waypoint.
    pub progress_m: f64,
    pub state: NavigationState,
    /// The location of the last fix, on the route.
    pub snapped: Option<Coordinate>,
    pub bearing: Option<f64>,
    off_route_fixes: u32,
    announced: Vec<(usize, Stage)>,
    /// The alerts already announced.
    alerts_announced: Vec<usize>,
    /// The distance along the route to each alert.
    alert_along: Vec<f64>,
    /// When a new route was last asked for, so that rerouting is not asked for at every fix.
    pub last_reroute_ms: Option<i64>,
    /// The last fix on the route: when, and how fast, for estimating the position without fixes.
    pub last_fix_ms: Option<i64>,
    pub last_speed_mps: Option<f64>,
    /// Whether the position is estimated (no fix for a while: a tunnel).
    pub estimated: bool,
    /// The progress at the last fix, and how far an estimate may go from it: a little past the
    /// maneuver that was next then.
    fix_progress_m: f64,
    fix_limit_m: f64,
}

impl Session {
    /// The distance from the route beyond which a fix is off it: at least this, more with a poor
    /// accuracy.
    const OFF_ROUTE_M: f64 = 35.0;
    /// How many fixes in a row off the route make it left (a single bad fix does not).
    const OFF_ROUTE_FIXES: u32 = 3;
    /// The distance from the destination at which it is reached.
    const ARRIVED_M: f64 = 25.0;
    /// The least time between two requests for a new route.
    pub const REROUTE_INTERVAL_MS: i64 = 10_000;

    pub fn new(route: Route, mode: TravelMode) -> Session {
        let line = route.line();
        let step_starts = route.steps().map(|s| line.along_m(s.geometry_start)).collect();
        // Where each alert lies on the segment that starts at its point: a router's geometry may
        // be sparse, and the point alone can be far before the alert.
        let alert_along = route
            .alerts
            .iter()
            .map(|a| {
                let at = line.along_m(a.index);
                line.project(&a.location, at, line.along_m(a.index + 1)).filter(|p| p.distance_m <= 50.0).map_or(at, |p| p.along_m.max(at))
            })
            .collect();
        Session {
            route,
            mode,
            line,
            step_starts,
            progress_m: 0.0,
            state: NavigationState::Navigating,
            snapped: None,
            bearing: None,
            off_route_fixes: 0,
            announced: Vec::new(),
            alerts_announced: Vec::new(),
            alert_along,
            last_reroute_ms: None,
            last_fix_ms: None,
            last_speed_mps: None,
            estimated: false,
            fix_progress_m: 0.0,
            fix_limit_m: 0.0,
        }
    }

    /// The longest the position is estimated without a fix, in milliseconds.
    pub const ESTIMATE_MAX_MS: i64 = 90_000;
    /// A fix older than this, in milliseconds, makes the position estimated.
    pub const ESTIMATE_AFTER_MS: i64 = 3_000;

    /// Moves the position along the route at the last speed when no fix came for a while (a
    /// tunnel), for at most `ESTIMATE_MAX_MS`; returns whether it moved. Announcements are made
    /// as for fixes.
    pub fn estimate(&mut self, now_ms: i64) -> Vec<Event> {
        let (Some(fix), Some(speed)) = (self.last_fix_ms, self.last_speed_mps) else { return Vec::new() };
        let gap = now_ms - fix;
        if self.state != NavigationState::Navigating || !(Self::ESTIMATE_AFTER_MS..=Self::ESTIMATE_MAX_MS).contains(&gap) || speed < 1.0 {
            self.estimated = false;
            return Vec::new();
        }
        self.estimated = true;
        let progress = (self.fix_progress_m + speed * gap as f64 / 1000.0).min(self.length_m());
        // Never past the maneuver that was next at the last fix by more than a little: an
        // estimate cannot see the turn.
        self.progress_m = self.progress_m.max(progress.min(self.fix_limit_m));
        self.snapped = self.line.point_at(self.progress_m);
        self.bearing = self.line.bearing_at(self.progress_m);
        self.announcements(speed)
    }

    /// The point of the geometry the traveller is on.
    pub fn index(&self) -> usize {
        self.line.segment_at(self.progress_m)
    }

    /// The speed limit where the traveller is, in km/h, when known.
    pub fn speed_limit_kmh(&self) -> Option<u16> {
        self.route.speed_limit_at(self.index())
    }

    /// The alerts ahead within `within_m`, with their distance, nearest first.
    pub fn alerts_ahead(&self, within_m: f64) -> Vec<(usize, f64)> {
        self.alert_along.iter().enumerate().map(|(i, a)| (i, a - self.progress_m)).filter(|(_, d)| *d >= -5.0 && *d <= within_m).collect()
    }

    /// The next congestion ahead (heavy or worse) within `within_m`: its distance and length.
    pub fn congestion_ahead(&self, within_m: f64) -> Option<(f64, f64, Congestion)> {
        let here = self.index();
        self.route
            .congestion
            .iter()
            .filter(|s: &&Span<Congestion>| s.end >= here && matches!(s.value, Congestion::Heavy | Congestion::Stopped | Congestion::Closed))
            .find_map(|s| {
                let start = self.line.along_m(s.start).max(self.progress_m);
                let end = self.line.along_m(s.end);
                let distance = start - self.progress_m;
                (distance <= within_m && end > start).then_some((distance, end - start, s.value))
            })
    }

    /// The route ahead from the position, `step_m` apart up to `ahead_m`, around the traveller
    /// (for drawing it over the camera): `up` from the elevations when known, else 0.
    pub fn ahead(&self, ahead_m: f64, step_m: f64) -> Vec<LocalPoint> {
        let Some(origin) = self.snapped.or_else(|| self.line.point_at(self.progress_m)) else { return Vec::new() };
        let base = self.elevation_at(self.progress_m);
        let end = (self.progress_m + ahead_m).min(self.length_m());
        let mut out = Vec::new();
        let mut d = self.progress_m;
        while d <= end + 1e-9 && out.len() < 400 {
            if let Some(p) = self.line.point_at(d) {
                let (east, north) = origin.offset_m(&p);
                let up = match (base, self.elevation_at(d)) {
                    (Some(b), Some(h)) => h - b,
                    _ => 0.0,
                };
                out.push(LocalPoint { east, north, up });
            }
            d += step_m.max(0.5);
        }
        out
    }

    /// The elevation of the route at `along_m`, when known.
    fn elevation_at(&self, along_m: f64) -> Option<f64> {
        if self.route.elevations.len() != self.route.geometry.len() {
            return None;
        }
        let s = self.line.segment_at(along_m);
        let (a, b) = (*self.route.elevations.get(s)? as f64, *self.route.elevations.get(s + 1).or(self.route.elevations.get(s))? as f64);
        let (start, end) = (self.line.along_m(s), self.line.along_m(s + 1));
        let t = if end > start { ((along_m - start) / (end - start)).clamp(0.0, 1.0) } else { 0.0 };
        Some(a + (b - a) * t)
    }

    /// The distance along the route to point `index` of its geometry.
    pub fn along_m(&self, index: usize) -> f64 {
        self.line.along_m(index)
    }

    /// The zoom and tilt the camera should take, by the speed and the next maneuver: further out
    /// at speed, closer before a turn.
    pub fn camera(&self, speed_mps: Option<f64>) -> (f64, f64) {
        let speed = speed_mps.or(self.last_speed_mps).unwrap_or_else(|| self.typical_speed()) * 3.6;
        let mut zoom: f64 = match self.mode {
            TravelMode::Car => match speed {
                s if s < 20.0 => 17.5,
                s if s < 45.0 => 17.0,
                s if s < 70.0 => 16.3,
                s if s < 95.0 => 15.7,
                _ => 15.2,
            },
            TravelMode::Bicycle => 17.3,
            _ => 18.0,
        };
        if let Some(next) = self.next_step() {
            let d = self.distance_to_step_m(next);
            if d < 150.0 && self.mode == TravelMode::Car {
                zoom = zoom.max(17.3);
            }
        }
        (zoom, if self.mode == TravelMode::Car { 55.0 } else { 45.0 })
    }

    /// Replaces the route (after a reroute), keeping nothing of the old one's progress.
    pub fn replace(&mut self, route: Route) {
        let mode = self.mode;
        let last_reroute_ms = self.last_reroute_ms;
        *self = Session::new(route, mode);
        self.last_reroute_ms = last_reroute_ms;
    }

    pub fn steps(&self) -> Vec<&routing::Step> {
        self.route.steps().collect()
    }

    pub fn length_m(&self) -> f64 {
        self.line.length_m()
    }

    pub fn remaining_m(&self) -> f64 {
        (self.length_m() - self.progress_m).max(0.0)
    }

    /// The index of the step being travelled.
    pub fn current_step(&self) -> usize {
        self.step_starts.partition_point(|&s| s <= self.progress_m + 0.5).saturating_sub(1)
    }

    /// The index of the next maneuver (the step after the current one).
    pub fn next_step(&self) -> Option<usize> {
        let next = self.current_step() + 1;
        (next < self.step_starts.len()).then_some(next)
    }

    pub fn distance_to_step_m(&self, step: usize) -> f64 {
        self.step_starts.get(step).map_or(0.0, |s| (s - self.progress_m).max(0.0))
    }

    /// The time left, in seconds: the rest of the current step, then every step after it.
    pub fn remaining_s(&self) -> f64 {
        let steps: Vec<_> = self.route.steps().collect();
        let current = self.current_step();
        let mut total = 0.0;
        for (i, step) in steps.iter().enumerate().skip(current) {
            if i == current {
                let start = self.step_starts[i];
                let end = self.step_starts.get(i + 1).copied().unwrap_or(self.length_m());
                let left = if end > start { ((end - self.progress_m) / (end - start)).clamp(0.0, 1.0) } else { 0.0 };
                total += step.duration_s * left;
            } else {
                total += step.duration_s;
            }
        }
        // With traffic, the whole route takes longer than its steps say: scaled alike.
        let ratio = self.route.duration_in_traffic_s.map_or(1.0, |t| if self.route.duration_s > 0.0 { t / self.route.duration_s } else { 1.0 });
        total * ratio
    }

    /// How far ahead to announce a maneuver: further at speed (about 25 s ahead, 150 m to 2 km by
    /// car).
    fn prepare_distance(&self, speed_mps: f64) -> f64 {
        match self.mode {
            TravelMode::Car => (speed_mps * 25.0).clamp(150.0, 2000.0),
            TravelMode::Bicycle => (speed_mps * 20.0).clamp(60.0, 300.0),
            _ => 50.0,
        }
    }

    /// How close to a maneuver to announce it now (about 6 s ahead).
    fn now_distance(&self, speed_mps: f64) -> f64 {
        match self.mode {
            TravelMode::Car => (speed_mps * 6.0).clamp(40.0, 400.0),
            TravelMode::Bicycle => (speed_mps * 5.0).clamp(15.0, 60.0),
            _ => 12.0,
        }
    }

    fn typical_speed(&self) -> f64 {
        match self.mode {
            TravelMode::Car => 12.0,
            TravelMode::Bicycle => 4.5,
            _ => 1.3,
        }
    }

    /// Takes a fix, and says what it changed.
    pub fn locate(&mut self, location: &Location) -> Vec<Event> {
        if self.state == NavigationState::Arrived {
            return Vec::new();
        }
        let speed = location.speed_mps.filter(|s| s.is_finite() && *s >= 0.0).unwrap_or_else(|| self.typical_speed());
        // Where the fix falls on the route: first near the last progress (a route may pass the same
        // place twice), then anywhere ahead.
        let ahead = (speed * 15.0).max(200.0);
        let near = self.line.project(&location.coordinate, (self.progress_m - 50.0).max(0.0), self.progress_m + ahead);
        let anywhere = || self.line.project(&location.coordinate, self.progress_m, f64::INFINITY);
        let projection = match near {
            Some(p) if p.distance_m <= Self::OFF_ROUTE_M => Some(p),
            other => anywhere().filter(|a| other.is_none_or(|o| a.distance_m < o.distance_m)).or(other),
        };
        let Some(p) = projection else { return Vec::new() };
        let limit = Self::OFF_ROUTE_M.max(location.accuracy_m.unwrap_or(0.0).min(100.0) * 1.5);
        let heading_away = match (location.bearing, location.speed_mps) {
            (Some(b), Some(s)) if s > 3.0 => geo::Coordinate::bearing_difference(p.bearing, b).abs() > 120.0,
            _ => false,
        };
        let mut events = Vec::new();
        if p.distance_m > limit || heading_away && p.distance_m > 15.0 {
            self.off_route_fixes += 1;
            if self.off_route_fixes >= Self::OFF_ROUTE_FIXES && self.state == NavigationState::Navigating {
                let due = self.last_reroute_ms.is_none_or(|t| location.time_ms - t >= Self::REROUTE_INTERVAL_MS);
                if due {
                    self.state = NavigationState::Rerouting;
                    self.last_reroute_ms = Some(location.time_ms);
                    events.push(Event::OffRoute);
                }
            }
            return events;
        }
        self.off_route_fixes = 0;
        if self.state == NavigationState::Rerouting {
            // Back on the route before a new one came.
            self.state = NavigationState::Navigating;
        }
        // Progress moves forward, except for a small step back (a jittery fix).
        if p.along_m >= self.progress_m - 20.0 {
            self.progress_m = p.along_m.max(self.progress_m - 20.0);
        }
        self.snapped = Some(p.point);
        self.bearing = Some(p.bearing);

        let end = self.route.geometry.last().copied().unwrap_or(location.coordinate);
        if self.remaining_m() <= Self::ARRIVED_M || location.coordinate.distance_m(&end) <= Self::ARRIVED_M && self.remaining_m() < 100.0 {
            self.state = NavigationState::Arrived;
            self.progress_m = self.length_m();
            events.push(Event::Arrived);
            return events;
        }
        self.last_fix_ms = Some(location.time_ms);
        self.last_speed_mps = location.speed_mps.filter(|s| s.is_finite() && *s >= 0.0).or(self.last_speed_mps);
        self.fix_progress_m = self.progress_m;
        self.fix_limit_m = self.next_step().map_or(self.length_m(), |n| self.step_starts[n] + 30.0).min(self.length_m());
        self.estimated = false;
        events.extend(self.announcements(speed));
        events
    }

    /// The announcements due at the position: the alerts ahead, then the next maneuver.
    fn announcements(&mut self, speed: f64) -> Vec<Event> {
        let mut events = Vec::new();
        let alert_at = match self.mode {
            TravelMode::Car => (speed * 20.0).clamp(150.0, 600.0),
            TravelMode::Bicycle => 80.0,
            _ => 40.0,
        };
        for (index, distance) in self.alerts_ahead(alert_at) {
            let speaks = matches!(self.route.alerts[index].kind, AlertKind::LevelCrossing | AlertKind::SpeedCamera | AlertKind::Incident | AlertKind::Steep);
            if speaks && distance > 0.0 && !self.alerts_announced.contains(&index) {
                self.alerts_announced.push(index);
                events.push(Event::Alert { index, distance_m: distance });
            }
        }
        let Some(next) = self.next_step() else { return events };
        let distance = self.distance_to_step_m(next);
        let next_is_arrival = self.route.steps().nth(next).is_some_and(|s| s.maneuver.kind == ManeuverKind::Arrive);
        let count = self.step_starts.len();
        let step_length = self.step_starts[next] - self.step_starts[next - 1];
        let now_at = self.now_distance(speed);
        let prepare_at = self.prepare_distance(speed);
        if distance <= now_at {
            if self.mark(next, Stage::Now) {
                self.mark(next, Stage::Prepare);
                // The maneuver after this one, when it follows within a few seconds.
                let then = (next + 1 < count && self.step_starts[next + 1] - self.step_starts[next] <= now_at * 2.0 && !next_is_arrival).then_some(next + 1);
                events.push(Event::Now { step: next, then });
            }
        } else if distance <= prepare_at && step_length > now_at * 2.0 && self.mark(next, Stage::Prepare) {
            events.push(Event::Prepare { step: next, distance_m: distance });
        }
        events
    }

    /// Records an announcement; `false` when it was made before.
    fn mark(&mut self, step: usize, stage: Stage) -> bool {
        if self.announced.contains(&(step, stage)) {
            return false;
        }
        self.announced.push((step, stage));
        true
    }

    /// The part of the route already travelled and the part ahead.
    pub fn split(&self) -> (Vec<Coordinate>, Vec<Coordinate>) {
        (self.line.slice(0.0, self.progress_m), self.line.slice(self.progress_m, self.length_m()))
    }
}
