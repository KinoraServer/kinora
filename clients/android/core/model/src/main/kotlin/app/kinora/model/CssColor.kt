package app.kinora.model

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlin.math.sin

/**
 * Parse the CSS colour values Kinora themes use into ARGB ints, so native
 * apps can use the same theme files as the web app. Supports `#rgb`,
 * `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()/rgba()`, `hsl()/hsla()` and
 * `oklch()`. Returns null for anything else (the caller keeps its default).
 */
fun parseCssColor(value: String): Int? {
    val v = value.trim().lowercase()
    return when {
        v.startsWith("#") -> parseHex(v.substring(1))
        v.startsWith("rgb") -> parseRgb(args(v) ?: return null)
        v.startsWith("hsl") -> parseHsl(args(v) ?: return null)
        v.startsWith("oklch") -> parseOklch(args(v) ?: return null)
        else -> null
    }
}

/** Parse a CSS length in rem or px into density-independent pixels (1rem = 16dp). */
fun parseCssLengthDp(value: String): Float? {
    val v = value.trim().lowercase()
    return when {
        v.endsWith("rem") -> v.removeSuffix("rem").trim().toFloatOrNull()?.times(16f)
        v.endsWith("px") -> v.removeSuffix("px").trim().toFloatOrNull()
        else -> v.toFloatOrNull()
    }
}

private fun argb(a: Double, r: Double, g: Double, b: Double): Int {
    fun ch(x: Double) = (x.coerceIn(0.0, 1.0) * 255).roundToInt()
    return (ch(a) shl 24) or (ch(r) shl 16) or (ch(g) shl 8) or ch(b)
}

private fun parseHex(hex: String): Int? {
    if (hex.any { it !in "0123456789abcdef" }) return null
    val full = when (hex.length) {
        3, 4 -> hex.map { "$it$it" }.joinToString("")
        6, 8 -> hex
        else -> return null
    }
    val rgb = full.substring(0, 6).toLong(16).toInt()
    val alpha = if (full.length == 8) full.substring(6, 8).toInt(16) else 0xFF
    return (alpha shl 24) or rgb
}

/** "rgb(1 2 3 / 50%)" or "rgba(1, 2, 3, .5)" -> ["1","2","3","50%"] */
private fun args(v: String): List<String>? {
    val open = v.indexOf('(')
    val close = v.lastIndexOf(')')
    if (open < 0 || close < open) return null
    val parts = v.substring(open + 1, close).replace(",", " ").replace("/", " ").split(Regex("\\s+")).filter { it.isNotEmpty() }
    return parts.takeIf { it.size in 3..4 }
}

/** A number or percentage; `percentOf` is what 100% means. */
private fun num(s: String, percentOf: Double): Double? =
    if (s.endsWith("%")) s.dropLast(1).toDoubleOrNull()?.div(100.0)?.times(percentOf) else s.toDoubleOrNull()

private fun alpha(parts: List<String>): Double? = if (parts.size == 4) num(parts[3], 1.0) else 1.0

private fun parseRgb(p: List<String>): Int? {
    val r = num(p[0], 255.0) ?: return null
    val g = num(p[1], 255.0) ?: return null
    val b = num(p[2], 255.0) ?: return null
    return argb(alpha(p) ?: return null, r / 255, g / 255, b / 255)
}

private fun parseHsl(p: List<String>): Int? {
    val h = (p[0].removeSuffix("deg").toDoubleOrNull() ?: return null).mod(360.0)
    val s = num(p[1], 1.0) ?: return null
    val l = num(p[2], 1.0) ?: return null
    val c = (1 - kotlin.math.abs(2 * l - 1)) * s
    val x = c * (1 - kotlin.math.abs((h / 60).mod(2.0) - 1))
    val m = l - c / 2
    val (r, g, b) = when {
        h < 60 -> Triple(c, x, 0.0)
        h < 120 -> Triple(x, c, 0.0)
        h < 180 -> Triple(0.0, c, x)
        h < 240 -> Triple(0.0, x, c)
        h < 300 -> Triple(x, 0.0, c)
        else -> Triple(c, 0.0, x)
    }
    return argb(alpha(p) ?: return null, r + m, g + m, b + m)
}

/** oklch(L C H): L as 0..1 or %, C as a number (100% = 0.4), H in degrees. */
private fun parseOklch(p: List<String>): Int? {
    val l = num(p[0], 1.0) ?: return null
    val c = num(p[1], 0.4) ?: return null
    val h = if (p[2] == "none") 0.0 else p[2].removeSuffix("deg").toDoubleOrNull() ?: return null
    val a = c * cos(h * PI / 180)
    val b = c * sin(h * PI / 180)
    // OKLab -> linear sRGB (Björn Ottosson's reference matrices).
    val l1 = (l + 0.3963377774 * a + 0.2158037573 * b).pow(3)
    val m1 = (l - 0.1055613458 * a - 0.0638541728 * b).pow(3)
    val s1 = (l - 0.0894841775 * a - 1.2914855480 * b).pow(3)
    val r = 4.0767416621 * l1 - 3.3077115913 * m1 + 0.2309699292 * s1
    val g = -1.2684380046 * l1 + 2.6097574011 * m1 - 0.3413193965 * s1
    val bl = -0.0041960863 * l1 - 0.7034186147 * m1 + 1.7076147010 * s1
    return argb(alpha(p) ?: return null, gamma(r), gamma(g), gamma(bl))
}

private fun gamma(x: Double): Double =
    if (x <= 0.0031308) 12.92 * x else 1.055 * x.pow(1 / 2.4) - 0.055
