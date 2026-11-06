package app.kinora.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ServerAddressTest {
    @Test
    fun bareHostsGetHttpAndThePort() {
        assertEquals("http://192.168.1.20:8420/", normalizeServerAddress("192.168.1.20"))
        assertEquals("http://nas:9000/", normalizeServerAddress(" nas:9000 "))
        assertEquals("http://kinora.tail1234.ts.net:8420/", normalizeServerAddress("kinora.tail1234.ts.net"))
    }

    @Test
    fun fullUrlsKeepTheirPortAndSubPath() {
        assertEquals("https://example.com/kinora/", normalizeServerAddress("https://example.com/kinora"))
        assertEquals("https://example.com/", normalizeServerAddress("HTTPS://example.com/"))
        assertEquals("http://10.0.0.5:8420/", normalizeServerAddress("http://10.0.0.5:8420/#/home"))
    }

    @Test
    fun rejectsJunk() {
        assertNull(normalizeServerAddress(""))
        assertNull(normalizeServerAddress("my server"))
        assertNull(normalizeServerAddress("http://"))
    }
}
