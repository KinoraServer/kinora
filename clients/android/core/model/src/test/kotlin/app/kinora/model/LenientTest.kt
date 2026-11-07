package app.kinora.model

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

class LenientTest {
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }
    private fun detail(genres: String) =
        json.decodeFromString<ItemDetail>("""{"id":"i","library_id":"l","kind":"movie","title":"Dune",$genres}""")

    @Test
    fun genresAsAList() {
        assertEquals(listOf("Action", "Science Fiction"), detail(""""genres":["Action","Science Fiction"]""").genres)
    }

    @Test
    fun genresAsAJsonStringFromOlderServers() {
        // Older servers sent the stored text and then the list under the same key.
        val d = detail(""""genres":"[\"Action\",\"Science Fiction\"]","genres":["Action","Science Fiction"]""")
        assertEquals(listOf("Action", "Science Fiction"), d.genres)
        assertEquals(listOf("Drama"), detail(""""genres":"[\"Drama\"]"""").genres)
    }

    @Test
    fun unreadableGenresBecomeEmpty() {
        assertEquals(emptyList<String>(), detail(""""genres":null""").genres)
        assertEquals(emptyList<String>(), detail(""""genres":"Action"""").genres)
        assertEquals(emptyList<String>(), detail(""""genres":42""").genres)
    }
}
