//! A client of the route service of OSRM, version 1
//! (<https://project-osrm.org/docs/v5.24.0/api/#route-service>).
//!
//! An OSRM server routes one mode of travel, set when its data was prepared; the profile in the
//! URL is not checked by the server. [`Osrm`] therefore holds a server for each mode, and refuses
//! a mode it has no server for.

use fetch::{Query, Request, Response};
use geo::{Coordinate, Polyline};
use routing::{Avoid, Lane, LaneIndication, Leg, Maneuver, ManeuverKind, Modifier, RemoteRouter, Route, RouteError, RouteRequest, Step, TravelMode};
use serde::Deserialize;

/// OSRM servers, one for each mode of travel it routes.
#[derive(Debug, Clone, PartialEq)]
pub struct Osrm {
    /// The base URL of each server, up to the service (`https://host/routed-car`), with the
    /// profile name used in its URLs.
    pub car: Option<Server>,
    pub bicycle: Option<Server>,
    pub foot: Option<Server>,
    pub user_agent: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub base: String,
    pub profile: String,
}

#[derive(Deserialize)]
struct Body {
    code: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    routes: Vec<RawRoute>,
}

#[derive(Deserialize)]
struct RawRoute {
    distance: f64,
    duration: f64,
    #[serde(default)]
    legs: Vec<RawLeg>,
}

#[derive(Deserialize)]
struct RawLeg {
    distance: f64,
    duration: f64,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    steps: Vec<RawStep>,
}

#[derive(Deserialize)]
struct RawStep {
    distance: f64,
    duration: f64,
    geometry: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "ref")]
    road_ref: Option<String>,
    #[serde(default)]
    destinations: Option<String>,
    #[serde(default)]
    exits: Option<String>,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    intersections: Vec<RawIntersection>,
    maneuver: RawManeuver,
}

#[derive(Deserialize)]
struct RawIntersection {
    #[serde(default)]
    classes: Vec<String>,
    #[serde(default)]
    lanes: Vec<RawLane>,
}

#[derive(Deserialize)]
struct RawLane {
    #[serde(default)]
    indications: Vec<String>,
    /// A boolean, though the documentation's example writes it as a string.
    #[serde(default)]
    valid: serde_json::Value,
}

#[derive(Deserialize)]
struct RawManeuver {
    location: [f64; 2],
    #[serde(default)]
    bearing_before: Option<f64>,
    #[serde(default)]
    bearing_after: Option<f64>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    modifier: Option<String>,
    #[serde(default)]
    exit: Option<u32>,
}

impl Osrm {
    /// The servers of FOSSGIS that openstreetmap.org uses, one per mode
    /// (<https://routing.openstreetmap.de/about.html>). The profile in their URLs is `driving`
    /// for all three.
    pub fn fossgis(user_agent: &str) -> Osrm {
        let server = |name: &str| Some(Server { base: format!("https://routing.openstreetmap.de/routed-{name}"), profile: "driving".into() });
        Osrm { car: server("car"), bicycle: server("bike"), foot: server("foot"), user_agent: user_agent.into() }
    }

    /// The classes of the car profile of OSRM that `exclude` accepts.
    fn exclusions(avoid: &Avoid) -> Vec<&'static str> {
        let mut classes = Vec::new();
        if avoid.tolls {
            classes.push("toll");
        }
        if avoid.highways {
            classes.push("motorway");
        }
        if avoid.ferries {
            classes.push("ferry");
        }
        classes
    }

    fn server(&self, mode: TravelMode) -> Option<&Server> {
        match mode {
            TravelMode::Car => self.car.as_ref(),
            TravelMode::Bicycle => self.bicycle.as_ref(),
            TravelMode::Foot => self.foot.as_ref(),
            TravelMode::Transit => None,
        }
    }

    fn kind(kind: &str, mode: &str) -> ManeuverKind {
        if mode == "ferry" && kind != "arrive" && kind != "depart" {
            return ManeuverKind::Ferry;
        }
        match kind {
            "depart" => ManeuverKind::Depart,
            "arrive" => ManeuverKind::Arrive,
            "new name" => ManeuverKind::NewName,
            "merge" => ManeuverKind::Merge,
            "on ramp" => ManeuverKind::OnRamp,
            "off ramp" => ManeuverKind::OffRamp,
            "fork" => ManeuverKind::Fork,
            "end of road" => ManeuverKind::EndOfRoad,
            "continue" | "use lane" | "notification" => ManeuverKind::Continue,
            "roundabout" | "rotary" | "roundabout turn" => ManeuverKind::Roundabout,
            "exit roundabout" | "exit rotary" => ManeuverKind::ExitRoundabout,
            // "turn", and types added after this client: a turn is the safe reading.
            _ => ManeuverKind::Turn,
        }
    }

    fn modifier(modifier: &str) -> Option<Modifier> {
        Some(match modifier {
            "uturn" => Modifier::UTurn,
            "sharp right" => Modifier::SharpRight,
            "right" => Modifier::Right,
            "slight right" => Modifier::SlightRight,
            "straight" => Modifier::Straight,
            "slight left" => Modifier::SlightLeft,
            "left" => Modifier::Left,
            "sharp left" => Modifier::SharpLeft,
            _ => return None,
        })
    }

    /// The lanes of an intersection; none when an indication is unknown (a later version's).
    fn lanes(raw: &[RawLane]) -> Vec<Lane> {
        let lanes: Option<Vec<Lane>> = raw
            .iter()
            .map(|l| {
                let indications = l.indications.iter().map(|i| LaneIndication::from_osrm(i)).collect::<Option<Vec<_>>>()?;
                let valid = l.valid.as_bool().unwrap_or_else(|| l.valid.as_str() == Some("true"));
                Some(Lane { indications, valid, preferred: false })
            })
            .collect();
        lanes.filter(|l| l.len() <= 32 && l.iter().any(|x| x.valid)).unwrap_or_default()
    }

    fn route(raw: RawRoute, request: &RouteRequest) -> Result<Route, RouteError> {
        let mut geometry: Vec<Coordinate> = Vec::new();
        let mut legs = Vec::with_capacity(raw.legs.len());
        let mut summaries = Vec::new();
        let (mut has_tolls, mut has_ferries) = (false, false);
        for leg in raw.legs {
            let mut steps = Vec::with_capacity(leg.steps.len());
            for step in leg.steps {
                let points = Polyline::decode(&step.geometry, 6).map_err(|e| RouteError::Parse(e.to_string()))?;
                // Each step begins where the previous one ended; the shared point is kept once.
                let skip = usize::from(!geometry.is_empty() && !points.is_empty());
                let start = geometry.len().saturating_sub(skip);
                geometry.extend(points.into_iter().skip(skip));
                let end = geometry.len().saturating_sub(1).max(start);
                for intersection in &step.intersections {
                    has_tolls |= intersection.classes.iter().any(|c| c == "toll");
                    has_ferries |= intersection.classes.iter().any(|c| c == "ferry");
                }
                has_ferries |= step.mode == "ferry";
                let [lon, lat] = step.maneuver.location;
                let location = Coordinate::checked(lat, lon).ok_or_else(|| RouteError::Parse("maneuver location out of range".into()))?;
                let maneuver = Maneuver {
                    kind: Self::kind(&step.maneuver.kind, &step.mode),
                    modifier: step.maneuver.modifier.as_deref().and_then(Self::modifier),
                    location,
                    bearing_before: step.maneuver.bearing_before,
                    bearing_after: step.maneuver.bearing_after,
                    exit: step.maneuver.exit,
                };
                let mut out = Step::new(maneuver, step.distance, step.duration, start, end);
                out.name = Some(step.name).filter(|n| !n.is_empty());
                out.road_ref = step.road_ref.filter(|r| !r.is_empty());
                out.destinations = step.destinations.filter(|d| !d.is_empty());
                out.exit_number = step.exits.filter(|e| !e.is_empty());
                // The lanes at the maneuver: those of its first intersection.
                out.lanes = step.intersections.first().map(|i| Self::lanes(&i.lanes)).unwrap_or_default();
                steps.push(out);
            }
            if !leg.summary.is_empty() {
                summaries.push(leg.summary);
            }
            legs.push(Leg { distance_m: leg.distance, duration_s: leg.duration, steps });
        }
        let mut route = Route::new(geometry, legs, raw.distance, raw.duration);
        route.summary = summaries.join(" / ");
        route.has_tolls = has_tolls;
        route.has_ferries = has_ferries;
        // Only the car profile of OSRM has the classes `exclude` names; it knows nothing of the
        // other rules, so a route may break them.
        let supported =
            if request.mode == TravelMode::Car { Avoid { tolls: true, highways: true, ferries: true, ..Avoid::default() } } else { Avoid::default() };
        route.ignored_avoid = request.avoid.strict().without(&supported);
        route.validate()?;
        Ok(route)
    }
}

impl RemoteRouter for Osrm {
    fn request(&self, request: &RouteRequest) -> Result<Request, RouteError> {
        if !request.is_valid() {
            return Err(RouteError::InvalidRequest("two or more valid waypoints are needed".into()));
        }
        let server = self.server(request.mode).ok_or(RouteError::UnsupportedMode)?;
        let coordinates = request.waypoints.iter().map(|c| format!("{:.6},{:.6}", c.longitude, c.latitude)).collect::<Vec<_>>().join(";");
        let mut query = Query::new()
            .pair("overview", "false")
            .pair("geometries", "polyline6")
            .pair("steps", "true")
            .pair("alternatives", if request.alternatives && request.waypoints.len() == 2 { "true" } else { "false" });
        let exclusions = Self::exclusions(&request.avoid);
        if request.mode == TravelMode::Car && !exclusions.is_empty() {
            query = query.pair("exclude", exclusions.join(","));
        }
        if let Some(heading) = request.heading {
            // A bearing for the first waypoint only (within 60°); the others are free.
            let first = format!("{},60", Coordinate::normalize_bearing(heading).round() as u32 % 360);
            let rest = vec![""; request.waypoints.len() - 1];
            query = query.pair("bearings", std::iter::once(first.as_str()).chain(rest).collect::<Vec<_>>().join(";"));
        }
        let url = query.append_to(&format!("{}/route/v1/{}/{}", server.base.trim_end_matches('/'), server.profile, coordinates));
        Ok(Request::get(url).header("User-Agent", &self.user_agent))
    }

    fn parse(&self, request: &RouteRequest, response: &Response) -> Result<Vec<Route>, RouteError> {
        if response.status == 0 {
            return Err(RouteError::Network(response.error.clone().unwrap_or_default()));
        }
        // Errors come with a JSON body and a 400 status; read the body first.
        let body: Body = match serde_json::from_str(&response.body) {
            Ok(body) => body,
            Err(e) => return Err(RouteError::of_response(response).unwrap_or(RouteError::Parse(e.to_string()))),
        };
        match body.code.as_str() {
            "Ok" => {}
            "NoRoute" => return Err(RouteError::NoRoute),
            "NoSegment" => return Err(RouteError::NoRoad),
            "InvalidQuery" | "InvalidValue" | "InvalidOptions" | "TooBig" => return Err(RouteError::InvalidRequest(body.message.unwrap_or(body.code))),
            _ => return Err(RouteError::Server(body.message.unwrap_or(body.code))),
        }
        let routes = body.routes.into_iter().map(|r| Self::route(r, request)).collect::<Result<Vec<_>, _>>()?;
        if routes.is_empty() {
            return Err(RouteError::NoRoute);
        }
        Ok(routes)
    }
}
