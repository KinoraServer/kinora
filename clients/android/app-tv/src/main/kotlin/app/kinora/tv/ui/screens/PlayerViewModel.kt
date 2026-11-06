// Media3 marks parts of its API as unstable; this app opts in deliberately.
@file:androidx.annotation.OptIn(markerClass = [androidx.media3.common.util.UnstableApi::class])

package app.kinora.tv.ui.screens

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.Tracks
import androidx.media3.exoplayer.ExoPlayer
import app.kinora.api.KinoraApi
import app.kinora.model.ItemDetail
import app.kinora.model.PlaybackMethod
import app.kinora.model.PlaybackRequest
import app.kinora.model.PlaybackStart
import app.kinora.model.ProgressUpdate
import app.kinora.model.StopRequest
import app.kinora.model.audioLabel
import app.kinora.model.episodeLabel
import app.kinora.model.languageName
import app.kinora.playback.PlayerFactory
import app.kinora.tv.container
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** One entry in the audio or subtitle menu. */
data class TrackOption(val key: String, val label: String, val selected: Boolean)

data class PlayerUiState(
    val title: String = "",
    val subtitle: String? = null,
    val loading: Boolean = true,
    val error: String? = null,
    val playing: Boolean = false,
    val buffering: Boolean = false,
    val positionMs: Long = 0,
    val durationMs: Long = 0,
    val bufferedMs: Long = 0,
    /** "Direct play", "Remux" or "Transcode (NVENC)", plus the server's reasons. */
    val method: String = "",
    val reasons: List<String> = emptyList(),
    /** What the decoder is actually getting, e.g. "3840×2160 · video/dolby-vision · 23.976 fps". */
    val videoInfo: String = "",
    val audioInfo: String = "",
    val audioOptions: List<TrackOption> = emptyList(),
    val subtitleOptions: List<TrackOption> = emptyList(),
)

/**
 * Owns the ExoPlayer for one item: asks the server how to play it, plays it,
 * reports progress, and tells the server when it's done.
 */
class PlayerViewModel(
    application: Application,
    private val itemId: String,
    private val requestedStartMs: Long,
) : AndroidViewModel(application) {
    private val container = application.container
    private val api: KinoraApi = container.api()
    val player: ExoPlayer = PlayerFactory.create(application, container.http)

    private val _state = MutableStateFlow(PlayerUiState())
    val state: StateFlow<PlayerUiState> = _state.asStateFlow()

    private var item: ItemDetail? = null
    private var start: PlaybackStart? = null
    private var lastSavedMs = -1L
    private var ticker: Job? = null

    private val listener = object : Player.Listener {
        override fun onIsPlayingChanged(isPlaying: Boolean) {
            _state.update { it.copy(playing = isPlaying) }
            if (!isPlaying) saveProgress()
        }

        override fun onPlaybackStateChanged(playbackState: Int) {
            _state.update { it.copy(buffering = playbackState == Player.STATE_BUFFERING, loading = false) }
            if (playbackState == Player.STATE_ENDED) saveProgress(ended = true)
        }

        override fun onTracksChanged(tracks: Tracks) = refreshTracks()

        override fun onTrackSelectionParametersChanged(parameters: TrackSelectionParameters) = refreshTracks()

        override fun onPlayerError(error: PlaybackException) {
            _state.update { it.copy(error = "${error.errorCodeName}: ${error.cause?.message ?: error.message ?: ""}".trim()) }
        }
    }

    init {
        player.addListener(listener)
        viewModelScope.launch { load(startMs = null, audioIndex = null, forceTranscode = false) }
        ticker = viewModelScope.launch {
            var sinceSave = 0
            while (isActive) {
                _state.update {
                    it.copy(
                        positionMs = player.currentPosition,
                        durationMs = player.duration.takeIf { d -> d > 0 } ?: (start?.durationMs ?: 0),
                        bufferedMs = player.bufferedPosition,
                    )
                }
                // Save progress every 10 s while playing, like the web app.
                if (player.isPlaying && ++sinceSave >= 20) {
                    sinceSave = 0
                    saveProgress()
                }
                delay(500)
            }
        }
    }

    private suspend fun load(startMs: Long?, audioIndex: Int?, forceTranscode: Boolean) {
        _state.update { it.copy(loading = true, error = null) }
        try {
            val detail = item ?: api.item(itemId).also { item = it }
            val file = detail.files.firstOrNull() ?: error("This item has no playable file.")
            val begin = startMs ?: if (requestedStartMs >= 0) requestedStartMs else detail.positionMs
            stopServerSession()
            val playback = api.startPlayback(file.id, PlaybackRequest(container.caps(), audioIndex = audioIndex, forceTranscode = forceTranscode))
            start = playback

            val builder = MediaItem.Builder().setUri(api.resolve(playback.url).toString())
            if (playback.method != PlaybackMethod.DirectPlay) {
                builder.setMimeType(MimeTypes.APPLICATION_M3U8)
                // HLS carries no subtitles; add the server's WebVTT tracks. During direct play
                // Media3 reads every embedded track (including PGS) straight from the file.
                builder.setSubtitleConfigurations(
                    playback.subtitles.filter { it.supported }.map { s ->
                        MediaItem.SubtitleConfiguration.Builder(android.net.Uri.parse(api.resolve(s.url).toString()))
                            .setMimeType(MimeTypes.TEXT_VTT)
                            .setLanguage(s.language)
                            .setLabel(s.label)
                            .setId("kinora-sub-${s.index}")
                            .setSelectionFlags(
                                (if (s.isDefault) C.SELECTION_FLAG_DEFAULT else 0) or (if (s.isForced) C.SELECTION_FLAG_FORCED else 0),
                            )
                            .build()
                    },
                )
            }
            player.setMediaItem(builder.build(), begin.coerceAtLeast(0))
            player.prepare()
            player.playWhenReady = true

            val methodLabel = when (playback.method) {
                PlaybackMethod.DirectPlay -> "Direct play"
                PlaybackMethod.Remux -> "Remux"
                PlaybackMethod.Transcode -> "Transcode" + (playback.hardware.takeIf { it.isNotBlank() }?.let { " ($it)" } ?: "")
            }
            _state.update {
                it.copy(
                    title = if (detail.seriesTitle != null) detail.seriesTitle!! else detail.title,
                    subtitle = listOfNotNull(episodeLabel(detail.kind, detail.seasonNumber, detail.episodeNumber), detail.title.takeIf { detail.seriesTitle != null })
                        .joinToString(" · ").ifEmpty { null },
                    method = methodLabel,
                    reasons = playback.plan.reasons,
                    loading = false,
                )
            }
        } catch (e: Exception) {
            _state.update { it.copy(loading = false, error = e.message ?: e.javaClass.simpleName) }
        }
    }

    fun togglePlay() {
        if (player.isPlaying) player.pause() else player.play()
    }

    fun seekBy(deltaMs: Long) {
        val target = (player.currentPosition + deltaMs).coerceIn(0, player.duration.takeIf { it > 0 } ?: Long.MAX_VALUE)
        player.seekTo(target)
    }

    /** After an error: ask the server for a transcode instead (e.g. a Dolby Vision file the TV rejected). */
    fun retryWithTranscode() {
        viewModelScope.launch { load(startMs = player.currentPosition, audioIndex = start?.plan?.audioIndex, forceTranscode = true) }
    }

    fun selectSubtitle(key: String) {
        val params = player.trackSelectionParameters.buildUpon()
        if (key == OFF) {
            params.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
        } else {
            val group = groupFor(C.TRACK_TYPE_TEXT, key) ?: return
            params.setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false)
                .setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup, 0))
        }
        player.trackSelectionParameters = params.build()
    }

    fun selectAudio(key: String) {
        val s = start ?: return
        if (s.method == PlaybackMethod.DirectPlay) {
            val group = groupFor(C.TRACK_TYPE_AUDIO, key) ?: return
            player.trackSelectionParameters = player.trackSelectionParameters.buildUpon()
                .setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup, 0))
                .build()
        } else {
            // A stream from the server carries one audio track, so switching restarts it with the new one.
            val index = key.removePrefix("server-").toIntOrNull() ?: return
            if (index == s.plan.audioIndex) return
            viewModelScope.launch { load(startMs = player.currentPosition, audioIndex = index, forceTranscode = false) }
        }
    }

    private fun groupFor(type: Int, key: String): Tracks.Group? =
        player.currentTracks.groups.filter { it.type == type }.getOrNull(key.removePrefix("track-").toIntOrNull() ?: -1)

    private fun refreshTracks() {
        val tracks = player.currentTracks
        val text = tracks.groups.filter { it.type == C.TRACK_TYPE_TEXT }
        val textDisabled = C.TRACK_TYPE_TEXT in player.trackSelectionParameters.disabledTrackTypes
        val subtitleOptions = listOf(TrackOption(OFF, "Off", textDisabled || text.none { it.isSelected })) +
            text.mapIndexed { i, g -> TrackOption("track-$i", trackLabel(g.getTrackFormat(0), i), g.isSelected && !textDisabled) }

        val s = start
        val audioOptions = if (s == null || s.method == PlaybackMethod.DirectPlay) {
            tracks.groups.filter { it.type == C.TRACK_TYPE_AUDIO }
                .mapIndexed { i, g -> TrackOption("track-$i", audioTrackLabel(g.getTrackFormat(0), i), g.isSelected) }
        } else {
            val streams = item?.files?.firstOrNull()?.info?.audio.orEmpty()
            streams.map { a -> TrackOption("server-${a.index}", audioLabel(a), a.index == s.plan.audioIndex) }
        }

        _state.update {
            it.copy(
                subtitleOptions = subtitleOptions,
                audioOptions = audioOptions,
                videoInfo = player.videoFormat?.let(::describeVideo).orEmpty(),
                audioInfo = player.audioFormat?.let { f -> audioTrackLabel(f, 0) }.orEmpty(),
            )
        }
    }

    private fun saveProgress(ended: Boolean = false) {
        val position = if (ended) player.duration else player.currentPosition
        val duration = player.duration.takeIf { it > 0 } ?: start?.durationMs
        if (position <= 0 || position == lastSavedMs) return
        lastSavedMs = position
        container.appScope.launch {
            runCatching { api.saveProgress(itemId, ProgressUpdate(position, duration)) }
        }
    }

    private suspend fun stopServerSession() {
        val sessionId = start?.sessionId ?: return
        start = null
        runCatching { api.stopHls(sessionId, StopRequest(player.currentPosition, itemId)) }
    }

    override fun onCleared() {
        ticker?.cancel()
        saveProgress()
        val sessionId = start?.sessionId
        val position = player.currentPosition
        if (sessionId != null) {
            container.appScope.launch { runCatching { api.stopHls(sessionId, StopRequest(position, itemId)) } }
        }
        player.removeListener(listener)
        player.release()
    }

    companion object {
        const val OFF = "off"
    }
}

private fun trackLabel(f: Format, i: Int): String {
    val name = f.label ?: f.language?.let(::languageName)
    val extra = listOfNotNull(
        if (f.selectionFlags and C.SELECTION_FLAG_FORCED != 0) "Forced" else null,
        when (f.sampleMimeType) {
            MimeTypes.APPLICATION_PGS -> "PGS"
            MimeTypes.TEXT_SSA -> "ASS"
            MimeTypes.APPLICATION_VOBSUB -> "VobSub"
            else -> null
        },
    )
    return (listOf(name ?: "Track ${i + 1}") + extra).joinToString(" · ")
}

private fun audioTrackLabel(f: Format, i: Int): String {
    val codec = when (f.sampleMimeType) {
        MimeTypes.AUDIO_E_AC3, MimeTypes.AUDIO_E_AC3_JOC -> "E-AC-3"
        MimeTypes.AUDIO_AC3 -> "AC-3"
        MimeTypes.AUDIO_TRUEHD -> "TrueHD"
        MimeTypes.AUDIO_DTS, MimeTypes.AUDIO_DTS_HD -> "DTS"
        MimeTypes.AUDIO_AAC -> "AAC"
        else -> f.sampleMimeType?.substringAfter('/')?.uppercase()
    }
    val channels = when (f.channelCount) {
        Format.NO_VALUE -> null
        1 -> "Mono"
        2 -> "Stereo"
        6 -> "5.1"
        8 -> "7.1"
        else -> "${f.channelCount}ch"
    }
    val name = f.label ?: f.language?.let(::languageName) ?: "Track ${i + 1}"
    return listOfNotNull(name, listOfNotNull(codec, channels).joinToString(" ").ifEmpty { null }).joinToString(" · ")
}

private fun describeVideo(f: Format): String {
    val parts = mutableListOf<String>()
    if (f.width > 0 && f.height > 0) parts += "${f.width}×${f.height}"
    f.sampleMimeType?.let { parts += it.removePrefix("video/") }
    f.codecs?.let { parts += it }
    f.colorInfo?.let { c ->
        parts += when (c.colorTransfer) {
            C.COLOR_TRANSFER_ST2084 -> "PQ (HDR10)"
            C.COLOR_TRANSFER_HLG -> "HLG"
            C.COLOR_TRANSFER_SDR -> "SDR"
            else -> "transfer ${c.colorTransfer}"
        }
    }
    if (f.frameRate > 0) parts += "%.3f fps".format(f.frameRate)
    return parts.joinToString(" · ")
}
