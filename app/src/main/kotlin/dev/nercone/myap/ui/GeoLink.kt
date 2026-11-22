package dev.nercone.myap.ui

import android.net.Uri

/** `geo:` links (RFC 5870, and the `q` of Android's form), as the phone and the car open them. */
object GeoLink {
    /**
     * What a `geo:` URI asks for: a search, or the coordinates as a search, which the core reads
     * as a point. `null` for another scheme or no point (`geo:0,0` without a query).
     */
    fun query(uri: Uri): String? {
        if (uri.scheme != "geo") return null
        val query = uri.encodedQuery?.split('&')?.firstOrNull { it.startsWith("q=") }?.removePrefix("q=")?.let(Uri::decode)?.trim()
        if (!query.isNullOrEmpty()) return query
        val point = uri.schemeSpecificPart.substringBefore('?').substringBefore(';')
        return point.takeIf { it.isNotBlank() && it != "0,0" }
    }
}
