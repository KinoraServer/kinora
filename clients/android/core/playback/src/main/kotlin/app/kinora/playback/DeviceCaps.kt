package app.kinora.playback

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.display.DisplayManager
import android.media.AudioFormat
import android.media.AudioManager
import android.media.MediaCodecInfo.CodecProfileLevel
import android.media.MediaCodecList
import android.os.Build
import android.view.Display
import app.kinora.model.ClientCaps
import app.kinora.model.DeviceProfile
import app.kinora.model.HdrFormat
import app.kinora.model.buildClientCaps

/**
 * Reads what this device and the screen it's connected to can play. All the
 * decisions live in the pure [buildClientCaps]; this file only gathers facts.
 */
object DeviceCaps {

    fun detect(context: Context): ClientCaps = buildClientCaps(profile(context))

    fun profile(context: Context): DeviceProfile {
        val codecs = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { !it.isEncoder }

        val videoMimes = mapOf(
            "video/avc" to "h264",
            "video/hevc" to "hevc",
            "video/av01" to "av1",
            "video/x-vnd.on2.vp9" to "vp9",
            "video/mpeg2" to "mpeg2video",
        )
        val audioMimes = mapOf(
            "audio/mp4a-latm" to "aac",
            "audio/mpeg" to "mp3",
            "audio/opus" to "opus",
            "audio/flac" to "flac",
            "audio/vorbis" to "vorbis",
            "audio/ac3" to "ac3",
            "audio/eac3" to "eac3",
        )

        val videoDecoders = mutableSetOf<String>()
        val audioDecoders = mutableSetOf<String>()
        var hevcMain10 = false
        var dolbyVision = false
        var maxHeight = 0
        for (info in codecs) {
            for (type in info.supportedTypes) {
                val mime = type.lowercase()
                videoMimes[mime]?.let { name ->
                    videoDecoders += name
                    val caps = runCatching { info.getCapabilitiesForType(type) }.getOrNull()
                    caps?.videoCapabilities?.supportedHeights?.upper?.let { maxHeight = maxOf(maxHeight, it) }
                    if (name == "hevc" && caps?.profileLevels?.any { it.profile == CodecProfileLevel.HEVCProfileMain10 } == true) {
                        hevcMain10 = true
                    }
                }
                audioMimes[mime]?.let { audioDecoders += it }
                if (mime == "video/dolby-vision") dolbyVision = true
            }
        }

        val (passthrough, channels) = hdmiAudio(context)
        return DeviceProfile(
            videoDecoders = videoDecoders,
            hevcMain10 = hevcMain10,
            dolbyVisionDecoder = dolbyVision,
            maxDecodeHeight = maxHeight.takeIf { it > 0 },
            displayHdr = displayHdr(context),
            audioDecoders = audioDecoders,
            audioPassthrough = passthrough,
            maxOutputChannels = channels,
        )
    }

    @Suppress("DEPRECATION")
    private fun displayHdr(context: Context): Set<HdrFormat> {
        val display = context.getSystemService(DisplayManager::class.java)?.getDisplay(Display.DEFAULT_DISPLAY) ?: return emptySet()
        val types: IntArray = if (Build.VERSION.SDK_INT >= 34) {
            display.mode.supportedHdrTypes
        } else {
            display.hdrCapabilities?.supportedHdrTypes ?: IntArray(0)
        }
        return types.toList().mapNotNull {
            when (it) {
                Display.HdrCapabilities.HDR_TYPE_HDR10 -> HdrFormat.Hdr10
                Display.HdrCapabilities.HDR_TYPE_HLG -> HdrFormat.Hlg
                Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION -> HdrFormat.DolbyVision
                else -> null
            }
        }.toSet()
    }

    /** Formats the HDMI sink (TV or receiver) accepts as a bitstream, and its channel count. */
    private fun hdmiAudio(context: Context): Pair<Set<String>, Int> {
        val intent: Intent = context.registerReceiver(null, IntentFilter(AudioManager.ACTION_HDMI_AUDIO_PLUG))
            ?: return emptySet<String>() to 2
        if (intent.getIntExtra(AudioManager.EXTRA_AUDIO_PLUG_STATE, 0) != 1) return emptySet<String>() to 2
        val encodings = intent.getIntArrayExtra(AudioManager.EXTRA_ENCODINGS) ?: IntArray(0)
        val names = encodings.toList().mapNotNull {
            when (it) {
                AudioFormat.ENCODING_AC3 -> "ac3"
                AudioFormat.ENCODING_E_AC3, AudioFormat.ENCODING_E_AC3_JOC -> "eac3"
                AudioFormat.ENCODING_DOLBY_TRUEHD -> "truehd"
                AudioFormat.ENCODING_DTS, AudioFormat.ENCODING_DTS_HD -> "dts"
                else -> null
            }
        }.toSet()
        val channels = intent.getIntExtra(AudioManager.EXTRA_MAX_CHANNEL_COUNT, 2)
        return names to channels
    }
}
