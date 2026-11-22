<img width="1920" src="https://raw.githubusercontent.com/nercone-dev/website/refs/heads/main/public/static/assets/images/projects/labs/myap/header.png" />

# Myap

A map and navigation app that is easy to use and modern: OpenStreetMap maps, search and routes, an offline router on the phone, and Google only when you ask for it.

> [!WARNING]
> **Routes can be wrong.** Roads change, the data has errors and the times are estimates. Follow the road signs, the traffic rules and the conditions of the road, and do not handle the phone while driving.
>
> **Warnings, limits and the camera view.** Speed limits, lanes, level crossings and speed cameras come from OpenStreetMap and may be missing or wrong; whether warnings of speed cameras may be used while driving depends on the country. The camera view is as precise as the compass and the location of the phone: never rely on it alone, and never hold the phone while driving.
>
> **Places combined from two sources.** When OpenStreetMap and Google are combined, two places are taken to be the same by their names and distance. Two different shops with alike names close together can be taken for one, and then the hours or phone shown may be the other's. The sheet of a place always says which source gave which part; check before relying on it.
>
> **About Google.** When you turn on a use of Google, Myap reads the web pages of Google Maps without an account or a key, as a browser does. They are **not a public API**: they may stop working at any time, and **their use may be against Google's terms of service**. Every use sends Google what it says (what you type, the area of the map, or the places of your trip) with your IP address. Every use is **off** until you turn it on, one by one, in *Settings → Google*.

## What it does

- **Maps** of OpenStreetMap data: vector maps from [OpenFreeMap](https://openfreemap.org) (light and dark) or the standard raster map of openstreetmap.org; Google's map, satellite, hybrid and terrain tiles and its traffic layer when allowed.
- **Search** as you type and on submit with [Photon](https://photon.komoot.io), addresses of dropped pins and the opening hours, phone, website and wheelchair access of places with [Nominatim](https://nominatim.org), and Google's search, suggestions and place details when allowed. Coordinates (`35.68, 139.76`) and `geo:` links work too.
- **OpenStreetMap and Google combined**: a search of Google is also searched in OpenStreetMap, and a place of one source is looked for in the other, so that each fills what the other lacks (Google's rating, OpenStreetMap's wheelchair access…); the place says which source gave which part. The routes of Google, OSRM and Valhalla get the speed limits, level crossings, speed cameras and junction names of your offline regions, on the phone.
- **Android Auto**: the map, search, saved places, routes and turn-by-turn guidance on the car's screen (Car App Library).
- **Routes** by car, bicycle and on foot from four routers, tried in the order you choose, each falling back to the next:
  - **Myap Router**, on the phone, offline, in the regions you download (see below);
  - **OSRM** and **Valhalla**, on the servers of FOSSGIS that openstreetmap.org uses (or your own);
  - **Google**, with live traffic, when allowed.
  Alternatives; avoiding tolls, highways, ferries, unpaved and narrow roads, steps, hills, level crossings, turns across traffic, unlit roads and areas of your choice; fastest, shortest, eco or simple routes; and the router of each route shown, with what it may not have avoided.
- **Live traffic** for Myap Router from HERE with your own key: routes round jams and closures, routes coloured by congestion, and faster routes offered during guidance (also from Google's traffic, when allowed).
- **Turn-by-turn guidance** with voice in Japanese or English: the next maneuver with the name of its junction, its signs and its lanes, and the one after it; warnings of level crossings, speed cameras, steep climbs and congestion; the speed limit and your speed; the time, distance, arrival and delay; the position estimated in tunnels; a dark map after sunset; avoiding the road ahead; leaving the route and finding a new one; and guidance with the screen off.
- **Camera view** (AR): the route and the next turn drawn over the camera's picture, from the location and the compass, without ARCore or any service.
- **Elevations** for the offline regions (Terrain Tiles of AWS Open Data, or the elevation tiles of GSI in Japan): climbs and profiles, avoiding hills, and walking and cycling times by the gradient.
- **Offline regions** for Myap Router: the area on the map (from the Overpass API), a PBF extract from a URL (for example [Geofabrik](https://download.geofabrik.de)), or a PBF file on the phone.
- **Google in one place.** *Settings → Google* turns Google on and off as a whole and each use alone, says what each use sends, and keeps a record on the phone of everything sent, which you can clear (which also ends the Google session).
- Saved places (home, work, favourites), recent places and searches.

See [docs/usage.md](docs/usage.md).

## How it is built

Like [TypeFace Droplet](https://github.com/nercone-labs/typeface-droplet), [Replay Integrity](https://github.com/nercone-labs/replay-integrity) and [FirstAlert](https://github.com/nercone-labs/firstalert), Myap is a core in Rust that does nearly all the work and a thin GUI (Kotlin, Compose, Material 3 Expressive; and the templates of the Car App Library in Android Auto):

- `crates/geo` — coordinates, distances, bearings, lines along a route, encoded polylines, map tiles.
- `crates/fetch` — HTTP requests and responses as values.
- `crates/protowire` — the Protocol Buffers wire format.
- `crates/osm` — OpenStreetMap data from PBF files and the Overpass API.
- `crates/routing` — the model of routes every router shares.
- `crates/geocoding` — places, and the Photon and Nominatim geocoders; whether two places of two sources are the same, and filling one from the other.
- `crates/osrm`, `crates/valhalla`, `crates/google` — the clients of each service.
- `crates/png` — a decoder of PNG images.
- `crates/elevation` — elevations from terrain tiles (Terrarium, GSI).
- `crates/traffic` — live traffic as values, and the client of HERE Traffic API v7.
- `crates/myap-router` — **Myap Router**: graphs from OpenStreetMap data (lanes, signs, junction names, surfaces, widths, lights, barriers, level crossings, elevations), an A* search over directed edges (turn restrictions, U-turns and turn costs exact; avoidances strict and soft; fastest, shortest, eco and simple; live traffic), alternatives, and turn-by-turn guidance with lanes, speed limits and alerts; with the command-line tool `myap-router`.
- `crates/myap` — the core: search, places combined from OpenStreetMap and Google, routing on every router with fallback, navigation, offline regions, the map style and overlay, the words of every text, and the control of Google.
- `crates/myap-ffi` — the C interface of the core (for Swift, C#, C …) and its Java Native Interface (Android, desktop JVMs).

The core performs no network access: it says what to fetch and the platform's HTTP client fetches it, with the system's TLS and proxy. A new platform (iOS, iPadOS, macOS, Linux, Windows) needs a small host for those requests, the location, the voice, and its screens with a MapLibre map. See [.docs/design.md](.docs/design.md).

### Building

```bash
cargo test --workspace --all-features
```

```bash
./gradlew :app:assembleDebug -Pengine.abis=arm64-v8a
```

The Android build compiles the core for each ABI with the NDK's clang (`rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`; `-Pengine.abis` picks some).

```bash
cargo run --release -p myap-router --features cli -- build region.osm.pbf region.myapgraph
```

## Credits

Map data © [OpenStreetMap contributors](https://www.openstreetmap.org/copyright) (ODbL). Elevations: [Terrain Tiles](https://registry.opendata.aws/terrain-tiles/) of AWS Open Data (Mapzen/Tilezen; [the attribution of its sources](https://github.com/tilezen/joerd/blob/master/docs/attribution.md)), and 出典: [国土地理院 標高タイル](https://maps.gsi.go.jp/development/ichiran.html)（加工して作成）. Live traffic: [HERE](https://www.here.com), with the user's own key. The speeds of cycling on gradients follow the table of Valhalla's bicycle costing. Map tiles by [OpenFreeMap](https://openfreemap.org) and OpenMapTiles. Routes by [OSRM](https://project-osrm.org) and [Valhalla](https://valhalla.github.io/valhalla/) on the servers of [FOSSGIS](https://www.fossgis.de). Search by [Photon](https://photon.komoot.io) of komoot and [Nominatim](https://nominatim.org). The map is drawn by [MapLibre Native](https://maplibre.org). How Google's web endpoints are read was learnt from the documentation of [Vela](https://github.com/PimpinPumpkin/Vela); no code of Vela is used.
