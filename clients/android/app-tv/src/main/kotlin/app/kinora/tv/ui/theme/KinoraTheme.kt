package app.kinora.tv.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.tv.material3.ColorScheme
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Shapes
import androidx.tv.material3.darkColorScheme
import androidx.tv.material3.lightColorScheme
import app.kinora.model.Theme
import app.kinora.model.parseCssColor
import app.kinora.model.parseCssLengthDp

/** Kinora Green, used until the server's theme loads and for any token a theme leaves out. */
private val KinoraDark = darkColorScheme(
    primary = Color(0xFF2BD48F),
    onPrimary = Color(0xFF04140D),
    background = Color(0xFF0B1612),
    onBackground = Color(0xFFECF7F1),
    surface = Color(0xFF13221C),
    onSurface = Color(0xFFECF7F1),
    surfaceVariant = Color(0xFF1C2E26),
    onSurfaceVariant = Color(0xFF8FA79B),
    secondaryContainer = Color(0xFF1A3328),
    onSecondaryContainer = Color(0xFFECF7F1),
    border = Color(0xFF22382E),
    error = Color(0xFFF2555A),
    inverseSurface = Color(0xFFECF7F1),
    inverseOnSurface = Color(0xFF0B1612),
)

/**
 * Map a Kinora theme (the same token files the web app uses, from
 * `/api/themes`) onto the TV Material colour scheme.
 */
fun colorSchemeFor(theme: Theme?): ColorScheme {
    if (theme == null) return KinoraDark
    val t = theme.tokens
    val base = if (theme.scheme == "light") lightColorScheme() else KinoraDark
    fun c(key: String, fallback: Color) = t[key]?.let(::parseCssColor)?.let { Color(it) } ?: fallback
    val background = c("background", base.background)
    val foreground = c("foreground", base.onBackground)
    return base.copy(
        primary = c("primary", base.primary),
        onPrimary = c("primary-foreground", base.onPrimary),
        background = background,
        onBackground = foreground,
        surface = c("card", base.surface),
        onSurface = c("card-foreground", foreground),
        surfaceVariant = c("muted", base.surfaceVariant),
        onSurfaceVariant = c("muted-foreground", base.onSurfaceVariant),
        secondaryContainer = c("accent", base.secondaryContainer),
        onSecondaryContainer = c("accent-foreground", foreground),
        border = c("border", base.border),
        error = c("destructive", base.error),
        // Focused buttons and cards use the inverse colours.
        inverseSurface = foreground,
        inverseOnSurface = background,
    )
}

@Composable
fun KinoraTheme(theme: Theme?, content: @Composable () -> Unit) {
    val colors = remember(theme) { colorSchemeFor(theme) }
    val radius = remember(theme) { theme?.tokens?.get("radius")?.let(::parseCssLengthDp) ?: 12f }
    val shapes = remember(radius) {
        Shapes(
            extraSmall = RoundedCornerShape((radius / 3).dp),
            small = RoundedCornerShape((radius * 2 / 3).dp),
            medium = RoundedCornerShape(radius.dp),
            large = RoundedCornerShape((radius * 4 / 3).dp),
            extraLarge = RoundedCornerShape((radius * 2).dp),
        )
    }
    MaterialTheme(colorScheme = colors, shapes = shapes) {
        // TV Material text is black unless something sets a content colour, and
        // only its Surface does that. Our screens draw straight on the background.
        CompositionLocalProvider(LocalContentColor provides colors.onBackground, content = content)
    }
}
