use crate::{NodeFlag, WayInfo};
use osm::Tags;
use routing::TravelMode;

/// The class of a road (`highway=*`), and the ferry routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Highway {
    Motorway = 0,
    MotorwayLink = 1,
    Trunk = 2,
    TrunkLink = 3,
    Primary = 4,
    PrimaryLink = 5,
    Secondary = 6,
    SecondaryLink = 7,
    Tertiary = 8,
    TertiaryLink = 9,
    Unclassified = 10,
    Residential = 11,
    LivingStreet = 12,
    Service = 13,
    Road = 14,
    Track = 15,
    Pedestrian = 16,
    Footway = 17,
    Cycleway = 18,
    Path = 19,
    Steps = 20,
    Bridleway = 21,
    Ferry = 22,
}

impl Highway {
    pub const COUNT: u8 = 23;

    pub fn from_u8(value: u8) -> Option<Highway> {
        use Highway::*;
        const ALL: [Highway; 23] = [
            Motorway,
            MotorwayLink,
            Trunk,
            TrunkLink,
            Primary,
            PrimaryLink,
            Secondary,
            SecondaryLink,
            Tertiary,
            TertiaryLink,
            Unclassified,
            Residential,
            LivingStreet,
            Service,
            Road,
            Track,
            Pedestrian,
            Footway,
            Cycleway,
            Path,
            Steps,
            Bridleway,
            Ferry,
        ];
        ALL.get(value as usize).copied()
    }

    /// The class of a way, or `None` for a way that is not a road or a ferry.
    pub fn of(tags: &Tags) -> Option<Highway> {
        use Highway::*;
        if tags.has("route", "ferry") {
            return Some(Ferry);
        }
        Some(match tags.get("highway")? {
            "motorway" => Motorway,
            "motorway_link" => MotorwayLink,
            "trunk" => Trunk,
            "trunk_link" => TrunkLink,
            "primary" => Primary,
            "primary_link" => PrimaryLink,
            "secondary" => Secondary,
            "secondary_link" => SecondaryLink,
            "tertiary" => Tertiary,
            "tertiary_link" => TertiaryLink,
            "unclassified" => Unclassified,
            "residential" => Residential,
            "living_street" => LivingStreet,
            "service" => Service,
            "road" => Road,
            "track" => Track,
            "pedestrian" => Pedestrian,
            "footway" => Footway,
            "cycleway" => Cycleway,
            "path" => Path,
            "steps" => Steps,
            "bridleway" => Bridleway,
            _ => return None,
        })
    }

    pub fn is_link(self) -> bool {
        matches!(self, Highway::MotorwayLink | Highway::TrunkLink | Highway::PrimaryLink | Highway::SecondaryLink | Highway::TertiaryLink)
    }

    /// Motorways and their links: what "avoid highways" avoids.
    pub fn is_motorway(self) -> bool {
        matches!(self, Highway::Motorway | Highway::MotorwayLink)
    }
}

/// What a mode of travel may use and how fast it goes: the rules that turn the tags of a way
/// into access and speed. Each mode is a `Profile`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub mode: TravelMode,
}

impl Profile {
    /// The fastest speed of each mode in km/h, for the heuristic of the search: no edge may be
    /// faster.
    pub const CAR_MAX_KMH: f64 = 140.0;
    pub const BICYCLE_MAX_KMH: f64 = 25.0;
    pub const FOOT_MAX_KMH: f64 = 5.0;
    /// The speed of a ferry without a duration, in km/h.
    pub const FERRY_KMH: f64 = 20.0;
    /// Cycling and walking on the flat, in km/h.
    pub const BICYCLE_FLAT_KMH: f64 = 16.0;
    pub const FOOT_FLAT_KMH: f64 = 4.8;
    /// The speed of a bicycle on a gradient, as a factor of its speed on the flat, by the gradient
    /// in percent: the table of Valhalla's bicycle costing (`kGradeBasedSpeedFactor` of
    /// `src/sif/bicyclecost.cc`), interpolated between its points and held beyond its ends.
    const BICYCLE_GRADES: [(f64, f64); 16] = [
        (-10.0, 2.2),
        (-8.0, 2.0),
        (-6.5, 1.9),
        (-5.0, 1.7),
        (-3.0, 1.4),
        (-1.5, 1.2),
        (0.0, 1.0),
        (1.5, 0.95),
        (3.0, 0.85),
        (5.0, 0.75),
        (6.5, 0.65),
        (8.0, 0.55),
        (10.0, 0.5),
        (11.5, 0.45),
        (13.0, 0.4),
        (15.0, 0.3),
    ];

    pub fn bicycle_grade_factor(percent: f64) -> f64 {
        let table = &Self::BICYCLE_GRADES;
        if !percent.is_finite() || percent <= table[0].0 {
            return table[0].1;
        }
        for w in table.windows(2) {
            let ((g0, f0), (g1, f1)) = (w[0], w[1]);
            if percent <= g1 {
                return f0 + (f1 - f0) * (percent - g0) / (g1 - g0);
            }
        }
        table[table.len() - 1].1
    }

    pub fn new(mode: TravelMode) -> Profile {
        Profile { mode }
    }

    pub fn max_kmh(&self) -> f64 {
        match self.mode {
            TravelMode::Car => Self::CAR_MAX_KMH,
            TravelMode::Bicycle => Self::BICYCLE_MAX_KMH,
            _ => Self::FOOT_MAX_KMH,
        }
    }

    /// Whether a way of these tags may be used at all, and in which directions:
    /// `(forward, backward)`.
    pub fn access(&self, highway: Highway, tags: &Tags) -> (bool, bool) {
        use Highway::*;
        let yes = |v: Option<&str>| matches!(v, Some("yes" | "designated" | "permissive" | "destination" | "customers" | "delivery"));
        let no = |v: Option<&str>| matches!(v, Some("no" | "private" | "agricultural" | "forestry" | "use_sidepath" | "discouraged"));
        // The most specific tag decides: motorcar over motor_vehicle over vehicle over access.
        let specific: &[&str] = match self.mode {
            TravelMode::Car => &["motorcar", "motor_vehicle", "vehicle", "access"],
            TravelMode::Bicycle => &["bicycle", "vehicle", "access"],
            _ => &["foot", "access"],
        };
        let decided = specific.iter().find_map(|k| tags.get(k));
        let allowed_by_class = match self.mode {
            TravelMode::Car => match highway {
                Motorway | MotorwayLink | Trunk | TrunkLink | Primary | PrimaryLink | Secondary | SecondaryLink | Tertiary | TertiaryLink | Unclassified
                | Residential | LivingStreet | Service | Road => true,
                // Ferries carry cars only when they say so.
                Ferry => yes(tags.get("motorcar").or(tags.get("motor_vehicle"))),
                _ => false,
            },
            TravelMode::Bicycle => match highway {
                Motorway | MotorwayLink => false,
                Footway | Pedestrian => yes(tags.get("bicycle")),
                Steps => false,
                Ferry => !tags.has("bicycle", "no"),
                _ => true,
            },
            _ => match highway {
                Motorway | MotorwayLink => false,
                Trunk | TrunkLink => !tags.has("motorroad", "yes"),
                Ferry => !tags.has("foot", "no"),
                _ => true,
            },
        };
        let allowed = if no(decided) {
            false
        } else if yes(decided) && highway != Ferry {
            // An explicit permission opens a class that is otherwise closed, except motorways
            // to walking and cycling.
            allowed_by_class || !(highway.is_motorway() && self.mode != TravelMode::Car)
        } else {
            allowed_by_class
        };
        if !allowed {
            return (false, false);
        }
        match self.oneway(highway, tags) {
            1 => (true, false),
            -1 => (false, true),
            _ => (true, true),
        }
    }

    /// The direction a way is one-way in for this mode: 1 forward, -1 backward, 0 both.
    fn oneway(&self, highway: Highway, tags: &Tags) -> i8 {
        let parse = |v: Option<&str>| match v {
            Some("yes" | "true" | "1") => Some(1),
            Some("-1" | "reverse") => Some(-1),
            Some("no" | "false" | "0") => Some(0),
            _ => None,
        };
        let general = parse(tags.get("oneway")).unwrap_or_else(|| {
            let roundabout = matches!(tags.get("junction"), Some("roundabout" | "circular"));
            i8::from(roundabout || highway == Highway::Motorway)
        });
        match self.mode {
            TravelMode::Car => parse(tags.get("oneway:motorcar")).or(parse(tags.get("oneway:motor_vehicle"))).unwrap_or(general),
            TravelMode::Bicycle => {
                if let Some(v) = parse(tags.get("oneway:bicycle")) {
                    return v;
                }
                if tags.get("cycleway").is_some_and(|c| c.starts_with("opposite")) {
                    return 0;
                }
                general
            }
            _ => parse(tags.get("oneway:foot")).unwrap_or(0),
        }
    }

    /// The speed of a car on a road of this class without a limit, in km/h: typical speeds, a
    /// little below the usual limits, since a route is not driven at the limit throughout.
    pub fn car_default_kmh(highway: Highway) -> f64 {
        use Highway::*;
        match highway {
            Motorway => 90.0,
            MotorwayLink => 45.0,
            Trunk => 60.0,
            TrunkLink => 40.0,
            Primary => 50.0,
            PrimaryLink => 35.0,
            Secondary => 45.0,
            SecondaryLink => 30.0,
            Tertiary => 40.0,
            TertiaryLink => 25.0,
            Unclassified => 30.0,
            Residential | Road => 25.0,
            Service => 15.0,
            LivingStreet => 10.0,
            Track => 15.0,
            Ferry => Self::FERRY_KMH,
            _ => 10.0,
        }
    }

    /// The speed on a way, in km/h, on the flat: by the class of the road, its limit and its
    /// surface (`flags` of `WayInfo`). For a ferry, `maxspeed_kmh` is its speed over its whole
    /// crossing, from its duration.
    pub fn speed_kmh(&self, highway: Highway, maxspeed_kmh: u8, flags: u16) -> f64 {
        if highway == Highway::Ferry {
            return if maxspeed_kmh > 0 { maxspeed_kmh as f64 } else { Self::FERRY_KMH };
        }
        let surface = |unpaved: f64, rough: f64| {
            let mut f = 1.0;
            if flags & WayInfo::UNPAVED != 0 {
                f *= unpaved;
            }
            if flags & WayInfo::ROUGH != 0 {
                f *= rough;
            }
            f
        };
        match self.mode {
            TravelMode::Car => {
                let base = if maxspeed_kmh > 0 {
                    // Below the limit, and never faster than the class allows for long.
                    (maxspeed_kmh as f64 * 0.85).clamp(5.0, Self::CAR_MAX_KMH)
                } else {
                    Self::car_default_kmh(highway)
                };
                let narrow = if flags & WayInfo::NARROW != 0 { 0.8 } else { 1.0 };
                (base * surface(0.6, 0.7) * narrow).max(5.0)
            }
            TravelMode::Bicycle => {
                let base = match highway {
                    Highway::Track | Highway::Path | Highway::Bridleway => 12.0,
                    Highway::Footway | Highway::Pedestrian => 8.0,
                    Highway::LivingStreet => 12.0,
                    Highway::Steps => 2.0,
                    _ => Self::BICYCLE_FLAT_KMH,
                };
                base * surface(0.7, 0.7)
            }
            _ => match highway {
                Highway::Steps => 2.5,
                _ => Self::FOOT_FLAT_KMH * surface(0.95, 0.9),
            },
        }
    }

    /// The time in seconds to travel `length_m` metres at `flat_kmh` on the flat, climbing `up_m`
    /// and descending `down_m` metres on the way.
    ///
    /// The climb and the descent are taken to be spread evenly over the length at one gradient,
    /// `(up + down) / length`, the climbing part over `up / (up + down)` of it: exact for a road
    /// that only climbs or only descends.
    /// - Walking: Tobler's hiking function, scaled to `flat_kmh` on the flat.
    /// - Cycling: `flat_kmh` times the factor of the gradient (`bicycle_grade_factor`).
    /// - Driving: the gradient does not change the time.
    pub fn graded_time_s(&self, length_m: f64, flat_kmh: f64, up_m: f64, down_m: f64) -> f64 {
        let flat_mps = flat_kmh / 3.6;
        if length_m <= 0.0 || flat_mps <= 0.0 {
            return 0.0;
        }
        let change = (up_m.max(0.0) + down_m.max(0.0)).min(length_m);
        if change < 1e-6 || self.mode == TravelMode::Car {
            return length_m / flat_mps;
        }
        let grade = change / length_m;
        let up_length = length_m * up_m.max(0.0) / (up_m.max(0.0) + down_m.max(0.0));
        let down_length = length_m - up_length;
        let (up_speed, down_speed) = match self.mode {
            TravelMode::Bicycle => (flat_mps * Self::bicycle_grade_factor(grade * 100.0), flat_mps * Self::bicycle_grade_factor(-grade * 100.0)),
            _ => {
                let tobler = |s: f64| 6.0 * (-3.5 * (s + 0.05).abs()).exp();
                let scale = flat_mps / tobler(0.0);
                (tobler(grade) * scale, tobler(-grade) * scale)
            }
        };
        up_length / up_speed + down_length / down_speed
    }

    /// How much less a road of this class is worth than its time says, for choosing routes:
    /// main roads are preferred to side streets of the same time, as drivers prefer them (they
    /// are wider, have priority and fewer surprises). It never changes the times told.
    pub fn preference(&self, highway: Highway) -> f32 {
        use Highway::*;
        match self.mode {
            TravelMode::Car => match highway {
                Residential | Road => 1.4,
                Service | LivingStreet | Track => 2.0,
                Unclassified => 1.2,
                _ => 1.0,
            },
            TravelMode::Bicycle => match highway {
                Motorway | MotorwayLink | Trunk | TrunkLink => 1.5,
                Primary | PrimaryLink => 1.2,
                Cycleway => 0.9,
                _ => 1.0,
            },
            _ => match highway {
                Trunk | TrunkLink | Primary | PrimaryLink => 1.1,
                _ => 1.0,
            },
        }
    }

    /// The flags of a way from its tags (`WayInfo::UNPAVED`, `NARROW`, `LIT`…), besides the toll
    /// and the roundabout.
    pub fn way_flags(highway: Highway, tags: &Tags) -> u16 {
        let mut flags = 0;
        let unpaved_surface = matches!(
            tags.get("surface"),
            Some(
                "unpaved"
                    | "compacted"
                    | "fine_gravel"
                    | "gravel"
                    | "shells"
                    | "rock"
                    | "pebblestone"
                    | "ground"
                    | "dirt"
                    | "earth"
                    | "mud"
                    | "laterite"
                    | "grass"
                    | "sand"
                    | "woodchips"
                    | "snow"
                    | "ice"
                    | "salt"
            )
        );
        // A track is unpaved unless it says otherwise (its surface, or grade 1: solid).
        let unpaved_track = highway == Highway::Track && tags.get("surface").is_none() && !tags.has("tracktype", "grade1");
        if unpaved_surface || unpaved_track {
            flags |= WayInfo::UNPAVED;
        }
        if matches!(tags.get("smoothness"), Some("bad" | "very_bad" | "horrible" | "very_horrible" | "impassable")) {
            flags |= WayInfo::ROUGH;
        }
        let width = Self::parse_width(tags.get("width")).or(Self::parse_width(tags.get("est_width")));
        let single_lane = tags.has("lanes", "1") && !tags.has("oneway", "yes");
        let side_street =
            matches!(highway, Highway::Residential | Highway::Unclassified | Highway::Service | Highway::Road | Highway::LivingStreet | Highway::Track);
        let alley = highway == Highway::Service && matches!(tags.get("service"), Some("alley" | "driveway"));
        if width.is_some_and(|w| w < Self::NARROW_WIDTH_M)
            || tags.has("narrow", "yes")
            || (single_lane && side_street)
            || alley
            || highway == Highway::LivingStreet
        {
            flags |= WayInfo::NARROW;
        }
        match tags.get("lit") {
            Some("yes" | "24/7" | "automatic" | "interval" | "limited") => flags |= WayInfo::LIT,
            Some("no" | "disused") => flags |= WayInfo::UNLIT,
            _ => {}
        }
        if tags.get("bridge").is_some_and(|b| b != "no") {
            flags |= WayInfo::BRIDGE;
        }
        if tags.get("tunnel").is_some_and(|t| t != "no") {
            flags |= WayInfo::TUNNEL;
        }
        let lane = |v: Option<&str>| matches!(v, Some("lane" | "track" | "opposite_lane" | "opposite_track" | "share_busway" | "separate"));
        if lane(tags.get("cycleway")) || lane(tags.get("cycleway:both")) || lane(tags.get("cycleway:left")) || lane(tags.get("cycleway:right")) {
            flags |= WayInfo::CYCLE_LANE;
        }
        flags
    }

    /// The flags of a node from its tags (`NodeFlag`): signals, stop signs, level crossings,
    /// speed cameras, toll booths, the junctions of motorways, and the barriers each mode cannot
    /// pass.
    pub fn node_flags(tags: &Tags) -> u16 {
        let mut flags = 0;
        let highway = tags.get("highway");
        if highway == Some("traffic_signals") || tags.has("crossing", "traffic_signals") {
            flags |= NodeFlag::SIGNALS;
        }
        match highway {
            Some("stop") => flags |= NodeFlag::STOP,
            Some("give_way") => flags |= NodeFlag::GIVE_WAY,
            Some("speed_camera") => flags |= NodeFlag::SPEED_CAMERA,
            Some("motorway_junction") => flags |= NodeFlag::MOTORWAY_JUNCTION,
            Some("crossing") => flags |= NodeFlag::CROSSING,
            _ => {}
        }
        if matches!(tags.get("railway"), Some("level_crossing" | "crossing")) {
            flags |= NodeFlag::LEVEL_CROSSING;
        }
        if let Some(barrier) = tags.get("barrier") {
            if barrier == "toll_booth" {
                flags |= NodeFlag::TOLL_BOOTH;
            }
            for mode in [TravelMode::Car, TravelMode::Bicycle, TravelMode::Foot] {
                if Profile::new(mode).blocked_by(barrier, tags) {
                    flags |= NodeFlag::blocks(mode);
                }
            }
        }
        flags
    }

    /// Whether this mode cannot pass a node of `barrier=<barrier>` with these tags. The access
    /// tags of the node decide first (as for ways); then the kind of barrier.
    pub fn blocked_by(&self, barrier: &str, tags: &Tags) -> bool {
        let specific: &[&str] = match self.mode {
            TravelMode::Car => &["motorcar", "motor_vehicle", "vehicle", "access"],
            TravelMode::Bicycle => &["bicycle", "vehicle", "access"],
            _ => &["foot", "access"],
        };
        match specific.iter().find_map(|k| tags.get(k)) {
            Some("yes" | "designated" | "permissive" | "destination" | "customers" | "delivery") => return false,
            Some("no" | "private") => return true,
            _ => {}
        }
        if tags.has("locked", "yes") {
            return true;
        }
        match self.mode {
            // What a car may pass: the list of OSRM's car profile, and gates that open (with a wait,
            // `Search::barrier_delay`). Any other barrier stops it.
            TravelMode::Car => !matches!(
                barrier,
                "cattle_grid"
                    | "border_control"
                    | "toll_booth"
                    | "sally_port"
                    | "no"
                    | "entrance"
                    | "height_restrictor"
                    | "arch"
                    | "gate"
                    | "lift_gate"
                    | "swing_gate"
                    | "sliding_gate"
            ),
            // A bicycle passes all but walls and fences (OSRM's bicycle profile) and what the wiki
            // describes as for walkers only (stiles, turnstiles, kissing gates).
            TravelMode::Bicycle => {
                matches!(
                    barrier,
                    "yes" | "wall" | "fence" | "hedge" | "city_wall" | "retaining_wall" | "stile" | "turnstile" | "full-height_turnstile" | "kissing_gate"
                )
            }
            _ => matches!(barrier, "yes" | "wall" | "fence" | "hedge" | "city_wall" | "retaining_wall" | "full-height_turnstile"),
        }
    }

    /// The width below which a road is narrow, in metres: two cars cannot pass at speed.
    pub const NARROW_WIDTH_M: f64 = 4.0;

    /// Reads `width` in metres: `4`, `4.5 m`, `12'6"` is not read (0 or unknown is `None`).
    pub fn parse_width(value: Option<&str>) -> Option<f64> {
        let v = value?.trim();
        let number = v.strip_suffix('m').unwrap_or(v).trim();
        number.parse::<f64>().ok().filter(|w| w.is_finite() && *w > 0.0)
    }

    /// Reads `incline` as a percentage along the way: `10%`, `-5%`, `8°` (in degrees). `up` and
    /// `down` say no amount and are not read.
    pub fn parse_incline(value: Option<&str>) -> Option<f64> {
        let v = value?.trim();
        let percent = if let Some(n) = v.strip_suffix('%') {
            n.trim().parse::<f64>().ok()?
        } else {
            let n = v.strip_suffix('°')?;
            n.trim().parse::<f64>().ok()?.to_radians().tan() * 100.0
        };
        percent.is_finite().then_some(percent.clamp(-100.0, 100.0))
    }

    /// Reads `maxspeed` in km/h: a number, `N mph`, or 0 when unknown (`signals`, `none`, a
    /// zone code).
    pub fn parse_maxspeed(value: Option<&str>) -> u8 {
        let Some(v) = value.map(str::trim) else { return 0 };
        let (number, factor) = match v.strip_suffix("mph") {
            Some(n) => (n.trim(), 1.609_344),
            None => (v.strip_suffix("km/h").unwrap_or(v).trim(), 1.0),
        };
        match number.parse::<f64>() {
            Ok(n) if n > 0.0 => (n * factor).round().clamp(1.0, 254.0) as u8,
            _ => 0,
        }
    }

    /// The duration of a ferry from its `duration` tag (`HH:MM`, `HH:MM:SS` or minutes), in
    /// seconds.
    pub fn parse_duration(value: Option<&str>) -> Option<u32> {
        let v = value?.trim();
        let parts: Vec<&str> = v.split(':').collect();
        let numbers: Option<Vec<u32>> = parts.iter().map(|p| p.trim().parse().ok()).collect();
        match numbers?.as_slice() {
            [m] => m.checked_mul(60),
            [h, m] => h.checked_mul(3600)?.checked_add(m.checked_mul(60)?),
            [h, m, s] => h.checked_mul(3600)?.checked_add(m.checked_mul(60)?)?.checked_add(*s),
            _ => None,
        }
    }
}
