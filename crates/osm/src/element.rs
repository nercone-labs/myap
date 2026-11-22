use geo::Coordinate;
use serde::{Deserialize, Serialize};

/// The tags of an element, in the order they were read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tags(pub Vec<(String, String)>);

impl Tags {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn has(&self, key: &str, value: &str) -> bool {
        self.get(key) == Some(value)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: i64,
    pub coordinate: Coordinate,
    pub tags: Tags,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Way {
    pub id: i64,
    /// The ids of the nodes, in order.
    pub nodes: Vec<i64>,
    pub tags: Tags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberKind {
    Node,
    Way,
    Relation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub kind: MemberKind,
    pub id: i64,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    pub id: i64,
    pub members: Vec<Member>,
    pub tags: Tags,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Node(Node),
    Way(Way),
    Relation(Relation),
}
