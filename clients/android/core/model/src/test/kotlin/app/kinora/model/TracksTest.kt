package app.kinora.model

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TracksTest {
    private fun sub(index: Int) = SubtitleTrack(index = index, label = "t$index", url = "u$index", codec = "ass", supported = true)
    private val subs = listOf(sub(3), sub(4), sub(5))

    @Test
    fun streamedSubtitlesAreFoundById() {
        // HLS: only our side-loaded tracks, plus a caption track from the stream.
        val ids = listOf("0:cea608", "1:kinora-sub-3", "2:kinora-sub-5")
        assertEquals(2, subtitlePosition(subs, ids, 5))
        assertNull(subtitlePosition(subs, ids, 4))
        assertEquals(3, subtitleIndexAt(subs, ids, 1))
        assertNull(subtitleIndexAt(subs, ids, 0))
    }

    @Test
    fun directPlaySubtitlesAreMatchedByPosition() {
        val ids = listOf("3", "4", "5")
        assertEquals(1, subtitlePosition(subs, ids, 4))
        assertEquals(5, subtitleIndexAt(subs, ids, 2))
        // Different track count: can't tell which is which.
        assertNull(subtitlePosition(subs, ids.take(2), 4))
        assertNull(subtitleIndexAt(subs, ids.take(2), 1))
    }

    @Test
    fun audioIsMatchedByPosition() {
        val audio = listOf(AudioStream(1, "aac", 2, "eng"), AudioStream(2, "aac", 2, "jpn"))
        assertEquals(1, audioPosition(audio, 2, 2))
        assertEquals(2, audioIndexAt(audio, 2, 1))
        assertNull(audioPosition(audio, 3, 2))
    }

    @Test
    fun autoTracksTellOffFromNoOpinion() {
        val json = Json { ignoreUnknownKeys = true; explicitNulls = false; encodeDefaults = true }
        val off = json.decodeFromString(AutoTracks.serializer(), """{"from":"series","audio_index":2,"subtitle_index":null}""")
        assertNull(off.subtitleIndex)
        val none = json.decodeFromString(AutoTracks.serializer(), """{"from":"library","audio_index":null}""")
        assertEquals(AutoTracks.NO_OPINION, none.subtitleIndex)
        val pick = json.decodeFromString(AutoTracks.serializer(), """{"from":"series","audio_index":1,"subtitle_index":5}""")
        assertEquals(5, pick.subtitleIndex)
        // Off is sent as a missing field, which the server reads as off too.
        assertEquals("""{"audio_index":2}""", json.encodeToString(TrackChoice.serializer(), TrackChoice(2, null)))
    }
}
