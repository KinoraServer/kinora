package app.kinora.api

import app.kinora.model.ClientCaps
import app.kinora.model.ItemKind
import app.kinora.model.PlaybackMethod
import app.kinora.model.PlaybackRequest
import app.kinora.model.StopRequest
import app.kinora.model.TrackChoice
import kotlinx.coroutines.test.runTest
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test

class KinoraApiTest {
    private lateinit var server: MockWebServer
    private var token: String? = null
    private lateinit var api: KinoraApi

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
        val client = OkHttpClient.Builder().addInterceptor(AuthInterceptor { url -> token.takeIf { url.host == server.hostName } }).build()
        // A sub-path, like a reverse proxy at https://example.com/kinora/.
        api = KinoraApi(server.url("/kinora/").toString(), client)
    }

    @After
    fun tearDown() = server.shutdown()

    @Test
    fun loginSendsDeviceAndReturnsToken() = runTest {
        server.enqueue(MockResponse().setBody("""{"user":{"id":"u1","username":"lan","role":"owner"},"token":"abc"}"""))
        val res = api.login("lan", "hunter22", "Shield (Kinora TV)")
        assertEquals("abc", res.token)
        val req = server.takeRequest()
        assertEquals("/kinora/api/auth/login", req.path)
        val body = req.body.readUtf8()
        assertTrue(body, body.contains("\"device\":\"Shield (Kinora TV)\""))
    }

    @Test
    fun bearerTokenIsSentWhenSignedIn() = runTest {
        token = "secret"
        server.enqueue(MockResponse().setBody("[]"))
        api.home()
        assertEquals("Bearer secret", server.takeRequest().getHeader("Authorization"))
    }

    @Test
    fun tokenIsNotSentToOtherHosts() {
        val base = "https://kinora.example/sub/".toHttpUrl()
        assertTrue(sameServer("https://kinora.example/sub/api/home".toHttpUrl(), base))
        assertFalse(sameServer("https://image.tmdb.org/x.jpg".toHttpUrl(), base))
        assertFalse(sameServer("http://kinora.example/sub/api/home".toHttpUrl(), base))
    }

    @Test
    fun errorsCarryTheServerMessage() = runTest {
        server.enqueue(MockResponse().setResponseCode(400).setBody("""{"error":"wrong username or password"}"""))
        try {
            api.login("x", "y", "d")
            fail("expected an error")
        } catch (e: ApiException) {
            assertEquals(400, e.status)
            assertEquals("wrong username or password", e.message)
        }
    }

    @Test
    fun itemListBuildsQueryAndIgnoresUnknownFields() = runTest {
        server.enqueue(
            MockResponse().setBody(
                """[{"id":"m1","library_id":"l1","kind":"movie","title":"Film","year":2001,"has_poster":true,
                   "position_ms":null,"something_new":42}]""",
            ),
        )
        val items = api.items(libraryId = "l1", sort = "added", limit = 50)
        assertEquals(ItemKind.Movie, items.single().kind)
        assertEquals("/kinora/api/items?library_id=l1&sort=added&limit=50&offset=0", server.takeRequest().path)
    }

    @Test
    fun playbackStartAndRelativeUrls() = runTest {
        server.enqueue(
            MockResponse().setBody(
                """{"method":"transcode","url":"api/hls/s1/main.m3u8","session_id":"s1",
                   "plan":{"method":"transcode","video":{"kind":"copy"},"audio":null,"audio_index":1,"reasons":["x"]},
                   "duration_ms":1000,"subtitles":[],"hardware":"NVENC"}""",
            ),
        )
        val caps = ClientCaps(listOf("mkv"), listOf("hevc"), listOf("aac"), 6, hdr10 = true, hlg = false, dolbyVision = true)
        val start = api.startPlayback("f1", PlaybackRequest(caps))
        assertEquals(PlaybackMethod.Transcode, start.method)
        assertEquals(server.url("/kinora/api/hls/s1/main.m3u8"), api.resolve(start.url))
        val body = server.takeRequest().body.readUtf8()
        assertTrue(body, body.contains("\"dolby_vision\":true"))
    }

    @Test
    fun noContentResponsesWork() = runTest {
        server.enqueue(MockResponse().setResponseCode(204))
        api.stopHls("s1", StopRequest(positionMs = 5000, itemId = "i1"))
        val req = server.takeRequest()
        assertEquals("DELETE", req.method)
        assertEquals("""{"position_ms":5000,"item_id":"i1"}""", req.body.readUtf8())
    }

    @Test
    fun rememberTracksSendsThePick() = runTest {
        server.enqueue(MockResponse().setResponseCode(204))
        api.rememberTracks("f1", TrackChoice(audioIndex = 2, subtitleIndex = 5))
        val req = server.takeRequest()
        assertEquals("PUT", req.method)
        assertTrue(req.path!!, req.path!!.endsWith("/api/playback/f1/tracks"))
        assertEquals("""{"audio_index":2,"subtitle_index":5}""", req.body.readUtf8())
    }
}
