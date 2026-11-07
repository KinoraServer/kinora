package app.kinora.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Shapes returned by the Kinora API. These mirror web/src/lib/types.ts and the
// Rust structs in crates/server. They will be replaced by a client generated
// from the server's OpenAPI spec once it exists (see docs/plan.md, gap G1).
// Times and positions are milliseconds, like everywhere else in Kinora.

@Serializable
enum class Role {
    @SerialName("owner") Owner,
    @SerialName("admin") Admin,
    @SerialName("editor") Editor,
    @SerialName("viewer") Viewer,
}

@Serializable
data class User(val id: String, val username: String, val role: Role)

@Serializable
data class SystemInfo(
    val name: String,
    val version: String,
    @SerialName("server_name") val serverName: String,
    @SerialName("setup_required") val setupRequired: Boolean,
)

@Serializable
data class LoginRequest(val username: String, val password: String, val device: String? = null)

@Serializable
data class LoginResponse(val user: User, val token: String)

@Serializable
enum class ItemKind {
    @SerialName("movie") Movie,
    @SerialName("series") Series,
    @SerialName("season") Season,
    @SerialName("episode") Episode,
}

@Serializable
data class ItemCard(
    val id: String,
    @SerialName("library_id") val libraryId: String,
    val kind: ItemKind,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("series_id") val seriesId: String? = null,
    val title: String,
    val year: Int? = null,
    @SerialName("season_number") val seasonNumber: Int? = null,
    @SerialName("episode_number") val episodeNumber: Int? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    @SerialName("has_poster") val hasPoster: Boolean = false,
    @SerialName("has_backdrop") val hasBackdrop: Boolean = false,
    @SerialName("has_thumb") val hasThumb: Boolean = false,
    @SerialName("duration_ms") val durationMs: Long? = null,
    @SerialName("position_ms") val positionMs: Long? = null,
    val played: Boolean? = null,
    val rating: Double? = null,
)

@Serializable
data class HomeRow(val id: String, val title: String, val items: List<ItemCard>)

@Serializable
data class Library(
    val id: String,
    val name: String,
    val kind: String,
    @SerialName("item_count") val itemCount: Long = 0,
)

@Serializable
enum class HdrFormat {
    @SerialName("sdr") Sdr,
    @SerialName("hdr10") Hdr10,
    @SerialName("hlg") Hlg,
    @SerialName("dolby_vision") DolbyVision,
}

@Serializable
data class VideoStream(
    val codec: String,
    val width: Int,
    val height: Int,
    @SerialName("bit_depth") val bitDepth: Int = 8,
    val hdr: HdrFormat = HdrFormat.Sdr,
    @SerialName("dv_profile") val dvProfile: Int? = null,
)

@Serializable
data class AudioStream(
    val index: Int,
    val codec: String,
    val channels: Int,
    val language: String? = null,
    val title: String? = null,
    @SerialName("is_default") val isDefault: Boolean = false,
)

@Serializable
data class SubtitleStream(
    val index: Int,
    val codec: String,
    val language: String? = null,
    val title: String? = null,
    @SerialName("is_text") val isText: Boolean = false,
)

@Serializable
data class MediaInfo(
    val container: String,
    @SerialName("duration_ms") val durationMs: Long,
    val bitrate: Long? = null,
    val video: VideoStream? = null,
    val audio: List<AudioStream> = emptyList(),
    val subtitles: List<SubtitleStream> = emptyList(),
)

@Serializable
data class MediaFile(
    val id: String,
    val size: Long = 0,
    @SerialName("duration_ms") val durationMs: Long? = null,
    val label: String? = null,
    val info: MediaInfo? = null,
)

@Serializable
data class ItemDetail(
    val id: String,
    @SerialName("library_id") val libraryId: String,
    val kind: ItemKind,
    @SerialName("parent_id") val parentId: String? = null,
    @SerialName("series_id") val seriesId: String? = null,
    val title: String,
    @SerialName("original_title") val originalTitle: String? = null,
    val year: Int? = null,
    @SerialName("season_number") val seasonNumber: Int? = null,
    @SerialName("episode_number") val episodeNumber: Int? = null,
    val overview: String? = null,
    val tagline: String? = null,
    val rating: Double? = null,
    @SerialName("official_rating") val officialRating: String? = null,
    @Serializable(with = LenientStringList::class) val genres: List<String> = emptyList(),
    @SerialName("runtime_minutes") val runtimeMinutes: Int? = null,
    @SerialName("series_title") val seriesTitle: String? = null,
    @SerialName("has_poster") val hasPoster: Boolean = false,
    @SerialName("has_backdrop") val hasBackdrop: Boolean = false,
    @SerialName("has_thumb") val hasThumb: Boolean = false,
    val files: List<MediaFile> = emptyList(),
    val children: List<ItemCard> = emptyList(),
    @SerialName("position_ms") val positionMs: Long = 0,
    val played: Boolean = false,
    @SerialName("next_up") val nextUp: ItemCard? = null,
)

/** What the client can play. Mirrors `kinora_core::playback::ClientCaps`. */
@Serializable
data class ClientCaps(
    val containers: List<String>,
    @SerialName("video_codecs") val videoCodecs: List<String>,
    @SerialName("audio_codecs") val audioCodecs: List<String>,
    @SerialName("max_audio_channels") val maxAudioChannels: Int,
    val hdr10: Boolean,
    val hlg: Boolean,
    @SerialName("dolby_vision") val dolbyVision: Boolean,
    @SerialName("max_height") val maxHeight: Int? = null,
)

@Serializable
data class PlaybackRequest(
    val caps: ClientCaps,
    @SerialName("audio_index") val audioIndex: Int? = null,
    @SerialName("max_bitrate") val maxBitrate: Long? = null,
    @SerialName("force_transcode") val forceTranscode: Boolean = false,
)

@Serializable
enum class PlaybackMethod {
    @SerialName("direct_play") DirectPlay,
    @SerialName("remux") Remux,
    @SerialName("transcode") Transcode,
}

@Serializable
data class PlaybackPlan(
    val method: PlaybackMethod,
    val reasons: List<String> = emptyList(),
    @SerialName("audio_index") val audioIndex: Int? = null,
)

@Serializable
data class SubtitleTrack(
    val index: Int,
    val label: String,
    val language: String? = null,
    val url: String,
    val codec: String,
    @SerialName("is_default") val isDefault: Boolean = false,
    @SerialName("is_forced") val isForced: Boolean = false,
    val supported: Boolean = false,
)

@Serializable
data class PlaybackStart(
    val method: PlaybackMethod,
    /** Relative to the server's base URL. */
    val url: String,
    @SerialName("session_id") val sessionId: String? = null,
    val plan: PlaybackPlan,
    @SerialName("duration_ms") val durationMs: Long,
    val subtitles: List<SubtitleTrack> = emptyList(),
    val hardware: String = "",
    /** Tracks the server picked from this viewer's choices on other episodes (module "auto-tracks"). */
    @SerialName("auto_tracks") val autoTracks: AutoTracks? = null,
)

/** `auto_tracks` in [PlaybackStart]. Mirrors `kinora_server::modules::auto_tracks::AutoTracks`. */
@Serializable
data class AutoTracks(
    /** "season", "series" or "library": which remembered choice was used. */
    val from: String,
    @SerialName("audio_index") val audioIndex: Int? = null,
    /** [NO_OPINION] when the server sent nothing (the player decides); null means subtitles off. */
    @SerialName("subtitle_index") val subtitleIndex: Int? = NO_OPINION,
) {
    companion object {
        const val NO_OPINION = -1
    }
}

/** `PUT api/playback/{file}/tracks`: what the viewer picked. Null subtitles means off. */
@Serializable
data class TrackChoice(
    @SerialName("audio_index") val audioIndex: Int?,
    @SerialName("subtitle_index") val subtitleIndex: Int?,
)

@Serializable
data class ProgressUpdate(
    @SerialName("position_ms") val positionMs: Long,
    @SerialName("duration_ms") val durationMs: Long?,
)

@Serializable
data class StopRequest(
    @SerialName("position_ms") val positionMs: Long?,
    @SerialName("item_id") val itemId: String?,
)

@Serializable
data class PlayedUpdate(val played: Boolean)

@Serializable
data class Theme(
    val id: String,
    val name: String,
    val author: String? = null,
    val scheme: String = "dark",
    val tokens: Map<String, String> = emptyMap(),
)

@Serializable
data class ThemeList(
    @SerialName("default_theme") val defaultTheme: String,
    val themes: List<Theme>,
)
