# Myap — developer notes

These notes record what the core and the application do, why they are shaped the way they are, and
the options that were considered and set aside. Read them with the documentation of each crate;
every `lib.rs` states its contract.

## The goal

A map and navigation application that is easy to use and modern, built on open data
(OpenStreetMap), with routes from several routers including one on the phone, and Google as an
option the user turns on, use by use, knowing what each sends.

## Why this split

As in TypeFace Droplet, Replay Integrity and FirstAlert, a core in Rust does nearly all the work
and the GUI is small. As in FirstAlert, the core is a library loaded into the application
(`crates/myap-ffi`, `libmyap_ffi.so`) and a state machine: the platform hands it inputs and carries
out the effects of each response, and draws the snapshot.

### The core performs no network access

`Core::handle(Input) -> Response` never opens a connection: it returns `Effect::Fetch` with a
`fetch::Request`, and the platform answers with `Input::Fetched`. Every library of the workspace
follows this (`fetch` holds requests and responses as values):

- every platform has a mature HTTP client with the system's TLS, proxy and certificate store
  (OkHttp on Android, `URLSession` on Apple platforms, `HttpClient` on .NET …): a new platform is a
  few hundred lines of host;
- the core and its libraries are tested with recorded responses (`tests/fixtures`), without the
  network;
- *set aside: an HTTP client in the core* (`reqwest`, or TLS through `boring`): a large native
  dependency to build for every ABI, which would bypass the system's trust store and proxy.

The protocol is JSON (`crates/myap/src/protocol.rs`), as in FirstAlert. The interface catches a
panic and answers with an error, so a bug never ends the application in the middle of a navigation
(the release profile does not use `panic = "abort"`).

The map is drawn by MapLibre Native on each platform (it exists for Android, iOS, macOS, Linux,
Windows and the web), from what the core decides: the style (`MapStyle`: a URL or a style, and
raster overlays), the features as GeoJSON (`Snapshot::overlay`, each with a `kind`), and the camera
(`Camera`, with an id so that a command is applied once). The platform's map code only draws.

## The crates

Each crate is a complete library for its subject, with nothing of the others:

| Crate | Subject |
| --- | --- |
| `geo` | coordinates, distances, bearings, `Line` (a route measured along its length: projection of a location, point at a distance, slices), encoded polylines, Web Mercator tiles |
| `fetch` | HTTP requests and responses as values, URL queries |
| `protowire` | the Protocol Buffers wire format, without schemas |
| `osm` | OpenStreetMap elements; the PBF reader and the Overpass JSON reader |
| `routing` | the shared model of routes: request, route, leg, step, maneuver; `RemoteRouter` |
| `geocoding` | places, suggestions, addresses; Photon and Nominatim |
| `osrm`, `valhalla`, `google` | the clients of each service |
| `myap-router` | Myap Router |
| `myap` | the core |
| `myap-ffi` | the C and Java interfaces |

Versions of a thing share a type and keep their differences in a module of their own: every router
reads its format into `routing::Route`; every geocoder into `geocoding::Place`; every format of
OpenStreetMap data into `osm::Element`.

## Myap Router

### The graph

`GraphBuilder` reads the ways of OpenStreetMap that are roads or ferries and keeps, for each of
car, bicycle and walking, whether the way may be used along it and against it (`Profile::access`:
the class of road, `access`/`motor_vehicle`/`bicycle`/`foot` and the most specific of them,
`oneway` and its exceptions, roundabouts and motorways one-way by default). Nodes used by one way
only are folded into the shape of the edge between two junctions; a node with traffic signals or of
a turn restriction stays a junction. Each road becomes one directed edge per direction allowed,
with the modes as flags; both share the points of the shape.

The file (`.myapgraph`, version 2) holds the nodes, the rows of edges, the shapes, the ways (class,
speed limits along and against, lanes, incline, toll, roundabout, surface, width, lights, bridge,
tunnel, cycle lanes, name, number, `turn:lanes` and the destinations of signs in each direction), the
strings, the restrictions, a flag word per node (signals, stop, give way, level crossing, speed
camera, crossing, toll booth, motorway junction, and the modes a barrier stops), the names of nodes
(the name of a junction, 交差点名, and the number of an exit) and, when added, the elevation of each
point of the shapes. Version 1 graphs are refused with a message, and the region is marked to be
downloaded again: *set aside: reading version 1*, since the new attributes cannot be recovered
without the data. A node with signals, a barrier, a level crossing, a camera or a name becomes a
node of the graph, so that its delay is counted and it can be told.

Tags of one direction (`turn:lanes`, `destination`, `maxspeed`, `lanes`) are read as the wiki
defines them: `key:forward`/`key:backward` on two-way roads, `key` alone in the direction a road
one-way for vehicles goes (walkers may use such a road both ways; the first design took the access
of walkers into account and read no lanes on one-way streets). Japanese junction names are read
from `name` on `highway=traffic_signals` nodes and on `junction=yes` nodes (the two practices the
Japanese wiki documents) and `junction:name`. Surfaces are unpaved by the wiki's list (`grass_paver`
is paved); a road is narrow below 4 m (`width`, `est_width`), when single-lane two-way side street,
an alley or driveway, or a living street. Barriers follow OSRM's profiles (cars pass only
`cattle_grid`, `border_control`, `toll_booth`, `sally_port`, `no`, `entrance`, `height_restrictor`,
`arch`, and gates that open; bicycles and walkers pass all but walls and fences), with stiles,
turnstiles and kissing gates closed to bicycles as the wiki describes them; the access tags of the
node decide first, and a locked gate is closed. It is
read as a stream, record by record, and **every index is checked** (`Graph::validate`), so that a
damaged or hostile file is an error, never a crash or a huge allocation: the space of each part
grows with the records actually read (at most doubling, never beyond its count), so a count larger
than the file ends in "truncated" having used no more than twice what the file holds, and bytes left
over are refused. The first design read the whole file into memory and then built the graph from
it, which held the file and the graph at once (twice the memory, on a phone, for a region of
hundreds of megabytes); it checked the counts against the size of the file first, which a stream
cannot know. The core lets the graph of one region go before it reads another's, for the same
reason, and only a damaged or old file (not an error of the storage) marks a region to be
downloaded again.

What is computed from the file and not stored: the source of each edge and its *twin* (the same
road the other way), found once when the graph is read. The first design looked the twin up among
the edges of the target at each use, and it is used in the index, the snapping of waypoints, the
alternatives, the matching of traffic and every U-turn the search considers.

PBF files are read twice (ways and relations, then only the nodes they need), so a file of any size
is read in the memory of one block; the memory of the build grows with the roads of the region.
*Known limit:* a whole country is too large for a phone; a prefecture or region is the intended
size.

### The search

An A* over **directed edges**: the state is the edge arrived by, not the node. This makes three
things exact that a search over nodes only approximates:

- turn restrictions (`no_*` and `only_*`, via a node) are a property of a pair of edges;
- U-turns are allowed only at dead ends;
- the cost of a turn depends on its angle.

The heuristic is the straight distance at the fastest speed of the mode (times the smallest
preference, see below), so it never overestimates: the first route found is the best.

Waypoints are matched to **several** nearby roads (up to five, within 60 m of the nearest), each with
a cost for the distance to it (3.6 s/m). The nearest road may lead nowhere (a station's service
road, a private yard): the first design matched one road and found *no route* from Tokyo Station by
car. The cost is steep so that a further road is never a shortcut through buildings or round a turn
restriction; the tests check both.

A road to avoid (tolls, motorways, ferries) is not used, except the road a waypoint lies on, part
way along, which cannot be left otherwise. When nothing avoids it, the route is found without the
avoidance and says what it could not avoid (`Route::ignored_avoid`).

### Times and preferences

Times are typical speeds below the limits (85 % of `maxspeed`, else by class), slower on unpaved,
rough and narrow roads, plus waiting at signals (15 s by car, about half a red phase), level
crossings (15 s), stop signs (5 s), toll booths (10 s) and turns (6 s, 12 s when sharp). With
elevations, walking follows Tobler's hiking function (scaled to 4.8 km/h on the flat) and cycling
the gradient table of Valhalla's bicycle costing; driving is timed alike on hills. The climb and
descent of an edge are spread evenly over its length at one gradient, exact for an edge that only
climbs or only descends. Elevations are counted with a 1 m threshold and gradients over 30 m or
more, so that the noise of 10–30 m elevation data does not add up. The search minimises
a **weight**: the time times a preference for the class of road (side streets ×1.4, service roads
×2 by car). Without it, the signal delays made routes weave through back streets to save seconds;
with it, Tokyo Station → Tokyo Tower takes the main roads (日比谷通り, 晴海通り, 祝田通り), in 848 s
against Google's 895 s typical. The times told are times, not weights.

What the search derives from a graph once lives in `Network`: the bearings at both ends of each
edge (the first design measured them by building the edge's points for every turn the search
looked at, an allocation per relaxation), the climb of each edge, the class and flags of its way
(read next to the edge instead of from the way record, a jump in memory for every edge looked at),
each node as a point of the unit sphere, and the time of each edge for a mode, computed the first
time that mode is routed (most travellers use one: 4 bytes an edge, not 12). The angle of a turn is
the difference of the bearings in tenths of a degree, in integers.
The estimate of the A* is the distance at the fastest speed of an edge of the graph for the mode,
with the least preference: it stays below every real cost with ferries, descents and the traffic of
now (whose speeds are never taken faster than that). The distance is the chord between the points
on the sphere, less a metre for the error of `f32`: never longer than the distance along the Earth
(a test checks it), and without the trigonometry of the haversine at each step. It is measured to
the nearest road of the destination; another of its roads may be nearer a node, by no more than the
distances of the two roads from the destination, but its approach costs 3.6 s a metre, more than
twice what a metre costs at any speed, so the estimate stays below the cost to every one of them.

In the search, each edge's cost is computed once per relaxation (the first design computed it for
the turn and again for the strict avoidances), and the sets it tests (the areas to avoid, the roads
of alternatives found) are bit sets over the edges (`EdgeSet`), not hashed sets; the traffic of now
is one factor per edge (4 bytes an edge while traffic is in use). The grid of the index is held in
compressed rows sorted by cell (three vectors for the whole grid, not a vector per cell in a map),
and the areas to avoid are looked up in the cells of their bounds, each edge tested once. On a
synthetic grid of 160 000 junctions and 640 000 edges these took a route by car from 32 ms to 25 ms,
one with traffic from 59 ms to 26 ms, the alternatives by a third, and the matching of 3 000 lines
of traffic from 12 s to 0.15 s (each sample measured each road of nine cells, building its points;
now a road whose box is not within 15 m is passed by, and a road and its twin are measured once),
with the same routes.

Avoidances are of two kinds. *Rules* (tolls, motorways, ferries, unpaved roads, steps, level
crossings, the areas to avoid, closures) leave the road out; when nothing else leads there, the
search runs again with them allowed but costly (×4, closures ×20), and the route says what it could
not avoid. *Preferences* (narrow roads ×3 by car, unlit roads ×3 on foot or by bicycle, hills: each
metre climbed 8–12 s and steep edges ×3, turns across traffic +45 s by car) only cost more. The
first design had only rules, and dropped all of them at once when one could not be kept. A route
says only rules it broke (computed from the edges it uses), so a note never claims a highway that
is not on it (the limit stated in the earlier review). Turns across traffic depend on the side
traffic drives on, by the region of the device (`DrivingSide::of_country`, from Wikipedia's list) or
the setting: *set aside: the side by the trip's location*, which needs the borders of countries.

`Optimize` changes what is minimised: *Fastest* the time weighted by the preference of the class,
*Shortest* the distance (turn costs a fifth), *Eco* the time with a cost for speeds above 80 km/h,
for each metre climbed and for stops, *Simple* the preference squared and turn costs ×3.

Live traffic is matched to edges (`TrafficMap::matching`): each line of traffic is sampled every
40 m, and each sample goes to the nearest edge within 15 m whose direction is within 35°; the
slowest line wins on an edge. *Set aside: OpenLR or TMC location references*, which HERE offers but
need tables or a decoder; shapes are enough at the precision of a road.

*Set aside: contraction hierarchies* (much faster on large graphs). They fix the costs when the graph
is built, which would lose the avoidances chosen per request, and they need more memory to build.
The A* answers within tens of milliseconds on a city and should stay within a second on a region; a
hierarchy can be added later beside it.

*Set aside for now: restrictions via ways, conditional restrictions and access, time-dependent
speeds, the direction of stop signs (`direction=*`: a stop sign of the other direction adds its 5 s
too), `destination:lanes`.*

### Alternatives and guidance

Alternatives are found by penalising the roads of the routes found (×1.6) and searching again; a
candidate is kept when its real cost is within 1.4 times the best and it shares at most 70 % of its
length with each route kept. Both real costs are measured alike (`Search::path_cost`: the parts of
the end edges travelled, the edges between and the turns, without the penalties): the first design
compared the whole cost of the best route, turns and approaches included, with the edges alone of
the candidate, which let through candidates slower than the ratio says.

Guidance (`guidance.rs`) also tells, for each maneuver, the lanes of the road arriving at it
(`turn:lanes`, those whose arrow is closest to the turn valid, or those without an arrow going
straight on; nothing when no arrow is within 60° of the turn), the name of its junction, the number
of a motorway exit and the destinations of the road taken (through its ramps); and along the route,
the speed limits, the congestion, the elevation of every point and the alerts (level crossings,
speed cameras, toll booths, steep climbs). Guidance walks the path: at each junction it compares the angle of the turn with the
other ways the traveller could take. No other way and the same road (same name or same number):
nothing is said, even on a bend. Another road and a real turn: a turn. Two ways ahead within 40°: a
fork, to the side of ours. Links from and to motorways: ramps and merges. Roundabouts: the exits
are counted until the roundabout is left, and leaving it is part of the same step.

## The core

### Live traffic and faster routes

Live traffic comes from HERE Traffic API v7 (`crates/traffic`), with the user's own key, because no
open source of the speeds of roads now exists for most of the world; Google's traffic is a use of
Google like the others. Before Myap Router is asked for directions, the flow and incidents of the
trip's area (with 3 km of margin, in pieces of at most a degree, at most six) are fetched, and Myap
Router waits for them; flow of a confidence of 0.5 or less (HERE: the speed limit only) is not used.
Closures of incidents close the roads under their shapes. Traffic is held in memory 15 minutes at
most (HERE's terms limit how long its data may be kept; their current terms could not be checked).

During guidance, every few minutes (`TrafficSettings::refresh_minutes`), the route left is asked
again: by Myap Router with fresh traffic when a region covers it, else by Google when *Traffic during
guidance* is allowed. The route followed is timed with the same traffic, whatever router made it,
by walking it every 100 m and slowing each part by the line of traffic along it
(`Core::remaining_with_traffic`). A route that runs 90 % along the one followed only updates the times;
another is offered when it saves the chosen minutes and 8 % of the time left (or taken at once when
the settings say so). *Set aside: rerouting at once without asking*, as the default: a route that
changes by itself in a city is confusing; it is a setting.

### Guidance

Besides the announcements of maneuvers, alerts (level crossings, speed cameras, steep climbs,
incidents) are announced once, 150–600 m ahead by car. Without a fix for 3 s, the position is
estimated along the route at the last speed (never past the next maneuver by more than 30 m, at most
90 s): tunnels. The camera's zoom and tilt follow the speed and the distance to the next maneuver.
The map is dark after sunset where the device is when *Dark map* says so (NOAA's solar position,
`geo::Sun`; sunset at −0.833°).

The camera view (`ArCamera.kt`) gets the route ahead from the core as metres east, north and up of
the traveller every 4 m up to 250 m (`ArView`), and projects it with the rotation vector of the
device and the field of view of the camera; it uses Camera2 and the sensors of the platform. *Set
aside: ARCore*, which is a part of Google Play services; its tracking would be steadier, but Myap
uses no Google Play services.

### Elevations

Elevations come from raster terrain tiles: Terrain Tiles of AWS Open Data (Terrarium, worldwide,
zoom 12, about 30 m a pixel) or GSI's DEM10B (Japan only, zoom 14, about 10 m), decoded by a PNG
decoder of the workspace (`crates/png`; *set aside: the `png` crate of crates.io*, to keep the
dependencies few and the decoder checked against the specification by tests of its own). Once a
region is built it is usable, and its tiles are downloaded four at a time, each to a file (the
responses of the platform are text, so binary data goes to files), then a second build adds the
heights to the graph (`GraphFormat::Graph`). Tiles that fail (the sea, an error) are left out: their
points take the nearest known height along their road. The tiles are removed once added.

### Search

Suggestions come from Google when allowed, else from Photon; the saved and recent places that
match come first. A submitted search goes to Google when allowed, else to Photon, then to Nominatim
if Photon fails. Coordinates are read as a point. Nominatim is asked at most once a second (its
policy), with a queue and `Effect::Wake`; it is used for the names of dropped pins.

The name of a place is shown with its branch ("スターバックス 渋谷店"). OpenStreetMap keeps the branch
in its own tag (`branch`, which the wiki says is added to `name`), and Google's names already hold it.
`Place::set_branch` adds it to the name after a space, unless the name already holds it once folded
as `PlaceMatch::normalize` folds names, and keeps it in `Place::branch`, so that it is added once.
Nominatim's `extratags` give it; Photon's public server gives no tags beyond its own fields (its
`extra` is empty there), so Photon's places of a submitted search are looked up in Nominatim
(`/lookup`, their ids, one request) before they count as an answer (`Part::Naming`), while
*Look up details in OpenStreetMap* is on, since it sends them to Nominatim. When the lookup fails,
the places are shown without branches. A place opened takes the branch of its element too. The
names with branches also make the combined search surer: two branches of one chain near each other
no longer have the same name. *Set aside: a field for the branch shown by the GUI beside the name*:
every place a name is shown (the results, the sheet, the car, the guidance, the saved places) would
have to join them, and Google gives no branch apart. *Set aside: the branches of the suggestions*:
a request to Nominatim at every pause in typing is against its policy. *Set aside: showing Photon's
places first and their branches later*: the list would change under the finger, and the combined
search would match on names without branches.

Addresses are written as they are written where they are: largest part first without separators
in Japan, China, Korea and Taiwan **when the parts are in Chinese characters, kana or hangul**;
otherwise from the smallest part with commas. The first design looked at the country alone and wrote
"TokyoMinatoShibakōen 4" for the Latin names of an English device.

### OpenStreetMap and Google combined

Neither source knows everything of a place: OpenStreetMap often has the wheelchair access, the
website and the hours as rules (`Mo-Fr 09:00-18:00`) but no rating; Google has ratings and the
status of now, but its endpoints give no wheelchair access. `geocoding::PlaceMatch` decides
whether two places are the same, and `Place::fill_from` fills what one lacks with what the other
knows; `Place::update_from` refreshes a place from its own source. The rules:

- **Never replaced, always said.** Filling never replaces what a place knows: its own source wins
  for every field it has, and its name, position and id stay. Each field filled is recorded with its
  source (`Place::filled`), and the sheet lists the sources with the parts each gave, as the
  licences ask and so that the hours of one are never taken for the other's. The ids of the same
  place elsewhere are kept (`Place::links`), so that a place opened again is looked up by id.
- **The same place** has alike names (the Dice coefficient of pairs of characters, after folding
  case, full-width forms, hiragana into katakana, spaces and punctuation; at least 0.75 when one name
  holds the other, as "スターバックス" in "スターバックス 渋谷店") and lies within 150 m (2 km when one
  is an area whose bounds hold the other), nearer being surer. A search is combined one to one, the
  surest pairs first. *Set aside: transliteration* (Tokyo Tower / 東京タワー): a match would be a guess,
  and a wrong match shows another place's hours; not combining is the safer failure. *Set aside:
  matching by category*, since the sources word categories differently ("ラーメン屋", `restaurant`).
- **Search.** With *Combine* and Google's search on, Photon is asked beside Google, and the results
  are shown when both answered: Google's in its order, filled from the same places of OpenStreetMap,
  then those of OpenStreetMap within 1 km of them that Google did not find. Either alone answers when
  the other fails. *Set aside: combining the suggestions while typing*: twice the requests at every
  pause in typing, to Photon's public server, for lines that show only a name.
- **A place opened.** A place of OpenStreetMap is looked up in Nominatim (`/lookup` with
  `extratags`) and, combining with Google's *Place details* allowed, in Google (by its name and
  address at its position; the result of the same place by `PlaceMatch`). A place of Google is looked
  for in OpenStreetMap by Photon (its name, bounded to about 300 m round it), then that element's
  tags in Nominatim. Each lookup carries the generation of the selection, so that an answer for a
  place no longer shown changes nothing; the place loads until every lookup answered, and what it
  became replaces its copies among the recent and saved places.
- Sending a place of OpenStreetMap to Google is a use of *Place details*, recorded like the others,
  and its description says so; with Google off nothing changes in what reaches Google
  (`nothing_reaches_google_while_it_is_off`). Nominatim's lookups go through its queue of one a
  second.

The first design of the search fell back from Photon to Nominatim but read Nominatim's answer with
Photon's parser, so the fallback never showed a result; each source is now read by its own parser
(`nominatim_answers_when_photon_fails`).

### Routes of other routers annotated

OSRM, Valhalla and Google's web directions give no speed limits, level crossings or speed cameras,
and Google no names of junctions; a downloaded region has them. `Router::annotate` matches the
route to the roads of the graph (`Router::match_line`: samples every 30 m, the nearest edge within
15 m whose direction is within 40°, the edge before winning a near tie by 3 m, edges the mode may
use only; the road followed and those leaving its end are tried first, and the cells of the index
are searched only when none of them lies within 5 m, since the core does this on its one thread) and adds, where the route has none, the limits of its roads, the alerts of the nodes it
passes from one road to the next (at the point of the route before them, never late), and the name
of a junction within 25 m of each turn. Nothing is added unless 60 % of the route's length lies on
the graph, so that a road the region does not have never gets another's limit. It runs on the phone,
on the graph already used for routing (read when needed), for the directions, reroutes and faster
routes of the guidance; *Add what offline regions know* turns it off. *Set aside: lanes*, whose
choice depends on the angle of the turn as the other router drew it; and *HMM map matching*
(Newson & Krumm), more robust on sparse lines but slower and more code, where routes are dense
enough near junctions for the nearest edge.

The guidance places each alert by projecting it on the segment that starts at its point, so that
an alert on a long straight segment of a sparse geometry is told at its distance, not at the start
of the segment.

### Routes

The routers are tried in the order of the settings (`RoutingSettings::order`, Google, Valhalla,
OSRM, Myap Router by default), skipping those that cannot answer (Google when not allowed, OSRM
without a server for the mode, Myap Router when no region holds the trip), and, after a failure, the
next one is asked. The sheet says when another router than the first answered, and why. *Prefer
Myap Router* moves it first when a region holds the trip. When every router fails, the most telling
error is shown (no route before a network error).

A plan from the device's location waits for the first location instead of failing; it was the first
thing that failed on a device just started.

Answers are matched to their plan by a generation number: the answer to a plan that was replaced
(another router chosen, another mode) is ignored.

### Navigation

`Session` follows the route with each location: the location is projected onto the route near the
progress so far (a route may pass the same place twice), and the progress only moves forward (except
a small step back for a jittery fix). Three fixes in a row more than 35 m off the route (more with a
poor accuracy), or going the other way at speed, ask for a new route, at most every 10 s; the new
route starts from the traveller's position, with their heading, by the router of the current route
first, to the waypoints not yet passed.

Each maneuver is announced twice: about 25 s ahead (150 m to 2 km by car) and about 6 s ahead, with
the next maneuver when it follows closely. The time left is the rest of the current step and the
steps after it, scaled by the traffic of the route when known.

### Words

The core words every text (`words/`), in Japanese and English: instructions, announcements, spoken
distances ("300メートル", which a Japanese voice reads correctly where "300 m" may be read as letters),
durations, clock times (the platform gives the offset of its time zone), and messages. Every platform
says the same. Road names come from the data and stay as they are.

### Files

`settings.json`, `places.json`, `google_log.json` and `regions.json` in the directory the platform
gives, each written to a temporary file that then replaces the old one. A file that cannot be read
is kept aside as `.broken` and the core starts with the defaults. The graphs are in `regions/`, the
downloads waiting to be built in `downloads/` (removed once built). A download or build interrupted
by the end of the application is marked failed at the next start.

## Google

### What is used, and how

Google's web endpoints of Google Maps, as the page of Google Maps in a desktop browser uses them:
`/s` (suggestions), `/search?tbm=map` (search), `/maps/preview/directions` (routes), and the tile
servers. How they are asked and how their positional JSON is read was learnt from the
documentation of [Vela](https://github.com/PimpinPumpkin/Vela) (GPL-3.0); **no code of Vela is
used**: only facts of Google's protocol (URLs, the `pb` templates, the positions of fields), checked
against responses recorded for the tests on 2026-11-22.

- The request shapes are data (`google::Templates`), so that a change of Google's is a change of
  data, as Vela keeps them.
- `pb` is held as a tree (`google::Pb`): the value of a message is the number of tokens it holds,
  those of nested messages included, and is computed when written, so adding the avoidances can
  never leave a wrong count.
- Directions: the line is in `[0][7]` (latitudes and longitudes in 10⁻⁷ degrees, deltas); the steps
  are the nodes with a location at `[7][2]` and the parts of their text at `[14]`, placed on the line
  in order. Google shortens the steps of long routes; where none are found, the steps are taken
  from the shape of the line (`Route::from_geometry`, turns of 35° or more, without names). *Set
  aside: Vela's approach of asking OSRM for the steps of Google's line*, which sends the trip to a
  second service and gives steps that may not follow Google's line.
- There is no lookup of details by id without a key: the details of a place are found by searching
  its name and address at its position and keeping the result of the same feature id.
- Google does not turn a coordinate into an address; Nominatim does.

### The session

Myap opens the page of Google Maps once before the first data request of a session, as a browser
does, and keeps the cookies Google gives **in the core, in memory only** (`google::Session`): a new
session with each launch, and a new one when the user clears the record or turns Google off. No `CONSENT`
cookie is sent, and none from a response is taken. The cookies are kept in the core rather than in the
platform's HTTP client so that every platform behaves alike and no client ever keeps Google's
cookies for other hosts.

The requests carry the headers of desktop Chrome, which the endpoints need to answer in the shape the
parsers read. *Set aside: what Vela does to look like many browsers* (a TLS fingerprint of Chrome,
spans and window sizes varied per install, jittered timing): Myap asks as one application, openly,
and does not try to avoid being recognised. Requests to open services carry Myap's own User-Agent.

The consent cookie sent is `SOCS=CAESHAgBEhIaAB`, which Vela documents and which is widely
reported to mean "reject all"; Google does not document it.

### Control

Every use (`GoogleSettings`: suggestions, search, place details, directions, map tiles, traffic) is
**off** until the user turns it on, and the switch *Use Google* turns them all off at once. The core
checks the setting before building any request (`fetch_google` is only reached when
`GoogleSettings::allows` the use), and a Google map is used only while map tiles are allowed
(`Settings::effective_map`). The test `nothing_reaches_google_while_it_is_off` drives every input
with every use chosen but the switch off, and checks that no request goes to a host of Google.

What each use sends is said next to its switch, and every request is recorded on the device
(`google_log.json`, the last 500: time, use, and what was sent) for the user to see and clear.

### Known limitations

These are stated in the application (the Google page) and the README:

- The endpoints are not a public API: they may change or stop at any time, and their use **may be
  against Google's terms of service**. When Google refuses (403, 429, its verification or consent
  page) or the answer cannot be read, the page shows it and Myap uses the open services instead.
- Google receives what each use sends, with the device's IP address. The record on the device cannot
  delete what Google received.
- Map tiles and the traffic layer are fetched by the map itself, so they are not in the record; the
  page says that Google sees which parts of the map are shown.

## The Android application

- **The host** (`service/Host.kt`) carries out the effects: fetches with OkHttp (a download to a file
  is written to `.part` and renamed when complete), graph builds on a thread of their own through
  `NativeCore.buildGraph` (the core goes on meanwhile and is told when it is done), the voice with
  the platform's `TextToSpeech`, wake-ups, and the guidance service. Only `Set-Cookie` of the
  response headers is handed to the core.
- **Location** uses the platform's `LocationManager`: the GPS with the fused (or network) provider,
  every 5 s, every second during guidance. Google Play services are not used. The first design used
  the fused provider alone outside guidance; at balanced power it gave no fix at all on the
  emulator, and phones without Google's services may have no fused provider.
- **Guidance** goes on with the screen off in a foreground service of type location; its notification
  shows the next maneuver.
- **The map** is MapLibre Native (the OpenGL ES build, for older devices), with Myap's own
  User-Agent on its requests. Myap's lines go under the labels of the base map but above its 3D
  buildings: the first design put them under the first label layer, which in OpenFreeMap's styles
  (the one-way arrows) comes before the buildings, and the route vanished under the buildings once
  the camera tilted for guidance. There is one map in one place of the composition for every mode;
  the first design composed a second map for guidance, and the controller drew on the style of the
  first one, which crashed.
- **Network security**: no cleartext HTTP, only the system's certificates; the servers in the settings
  must be HTTPS.
- `geo:` links open a search or a point.

## Android Auto

Myap is a navigation application of the Car App Library (`androidx.car.app`, a library of
AndroidX that needs no Google Play services in the application; Android Auto is the host). The car
is a third interface of the core beside the phone's screens and the guidance service, through the
same `Host`: the screens of the car read the snapshot and send inputs, and nothing of the core is
specific to the car (the guidance view gained the distance and time left in numbers, the time of
arrival and the side traffic drives on, which the car words itself).

- **The map** is drawn by MapLibre, as on the phone, on the surface the host gives: a virtual
  display of that surface, and the `MapView` shown on it in a `Presentation`, as the library
  advises for drawing with views. It draws the same style and features (`MapPane`'s drawing is
  shared, not copied), keeps its camera in the area the templates leave visible, and follows the
  position (zoom and tilt of the core) unless the user moved it (pan, zoom).
- **The templates**: the map is a `NavigationTemplate` (search, saved places; during guidance the
  next maneuver, the one after it, the estimate, the voice and *End*); search is a `SearchTemplate`
  with the suggestions; results and saved places a `PlaceListNavigationTemplate`; routes a
  `RoutePreviewNavigationTemplate`. A screen is drawn again only when what it shows changes, so
  that the host's limits on templates are never reached by updates that change nothing.
- **The guidance** is told to the host (`navigationStarted`, `updateTrip`, `navigationEnded`), so
  that the car can show it on the cluster and end another application's; the host can end it.
  Guidance started on either screen is shown on both.
- **The location** is asked for by each user (the phone's screens, the car, the guidance service)
  in `Host`, and the device is located while any needs it: the first design stopped the location
  when the phone's screen went off, which would have frozen the car's map.
- The maneuver icons are Myap's own drawings (`ic_car_*`), mirrored for the left; the type of each
  maneuver tells the host which way U-turns and roundabouts go by the side traffic drives on.
- Hosts are checked against the library's list of known hosts in a release build, any host in a
  debug build (the Desktop Head Unit).

The lists of places of the car wait for the first fix: the library refuses a row of
`PlaceListNavigationTemplate` without its distance, and the routes start from the position anyway.
The minimum level of the cars' API is 1; the buttons of the map and panning, of level 2, are left
out on hosts of level 1.

*Set aside*: lanes in the car (the host needs one picture of all the lanes, drawn by the
application); the warnings ahead and the speed limit, which the templates have no place for;
Android Automotive OS (a build of its own, with the map on the car's own system). *Known limit*: it
was built and checked against the library's rules, not driven on a head unit (see Testing).

## Universal design and safety

- The guidance banner is large, in fixed colours of high contrast, and a polite live region for
  screen readers; the voice says what the banner shows.
- The About page and the README say that routes can be wrong and that the road and its rules come
  first.

## Review

The code was reviewed independently (a separate agent that had not written it) in three steps:
listing every candidate problem, numbering them, then verifying each by tracing the code and by
small programs against the core. Twelve problems were confirmed; all are fixed, and those of the core
have tests of their own in `crates/myap/tests/review.rs` and `crates/myap-router/tests/router.rs`:

- a route without steps (Valhalla with no maneuvers) made every call panic during guidance: routes
  without a step in every leg are refused, and the view of the guidance never indexes blindly;
- a second reroute after passing a stop went back to the stop: stops are dropped from the targets
  when the traveller comes within 40 m of them, whatever router made the route;
- a Google request waiting for the session was sent after its use was turned off, and directions
  waiting for Google stayed loading when Google was turned off: queued requests keep their use, and
  those no longer allowed are answered as failed (the search falls back to open data, the
  directions to the next router);
- a ferry faster than walking could make the A* return a slower route first: the estimate uses the
  fastest edge of the graph for the mode;
- a region removed during its build left its graph, and an imported PBF was never removed: the core
  remembers the output of each build and removes copies of imported files (only inside its
  downloads);
- on 32-bit systems a damaged graph could pass validation by wrapping an index: checked in 64 bits;
- on Android: a URL OkHttp refused left its request waiting forever; partial downloads were left on
  disk; an imported file was copied on the main thread; two quick changes of the settings could lose
  one (each change now applies to the last one sent).

Also changed after the review: stale and poor fixes are ignored, a last known fix older than two
minutes is not used, a `no_u_turn` restriction no longer forbids going straight on, the spatial index
samples long segments, the opening of a Google session is recorded, websites open only over HTTP(S),
the guidance service does not start without the location permission, graph builds run one at a
time, and text responses are limited to 32 MiB. The consent cookie `CONSENT=YES+` that Vela also
sends is a grant of consent, and is no longer sent; only `SOCS` is (its meaning is reported, not
documented by Google).

Left for later, with their risk: the record of Google requests is written on every suggestion; a
Rust allocation failure on an enormous PBF ends the process (the build of a whole country is not
supported). Fixed since: Myap Router says only the rules its route breaks, so a note never claims
highways a route does not use; the other routers say what they *may* not have avoided.

### Review of the routing and navigation overhaul

The overhaul (graph version 2, avoidances, optimisations, live traffic, faster routes, dead
reckoning, elevations, the camera view) was reviewed by an independent agent in the same three
steps. Eight problems were confirmed and fixed, each with a test in
`crates/myap/tests/review_live.rs` where it can be driven: an answer of HERE asked a second router
while another was being asked; the check of the guidance made a search in progress stale (directions
and checks now have a generation of their own); the estimate in tunnels ran past maneuvers (it is
now bound to the maneuver that was next at the last fix); one-way roads open to cyclists both ways
lost their lanes and signs (the direction is that of motor traffic); a failed traffic fetch counted
its area as covered; Google's routes did not say what they may not have avoided; the time left on
the route followed ignored closures; and the tiles of elevations interrupted were left on the
device. Also fixed: one wake-up at the earliest time asked (a later one replaces an earlier one at
the platform), a stale offer of a faster route is dropped on a reroute, a quadratic check of the
names of junctions in the builder, and an overflow of a hostile `duration` of ferries.

Left for later, with their risk: a faster route is compared with the route followed timed by its
own router (a difference of a few percent between routers eats into the threshold of 8 %); the cost
of an edge is computed twice per relaxation when traffic is held, and matching traffic allocates a
line per candidate (acceptable once every few minutes); `turn:lanes` is told at whatever junction
of a way the maneuver is, while the wiki ties it to the end of the way. The camera view was tried
on an emulator only (its virtual scene): its projection was checked against the mathematics, not
against a real street.

### Review of the stability of the application

An independent agent read the application and the boundary with the core in the same three steps,
and confirmed or found plausible twelve problems, all fixed:

- a `LocationListener` written as a lambda ended the process on Android 8–10 when the location was
  turned on or off (those versions have no default bodies for its other methods);
- *Start* left a flag that started guidance later for directions asked without it: it is now tied
  to the destination asked, and dropped when those directions are closed;
- fixes of two providers were ordered by their own clocks; they are now timed by the clock of
  elapsed time, the same for every provider;
- guidance went on for ever after arriving: it ends by itself (two minutes by default,
  *End after arriving*), and its notification can end it;
- the map was destroyed without being paused and stopped when the settings opened, leaving
  MapLibre's receiver registered; the map and its style are now state of Compose, so that a map that
  becomes ready is drawn at once;
- the location never picked up a provider turned on later, nor a finer permission granted later,
  and was started and stopped from two threads: providers are registered on or off, on the main
  thread, and chosen again when the permission changes;
- an imported file was copied by a screen, and left behind when the screen was left during the
  copy; the host copies it, removes a failed copy, and a refused permission is not a crash;
- a text-to-speech engine that failed to start silenced the voice for the rest of the process;
  it is made again for the next announcement, and only the latest announcement waits for it;
- the guidance service could be started when it could not run; it is started only with the
  permission, and a refusal of the system is not a crash;
- an exception other than of I/O while reading a response ended the process on OkHttp's thread,
  and left the core waiting;
- downloads interrupted by the end of the application stayed on the device: nothing is downloading
  at a start, so what is left in the downloads is removed.

### Review of combining, the annotation of routes and Android Auto

The new code was reviewed by an independent agent in the same three steps, the rules of the Car
App Library checked against the classes of the library itself. No request reaches Google that its
settings do not allow. Six problems were confirmed or found plausible, and fixed:

- the car's list of places had rows without a distance before the first fix, which the library
  refuses with an exception (a "navigate to…" right after connecting);
- a place of Google whose hours or phone a combined search had filled from OpenStreetMap was
  never asked of Google, so OpenStreetMap's stood where Google's own should win
  (`google_is_asked_for_its_own_details_when_openstreetmap_filled_them`);
- the route chosen on the phone could lie beyond the car's limit of rows, an index the library
  refuses;
- matching a long route searched every road round every sample on the core's thread; the roads
  followed are now tried first;
- a zoom of the car without a focus (negative) zoomed round the corner;
- the buttons of the map, of the cars' API level 2, were used on hosts of level 1.

## Testing

- `cargo test --workspace --all-features`: the formats against their specifications and recorded
  responses (OSRM, Valhalla, Photon, Nominatim, Google), Myap Router on networks where the answer is
  known (oneways, access, restrictions, avoidance, roundabout exits, alternatives, legs, damaged
  files), and the core through its protocol (privacy, Google's session, fallback, regions end to end,
  navigation and rerouting, rate limits, files).
- `ProtocolTest.kt` (Kotlin) and `crates/myap/tests/protocol.rs` read the same JSON on both sides,
  and the Kotlin test reads responses recorded from the core, so that neither side can change the
  protocol alone.
- The application was tried on an emulator (API 37): search, a place, directions by Valhalla and by
  Google with traffic, guidance with announcements, and the settings.

## Continuous integration

The workflows are those of the family (`test.yml` on every push and pull request; `release.yml` on
a tag `vMAJOR.MINOR.PATCH`), with Gradle run through `.github/scripts/gradle.sh`, which retries only
when a dependency repository could not be reached. Every dependency of Gradle is pinned by its
SHA-256 in `gradle/verification-metadata.xml`.
