package app.kinora.tv.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * The Kinora mark: a lens ring around a swirl of fanned cards. Drawn the same
 * way as the web app's `Logo.tsx` (on a 100×100 grid) and coloured from the
 * theme, so it follows whatever theme the server uses.
 */
@Composable
fun KinoraMark(size: Dp, modifier: Modifier = Modifier, color: Color = MaterialTheme.colorScheme.primary) {
    val centre = lerp(color, Color.White, 0.6f)
    Canvas(modifier.size(size)) {
        val u = this.size.minDimension / 100f
        fun p(x: Double, y: Double) = Offset(x.toFloat() * u, y.toFloat() * u)
        // Ring: outer radius 40, inner 30.
        drawCircle(color, radius = 35f * u, center = p(50.0, 50.0), style = Stroke(width = 10f * u))
        for (i in 0 until 16) {
            val a1 = i * 22.5 * PI / 180
            val a2 = (i * 22.5 + 38) * PI / 180
            drawLine(
                color = color.copy(alpha = 0.35f + 0.65f * (i / 15f)),
                start = p(50 + 7 * cos(a1), 50 + 7 * sin(a1)),
                end = p(50 + 22 * cos(a2), 50 + 22 * sin(a2)),
                strokeWidth = 4.2f * u,
                cap = StrokeCap.Round,
            )
        }
        drawCircle(centre, radius = 5f * u, center = p(50.0, 50.0))
    }
}

/** The mark followed by the "kinora" wordmark. */
@Composable
fun KinoraLogo(height: Dp, modifier: Modifier = Modifier) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(height * 0.25f)) {
        KinoraMark(height)
        Text(
            "kinora",
            style = TextStyle(fontSize = (height.value * 0.7f).sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.sp),
            color = MaterialTheme.colorScheme.onBackground,
        )
    }
}
