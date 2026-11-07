package app.kinora.model

// Mapping between the server's stream indexes (ffprobe) and the player's list
// of tracks, for the "auto-tracks" module. The player numbers tracks by
// position, so these work on positions and the track ids Media3 reports.
//
// - Streamed (HLS) subtitles are added by us with the id "kinora-sub-<index>";
//   Media3 may put a prefix in front ("1:kinora-sub-5").
// - During direct play the player reads the file itself and lists its tracks
//   in file order, the same order as the server's list, so the n-th player
//   track is the n-th server stream when both lists are the same length.

private val SUB_ID = Regex("kinora-sub-(\\d+)$")

/** Position in the player's subtitle list of server subtitle [index], or null if it isn't there. */
fun subtitlePosition(subtitles: List<SubtitleTrack>, trackIds: List<String?>, index: Int): Int? {
    trackIds.indexOfFirst { id -> id?.let { SUB_ID.find(it)?.groupValues?.get(1)?.toIntOrNull() } == index }
        .takeIf { it >= 0 }?.let { return it }
    if (!byPosition(subtitles, trackIds)) return null
    return subtitles.indexOfFirst { it.index == index }.takeIf { it >= 0 }
}

/** Server subtitle index of the player's subtitle at [position], or null if it can't be told. */
fun subtitleIndexAt(subtitles: List<SubtitleTrack>, trackIds: List<String?>, position: Int): Int? {
    if (position !in trackIds.indices) return null
    trackIds[position]?.let { id -> SUB_ID.find(id)?.groupValues?.get(1)?.toIntOrNull()?.let { return it } }
    if (!byPosition(subtitles, trackIds)) return null
    return subtitles[position].index
}

/** Direct play: the player read the file itself (none of our ids) and lists as many tracks as the server. */
private fun byPosition(subtitles: List<SubtitleTrack>, trackIds: List<String?>) =
    trackIds.size == subtitles.size && trackIds.none { it != null && SUB_ID.containsMatchIn(it) }

/** Position in the player's audio list of server audio stream [index] (direct play). */
fun audioPosition(audio: List<AudioStream>, trackCount: Int, index: Int): Int? {
    if (trackCount != audio.size) return null
    return audio.indexOfFirst { it.index == index }.takeIf { it >= 0 }
}

/** Server audio index of the player's audio track at [position] (direct play). */
fun audioIndexAt(audio: List<AudioStream>, trackCount: Int, position: Int): Int? {
    if (trackCount != audio.size) return null
    return audio.getOrNull(position)?.index
}
