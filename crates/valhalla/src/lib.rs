//! A client of the route service of Valhalla (<https://valhalla.github.io/valhalla/api/turn-by-turn/api-reference/>).

use fetch::{Request, Response};
use geo::{Coordinate, Polyline};
use routing::{Avoid, Lane, LaneIndication, Leg, Maneuver, ManeuverKind, Modifier, Optimize, RemoteRouter, Route, RouteError, RouteRequest, Step, TravelMode};
use serde::Deserialize;
use serde_json::{Value, json};

/// A Valhalla server.
#[derive(Debug, Clone, PartialEq)]
pub struct Valhalla {
    /// The base URL, before `/route`.
    pub base: String,
    pub user_agent: String,
}

#[derive(Deserialize)]
struct Body {
    #[serde(default)]
    trip: Option<Trip>,
    #[serde(default)]
    alternates: Vec<Alternate>,
    #[serde(default)]
    error_code: Option<u32>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct Alternate {
    trip: Trip,
}

#[derive(Deserialize)]
struct Trip {
    #[serde(default)]
    legs: Vec<RawLeg>,
    summary: Summary,
}

#[derive(Deserialize)]
struct Summary {
    /// In the unit of the request (kilometres here).
    length: f64,
    time: f64,
    #[serde(default)]
    has_toll: bool,
    #[serde(default)]
    has_ferry: bool,
    #[serde(default)]
    has_highway: bool,
}

#[derive(Deserialize)]
struct RawLeg {
    shape: String,
    summary: Summary,
    #[serde(default)]
    maneuvers: Vec<RawManeuver>,
}

#[derive(Deserialize)]
struct RawManeuver {
    #[serde(rename = "type")]
    kind: u32,
    #[serde(default)]
    instruction: Option<String>,
    #[serde(default)]
    street_names: Vec<String>,
    #[serde(default)]
    begin_street_names: Vec<String>,
    begin_shape_index: usize,
    end_shape_index: usize,
    length: f64,
    time: f64,
    #[serde(default)]
    bearing_before: Option<f64>,
    #[serde(default)]
    bearing_after: Option<f64>,
    #[serde(default)]
    roundabout_exit_count: Option<u32>,
    #[serde(default)]
    sign: Option<Value>,
    #[serde(default)]
    transit_info: Option<Value>,
    #[serde(default)]
    lanes: Vec<RawLane>,
}

/// A lane of a maneuver: bit masks of its arrows (`directions`), of those that lead where the
/// route goes (`valid`) and of the one to take (`active`).
#[derive(Deserialize)]
struct RawLane {
    #[serde(default)]
    directions: u32,
    #[serde(default)]
    valid: Option<u32>,
    #[serde(default)]
    active: Option<u32>,
}

impl Valhalla {
    /// The public server of FOSSGIS (<https://valhalla1.openstreetmap.de>).
    pub fn fossgis(user_agent: &str) -> Valhalla {
        Valhalla { base: "https://valhalla1.openstreetmap.de".into(), user_agent: user_agent.into() }
    }

    fn costing(mode: TravelMode) -> &'static str {
        match mode {
            TravelMode::Car => "auto",
            TravelMode::Bicycle => "bicycle",
            TravelMode::Foot => "pedestrian",
            TravelMode::Transit => "multimodal",
        }
    }

    /// The body of the request, as JSON.
    pub fn body(request: &RouteRequest) -> Value {
        let locations: Vec<Value> = request
            .waypoints
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut location = json!({"lat": c.latitude, "lon": c.longitude, "type": "break"});
                if i == 0
                    && let Some(heading) = request.heading
                {
                    location["heading"] = json!(Coordinate::normalize_bearing(heading).round() as u32 % 360);
                    location["heading_tolerance"] = json!(60);
                }
                location
            })
            .collect();
        let costing = Self::costing(request.mode);
        let mut options = json!({});
        let a = &request.avoid;
        // Each `use_*` is a preference from 0 (avoid) to 1 (prefer); 0.5 is neutral.
        match request.mode {
            TravelMode::Car => {
                if a.tolls {
                    options["use_tolls"] = json!(0.0);
                }
                if a.highways {
                    options["use_highways"] = json!(0.0);
                }
                if a.unpaved {
                    options["exclude_unpaved"] = json!(true);
                }
                if a.narrow {
                    options["use_living_streets"] = json!(0.0);
                    options["use_tracks"] = json!(0.0);
                }
                if request.optimize == Optimize::Eco {
                    // Steady speeds below motorway speeds use less fuel.
                    options["top_speed"] = json!(100);
                }
            }
            TravelMode::Bicycle => {
                if a.hills {
                    options["use_hills"] = json!(0.0);
                }
                if a.unpaved {
                    options["avoid_bad_surfaces"] = json!(1.0);
                }
                if a.narrow {
                    options["use_living_streets"] = json!(0.2);
                }
            }
            TravelMode::Foot => {
                if a.hills {
                    options["use_hills"] = json!(0.0);
                }
                if a.unlit {
                    options["use_lit"] = json!(1.0);
                }
                if a.steps {
                    // Seconds added for each flight of steps: far more than any detour.
                    options["step_penalty"] = json!(3600);
                }
                if a.unpaved {
                    options["use_tracks"] = json!(0.0);
                }
            }
            TravelMode::Transit => {}
        }
        if a.ferries {
            options["use_ferry"] = json!(0.0);
        }
        if request.optimize == Optimize::Shortest {
            options["shortest"] = json!(true);
        }
        let mut body = json!({
            "locations": locations,
            "costing": costing,
            "costing_options": { costing: options },
            "directions_options": { "units": "kilometers", "language": request.language },
            "turn_lanes": true,
        });
        let areas: Vec<Vec<[f64; 2]>> =
            request.effective_avoid_areas().iter().map(|area| area.ring.iter().map(|c| [c.longitude, c.latitude]).collect()).collect();
        if !areas.is_empty() {
            body["exclude_polygons"] = json!(areas);
        }
        if request.alternatives && request.waypoints.len() == 2 {
            body["alternates"] = json!(2);
        }
        body
    }

    fn kind(kind: u32) -> (ManeuverKind, Option<Modifier>) {
        use ManeuverKind as K;
        use Modifier as M;
        match kind {
            1 => (K::Depart, None),
            2 => (K::Depart, Some(M::Right)),
            3 => (K::Depart, Some(M::Left)),
            4 => (K::Arrive, None),
            5 => (K::Arrive, Some(M::Right)),
            6 => (K::Arrive, Some(M::Left)),
            7 => (K::NewName, None),
            8 => (K::Continue, Some(M::Straight)),
            9 => (K::Turn, Some(M::SlightRight)),
            10 => (K::Turn, Some(M::Right)),
            11 => (K::Turn, Some(M::SharpRight)),
            12 | 13 => (K::Turn, Some(M::UTurn)),
            14 => (K::Turn, Some(M::SharpLeft)),
            15 => (K::Turn, Some(M::Left)),
            16 => (K::Turn, Some(M::SlightLeft)),
            17 => (K::OnRamp, Some(M::Straight)),
            18 => (K::OnRamp, Some(M::Right)),
            19 => (K::OnRamp, Some(M::Left)),
            20 => (K::OffRamp, Some(M::Right)),
            21 => (K::OffRamp, Some(M::Left)),
            22 => (K::Fork, Some(M::Straight)),
            23 => (K::Fork, Some(M::SlightRight)),
            24 => (K::Fork, Some(M::SlightLeft)),
            25 => (K::Merge, None),
            26 => (K::Roundabout, None),
            27 => (K::ExitRoundabout, None),
            28 => (K::Ferry, None),
            29 => (K::Continue, None),
            30..=36 => (K::Transit, None),
            37 => (K::Merge, Some(M::SlightRight)),
            38 => (K::Merge, Some(M::SlightLeft)),
            // 0 (none) and the indoor maneuvers (elevators, stairs, buildings) and any added later.
            _ => (K::Continue, None),
        }
    }

    /// The texts of the elements `key` of the sign of a maneuver, joined.
    fn sign_texts(sign: &Value, key: &str) -> Option<String> {
        let elements = sign.get(key)?.as_array()?;
        let texts: Vec<&str> = elements.iter().filter_map(|e| e.get("text")?.as_str()).filter(|t| !t.is_empty()).collect();
        (!texts.is_empty()).then(|| texts.join(", "))
    }

    /// The destinations on the sign of a maneuver: where it leads (`exit_toward_elements`), else
    /// the roads it branches to (`exit_branch_elements`).
    fn destinations(sign: &Value) -> Option<String> {
        Self::sign_texts(sign, "exit_toward_elements").or_else(|| Self::sign_texts(sign, "exit_branch_elements"))
    }

    /// The arrows of a bit mask of `directions`.
    fn indications(mask: u32) -> Vec<LaneIndication> {
        use LaneIndication as L;
        let bits = [
            (2, L::Through),
            (4, L::SharpLeft),
            (8, L::Left),
            (16, L::SlightLeft),
            (32, L::SlightRight),
            (64, L::Right),
            (128, L::SharpRight),
            (256, L::UTurn),
            (512, L::MergeToLeft),
            (1024, L::MergeToRight),
        ];
        let out: Vec<LaneIndication> = bits.iter().filter(|(b, _)| mask & b != 0).map(|(_, l)| *l).collect();
        if out.is_empty() { vec![L::None] } else { out }
    }

    fn lanes(raw: &[RawLane]) -> Vec<Lane> {
        if raw.len() > 32 {
            return Vec::new();
        }
        let lanes: Vec<Lane> = raw
            .iter()
            .map(|l| Lane { indications: Self::indications(l.directions), valid: l.valid.is_some_and(|v| v != 0), preferred: l.active.is_some_and(|v| v != 0) })
            .collect();
        if lanes.iter().any(|l| l.valid) { lanes } else { Vec::new() }
    }

    fn transit_line(info: &Value) -> Option<String> {
        ["short_name", "long_name", "onestop_id"].iter().find_map(|k| info.get(*k)?.as_str().filter(|s| !s.is_empty()).map(str::to_string))
    }

    fn route(trip: Trip, request: &RouteRequest) -> Result<Route, RouteError> {
        let avoid = &request.avoid;
        let mut geometry: Vec<Coordinate> = Vec::new();
        let mut legs = Vec::with_capacity(trip.legs.len());
        for leg in trip.legs {
            let points = Polyline::decode(&leg.shape, 6).map_err(|e| RouteError::Parse(e.to_string()))?;
            // The legs share their joining point; it is kept once.
            let offset = geometry.len().saturating_sub(usize::from(!geometry.is_empty()));
            let skip = usize::from(!geometry.is_empty());
            geometry.extend(points.iter().skip(skip).copied());
            let mut steps = Vec::with_capacity(leg.maneuvers.len());
            for m in leg.maneuvers {
                let location = *points.get(m.begin_shape_index).ok_or_else(|| RouteError::Parse("maneuver outside its shape".into()))?;
                let (kind, modifier) = Self::kind(m.kind);
                let mut maneuver = Maneuver::new(kind, modifier, location);
                maneuver.bearing_before = m.bearing_before;
                maneuver.bearing_after = m.bearing_after;
                maneuver.exit = m.roundabout_exit_count;
                let names = if m.street_names.is_empty() { &m.begin_street_names } else { &m.street_names };
                let mut step =
                    Step::new(maneuver, m.length * 1000.0, m.time, offset + m.begin_shape_index, offset + m.end_shape_index.max(m.begin_shape_index));
                step.name = names.first().cloned();
                step.destinations = m.sign.as_ref().and_then(Self::destinations);
                step.exit_number = m.sign.as_ref().and_then(|s| Self::sign_texts(s, "exit_number_elements"));
                step.instruction = m.instruction;
                step.transit = m.transit_info.as_ref().and_then(Self::transit_line);
                step.lanes = Self::lanes(&m.lanes);
                steps.push(step);
            }
            legs.push(Leg { distance_m: leg.summary.length * 1000.0, duration_s: leg.summary.time, steps });
        }
        let mut route = Route::new(geometry, legs, trip.summary.length * 1000.0, trip.summary.time);
        route.has_tolls = trip.summary.has_toll;
        route.has_ferries = trip.summary.has_ferry;
        // Avoiding is a preference in Valhalla: a route may still use what was to be avoided when
        // there is no other way. Its summary tells tolls, highways and ferries; of unpaved roads
        // and steps it says nothing, and level crossings it does not avoid.
        let used = Avoid { tolls: trip.summary.has_toll, highways: trip.summary.has_highway, ferries: trip.summary.has_ferry, ..Avoid::default() };
        let told = Avoid { tolls: true, highways: true, ferries: true, ..Avoid::default() };
        let mut unknown = Avoid { level_crossings: true, ..Avoid::default() };
        // Unpaved roads are excluded for cars, only disliked for cycling and walking; steps are
        // all but excluded for walkers, and may be used by bicycles.
        unknown.unpaved = request.mode != TravelMode::Car;
        unknown.steps = request.mode == TravelMode::Bicycle;
        route.ignored_avoid = avoid.and(&used).or(&avoid.strict().without(&told).and(&unknown));
        route.validate()?;
        Ok(route)
    }
}

impl RemoteRouter for Valhalla {
    fn request(&self, request: &RouteRequest) -> Result<Request, RouteError> {
        if !request.is_valid() {
            return Err(RouteError::InvalidRequest("two or more valid waypoints are needed".into()));
        }
        let url = format!("{}/route", self.base.trim_end_matches('/'));
        Ok(Request::post(url, "application/json", Self::body(request).to_string()).header("User-Agent", &self.user_agent))
    }

    fn parse(&self, request: &RouteRequest, response: &Response) -> Result<Vec<Route>, RouteError> {
        if response.status == 0 {
            return Err(RouteError::Network(response.error.clone().unwrap_or_default()));
        }
        let body: Body = match serde_json::from_str(&response.body) {
            Ok(body) => body,
            Err(e) => return Err(RouteError::of_response(response).unwrap_or(RouteError::Parse(e.to_string()))),
        };
        if let Some(code) = body.error_code {
            let message = body.error.unwrap_or_default();
            return Err(match code {
                // 442: no path; 443: exact route match failed.
                442 | 443 => RouteError::NoRoute,
                // 170, 171: no suitable edges near a location.
                170 | 171 => RouteError::NoRoad,
                // 125: no costing (the server does not route this mode).
                125 => RouteError::UnsupportedMode,
                100..=199 => RouteError::InvalidRequest(format!("{code}: {message}")),
                _ => RouteError::Server(format!("{code}: {message}")),
            });
        }
        let trip = body.trip.ok_or_else(|| RouteError::of_response(response).unwrap_or(RouteError::Parse("no trip".into())))?;
        let mut routes = vec![Self::route(trip, request)?];
        for alternate in body.alternates {
            routes.push(Self::route(alternate.trip, request)?);
        }
        Ok(routes)
    }
}
