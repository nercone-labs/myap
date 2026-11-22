use crate::{Google, GoogleError, Pb, PbNode, Positional};
use fetch::{Request, Response};
use geo::{Bounds, Coordinate, Line};
use routing::{Avoid, Maneuver, ManeuverKind, Modifier, RemoteRouter, Route, RouteError, RouteRequest, Step, TravelMode};
use serde_json::Value;

/// A step as Google gives it: where, what kind, and the road.
#[derive(Debug, Clone, PartialEq)]
struct RawStep {
    location: Coordinate,
    kind: ManeuverKind,
    modifier: Option<Modifier>,
    exit: Option<u32>,
    name: Option<String>,
    text: String,
}

impl Google {
    /// The request of directions. Google routes driving, cycling and walking; public transport is
    /// not answered in a shape this client reads.
    pub fn directions_request(&self, request: &RouteRequest) -> Result<Request, RouteError> {
        if !request.is_valid() {
            return Err(RouteError::InvalidRequest("two or more valid waypoints are needed".into()));
        }
        let mode = match request.mode {
            TravelMode::Car => 0,
            TravelMode::Bicycle => 1,
            TravelMode::Foot => 2,
            TravelMode::Transit => return Err(RouteError::UnsupportedMode),
        };
        let origin = request.waypoints[0];
        let destination = *request.waypoints.last().unwrap_or(&origin);
        let bounds =
            Bounds::of(&request.waypoints).unwrap_or(Bounds { south: origin.latitude, west: origin.longitude, north: origin.latitude, east: origin.longitude });
        let center = bounds.center();
        let span = (Self::span(Some(&bounds), 1_500.0, 0.0, f64::INFINITY) * 1.35).max(1_500.0);
        let filled = self.fill(
            &self.templates.directions,
            &[
                ("OLAT", format!("{:.7}", origin.latitude)),
                ("OLNG", format!("{:.7}", origin.longitude)),
                ("DLAT", format!("{:.7}", destination.latitude)),
                ("DLNG", format!("{:.7}", destination.longitude)),
                ("CLAT", format!("{:.7}", center.latitude)),
                ("CLNG", format!("{:.7}", center.longitude)),
                ("SPAN", format!("{span:.3}")),
                ("MODE", mode.to_string()),
            ],
        );
        let mut pb = Pb::parse(&filled).map_err(|e| RouteError::InvalidRequest(e.to_string()))?;
        // Stops between: more waypoints of field 1, before the destination (the second).
        for (n, stop) in request.waypoints[1..request.waypoints.len() - 1].iter().enumerate() {
            pb.nodes.insert(1 + n, Self::waypoint(stop));
        }
        if request.mode == TravelMode::Car && request.avoid.any() {
            Self::add_avoid(&mut pb, &request.avoid)?;
        }
        let query = Self::query().pair("authuser", "0").pair("hl", "en").pair("gl", "us").pair("pb", pb.to_string());
        Ok(self.data_request(query.append_to(&format!("{}/maps/preview/directions", Self::ORIGIN))))
    }

    fn waypoint(c: &Coordinate) -> PbNode {
        PbNode::message(
            1,
            vec![
                PbNode::message(3, vec![PbNode::field(3, 'd', format!("{:.7}", c.latitude)), PbNode::field(4, 'd', format!("{:.7}", c.longitude))]),
                PbNode::field(6, 'e', 2),
            ],
        )
    }

    /// The avoidances live in the options (field 6): highways (`1b1`) and tolls (`2b1`) in its
    /// message 2, ferries (`7b1`) in the options themselves.
    fn add_avoid(pb: &mut Pb, avoid: &Avoid) -> Result<(), RouteError> {
        let missing = || RouteError::InvalidRequest("the directions template has no options".into());
        let options = Pb::find_message(&pb.nodes, 6, Some(1)).ok_or_else(missing)?;
        let PbNode::Message { children, .. } = &mut pb.nodes[options] else { return Err(missing()) };
        if avoid.highways || avoid.tolls {
            let inner = Pb::find_message(children, 2, None).ok_or_else(missing)?;
            let PbNode::Message { children: flags, .. } = &mut children[inner] else { return Err(missing()) };
            if avoid.highways {
                Pb::insert_ordered(flags, PbNode::field(1, 'b', 1));
            }
            if avoid.tolls {
                Pb::insert_ordered(flags, PbNode::field(2, 'b', 1));
            }
        }
        if avoid.ferries {
            Pb::insert_ordered(children, PbNode::field(7, 'b', 1));
        }
        Ok(())
    }

    /// The routes of a directions response, best first.
    pub fn parse_directions(&self, response: &Response) -> Result<Vec<Route>, RouteError> {
        let root = Self::json(response).map_err(|e| match e {
            GoogleError::Network(s) => RouteError::Network(s),
            other => RouteError::Server(other.to_string()),
        })?;
        let nodes = Positional::array(&root, &[0, 1]).filter(|r| !r.is_empty()).ok_or(RouteError::NoRoute)?;
        let mut routes = Vec::with_capacity(nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            let geometry = Positional::at(&root, &[0, 7, i]).map(Self::geometry).unwrap_or_default();
            if geometry.len() < 2 {
                continue;
            }
            routes.push(Self::route(node, geometry)?);
        }
        if routes.is_empty() {
            return Err(RouteError::Parse("routes without lines".into()));
        }
        Ok(routes)
    }

    /// A line as two arrays, of latitudes and of longitudes, in units of 10⁻⁷ degree: the first
    /// value absolute, each next one the difference from the one before.
    pub fn geometry(node: &Value) -> Vec<Coordinate> {
        let (Some(lats), Some(lons)) = (Positional::array(node, &[0]), Positional::array(node, &[1])) else { return Vec::new() };
        let (mut lat, mut lon) = (0i64, 0i64);
        let mut points = Vec::with_capacity(lats.len());
        for (a, b) in lats.iter().zip(lons) {
            let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) else { return Vec::new() };
            lat = lat.saturating_add(a);
            lon = lon.saturating_add(b);
            match Coordinate::checked(lat as f64 / 1e7, lon as f64 / 1e7) {
                Some(c) => points.push(c),
                None => return Vec::new(),
            }
        }
        points
    }

    fn route(node: &Value, geometry: Vec<Coordinate>) -> Result<Route, RouteError> {
        let summary = Positional::at(node, &[0]).ok_or_else(|| RouteError::Parse("route without summary".into()))?;
        let distance = Positional::f64(summary, &[2, 0]);
        let duration = Positional::f64(summary, &[3, 0]).ok_or_else(|| RouteError::Parse("route without duration".into()))?;
        let traffic = Positional::f64(summary, &[10, 0, 0]);
        let mut raw = Vec::new();
        Self::collect_steps(node, &mut raw);
        let line = Line::new(geometry.clone());
        let mut route =
            if raw.len() >= 2 { Self::with_steps(line, distance, duration, &raw) } else { Route::from_geometry(geometry, distance, duration, 35.0) };
        route.duration_in_traffic_s = traffic;
        route.summary = Positional::str(summary, &[1]).unwrap_or("").to_string();
        route.has_tolls = raw.iter().any(|s| s.text.to_ascii_lowercase().contains("toll road"));
        route.has_ferries = raw.iter().any(|s| s.kind == ManeuverKind::Ferry);
        route.validate()?;
        Ok(route)
    }

    /// The step nodes under `value`, in order: arrays with the location of the maneuver at
    /// `[7][2]` and the parts of its text at `[14]`.
    fn collect_steps(value: &Value, out: &mut Vec<RawStep>) {
        let Some(array) = value.as_array() else { return };
        if array.len() > 14
            && array[14].is_array()
            && let (Some(lat), Some(lon)) = (Positional::f64(value, &[7, 2, 2]), Positional::f64(value, &[7, 2, 3]))
            && let Some(location) = Coordinate::checked(lat, lon)
        {
            out.push(Self::raw_step(location, Positional::str(value, &[1]).unwrap_or(""), &array[14]));
            return;
        }
        for child in array {
            Self::collect_steps(child, out);
        }
    }

    /// The value of `name='…'` in a tag of `markup`.
    fn attribute<'a>(markup: &'a str, name: &str) -> Option<&'a str> {
        let start = markup.find(&format!("{name}='"))? + name.len() + 2;
        let end = markup[start..].find('\'')? + start;
        Some(&markup[start..end])
    }

    fn raw_step(location: Coordinate, markup: &str, parts: &Value) -> RawStep {
        // The parts: [kind, [text, 1 when the text is a name]].
        let mut text = String::new();
        let mut names = Vec::new();
        for part in parts.as_array().map(Vec::as_slice).unwrap_or_default() {
            if let Some(t) = Positional::str(part, &[1, 0]) {
                text.push_str(t);
                if Positional::at(part, &[1, 1]).and_then(Value::as_i64) == Some(1) && !matches!(t, "left" | "right" | "north" | "south" | "east" | "west") {
                    names.push(t.to_string());
                }
            }
        }
        let maneuver = Self::attribute(markup, "maneuver").unwrap_or("");
        let side = Self::attribute(markup, "side");
        let sharpness = Self::attribute(markup, "type");
        let lower = text.to_ascii_lowercase();
        let side = side.or(if lower.contains("left") {
            Some("LEFT")
        } else if lower.contains("right") {
            Some("RIGHT")
        } else {
            None
        });
        let modifier = match (side, sharpness) {
            (Some("LEFT"), Some("SLIGHT")) => Some(Modifier::SlightLeft),
            (Some("LEFT"), Some("SHARP")) => Some(Modifier::SharpLeft),
            (Some("LEFT"), _) => Some(Modifier::Left),
            (Some("RIGHT"), Some("SLIGHT")) => Some(Modifier::SlightRight),
            (Some("RIGHT"), Some("SHARP")) => Some(Modifier::SharpRight),
            (Some("RIGHT"), _) => Some(Modifier::Right),
            _ => None,
        };
        let (kind, modifier) = match maneuver {
            "UTURN" => (ManeuverKind::Turn, Some(Modifier::UTurn)),
            "TURN" if modifier.is_some() => (ManeuverKind::Turn, modifier),
            "ON_RAMP" => (ManeuverKind::OnRamp, modifier),
            "OFF_RAMP" => (ManeuverKind::OffRamp, modifier),
            "MERGE" => (ManeuverKind::Merge, modifier),
            "FORK" | "KEEP" => (ManeuverKind::Fork, modifier),
            "NAME_CHANGE" => (ManeuverKind::NewName, None),
            "FERRY" | "FERRY_TRAIN" => (ManeuverKind::Ferry, None),
            m if m.starts_with("ROUNDABOUT") => (ManeuverKind::Roundabout, modifier),
            "DEPART" => (ManeuverKind::Depart, None),
            _ if lower.starts_with("head") => (ManeuverKind::Depart, None),
            _ if lower.contains("u-turn") => (ManeuverKind::Turn, Some(Modifier::UTurn)),
            _ if modifier.is_some() && lower.starts_with("turn") => (ManeuverKind::Turn, modifier),
            _ => (ManeuverKind::Continue, Some(Modifier::Straight)),
        };
        let road = markup.find("<road ").or_else(|| markup.find("<road>")).and_then(|s| {
            let open = markup[s..].find('>')? + s + 1;
            let close = markup[open..].find("</road>")? + open;
            Some(markup[open..close].to_string())
        });
        let exit = if kind == ManeuverKind::Roundabout {
            ["1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th"].iter().position(|o| lower.contains(o)).map(|p| p as u32 + 1)
        } else {
            None
        };
        RawStep { location, kind, modifier, exit, name: road.or_else(|| names.into_iter().next()), text }
    }

    /// A route whose steps begin at Google's maneuvers, placed on the line in order.
    fn with_steps(line: Line, distance: Option<f64>, duration: f64, raw: &[RawStep]) -> Route {
        let length = line.length_m().max(1e-9);
        let distance = distance.unwrap_or(length);
        let geometry = line.points().to_vec();
        let last = geometry.len() - 1;
        // Each maneuver's place on the line, never before the previous one.
        let mut placed: Vec<(usize, &RawStep)> = Vec::new();
        let mut from = 0.0;
        for (n, step) in raw.iter().enumerate() {
            let index = if n == 0 {
                0
            } else {
                match line.project(&step.location, from, f64::INFINITY) {
                    Some(p) if p.distance_m < 60.0 => {
                        from = p.along_m;
                        // The point that begins the step: the nearer end of the segment.
                        let a = line.along_m(p.segment);
                        let b = line.along_m(p.segment + 1);
                        if p.along_m - a <= b - p.along_m { p.segment } else { p.segment + 1 }
                    }
                    _ => continue,
                }
            };
            if placed.last().is_some_and(|(i, _)| *i >= index && n > 0) {
                continue;
            }
            placed.push((index.min(last), step));
        }
        let mut steps = Vec::with_capacity(placed.len() + 1);
        for (n, &(index, step)) in placed.iter().enumerate() {
            let end = placed.get(n + 1).map_or(last, |p| p.0);
            let share = (line.along_m(end) - line.along_m(index)) / length;
            let kind = if n == 0 { ManeuverKind::Depart } else { step.kind };
            let mut maneuver = Maneuver::new(kind, if n == 0 { None } else { step.modifier }, geometry[index]);
            maneuver.exit = step.exit;
            maneuver.bearing_after = line.bearing_at(line.along_m(index));
            let mut out = Step::new(maneuver, distance * share, duration * share, index, end);
            out.name = step.name.clone();
            out.instruction = Some(step.text.clone()).filter(|t| !t.is_empty());
            steps.push(out);
        }
        steps.push(Step::new(Maneuver::new(ManeuverKind::Arrive, None, geometry[last]), 0.0, 0.0, last, last));
        let mut route = Route::from_geometry(geometry, Some(distance), duration, 180.0);
        route.legs[0].steps = steps;
        route
    }
}

/// Google as a router.
impl RemoteRouter for Google {
    fn request(&self, request: &RouteRequest) -> Result<Request, RouteError> {
        self.directions_request(request)
    }

    fn parse(&self, request: &RouteRequest, response: &Response) -> Result<Vec<Route>, RouteError> {
        // Google avoids tolls, highways and ferries; of the other rules it knows nothing, so a
        // route may break them.
        let supported = Avoid { tolls: true, highways: true, ferries: true, ..Avoid::default() };
        let unknown = request.avoid.strict().without(&supported);
        let mut routes = self.parse_directions(response)?;
        for route in &mut routes {
            route.ignored_avoid = route.ignored_avoid.or(&unknown);
        }
        Ok(routes)
    }
}
