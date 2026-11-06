package app.kinora.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FormatTest {
    @Test
    fun clock() {
        assertEquals("0:05", formatClock(5_000))
        assertEquals("42:05", formatClock(2_525_000))
        assertEquals("1:42:05", formatClock(6_125_000))
        assertEquals("0:00", formatClock(-1))
    }

    @Test
    fun runtime() {
        assertEquals("48m", formatRuntime(48 * 60_000L))
        assertEquals("2h 5m", formatRuntime(125 * 60_000L))
    }

    @Test
    fun episodeCards() {
        val ep = ItemCard(
            id = "e", libraryId = "l", kind = ItemKind.Episode, title = "Pilot",
            seasonNumber = 1, episodeNumber = 2, seriesTitle = "Show",
        )
        assertEquals("Show" to "S1 · E2 · Pilot", cardTitles(ep))
        val movie = ItemCard(id = "m", libraryId = "l", kind = ItemKind.Movie, title = "Film", year = 1999)
        assertEquals("Film" to "1999", cardTitles(movie))
    }

    @Test
    fun progress() {
        assertNull(progressFraction(0, 100))
        assertNull(progressFraction(10, null))
        assertEquals(0.25f, progressFraction(25, 100))
    }

    @Test
    fun audio() {
        assertEquals("English · E-AC-3 5.1", audioLabel(AudioStream(index = 1, codec = "eac3", channels = 6, language = "eng")))
        assertEquals("Commentary · AAC Stereo", audioLabel(AudioStream(index = 2, codec = "aac", channels = 2, title = "Commentary")))
    }
}
