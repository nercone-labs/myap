package dev.nercone.myap.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The protocol as both sides write it. The JSON of the inputs here is also read by the core in
 * `crates/myap/tests/protocol.rs` (the same strings), and the responses are recorded from the core
 * (`src/test/resources`), so a change on either side that the other cannot read fails a test.
 */
class ProtocolTest {
    private fun encode(input: Input): String = CoreJson.json.encodeToString(Input.serializer(), input)

    private fun resource(name: String): String = javaClass.classLoader!!.getResource(name)!!.readText()

    @Test
    fun inputsAreTaggedAsTheCoreReadsThem() {
        val place = Place("b", PlaceSource.COORDINATE, "東京タワー", Coordinate(35.6586, 139.7454))
        assertEquals(
            """{"type":"plan","now":1,"origin":{"kind":"my_location"},"destination":{"kind":"place","place":{"id":"b","source":"coordinate","name":"東京タワー","coordinate":{"latitude":35.6586,"longitude":139.7454},"hours":[],"links":[],"filled":[]}},"stops":[],"mode":"car","avoid":{"tolls":false,"highways":false,"ferries":false,"unpaved":false,"narrow":false,"steps":false,"hills":false,"level_crossings":false,"crossing_turns":false,"unlit":false},"router":{"kind":"automatic"}}""",
            encode(Input.Plan(1, Endpoint.MyLocation, Endpoint.Of(place), emptyList(), TravelMode.CAR, Avoid(), RouterChoice.AUTOMATIC)),
        )
        assertEquals(
            """{"type":"fetched","now":2,"id":3,"response":{"status":200,"body":"x","headers":[["Set-Cookie","a=b"]]}}""",
            encode(Input.Fetched(2, 3, HttpResponse(200, "x", listOf(listOf("Set-Cookie", "a=b"))))),
        )
        assertEquals("""{"type":"snapshot","now":4}""", encode(Input.Snap(4)))
        assertEquals(
            """{"type":"avoid_area","now":8,"name":"工事","center":{"latitude":35.0,"longitude":139.0},"radius_m":150.0}""",
            encode(Input.AvoidAreaAt(8, "工事", Coordinate(35.0, 139.0), 150.0)),
        )
        assertEquals("""{"type":"avoid_ahead","now":9}""", encode(Input.AvoidAhead(9)))
        val only = encode(Input.Plan(5, Endpoint.MyLocation, Endpoint.MyLocation, emptyList(), TravelMode.FOOT, Avoid(tolls = true), RouterChoice.only(RouterKind.MYAP)))
        assertTrue(only, only.contains(""""router":{"kind":"only","router":"myap"}"""))
    }

    @Test
    fun settingsRoundTrip() {
        val settings = Settings(google = GoogleSettings(enabled = true, search = true), routing = RoutingSettings(order = listOf(RouterKind.MYAP, RouterKind.OSRM)))
        val text = encode(Input.Configure(1, settings))
        val back = CoreJson.json.decodeFromString(Input.serializer(), text) as Input.Configure
        assertEquals(settings, back.settings)
        assertTrue(text.contains(""""place_details":false"""))
        assertTrue(text.contains(""""prefer_offline":false"""))
    }

    @Test
    fun readsTheResponsesOfTheCore() {
        val planned = Core.decode(resource("fetch_effect.json"))
        assertNull(planned.error)
        val fetch = planned.effects.single() as Effect.Fetch
        assertTrue(fetch.request.url.startsWith("https://routing.openstreetmap.de/routed-foot/"))
        assertEquals("GET", fetch.request.method)
        assertTrue(planned.snapshot!!.directions!!.loading)

        val routed = Core.decode(resource("response.json"))
        assertNull(routed.error)
        val snapshot = routed.snapshot!!
        val directions = snapshot.directions!!
        assertFalse(directions.loading)
        assertEquals(2, directions.routes.size)
        assertEquals(RouterKind.OSRM, directions.routes[0].router)
        assertTrue(directions.routes[0].steps.isNotEmpty())
        assertEquals(2, directions.endpoints.size)
        assertTrue(directions.endpoints[0] is Endpoint.Of)
        assertEquals("ja-JP", snapshot.language)
        assertNotNull(snapshot.style.url)
        val overlay = CoreJson.json.parseToJsonElement(snapshot.overlay) as JsonObject
        assertEquals("FeatureCollection", overlay["type"]!!.jsonPrimitive.content)
        assertNotNull(snapshot.camera?.bounds)
        assertFalse(snapshot.settings.google.enabled)
        assertTrue(overlay.jsonObject.containsKey("features"))
    }

    @Test
    fun anUnreadableEffectCostsOnlyItself() {
        val text = """{"effects":[{"type":"speak","text":"右方向です","language":"ja-JP"},{"type":"future_effect"},{"type":"wake","at":5}],"error":null}"""
        val response = Core.decode(text)
        assertEquals(2, response.effects.size)
        assertEquals(Effect.Wake(5), response.effects[1])
        assertNotNull(response.error)
        assertEquals("unreadable response of the core", Core.decode("not json").error)
    }

    @Test
    fun placesCombinedFromTwoSourcesAreRead() {
        // As the core writes a place of Google filled from OpenStreetMap (`geocoding::Place`).
        val text = """{"id":"google:0x1:0x2","source":"google","name":"カフェ","coordinate":{"latitude":35.68,"longitude":139.76},"rating":4.2,"hours":["Mo-Fr 08:00-20:00"],"wheelchair":"limited","links":["osm:N9"],"filled":[{"field":"hours","source":"open_street_map"},{"field":"wheelchair","source":"open_street_map"}]}"""
        val place = CoreJson.json.decodeFromString(Place.serializer(), text)
        assertEquals(Wheelchair.LIMITED, place.wheelchair)
        assertEquals(listOf("osm:N9"), place.links)
        assertEquals(PlaceSource.OPEN_STREET_MAP, place.sourceOf(PlaceField.HOURS))
        assertEquals(PlaceSource.GOOGLE, place.sourceOf(PlaceField.RATING))
        assertEquals(listOf(PlaceSource.GOOGLE, PlaceSource.OPEN_STREET_MAP), place.sources())
        // Sent back to the core (saved, planned to) with all it knows.
        val back = CoreJson.json.encodeToString(Place.serializer(), place)
        assertTrue(back, back.contains(""""filled":[{"field":"hours","source":"open_street_map"}"""))
    }
}
