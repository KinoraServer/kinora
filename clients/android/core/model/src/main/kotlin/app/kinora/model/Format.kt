package app.kinora.model

/** "1:42:05" or "42:05". */
fun formatClock(ms: Long): String {
    val total = (ms.coerceAtLeast(0) / 1000)
    val h = total / 3600
    val m = (total % 3600) / 60
    val s = total % 60
    return if (h > 0) "%d:%02d:%02d".format(h, m, s) else "%d:%02d".format(m, s)
}

/** "2h 5m", "48m". */
fun formatRuntime(ms: Long): String {
    val minutes = (ms / 60_000).coerceAtLeast(1)
    return if (minutes >= 60) "${minutes / 60}h ${minutes % 60}m" else "${minutes}m"
}

/** "S2 · E5" for episodes, "Season 2" for seasons, else null. */
fun episodeLabel(kind: ItemKind, season: Int?, episode: Int?): String? = when (kind) {
    ItemKind.Episode -> listOfNotNull(season?.let { "S$it" }, episode?.let { "E$it" }).joinToString(" · ").ifEmpty { null }
    ItemKind.Season -> season?.let { "Season $it" }
    else -> null
}

/** Main and secondary line for an item card. */
fun cardTitles(item: ItemCard): Pair<String, String?> = when (item.kind) {
    ItemKind.Episode -> (item.seriesTitle ?: item.title) to
        listOfNotNull(episodeLabel(item.kind, item.seasonNumber, item.episodeNumber), item.title.takeIf { item.seriesTitle != null })
            .joinToString(" · ").ifEmpty { null }
    else -> item.title to item.year?.toString()
}

/** Watched fraction for a progress bar, or null when there's nothing to show. */
fun progressFraction(positionMs: Long?, durationMs: Long?): Float? {
    if (positionMs == null || durationMs == null || durationMs <= 0 || positionMs <= 0) return null
    return (positionMs.toFloat() / durationMs).coerceIn(0f, 1f)
}

/** Human label for an audio stream: "English · E-AC-3 5.1". */
fun audioLabel(a: AudioStream): String {
    val codec = when (a.codec) {
        "eac3" -> "E-AC-3"
        "ac3" -> "AC-3"
        "truehd" -> "TrueHD"
        "dts" -> "DTS"
        else -> a.codec.uppercase()
    }
    val layout = when (a.channels) {
        1 -> "Mono"
        2 -> "Stereo"
        6 -> "5.1"
        8 -> "7.1"
        else -> "${a.channels}ch"
    }
    return listOfNotNull(a.title ?: a.language?.let(::languageName), "$codec $layout").joinToString(" · ")
}

/** A few common ISO 639-2 codes; anything else is shown as-is. */
fun languageName(code: String): String = when (code.lowercase()) {
    "eng", "en" -> "English"
    "spa", "es" -> "Spanish"
    "fre", "fra", "fr" -> "French"
    "ger", "deu", "de" -> "German"
    "ita", "it" -> "Italian"
    "jpn", "ja" -> "Japanese"
    "kor", "ko" -> "Korean"
    "chi", "zho", "zh" -> "Chinese"
    "por", "pt" -> "Portuguese"
    "rus", "ru" -> "Russian"
    "und" -> "Unknown"
    else -> code
}
