# Using Myap

## The map

- **Search** with the bar at the top. Suggestions come while you type; *Search* on the keyboard shows every result on the map and in the sheet below. Coordinates (`35.6812, 139.7671`) show that point.
- The results name the **branch** of a place (*スターバックス 渋谷店*). OpenStreetMap keeps the branch apart from the name, and its search (Photon) does not give it, so Myap looks the results up in Nominatim (their ids, one request a search) while *Settings → Places → Look up details in OpenStreetMap* is on; with it off, or when Nominatim does not answer, the results of OpenStreetMap show the name alone. The suggestions while typing show the name alone, since Nominatim may not be asked at every pause in typing.
- **Hold** a point of the map to drop a pin; its address is looked up (with Nominatim, at most once a second).
- The **layers** button opens the map settings; the **location** button centres the map on you (it asks for the location permission the first time).
- The text at the bottom left says whose map and data you see.

## A place

The sheet shows what is known of the place: its category, rating, whether it is open, its opening hours, phone, website and wheelchair access, its address and distance. *Directions* plans the trip from your location; *Start* plans it and starts the guidance at once; *Save* keeps it as home, work or a favourite; *Share* sends its name, address and a link to openstreetmap.org.

### OpenStreetMap and Google combined

What is known of a place comes from OpenStreetMap (its tags, looked up with Nominatim when you open the place) and, when you allow it, from Google. With *Settings → Places → Combine OpenStreetMap and Google* (on by default), each fills what the other lacks:

- a search of Google is also searched in OpenStreetMap; the same place found by both is shown once, with what each knows, and the places of OpenStreetMap around the results of Google that Google did not find are shown after them;
- a place of Google you open is looked for in OpenStreetMap (by its name, within about 300 m), and its opening hours, phone, website and wheelchair access are added where Google has none;
- a place of OpenStreetMap you open is looked for in Google, only while Google's *Details of places* is on (it sends the place's name, address and position, which the record shows), and its rating and what else OpenStreetMap lacks are added.

What a place knows is never replaced by the other source, and the sheet says under *Sources* which source gave which part. Two places are taken to be the same only when their names are alike (spaces, punctuation, full-width letters and hiragana or katakana do not matter) and they are near each other; a name written in two scripts ("Tokyo Tower", "東京タワー") is not matched, so the sources are sometimes not combined rather than combined wrongly. The hours of OpenStreetMap are shown as OpenStreetMap writes them (`Mo-Fr 09:00-18:00`).

## Directions

- **Mode**: car, bicycle or walking.
- **Router**: *Automatic* tries the routers in the order of *Settings → Routes*, skipping those that cannot answer (Google when it is off, Myap Router outside the downloaded regions, a server that does not answer) — the sheet says when another router than the first answered. Choose one router to use only it.
- **Avoid**, by the mode: tolls, highways, ferries, unpaved roads, narrow roads, level crossings and turns across oncoming traffic (right turns in Japan) by car; hills, unpaved roads, steps, narrow and unlit roads, level crossings and ferries by bicycle or on foot.
  - *Rules* (tolls, highways, ferries, unpaved roads, steps, level crossings, areas to avoid) are left out wherever another way exists. When none does, the route uses them and says so.
  - *Preferences* (narrow and unlit roads, hills, turns across traffic) make those roads cost more: they are taken only when they save much.
  - **Myap Router** follows them all. OSRM, Valhalla and Google follow what they can; the route then says what *may not* have been avoided (for example "Unpaved roads may not have been avoided").
- **Choose routes for**: *Fastest* (main roads preferred), *Shortest*, *Eco* (few stops, starts and climbs, steady speeds) or *Simple* (few turns).
- **Areas to avoid**: *Avoid this area* on a place keeps routes out of 150 m round it; they are listed under the options and in *Settings → Routes*, and can be removed there.
- Each route shows its time (red when traffic makes it 5 minutes or more longer), distance, arrival time, main roads, router, whether it uses tolls or ferries, the climb and descent with its profile (where elevations are known), and how many level crossings, speed cameras and steep climbs it has. Tap a route, on the map or in the list, to choose it; *Steps* lists its turns.

## Guidance

The banner at the top shows the next maneuver: its distance, what to do, the road and the name of the junction (日比谷交差点を左方向です), the signs (方面, the number of an exit) and, when the road has them, its **lanes** with those to take bright. The small tab under it shows the maneuver right after.

- **Warnings ahead**: level crossings, speed cameras, steep climbs and congestion, on the screen and by voice (*Settings → Guidance*). Whether warnings of speed cameras may be used while driving depends on the country (some forbid it): check the law where you drive.
- **Speed limit** where you are (from OpenStreetMap; it may be missing or wrong) and your speed, red above it.
- The camera zooms out at speed and in before a turn; the map turns dark after sunset when *Dark map* is *By sunset and sunrise*.
- **Without GPS** (a tunnel), the position moves on along the route at your last speed for up to 90 s, and says it is estimated.
- **Faster routes**: with live traffic (below), the route left is checked every few minutes; a route that saves enough is offered (*Take it* or *Keep this route*), or taken at once if you chose so.
- **Avoid the road ahead** (⊘): the road from about 30 m to 250 m ahead is avoided for three hours and a new route is found — for a closure or a jam you see.
- The buttons at the side: the overview of the route, the voice on and off, the **camera view**, and back to your position.
- The bar at the bottom shows the time and distance left, the arrival time and the delay of traffic; tap it for the steps ahead.

If you leave the route, a new one is found from where you are, by the same router first. Guidance goes on with the screen off; its notification shows the next maneuver, and *End navigation* ends it. After you arrive, the guidance ends by itself after two minutes (*Settings → Guidance → End after arriving*).

**Routes of other routers with what OpenStreetMap knows.** When a downloaded region holds the route, the routes of Google, OSRM and Valhalla get the speed limits, level crossings, speed cameras, toll booths and names of junctions of the region, where they have none (*Settings → Routes → Add what offline regions know*, on by default; done on the phone, nothing is sent). The route is matched to the roads of the region by place and direction; where less than 60 % of it lies on them, nothing is added.

### The camera view

The camera view draws the route ahead and the next turn over the picture of the back camera, from the phone's position, compass and tilt. The picture is neither recorded nor sent. It is as precise as the compass and the location: calibrate the compass when asked (move the phone in a figure of eight), and expect the drawing to be off by a few metres. **Look at the road, not the screen**, and never hold the phone while driving: use it on foot, or mounted, only when it is safe.

## Android Auto

Myap works in Android Auto as a navigation application: connect the phone to the car (or open Android Auto) and choose Myap.

- **The map** is Myap's own, with your position, the results and the route. Its buttons move it (*pan*), bring it back to your position and zoom.
- **Search** (🔍) with the car's keyboard or voice; suggestions come as you type. **Saved places** (☆) lists your saved and recent places. Choose a place to see its routes; choose a route and *Start*.
- **Guidance** shows the next maneuver, the one after it when it follows closely, and the time, distance and arrival; the voice can be turned off and the guidance ended from the car. Guidance started on the phone shows in the car too, and the other way.
- "Navigate to …" by the car's voice assistant searches the place in Myap.
- The location of the phone is used; if it is not allowed yet, the car asks you to allow it on the phone.

What the car shows is limited by Android Auto: lanes, signs and warnings of what is ahead are shown on the phone only, and the car decides how many rows a list may show. Myap does not use Google Play services; Android Auto itself is Google's application. **Keep your eyes on the road**: the car's screen locks what may be used while driving.

## Live traffic

With a key of your own HERE account (*Settings → Live traffic*), Myap Router uses the traffic of now: before a route by Myap Router, the traffic of the area of the trip is fetched from HERE (the area and the key are sent), and the route goes round jams and closures; its line is coloured by congestion. During guidance it is fetched again every few minutes to look for faster routes. Without a key nothing is sent to HERE. HERE's terms may limit how long its data may be kept; Myap keeps it in memory for at most 15 minutes.

Google's traffic during guidance is a use of Google, off until turned on in *Settings → Google* (*Traffic during guidance*: it sends your position and the places still to reach every few minutes).

## Offline regions

*Settings → Offline regions* holds the regions where **Myap Router** routes on the phone, without the network:

- **The area on the map**: the roads of the area shown (up to about 1,500 km²), downloaded from the Overpass API. Zoom to the area first.
- **From a URL**: an OpenStreetMap PBF extract, for example a prefecture or region of `download.geofabrik.de/asia/japan/`.
- **From a file**: a PBF file on the phone.

The routing data is built on the phone after the download; a large extract takes minutes and memory (an extract of a whole country may be too large for a phone: prefer a region). The map itself still needs the network (it is cached as you look at it).

Once built, the region routes at once while its **elevations** are added: the terrain tiles over it are downloaded from the source of *Settings → Offline regions → Elevations* (Terrain Tiles of AWS Open Data, worldwide; or the elevation tiles of GSI 国土地理院, in Japan; or none) and removed once added. The source sees the area. With elevations, routes avoid hills when asked, walking and cycling times follow the gradient, and routes show their climb. Regions built by an earlier version of Myap must be downloaded again.

## Google

*Settings → Google* is the one place for Google:

- **Use Google** turns every use on or off at once.
- Each **use** says what it sends: suggestions while typing (what you type and the area of the map), search (the search and the area), details of places (the name, address and position of a place you open: of Google, and of OpenStreetMap while the sources are combined), routes with traffic (the places of your trip), map tiles and the traffic layer (which parts of the map are shown).
- The **record** keeps, on the phone only, every request sent to Google, with the time and what was sent. *Clear the record* deletes it and also ends the Google session (Myap keeps Google's cookies in memory only, and a new session starts with every launch). It cannot delete what Google already received.

Google's web pages are not a public API: a use may stop working, and Google may refuse requests (the page then shows Google's last answer). Myap then uses OpenStreetMap data and the open routers instead.

## Servers

*Settings → Routes → Servers* lists every service Myap uses; each can be replaced by your own (HTTPS only). The public servers are run by volunteers (FOSSGIS, OpenStreetMap, komoot); Myap follows their policies (an honest User-Agent, Nominatim at most once a second, no search as you type on Nominatim).
