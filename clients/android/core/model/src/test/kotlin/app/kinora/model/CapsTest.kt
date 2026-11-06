package app.kinora.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class CapsTest {
    // NVIDIA Shield TV (2019) on a Dolby Vision TV with a 7.1 receiver.
    private val shield = DeviceProfile(
        videoDecoders = setOf("h264", "hevc", "vp9", "mpeg2video"),
        hevcMain10 = true,
        dolbyVisionDecoder = true,
        maxDecodeHeight = 2160,
        displayHdr = setOf(HdrFormat.Hdr10, HdrFormat.Hlg, HdrFormat.DolbyVision),
        audioDecoders = setOf("aac", "mp3", "ac3", "eac3", "flac", "opus"),
        audioPassthrough = setOf("ac3", "eac3", "truehd", "dts"),
        maxOutputChannels = 8,
    )

    // Pixel-style phone: AV1, HDR10 screen, no Dolby Vision decoder.
    private val phone = DeviceProfile(
        videoDecoders = setOf("h264", "hevc", "vp9", "av1"),
        hevcMain10 = true,
        dolbyVisionDecoder = false,
        maxDecodeHeight = 2160,
        displayHdr = setOf(HdrFormat.Hdr10, HdrFormat.Hlg),
        audioDecoders = setOf("aac", "mp3", "flac", "opus", "vorbis"),
        audioPassthrough = emptySet(),
        maxOutputChannels = 2,
    )

    @Test
    fun shieldOnDolbyVisionTvGetsEverything() {
        val caps = buildClientCaps(shield)
        assertEquals(listOf("mp4", "mkv", "webm", "mpegts"), caps.containers)
        assertEquals(listOf("h264", "hevc", "vp9", "mpeg2video"), caps.videoCodecs)
        assertTrue(caps.hdr10 && caps.hlg && caps.dolbyVision)
        assertTrue("truehd" in caps.audioCodecs && "dts" in caps.audioCodecs)
        assertEquals(8, caps.maxAudioChannels)
        assertEquals(2160, caps.maxHeight)
    }

    @Test
    fun shieldOnSdrTvReportsNoHdr() {
        val caps = buildClientCaps(shield.copy(displayHdr = emptySet(), audioPassthrough = emptySet(), maxOutputChannels = 2))
        assertFalse(caps.hdr10 || caps.hlg || caps.dolbyVision)
        // AC-3 / E-AC-3 still decode on the device; TrueHD and DTS need passthrough.
        assertTrue("eac3" in caps.audioCodecs)
        assertFalse("truehd" in caps.audioCodecs || "dts" in caps.audioCodecs)
        assertEquals(6, caps.maxAudioChannels)
    }

    @Test
    fun dolbyVisionNeedsBothDecoderAndDisplay() {
        assertFalse(buildClientCaps(phone).dolbyVision)
        assertFalse(buildClientCaps(phone.copy(displayHdr = phone.displayHdr + HdrFormat.DolbyVision)).dolbyVision)
        assertFalse(buildClientCaps(shield.copy(displayHdr = setOf(HdrFormat.Hdr10))).dolbyVision)
    }

    @Test
    fun hdrNeedsTenBitHevc() {
        val caps = buildClientCaps(phone.copy(hevcMain10 = false))
        assertFalse(caps.hdr10)
        assertTrue(buildClientCaps(phone).hdr10)
    }

    @Test
    fun phoneListsAv1AndBaselineAudio() {
        val caps = buildClientCaps(phone)
        assertTrue("av1" in caps.videoCodecs)
        assertEquals(listOf("aac", "mp3", "opus", "flac", "vorbis"), caps.audioCodecs)
    }

    @Test
    fun describeIsReadable() {
        assertEquals(
            "H264 · HEVC · VP9 · MPEG-2 · HDR10 · HLG · Dolby Vision · Dolby Digital · Dolby Digital+ · TrueHD · DTS · up to 2160p",
            describeCaps(buildClientCaps(shield)),
        )
    }
}
