package app.kinora.tv.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.OutlinedButton
import androidx.tv.material3.Text
import app.kinora.model.ItemCard
import app.kinora.model.ItemDetail
import app.kinora.model.ItemKind
import app.kinora.model.MediaInfo
import app.kinora.model.HdrFormat
import app.kinora.model.audioLabel
import app.kinora.model.describeCaps
import app.kinora.model.episodeLabel
import app.kinora.model.formatClock
import app.kinora.model.formatRuntime
import app.kinora.tv.BuildConfig
import app.kinora.tv.container
import app.kinora.tv.ui.components.CenteredMessage
import app.kinora.tv.ui.components.ItemRow
import app.kinora.tv.ui.components.ItemTile
import coil3.compose.AsyncImage
import kotlinx.coroutines.launch

@Composable
fun HomeScreen(
    onOpenItem: (ItemCard) -> Unit,
    onOpenLibrary: (id: String, name: String) -> Unit,
    onOpenSettings: () -> Unit,
    onSignOut: () -> Unit,
) {
    val container = LocalContext.current.container
    val api = remember { container.api() }
    val loader = rememberLoader { Pair(api.libraries(), api.home()) }
    LaunchedEffect(Unit) { container.refreshTheme() }

    LoadContent(loader, onSignOut) { (libraries, rows) ->
        val firstFocus = remember { FocusRequester() }
        val nonEmpty = rows.filter { it.items.isNotEmpty() }
        LaunchedEffect(Unit) { runCatching { firstFocus.requestFocus() } }

        LazyColumn(
            contentPadding = PaddingValues(vertical = 32.dp),
            verticalArrangement = Arrangement.spacedBy(32.dp),
        ) {
            item {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 48.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text("kinora", style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.primary)
                    Spacer(Modifier.width(24.dp))
                    libraries.forEachIndexed { i, lib ->
                        val m = if (i == 0 && nonEmpty.isEmpty()) Modifier.focusRequester(firstFocus) else Modifier
                        OutlinedButton(onClick = { onOpenLibrary(lib.id, lib.name) }, modifier = m) { Text(lib.name) }
                    }
                    Spacer(Modifier.weight(1f))
                    OutlinedButton(onClick = onOpenSettings) { Text("Settings") }
                }
            }
            if (nonEmpty.isEmpty() && libraries.isEmpty()) {
                item { CenteredMessage("No libraries yet. Add one in the Kinora web app under Admin › Libraries.") }
            }
            itemsIndexed(nonEmpty, key = { _, row -> row.id }) { index, row ->
                ItemRow(
                    title = row.title,
                    items = row.items,
                    api = api,
                    onItemClick = onOpenItem,
                    firstItemFocus = if (index == 0) firstFocus else null,
                )
            }
        }
    }
}

private val SORTS = listOf("title" to "A–Z", "added" to "Recently added", "year" to "Year", "rating" to "Rating")

@Composable
fun LibraryScreen(libraryId: String, name: String, onOpenItem: (ItemCard) -> Unit, onSignOut: () -> Unit) {
    val container = LocalContext.current.container
    val api = remember { container.api() }
    var sort by rememberSaveable { mutableStateOf("title") }
    val loader = rememberLoader(sort) { api.items(libraryId = libraryId, sort = sort.takeIf { it != "title" }, limit = 2000) }
    val firstFocus = remember { FocusRequester() }

    Column(Modifier.fillMaxSize().padding(top = 32.dp)) {
        Row(
            Modifier.padding(horizontal = 48.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(name, style = MaterialTheme.typography.headlineMedium)
            Spacer(Modifier.width(24.dp))
            SORTS.forEach { (id, label) ->
                if (id == sort) Button(onClick = {}) { Text(label) } else OutlinedButton(onClick = { sort = id }) { Text(label) }
            }
        }
        Spacer(Modifier.height(24.dp))
        LoadContent(loader, onSignOut) { items ->
            LaunchedEffect(items) { runCatching { firstFocus.requestFocus() } }
            if (items.isEmpty()) {
                CenteredMessage("This library is empty. Scan it from the web app.")
            } else {
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(150.dp),
                    contentPadding = PaddingValues(start = 48.dp, end = 48.dp, bottom = 48.dp),
                    horizontalArrangement = Arrangement.spacedBy(20.dp),
                    verticalArrangement = Arrangement.spacedBy(24.dp),
                ) {
                    itemsIndexed(items, key = { _, it -> it.id }) { index, item ->
                        val m = if (index == 0) Modifier.focusRequester(firstFocus) else Modifier
                        ItemTile(item, api, onClick = { onOpenItem(item) }, modifier = m)
                    }
                }
            }
        }
    }
}

@Composable
fun ItemScreen(
    itemId: String,
    onOpenItem: (ItemCard) -> Unit,
    onPlay: (itemId: String, startMs: Long) -> Unit,
    onSignOut: () -> Unit,
) {
    val container = LocalContext.current.container
    val api = remember { container.api() }
    val scope = rememberCoroutineScope()
    val loader = rememberLoader(itemId) { api.item(itemId) }

    LoadContent(loader, onSignOut) { item ->
        val playFocus = remember { FocusRequester() }
        LaunchedEffect(item.id) { runCatching { playFocus.requestFocus() } }

        Box(Modifier.fillMaxSize()) {
            if (item.hasBackdrop || item.seriesId != null) {
                AsyncImage(
                    model = api.imageUrl(item.id, "backdrop"),
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            }
            val bg = MaterialTheme.colorScheme.background
            Box(
                Modifier.fillMaxSize().background(
                    Brush.horizontalGradient(0f to bg, 0.55f to bg.copy(alpha = 0.85f), 1f to Color.Transparent),
                ),
            )
            Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0.5f to Color.Transparent, 1f to bg)))

            LazyColumn(contentPadding = PaddingValues(vertical = 48.dp), verticalArrangement = Arrangement.spacedBy(24.dp)) {
                item { ItemHeader(item) }
                item {
                    Row(Modifier.padding(horizontal = 48.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        val target = playTarget(item)
                        if (target != null) {
                            val (targetId, label, resumeAt) = target
                            Button(onClick = { onPlay(targetId, resumeAt) }, modifier = Modifier.focusRequester(playFocus)) {
                                Text(label)
                            }
                            if (resumeAt > 0) {
                                OutlinedButton(onClick = { onPlay(targetId, 0) }) { Text("Play from start") }
                            }
                        }
                        if (item.kind == ItemKind.Movie || item.kind == ItemKind.Episode || item.children.isNotEmpty()) {
                            val m = if (target == null) Modifier.focusRequester(playFocus) else Modifier
                            OutlinedButton(
                                onClick = {
                                    scope.launch {
                                        runCatching { api.setPlayed(item.id, !item.played) }
                                        loader.reload()
                                    }
                                },
                                modifier = m,
                            ) { Text(if (item.played) "Mark unwatched" else "Mark watched") }
                        }
                    }
                }
                if (item.children.isNotEmpty()) {
                    item {
                        ItemRow(
                            title = if (item.kind == ItemKind.Series) "Seasons" else "Episodes",
                            items = item.children,
                            api = api,
                            onItemClick = onOpenItem,
                        )
                    }
                }
            }
        }
    }
}

/** (item to play, button label, resume position) for this page, or null when there's nothing to play. */
private fun playTarget(item: ItemDetail): Triple<String, String, Long>? = when (item.kind) {
    ItemKind.Movie, ItemKind.Episode -> {
        if (item.files.isEmpty()) {
            null
        } else if (item.positionMs > 0) {
            Triple(item.id, "Resume from ${formatClock(item.positionMs)}", item.positionMs)
        } else {
            Triple(item.id, "Play", 0L)
        }
    }
    ItemKind.Series, ItemKind.Season -> item.nextUp?.let { next ->
        val label = episodeLabel(next.kind, next.seasonNumber, next.episodeNumber)?.let { "Play $it" } ?: "Play next"
        Triple(next.id, label, next.positionMs ?: 0L)
    }
}

@Composable
private fun ItemHeader(item: ItemDetail) {
    Column(Modifier.padding(horizontal = 48.dp).width(760.dp)) {
        if (item.seriesTitle != null && item.kind != ItemKind.Series) {
            Text(item.seriesTitle!!, style = MaterialTheme.typography.titleLarge, color = MaterialTheme.colorScheme.primary)
        }
        Text(item.title, style = MaterialTheme.typography.displaySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Spacer(Modifier.height(8.dp))
        val meta = listOfNotNull(
            episodeLabel(item.kind, item.seasonNumber, item.episodeNumber),
            item.year?.toString(),
            item.runtimeMinutes?.let { formatRuntime(it * 60_000L) },
            item.officialRating,
            item.rating?.let { "★ %.1f".format(it) },
        ).joinToString("  ·  ")
        if (meta.isNotEmpty()) Text(meta, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (item.genres.isNotEmpty()) {
            Text(item.genres.joinToString(", "), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        item.files.firstOrNull()?.info?.let { info ->
            Spacer(Modifier.height(8.dp))
            Text(mediaSummary(info), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
        }
        item.overview?.let {
            Spacer(Modifier.height(16.dp))
            Text(it, style = MaterialTheme.typography.bodyLarge, maxLines = 5, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** "4K · HEVC 10-bit · Dolby Vision 8 · TrueHD 7.1 · MKV" */
private fun mediaSummary(info: MediaInfo): String {
    val parts = mutableListOf<String>()
    info.video?.let { v ->
        parts += when {
            v.height >= 2000 || v.width >= 3800 -> "4K"
            v.height >= 1000 || v.width >= 1900 -> "1080p"
            v.height >= 700 -> "720p"
            else -> "${v.height}p"
        }
        parts += v.codec.uppercase() + if (v.bitDepth > 8) " ${v.bitDepth}-bit" else ""
        when (v.hdr) {
            HdrFormat.DolbyVision -> parts += "Dolby Vision" + (v.dvProfile?.let { " $it" } ?: "")
            HdrFormat.Hdr10 -> parts += "HDR10"
            HdrFormat.Hlg -> parts += "HLG"
            HdrFormat.Sdr -> {}
        }
    }
    (info.audio.firstOrNull { it.isDefault } ?: info.audio.firstOrNull())?.let { parts += audioLabel(it.copy(title = null, language = null)) }
    parts += info.container.replace("matroska", "mkv").substringBefore(',').uppercase()
    return parts.joinToString("  ·  ")
}

@Composable
fun SettingsScreen(onChangeServer: () -> Unit, onSignedOut: () -> Unit) {
    val container = LocalContext.current.container
    val scope = rememberCoroutineScope()
    val session by container.session.collectAsStateWithLifecycle()
    val caps = remember { runCatching { describeCaps(container.caps()) }.getOrElse { "Couldn't read: ${it.message}" } }
    val firstFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { firstFocus.requestFocus() } }

    Column(Modifier.fillMaxSize().padding(horizontal = 96.dp, vertical = 64.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Settings", style = MaterialTheme.typography.displaySmall)
        Spacer(Modifier.height(16.dp))
        SettingLine("Server", session?.baseUrl ?: "–")
        SettingLine("Signed in as", session?.username ?: "–")
        SettingLine("This TV can play", caps)
        SettingLine("App version", "${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})")
        Spacer(Modifier.height(24.dp))
        LazyRow(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            items(listOf("Sign out", "Change server")) { label ->
                val m = if (label == "Sign out") Modifier.focusRequester(firstFocus) else Modifier
                OutlinedButton(
                    modifier = m,
                    onClick = {
                        scope.launch {
                            container.signOut()
                            if (label == "Sign out") onSignedOut() else onChangeServer()
                        }
                    },
                ) { Text(label) }
            }
        }
    }
}

@Composable
private fun SettingLine(label: String, value: String) {
    Row {
        Text(label, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(260.dp))
        Text(value, style = MaterialTheme.typography.titleMedium)
    }
}
