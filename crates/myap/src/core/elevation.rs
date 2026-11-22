//! The elevations of the regions of Myap Router: once a region is built, the terrain tiles over
//! it are downloaded (a few at a time, each to a file) and added to its graph by a second build.

use super::{Core, ElevationDownload, Pending};
use crate::protocol::{Effect, ElevationInput, GraphFormat, GraphStats};
use crate::regions::RegionStatus;
use crate::settings::ElevationSource;
use fetch::Request;
use geo::Bounds;

impl Core {
    /// The source of terrain tiles for `bounds`: the GSI's only within Japan, else Terrarium.
    fn elevation_source(choice: ElevationSource, bounds: &Bounds) -> Option<(ElevationSource, elevation::Source, u8)> {
        let japan = Bounds { south: 20.0, west: 122.0, north: 46.0, east: 154.0 };
        let inside = |b: &Bounds| b.south >= japan.south && b.north <= japan.north && b.west >= japan.west && b.east <= japan.east;
        match choice {
            ElevationSource::Off => None,
            // DEM10B: about 10 m a pixel at zoom 14.
            ElevationSource::Gsi if inside(bounds) => Some((ElevationSource::Gsi, elevation::Source::gsi(), 14)),
            // About 30 m a pixel at zoom 12, near the resolution of the data over most land.
            _ => Some((ElevationSource::Terrarium, elevation::Source::terrarium(), 12)),
        }
    }

    /// Begins the download of the elevations of region `id` over `bounds`, when the settings ask
    /// for them. Returns whether it began.
    pub(super) fn start_elevation(&mut self, id: &str, bounds: &Bounds) -> bool {
        let Some((kind, source, preferred)) = Self::elevation_source(self.settings.elevation, bounds) else { return false };
        if self.store.subdirectory(crate::store::Store::DOWNLOADS_DIRECTORY).is_none() {
            return false;
        }
        let Some(directory) = self.dem_directory(id) else { return false };
        if std::fs::create_dir_all(&directory).is_err() {
            return false;
        }
        let zoom = source.zoom_for(bounds, Self::ELEVATION_MAX_TILES, preferred);
        let tiles = source.tiles(bounds, zoom);
        if tiles.is_empty() || tiles.len() > Self::ELEVATION_MAX_TILES {
            let _ = std::fs::remove_dir_all(&directory);
            return false;
        }
        let total = tiles.len();
        self.elevation_downloads.insert(
            id.to_string(),
            ElevationDownload { source: kind, zoom, directory: directory.to_string_lossy().into_owned(), waiting: tiles.into(), in_flight: 0, total },
        );
        if let Some(region) = self.regions.regions.iter_mut().find(|r| r.id == id) {
            region.status = RegionStatus::Elevating;
            region.progress = Some(0.0);
        }
        self.pump_elevation(id);
        true
    }

    /// Sends the next tiles of region `id`, a few at a time; once all came, the build that adds
    /// them to the graph.
    fn pump_elevation(&mut self, id: &str) {
        let Some(download) = self.elevation_downloads.get(id).cloned() else { return };
        let source = match download.source {
            ElevationSource::Gsi => elevation::Source::gsi(),
            _ => elevation::Source::terrarium(),
        };
        let mut download = download;
        while download.in_flight < Self::ELEVATION_PARALLEL
            && let Some(tile) = download.waiting.pop_front()
        {
            let path = std::path::Path::new(&download.directory).join(elevation::Source::file_name(&tile));
            let request =
                Request::get(source.url(&tile)).header("User-Agent", Self::USER_AGENT).timeout_ms(60_000).save_to(path.to_string_lossy().into_owned());
            self.fetch(request, Pending::ElevationTile { region: id.to_string() });
            download.in_flight += 1;
        }
        let done = download.total - download.waiting.len() - download.in_flight;
        if let Some(region) = self.regions.regions.iter_mut().find(|r| r.id == id) {
            region.progress = Some(done as f64 / download.total.max(1) as f64);
        }
        if download.waiting.is_empty() && download.in_flight == 0 {
            let (Some(graph), Some(region)) = (self.region_path(id), self.regions.regions.iter().position(|r| r.id == id)) else { return };
            let job = self.id();
            self.regions.regions[region].job = Some(job);
            self.builds.insert(job, graph.clone());
            let encoding = match download.source {
                ElevationSource::Gsi => elevation::Encoding::Gsi,
                _ => elevation::Encoding::Terrarium,
            };
            let elevation = Some(ElevationInput { directory: download.directory.clone(), zoom: download.zoom, encoding });
            self.effects.push(Effect::BuildGraph { id: job, input: graph.clone(), format: GraphFormat::Graph, output: graph, elevation });
        }
        self.elevation_downloads.insert(id.to_string(), download);
        self.save_regions();
    }

    /// A tile came (saved to its file) or failed (the sea, an error: left out).
    pub(super) fn elevation_tile_arrived(&mut self, region: &str) {
        let Some(download) = self.elevation_downloads.get_mut(region) else { return };
        download.in_flight = download.in_flight.saturating_sub(1);
        self.pump_elevation(region);
    }

    /// The build that added elevations to region `index` ended: the region is ready either way
    /// (without elevations when it failed: its graph was kept).
    pub(super) fn elevation_built(&mut self, index: usize, result: Result<GraphStats, String>) {
        let id = self.regions.regions[index].id.clone();
        let download = self.elevation_downloads.remove(&id);
        if let Some(d) = &download {
            let _ = std::fs::remove_dir_all(&d.directory);
        }
        let region = &mut self.regions.regions[index];
        region.status = RegionStatus::Ready;
        region.progress = None;
        match result {
            Ok(stats) if stats.elevations > 0 => {
                region.elevations = download.map(|d| d.source);
                region.size_bytes = stats.size_bytes;
            }
            Ok(_) => region.elevations = None,
            Err(e) => {
                region.elevations = None;
                self.log(crate::protocol::LogLevel::Warning, format!("elevations of {id}: {e}"));
            }
        }
        if self.router.as_ref().is_some_and(|(r, _)| *r == id) {
            self.router = None;
        }
        self.save_regions();
    }

    /// Stops the download of the elevations of region `id` and removes its tiles.
    pub(super) fn cancel_elevation(&mut self, id: &str) {
        let tiles: Vec<u64> = self.pending.iter().filter(|(_, p)| matches!(p, Pending::ElevationTile { region } if region == id)).map(|(k, _)| *k).collect();
        for tile in tiles {
            self.pending.remove(&tile);
            self.effects.push(Effect::Cancel { id: tile });
        }
        if let Some(d) = self.elevation_downloads.remove(id) {
            let _ = std::fs::remove_dir_all(&d.directory);
        }
    }
}
