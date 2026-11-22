use crate::protocol::{ElevationInput, GraphFormat, GraphStats};
use myap_router::{Graph, GraphBuilder};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;

/// A build of a graph of Myap Router, run by the platform away from the thread of the core
/// (`Effect::BuildGraph`), since it takes seconds to minutes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphBuild {
    pub input: String,
    pub format: GraphFormat,
    pub output: String,
    /// The elevations to add, from terrain tiles on the device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation: Option<ElevationInput>,
}

impl GraphBuild {
    /// Builds the graph, writing it to a temporary file that then replaces `output`, so that a
    /// failed build leaves no half graph.
    pub fn run(&self) -> Result<GraphStats, String> {
        let mut graph = match self.format {
            GraphFormat::Pbf => GraphBuilder::from_pbf(Path::new(&self.input), |_| {}).map_err(|e| e.to_string())?.0,
            GraphFormat::OverpassJson => {
                let file = File::open(&self.input).map_err(|e| e.to_string())?;
                let elements = osm::Overpass::parse(file).map_err(|e| e.to_string())?;
                GraphBuilder::from_elements(&elements).map_err(|e| e.to_string())?.0
            }
            GraphFormat::Graph => Graph::read(&mut File::open(&self.input).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
        };
        let mut elevations = 0;
        if let Some(e) = &self.elevation {
            // Tiles that cannot be read (the sea, a broken download) are left out: their points
            // take the nearest known height along their road, or none.
            let (dem, _skipped) = elevation::Dem::load_directory(Path::new(&e.directory), e.zoom, e.encoding).map_err(|e| e.to_string())?;
            elevations = graph.set_elevations(|c| dem.height(c));
        }
        let output = Path::new(&self.output);
        let temporary = output.with_extension("tmp");
        {
            let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
            graph.write(&mut file).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        std::fs::rename(&temporary, output).map_err(|e| e.to_string())?;
        let size_bytes = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
        Ok(GraphStats { nodes: graph.nodes.len(), edges: graph.edges.len(), bounds: graph.bounds, size_bytes, elevations })
    }

    /// Runs the build given as JSON, and answers with the JSON of `Result<GraphStats, String>`
    /// (`{"Ok": …}` or `{"Err": …}`): what the C interface calls.
    pub fn run_json(input: &str) -> String {
        let result: Result<GraphStats, String> = serde_json::from_str::<GraphBuild>(input).map_err(|e| format!("unreadable build: {e}")).and_then(|b| b.run());
        serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"Err":"{e}"}}"#))
    }
}
