//! The offline regions: adding, downloading, building and removing them.

use super::*;

impl Core {
    pub(super) fn region_path(&self, id: &str) -> Option<String> {
        let directory = self.store.subdirectory(Store::REGIONS_DIRECTORY)?;
        Some(directory.join(format!("{id}.myapgraph")).to_string_lossy().into_owned())
    }

    pub(super) fn download_path(&self, id: &str, extension: &str) -> Option<String> {
        let directory = self.store.subdirectory(Store::DOWNLOADS_DIRECTORY)?;
        Some(directory.join(format!("{id}.{extension}")).to_string_lossy().into_owned())
    }

    pub(super) fn add_region(&mut self, name: String, source: RegionSource, file: Option<String>) {
        if self.store.directory.is_none() {
            return;
        }
        if let RegionSource::Area { bounds } = &source
            && bounds.area_km2() > Regions::MAX_AREA_KM2
        {
            self.tell(self.book().message(Message::RegionTooLarge));
            return;
        }
        let id = self.regions.new_id(self.now);
        let mut region = Region {
            id: id.clone(),
            name,
            source: source.clone(),
            status: RegionStatus::Downloading,
            bounds: None,
            size_bytes: 0,
            built_ms: None,
            error: None,
            job: None,
            elevations: None,
            progress: None,
        };
        match source {
            RegionSource::Area { bounds } => {
                let Some(path) = self.download_path(&id, "json") else { return };
                let query = myap_router::GraphBuilder::overpass_query(&bounds);
                let request = osm::Overpass::request(&self.settings.servers.overpass, &query, Self::USER_AGENT).save_to(path);
                region.job = Some(self.fetch(request, Pending::Region { id: id.clone() }));
            }
            RegionSource::Url { url } => {
                let Some(path) = self.download_path(&id, "osm.pbf") else { return };
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    self.tell(self.book().message(Message::RegionFailed));
                    return;
                }
                let request = Request::get(url).header("User-Agent", Self::USER_AGENT).timeout_ms(3_600_000).save_to(path);
                region.job = Some(self.fetch(request, Pending::Region { id: id.clone() }));
            }
            RegionSource::File { .. } => {
                let (Some(input), Some(output)) = (file, self.region_path(&id)) else { return };
                region.status = RegionStatus::Building;
                let job = self.id();
                region.job = Some(job);
                self.builds.insert(job, output.clone());
                self.effects.push(Effect::BuildGraph { id: job, input, format: GraphFormat::Pbf, output, elevation: None });
            }
        }
        self.regions.regions.push(region);
        self.save_regions();
    }

    pub(super) fn region_downloaded(&mut self, id: &str, response: &HttpResponse) {
        let Some(index) = self.regions.regions.iter().position(|r| r.id == id) else { return };
        let (extension, format) = match self.regions.regions[index].source {
            RegionSource::Area { .. } => ("json", GraphFormat::OverpassJson),
            _ => ("osm.pbf", GraphFormat::Pbf),
        };
        let input = self.download_path(id, extension);
        let output = self.region_path(id);
        if !response.is_success() {
            let error = response.error.clone().unwrap_or_else(|| format!("HTTP {}", response.status));
            let region = &mut self.regions.regions[index];
            region.status = RegionStatus::Failed;
            region.error = Some(error);
            region.job = None;
            if let Some(input) = input {
                let _ = std::fs::remove_file(input);
            }
            self.save_regions();
            return;
        }
        let (Some(input), Some(output)) = (input, output) else { return };
        let job = self.id();
        let region = &mut self.regions.regions[index];
        region.status = RegionStatus::Building;
        region.job = Some(job);
        self.builds.insert(job, output.clone());
        self.effects.push(Effect::BuildGraph { id: job, input, format, output, elevation: None });
        self.save_regions();
    }

    pub(super) fn graph_built(&mut self, job: u64, result: Result<GraphStats, String>) {
        let output = self.builds.remove(&job);
        let Some(index) = self.regions.regions.iter().position(|r| r.job == Some(job)) else {
            // The region was removed during its build: its graph is not wanted.
            if let Some(output) = output {
                let _ = std::fs::remove_file(output);
            }
            return;
        };
        let id = self.regions.regions[index].id.clone();
        if self.elevation_downloads.contains_key(&id) {
            self.regions.regions[index].job = None;
            self.elevation_built(index, result);
            return;
        }
        // The download is not needed once built, nor the copy of a file the user chose.
        self.remove_downloads(&id, &self.regions.regions[index].source.clone());
        let now = self.now;
        let region = &mut self.regions.regions[index];
        region.job = None;
        match result {
            Ok(stats) => {
                region.status = RegionStatus::Ready;
                region.bounds = Some(stats.bounds);
                region.size_bytes = stats.size_bytes;
                region.built_ms = Some(now);
                region.error = None;
                region.progress = None;
                region.elevations = None;
                let bounds = stats.bounds;
                self.start_elevation(&id, &bounds);
            }
            Err(e) => {
                region.status = RegionStatus::Failed;
                region.error = Some(e);
            }
        }
        if self.router.as_ref().is_some_and(|(r, _)| *r == id) {
            self.router = None;
        }
        self.save_regions();
    }

    /// Removes the downloads of region `id`, and the copy of an imported file when it lies in the
    /// directory of downloads (the core removes nothing of the platform's elsewhere).
    /// The directory of the terrain tiles of region `id`.
    pub(crate) fn dem_directory(&self, id: &str) -> Option<PathBuf> {
        Some(self.store.path(Store::DOWNLOADS_DIRECTORY)?.join(format!("{id}-dem")))
    }

    pub(super) fn remove_downloads(&self, id: &str, source: &RegionSource) {
        if let Some(dem) = self.dem_directory(id) {
            let _ = std::fs::remove_dir_all(dem);
        }
        for extension in ["json", "osm.pbf"] {
            if let Some(path) = self.download_path(id, extension) {
                let _ = std::fs::remove_file(&path);
                let _ = std::fs::remove_file(format!("{path}.part"));
            }
        }
        if let (RegionSource::File { path: Some(path) }, Some(downloads)) = (source, self.store.path(Store::DOWNLOADS_DIRECTORY)) {
            let path = std::path::Path::new(path);
            let inside = path.parent().and_then(|p| p.canonicalize().ok()).zip(downloads.canonicalize().ok()).is_some_and(|(p, d)| p == d);
            if inside {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub(super) fn remove_region(&mut self, id: &str) {
        let Some(index) = self.regions.regions.iter().position(|r| r.id == id) else { return };
        let region = self.regions.regions.remove(index);
        self.cancel_elevation(id);
        if let Some(job) = region.job
            && self.pending.remove(&job).is_some()
        {
            self.effects.push(Effect::Cancel { id: job });
        }
        if let Some(path) = self.region_path(id) {
            let _ = std::fs::remove_file(path);
        }
        self.remove_downloads(id, &region.source);
        if self.router.as_ref().is_some_and(|(r, _)| r == id) {
            self.router = None;
        }
        self.save_regions();
    }
}
