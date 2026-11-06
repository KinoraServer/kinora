package app.kinora.tv.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.Card
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.kinora.api.KinoraApi
import app.kinora.model.ItemCard
import app.kinora.model.ItemKind
import app.kinora.model.cardTitles
import app.kinora.model.progressFraction
import coil3.compose.AsyncImage

/** Poster (2:3) for movies, series and seasons; a 16:9 still for episodes. */
@Composable
fun ItemTile(
    item: ItemCard,
    api: KinoraApi,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    width: Dp = if (item.kind == ItemKind.Episode) 260.dp else 150.dp,
) {
    val wide = item.kind == ItemKind.Episode
    val (title, subtitle) = cardTitles(item)
    Column(modifier.width(width)) {
        Card(onClick = onClick, modifier = Modifier.fillMaxWidth().aspectRatio(if (wide) 16f / 9f else 2f / 3f)) {
            Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.surfaceVariant)) {
                // The title shows through until (or unless) the artwork loads.
                Text(
                    title,
                    modifier = Modifier.align(Alignment.Center).padding(12.dp),
                    textAlign = TextAlign.Center,
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                val kind = when {
                    wide -> "thumb"
                    item.hasPoster -> "poster"
                    else -> null
                }
                if (kind != null) {
                    AsyncImage(
                        model = api.imageUrl(item.id, kind),
                        contentDescription = null,
                        contentScale = ContentScale.Crop,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
                progressFraction(item.positionMs, item.durationMs)?.let { ProgressBar(it, Modifier.align(Alignment.BottomCenter)) }
                if (item.played == true) WatchedBadge(Modifier.align(Alignment.TopEnd).padding(8.dp))
            }
        }
        Spacer(Modifier.height(6.dp))
        Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
        if (subtitle != null) {
            Text(
                subtitle,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
fun ProgressBar(fraction: Float, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxWidth().height(4.dp).background(MaterialTheme.colorScheme.background.copy(alpha = 0.6f))) {
        Box(Modifier.fillMaxWidth(fraction).height(4.dp).background(MaterialTheme.colorScheme.primary))
    }
}

@Composable
private fun WatchedBadge(modifier: Modifier = Modifier) {
    Box(
        modifier.size(24.dp).background(MaterialTheme.colorScheme.primary, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text("✓", color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.labelMedium)
    }
}

/** A titled horizontal row of tiles. [firstItemFocus] lets a screen put the initial focus on its first tile. */
@Composable
fun ItemRow(
    title: String,
    items: List<ItemCard>,
    api: KinoraApi,
    onItemClick: (ItemCard) -> Unit,
    modifier: Modifier = Modifier,
    firstItemFocus: FocusRequester? = null,
) {
    Column(modifier) {
        Text(title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(start = 48.dp, bottom = 12.dp))
        LazyRow(
            contentPadding = PaddingValues(horizontal = 48.dp),
            horizontalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            itemsIndexed(items, key = { _, it -> it.id }) { index, item ->
                val m = if (index == 0 && firstItemFocus != null) Modifier.focusRequester(firstItemFocus) else Modifier
                ItemTile(item, api, onClick = { onItemClick(item) }, modifier = m)
            }
        }
    }
}

/**
 * A text field that works with a remote: focus it with the D-pad and press OK
 * to open the on-screen keyboard.
 */
@Composable
fun TvTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    password: Boolean = false,
    keyboardType: KeyboardType = KeyboardType.Text,
    imeAction: ImeAction = ImeAction.Next,
    onImeAction: () -> Unit = {},
    focusRequester: FocusRequester? = null,
) {
    var focused by remember { mutableStateOf(false) }
    val colors = MaterialTheme.colorScheme
    Column(modifier) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = colors.onSurfaceVariant)
        Spacer(Modifier.height(6.dp))
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            textStyle = MaterialTheme.typography.titleMedium.copy(color = colors.onSurface),
            cursorBrush = SolidColor(colors.primary),
            visualTransformation = if (password) PasswordVisualTransformation() else VisualTransformation.None,
            keyboardOptions = KeyboardOptions(
                keyboardType = if (password) KeyboardType.Password else keyboardType,
                imeAction = imeAction,
                autoCorrectEnabled = false,
            ),
            keyboardActions = KeyboardActions(onAny = { onImeAction() }),
            modifier = Modifier
                .width(520.dp)
                .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
                .onFocusChanged { focused = it.isFocused }
                .background(colors.surface, MaterialTheme.shapes.small)
                .border(2.dp, if (focused) colors.primary else colors.border, MaterialTheme.shapes.small)
                .padding(horizontal = 16.dp, vertical = 14.dp),
        )
    }
}

@Composable
fun CenteredMessage(text: String, modifier: Modifier = Modifier, action: Pair<String, () -> Unit>? = null) {
    Column(
        modifier.fillMaxSize(),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center, modifier = Modifier.width(640.dp))
        if (action != null) {
            Spacer(Modifier.height(24.dp))
            Button(onClick = action.second) { Text(action.first) }
        }
    }
}
