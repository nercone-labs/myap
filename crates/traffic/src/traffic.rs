use crate::{FlowSegment, Incident};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

/// The traffic of an area: the flow on its roads and its incidents.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Traffic {
    #[serde(default)]
    pub flow: Vec<FlowSegment>,
    #[serde(default)]
    pub incidents: Vec<Incident>,
    /// When the provider last updated the data, in milliseconds since the Unix epoch; after a
    /// [`Traffic::merge`], the oldest of the parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_ms: Option<i64>,
}

impl Traffic {
    /// Adds `other` to this traffic, as when joining the answers for the pieces of an area
    /// ([`crate::TrafficProvider::pieces`]). A provider answers for every road that touches an
    /// area, so neighbouring pieces repeat the roads on their common edge: a flow segment of the
    /// same shape as one already held, and an incident of an `id` already held, are kept once
    /// (the one held first). `updated_ms` becomes the older of the two, so that it never
    /// claims data fresher than its oldest part.
    pub fn merge(&mut self, other: Traffic) {
        let mut shapes: HashMap<u64, Vec<usize>> = HashMap::with_capacity(self.flow.len() + other.flow.len());
        for (i, segment) in self.flow.iter().enumerate() {
            shapes.entry(Self::shape_hash(segment)).or_default().push(i);
        }
        for segment in other.flow {
            let indices = shapes.entry(Self::shape_hash(&segment)).or_default();
            if indices.iter().any(|&i| self.flow[i].shape == segment.shape) {
                continue;
            }
            indices.push(self.flow.len());
            self.flow.push(segment);
        }
        let mut ids: HashSet<String> = self.incidents.iter().map(|i| i.id.clone()).collect();
        for incident in other.incidents {
            if ids.insert(incident.id.clone()) {
                self.incidents.push(incident);
            }
        }
        self.updated_ms = match (self.updated_ms, other.updated_ms) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }

    /// A hash of the exact shape of `segment` (the bits of its coordinates).
    fn shape_hash(segment: &FlowSegment) -> u64 {
        let mut hasher = DefaultHasher::new();
        for point in &segment.shape {
            point.latitude.to_bits().hash(&mut hasher);
            point.longitude.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }
}
