package app.kinora.model

/**
 * Turn what someone typed with a TV remote into a base URL that relative API
 * paths resolve against. Kinora's URLs are all relative ("api/stream/..."),
 * so the base must end with "/" to keep reverse-proxy sub-paths working:
 *
 * - "192.168.1.20"            -> "http://192.168.1.20:8420/"
 * - "nas:8420"                -> "http://nas:8420/"
 * - "https://example.com/kinora" -> "https://example.com/kinora/"
 *
 * Returns null when it can't be a URL.
 */
fun normalizeServerAddress(input: String, defaultPort: Int = 8420): String? {
    var s = input.trim()
    if (s.isEmpty() || s.any { it.isWhitespace() }) return null
    val hasScheme = s.startsWith("http://", ignoreCase = true) || s.startsWith("https://", ignoreCase = true)
    if (!hasScheme) s = "http://$s"
    val schemeEnd = s.indexOf("://") + 3
    val pathStart = s.indexOf('/', schemeEnd).let { if (it < 0) s.length else it }
    val host = s.substring(schemeEnd, pathStart)
    if (host.isEmpty() || host.startsWith(":")) return null
    // Add the default port only when the person typed a bare host without a
    // scheme. An explicit http(s):// URL means "use the standard port".
    val hostWithPort = if (!hasScheme && !hostHasPort(host)) "$host:$defaultPort" else host
    val path = s.substring(pathStart).substringBefore('?').substringBefore('#').trimEnd('/')
    return s.substring(0, schemeEnd).lowercase() + hostWithPort + path + "/"
}

private fun hostHasPort(host: String): Boolean =
    if (host.startsWith("[")) host.contains("]:") else host.contains(':')

/** True when the connection to this server is not encrypted. */
fun isPlainHttp(baseUrl: String): Boolean = baseUrl.startsWith("http://")
