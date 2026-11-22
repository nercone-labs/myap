use crate::Graph;
use geo::{Bounds, Coordinate};

/// A grid over the edges of a graph, to find the roads near a point.
///
/// The cells are held in compressed rows, sorted by cell: the edges of cell `cells[i]` are
/// `edges[first[i]..first[i + 1]]`. This is three vectors for the whole grid, where a map of a
/// vector per cell costs an allocation and the overhead of a map entry for each of thousands of
/// cells.
pub struct SpatialIndex {
    cells: Vec<u64>,
    first: Vec<u32>,
    edges: Vec<u32>,
}

impl SpatialIndex {
    /// About 550 m of latitude: a point is searched in its cell and those around it first.
    pub const CELL_DEGREES: f64 = 0.005;
    /// A segment crossing more cells than this is sampled along its length (a ferry, a long
    /// straight road), instead of filling every cell of its box.
    const SAMPLED_STEPS: f64 = 16.0;

    pub fn new(graph: &Graph) -> SpatialIndex {
        let mut pairs: Vec<(u64, u32)> = Vec::new();
        for (index, edge) in graph.edges.iter().enumerate() {
            let index = index as u32;
            // One direction of each road is enough; the search finds its twin.
            if edge.is_reversed() && graph.twin(index).is_some() {
                continue;
            }
            let start = edge.shape_start as usize;
            let points = &graph.shapes[start..start + edge.shape_len as usize];
            for pair in points.windows(2) {
                let (a, b) = (Graph::coordinate(pair[0]), Graph::coordinate(pair[1]));
                let steps = ((a.latitude - b.latitude).abs().max((a.longitude - b.longitude).abs()) / Self::CELL_DEGREES * 4.0).ceil();
                if steps > Self::SAMPLED_STEPS {
                    // A quarter of a cell apart.
                    let steps = steps.min(1_000_000.0) as usize;
                    for k in 0..=steps {
                        let p = a.lerp(&b, k as f64 / steps as f64);
                        pairs.push((Self::cell_of(&p), index));
                    }
                    continue;
                }
                let (y0, y1) = (Self::key(a.latitude.min(b.latitude)), Self::key(a.latitude.max(b.latitude)));
                let (x0, x1) = (Self::key(a.longitude.min(b.longitude)), Self::key(a.longitude.max(b.longitude)));
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        pairs.push((Self::cell(y, x), index));
                    }
                }
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        let mut cells: Vec<u64> = Vec::new();
        let mut first: Vec<u32> = Vec::new();
        let mut edges: Vec<u32> = Vec::with_capacity(pairs.len());
        for (cell, edge) in pairs {
            if cells.last() != Some(&cell) {
                cells.push(cell);
                first.push(edges.len() as u32);
            }
            edges.push(edge);
        }
        first.push(edges.len() as u32);
        cells.shrink_to_fit();
        first.shrink_to_fit();
        SpatialIndex { cells, first, edges }
    }

    fn key(degrees: f64) -> i32 {
        (degrees / Self::CELL_DEGREES).floor() as i32
    }

    /// The cell of row `y` and column `x`, as one number that sorts by row, then column.
    fn cell(y: i32, x: i32) -> u64 {
        ((y as u32 as u64) << 32) | x as u32 as u64
    }

    fn cell_of(point: &Coordinate) -> u64 {
        Self::cell(Self::key(point.latitude), Self::key(point.longitude))
    }

    /// The edges of the cell of row `y` and column `x`.
    fn edges_of(&self, y: i32, x: i32) -> &[u32] {
        match self.cells.binary_search(&Self::cell(y, x)) {
            Ok(i) => &self.edges[self.first[i] as usize..self.first[i + 1] as usize],
            Err(_) => &[],
        }
    }

    /// The edges in the cells within `rings` cells of `point`. An edge in several of these cells
    /// is given once for each.
    pub fn near(&self, point: &Coordinate, rings: i32) -> impl Iterator<Item = u32> + '_ {
        let (y, x) = (Self::key(point.latitude), Self::key(point.longitude));
        (-rings..=rings).flat_map(move |dy| (-rings..=rings).flat_map(move |dx| self.edges_of(y + dy, x + dx).iter().copied()))
    }

    /// The edges in the cells that `bounds` covers. An edge in several of these cells is given
    /// once for each.
    pub fn within(&self, bounds: &Bounds) -> impl Iterator<Item = u32> + '_ {
        let (y0, y1) = (Self::key(bounds.south), Self::key(bounds.north));
        let (x0, x1) = (Self::key(bounds.west), Self::key(bounds.east));
        (y0..=y1).flat_map(move |y| (x0..=x1).flat_map(move |x| self.edges_of(y, x).iter().copied()))
    }
}
