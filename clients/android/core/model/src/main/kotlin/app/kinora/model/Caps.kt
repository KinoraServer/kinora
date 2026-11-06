package app.kinora.model

/**
 * What the device and the screen it's plugged into can do, in plain data.
 * `core:playback` fills this in from Android APIs (MediaCodecList, the
 * display's HDR types, the HDMI audio sink); everything below is pure so it
 * can be unit-tested with fake device profiles.
 */
data class DeviceProfile(
    /** Video decoders present, as ffmpeg codec names ("h264", "hevc", "av1", "vp9", "mpeg2video"). */
    val videoDecoders: Set<String>,
    /** The HEVC decoder handles Main10 (needed for HDR10/HLG/DV base layers). */
    val hevcMain10: Boolean,
    /** A `video/dolby-vision` decoder exists. */
    val dolbyVisionDecoder: Boolean,
    /** Largest height any video decoder supports. */
    val maxDecodeHeight: Int?,
    /** HDR formats the connected display accepts. */
    val displayHdr: Set<HdrFormat>,
    /** Audio decoders present, as ffmpeg codec names ("aac", "ac3", "eac3", "flac", "opus", ...). */
    val audioDecoders: Set<String>,
    /** Bitstream formats the HDMI/audio sink accepts for passthrough ("ac3", "eac3", "truehd", "dts"). */
    val audioPassthrough: Set<String>,
    /** Channels the audio output can take (8 for a 7.1 receiver, 2 for TV speakers). */
    val maxOutputChannels: Int,
)

/** Containers Media3 can direct play from a plain HTTP URL (ffprobe names on the server side). */
val DIRECT_PLAY_CONTAINERS = listOf("mp4", "mkv", "webm", "mpegts")

private val KNOWN_VIDEO = listOf("h264", "hevc", "av1", "vp9", "mpeg2video")

/** Every Android device decodes these, so they're always listed. */
private val BASELINE_AUDIO = listOf("aac", "mp3")
private val OPTIONAL_AUDIO = listOf("opus", "flac", "vorbis", "ac3", "eac3")
private val PASSTHROUGH_ONLY_AUDIO = listOf("truehd", "dts")

/**
 * Turn a device profile into the capabilities the server's playback decision
 * engine expects. The server then picks direct play, remux or transcode.
 */
fun buildClientCaps(device: DeviceProfile): ClientCaps {
    val video = KNOWN_VIDEO.filter { it in device.videoDecoders }

    val audio = BASELINE_AUDIO +
        OPTIONAL_AUDIO.filter { it in device.audioDecoders || it in device.audioPassthrough } +
        PASSTHROUGH_ONLY_AUDIO.filter { it in device.audioPassthrough }

    // Android's decoders downmix 5.1 to stereo on their own, so 6 channels are
    // always fine. More than that only when the output really takes them.
    val channels = maxOf(6, device.maxOutputChannels).coerceAtMost(8)

    val tenBitHevc = "hevc" in device.videoDecoders && device.hevcMain10
    return ClientCaps(
        containers = DIRECT_PLAY_CONTAINERS,
        videoCodecs = video.distinct(),
        audioCodecs = audio.distinct(),
        maxAudioChannels = channels,
        hdr10 = tenBitHevc && HdrFormat.Hdr10 in device.displayHdr,
        hlg = tenBitHevc && HdrFormat.Hlg in device.displayHdr,
        dolbyVision = device.dolbyVisionDecoder && HdrFormat.DolbyVision in device.displayHdr,
        maxHeight = device.maxDecodeHeight,
    )
}

/** Short summary for the settings/diagnostics screen, e.g. "HEVC · AV1 · HDR10 · Dolby Vision · TrueHD". */
fun describeCaps(caps: ClientCaps): String {
    val parts = mutableListOf<String>()
    caps.videoCodecs.forEach { parts += it.uppercase().replace("MPEG2VIDEO", "MPEG-2") }
    if (caps.hdr10) parts += "HDR10"
    if (caps.hlg) parts += "HLG"
    if (caps.dolbyVision) parts += "Dolby Vision"
    caps.audioCodecs.filter { it in listOf("ac3", "eac3", "truehd", "dts") }.forEach {
        parts += when (it) {
            "ac3" -> "Dolby Digital"
            "eac3" -> "Dolby Digital+"
            "truehd" -> "TrueHD"
            else -> "DTS"
        }
    }
    caps.maxHeight?.let { parts += "up to ${it}p" }
    return parts.joinToString(" · ")
}
