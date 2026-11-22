use crate::{DemTile, Encoding, MercatorPoint};
use geo::{Bounds, Coordinate, Tile};
use serde::{Deserialize, Serialize};

/// A provider of terrain tiles: where they are, at which zooms, how they are encoded, and the
/// attribution its terms require wherever its elevations are shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub encoding: Encoding,
    /// The URL of a tile, with `{z}`, `{x}` and `{y}` replaced by its zoom and coordinates.
    pub url_template: String,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub attribution: String,
}

impl Source {
    /// The Terrarium tiles of AWS Open Data (Terrain Tiles, made by Mapzen/Tilezen): worldwide,
    /// 256 by 256 pixels, zoom 0 to 15. The attribution is the one the project requires
    /// (<https://github.com/tilezen/joerd/blob/master/docs/attribution.md>).
    pub fn terrarium() -> Source {
        Source {
            name: "Terrain Tiles (AWS Open Data, Terrarium)".into(),
            encoding: Encoding::Terrarium,
            url_template: "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png".into(),
            min_zoom: 0,
            max_zoom: 15,
            attribution: [
                "ArcticDEM terrain data DEM(s) were created from DigitalGlobe, Inc., imagery and funded under National Science Foundation awards 1043681, 1559691, and 1542736",
                "Australia terrain data © Commonwealth of Australia (Geoscience Australia) 2017",
                "Austria terrain data © offene Daten Österreichs – Digitales Geländemodell (DGM) Österreich",
                "Canada terrain data contains information licensed under the Open Government Licence – Canada",
                "Europe terrain data produced using Copernicus data and information funded by the European Union - EU-DEM layers",
                "Global ETOPO1 terrain data U.S. National Oceanic and Atmospheric Administration",
                "Mexico terrain data source: INEGI, Continental relief, 2016",
                "New Zealand terrain data Copyright 2011 Crown copyright (c) Land Information New Zealand and the New Zealand Government (All rights reserved)",
                "Norway terrain data © Kartverket",
                "United Kingdom terrain data © Environment Agency copyright and/or database right 2015. All rights reserved",
                "United States 3DEP (formerly NED) and global GMTED2010 and SRTM terrain data courtesy of the U.S. Geological Survey",
            ]
            .join("; "),
        }
    }

    /// The elevation tiles of GSI Japan from DEM10B (`dem_png`): Japan only, 256 by 256 pixels,
    /// zoom 1 to 14. Usable without application with the source stated as 国土地理院 and a
    /// link to the list of tiles (<https://maps.gsi.go.jp/development/ichiran.html>).
    pub fn gsi() -> Source {
        Self::gsi_tiles("DEM10B", "dem_png", 14)
    }

    /// The elevation tiles of GSI Japan from DEM5A (`dem5a_png`, 5 m laser survey): zoom 1 to 15,
    /// covering only the surveyed parts of Japan (elsewhere the tiles are missing or no data).
    pub fn gsi_dem5a() -> Source {
        Self::gsi_tiles("DEM5A", "dem5a_png", 15)
    }

    fn gsi_tiles(model: &str, path: &str, max_zoom: u8) -> Source {
        Source {
            name: format!("国土地理院 標高タイル（{model}）"),
            encoding: Encoding::Gsi,
            url_template: format!("https://cyberjapandata.gsi.go.jp/xyz/{path}/{{z}}/{{x}}/{{y}}.png"),
            min_zoom: 1,
            max_zoom,
            attribution: format!("出典：国土地理院 標高タイル（{model}）を加工して作成 https://maps.gsi.go.jp/development/ichiran.html"),
        }
    }

    /// The URL of `tile`.
    pub fn url(&self, tile: &Tile) -> String {
        self.url_template.replace("{z}", &tile.zoom.to_string()).replace("{x}", &tile.x.to_string()).replace("{y}", &tile.y.to_string())
    }

    /// The name of the file of `tile` on disk: `{z}-{x}-{y}.png`, in decimal.
    pub fn file_name(tile: &Tile) -> String {
        format!("{}-{}-{}.png", tile.zoom, tile.x, tile.y)
    }

    /// The tile of a file name made by [`Source::file_name`], or `None` for any other name or a
    /// tile that does not exist.
    pub fn parse_file_name(name: &str) -> Option<Tile> {
        let mut parts = name.strip_suffix(".png")?.split('-');
        let mut number = || parts.next().filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).and_then(|p| p.parse::<u32>().ok());
        let (zoom, x, y) = (number()?, number()?, number()?);
        if parts.next().is_some() {
            return None;
        }
        let tile = Tile { zoom: u8::try_from(zoom).ok()?, x, y };
        DemTile::check(&tile).ok().map(|_| tile)
    }

    /// `zoom` brought into the source's range.
    pub fn clamp_zoom(&self, zoom: u8) -> u8 {
        zoom.max(self.min_zoom).min(self.max_zoom).min(DemTile::MAX_ZOOM)
    }

    /// The columns and rows of the tiles at `zoom` covering `bounds`, or `None` for invalid
    /// bounds. Latitudes are clamped to those Web Mercator draws.
    fn range(bounds: &Bounds, zoom: u8) -> Option<(u32, u32, u32, u32)> {
        let values = [bounds.south, bounds.west, bounds.north, bounds.east];
        if values.iter().any(|v| !v.is_finite()) || bounds.south > bounds.north || bounds.west > bounds.east {
            return None;
        }
        let clamp = |latitude: f64, longitude: f64| {
            let point = Coordinate::new(latitude.clamp(-Tile::MAX_LATITUDE, Tile::MAX_LATITUDE), longitude.clamp(-180.0, 180.0));
            MercatorPoint::from_coordinate(&point).map(|p| p.to_pixel(zoom, 1, 1))
        };
        let (west, north) = clamp(bounds.north, bounds.west)?;
        let (east, south) = clamp(bounds.south, bounds.east)?;
        let max = f64::from((1u32 << zoom) - 1);
        // A tile only touching the bounds along an edge is left out; bounds of no width or
        // height still take the tile they are in.
        let first = |v: f64| v.floor().clamp(0.0, max);
        let last = |v: f64, start: f64| (v.ceil() - 1.0).clamp(start, max);
        let (x0, y0) = (first(west), first(north));
        Some((x0 as u32, y0 as u32, last(east, x0) as u32, last(south, y0) as u32))
    }

    /// The number of tiles [`Source::tiles`] gives.
    pub fn tile_count(&self, bounds: &Bounds, zoom: u8) -> u64 {
        match Self::range(bounds, self.clamp_zoom(zoom)) {
            Some((x0, y0, x1, y1)) => u64::from(x1 - x0 + 1) * u64::from(y1 - y0 + 1),
            None => 0,
        }
    }

    /// Every tile whose area overlaps `bounds`, at `zoom` brought into the source's range, from
    /// the north-west row by row. Empty for invalid bounds. Use [`Source::zoom_for`] first: a
    /// large area at a high zoom has very many tiles.
    pub fn tiles(&self, bounds: &Bounds, zoom: u8) -> Vec<Tile> {
        let zoom = self.clamp_zoom(zoom);
        let Some((x0, y0, x1, y1)) = Self::range(bounds, zoom) else { return Vec::new() };
        (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| Tile { zoom, x, y })).collect()
    }

    /// The highest zoom of the source, at most `preferred`, at which `bounds` takes at most
    /// `max_tiles` tiles; the source's lowest zoom if even that takes more.
    pub fn zoom_for(&self, bounds: &Bounds, max_tiles: usize, preferred: u8) -> u8 {
        let lowest = self.clamp_zoom(0);
        let mut zoom = self.clamp_zoom(preferred);
        while zoom > lowest && self.tile_count(bounds, zoom) > max_tiles as u64 {
            zoom -= 1;
        }
        zoom
    }
}
