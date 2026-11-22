use crate::{Element, Member, MemberKind, Node, Relation, Tags, Way};
use fetch::{Query, Request};
use geo::{Bounds, Coordinate};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;

/// The Overpass API (<https://wiki.openstreetmap.org/wiki/Overpass_API>): the request for a query
/// in Overpass QL, and its JSON output.
pub struct Overpass;

#[derive(Debug)]
pub enum OverpassError {
    Json(serde_json::Error),
    /// The server answered with a remark instead of data (a timeout or an exhausted memory).
    Remark(String),
}

impl fmt::Display for OverpassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverpassError::Json(e) => write!(f, "{e}"),
            OverpassError::Remark(r) => write!(f, "Overpass: {r}"),
        }
    }
}

impl std::error::Error for OverpassError {}

#[derive(Deserialize)]
struct Output {
    #[serde(default)]
    elements: Vec<RawElement>,
    #[serde(default)]
    remark: Option<String>,
}

#[derive(Deserialize)]
struct RawElement {
    #[serde(rename = "type")]
    kind: String,
    id: i64,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    nodes: Vec<i64>,
    #[serde(default)]
    members: Vec<RawMember>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct RawMember {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "ref")]
    id: i64,
    #[serde(default)]
    role: String,
}

impl Overpass {
    /// The public instance of the OpenStreetMap community, run by FOSSGIS.
    pub const DEFAULT_SERVER: &'static str = "https://overpass-api.de/api/interpreter";

    /// The request that runs `query` (Overpass QL) on `server`.
    pub fn request(server: &str, query: &str, user_agent: &str) -> Request {
        Request::post(server, "application/x-www-form-urlencoded", Query::new().pair("data", query).as_str())
            .header("User-Agent", user_agent)
            .timeout_ms(300_000)
    }

    /// `bounds` as the bounding box of Overpass QL: `south,west,north,east`.
    pub fn bbox(bounds: &Bounds) -> String {
        format!("{:.7},{:.7},{:.7},{:.7}", bounds.south, bounds.west, bounds.north, bounds.east)
    }

    /// Reads the JSON output (`[out:json]`). Elements of unknown types and nodes without a valid
    /// position are skipped; tags are sorted by key.
    pub fn parse(reader: impl Read) -> Result<Vec<Element>, OverpassError> {
        let output: Output = serde_json::from_reader(std::io::BufReader::new(reader)).map_err(OverpassError::Json)?;
        if output.elements.is_empty()
            && let Some(remark) = output.remark
        {
            return Err(OverpassError::Remark(remark));
        }
        Ok(output.elements.into_iter().filter_map(Self::element).collect())
    }

    fn element(raw: RawElement) -> Option<Element> {
        let tags = Tags(raw.tags.into_iter().collect());
        match raw.kind.as_str() {
            "node" => {
                let coordinate = Coordinate::checked(raw.lat?, raw.lon?)?;
                Some(Element::Node(Node { id: raw.id, coordinate, tags }))
            }
            "way" => Some(Element::Way(Way { id: raw.id, nodes: raw.nodes, tags })),
            "relation" => {
                let members = raw
                    .members
                    .into_iter()
                    .filter_map(|m| {
                        let kind = match m.kind.as_str() {
                            "node" => MemberKind::Node,
                            "way" => MemberKind::Way,
                            "relation" => MemberKind::Relation,
                            _ => return None,
                        };
                        Some(Member { kind, id: m.id, role: m.role })
                    })
                    .collect();
                Some(Element::Relation(Relation { id: raw.id, members, tags }))
            }
            _ => None,
        }
    }
}
