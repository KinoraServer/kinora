// Media3 marks parts of its API as unstable; this app opts in deliberately.
@file:androidx.annotation.OptIn(markerClass = [androidx.media3.common.util.UnstableApi::class])

package app.kinora.tv.ui.screens

import android.app.Application
import android.view.KeyEvent as AndroidKeyEvent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.nativeKeyCode
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.media3.ui.PlayerView
import androidx.tv.material3.Button
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.OutlinedButton
import androidx.tv.material3.Text
import app.kinora.model.formatClock
import kotlinx.coroutines.delay

private enum class Menu { None, Audio, Subtitles, Info }

/**
 * Full-screen player for a TV remote.
 *
 * Controls hidden: OK plays/pauses, left/right seek 10 s, up/down show the controls.
 * Controls shown: the D-pad moves between buttons; on the seek bar left/right seek.
 * Back closes a menu, then the controls, then the player. Media keys always work.
 */
@Composable
fun PlayerScreen(itemId: String, startMs: Long, onExit: () -> Unit) {
    val app = LocalContext.current.applicationContext as Application
    val vm: PlayerViewModel = viewModel(key = "player-$itemId-$startMs") { PlayerViewModel(app, itemId, startMs) }
    val state by vm.state.collectAsStateWithLifecycle()

    var controlsVisible by remember { mutableStateOf(true) }
    var menu by remember { mutableStateOf(Menu.None) }
    var lastInput by remember { mutableLongStateOf(System.currentTimeMillis()) }
    val rootFocus = remember { FocusRequester() }
    val playFocus = remember { FocusRequester() }
    val menuFocus = remember { FocusRequester() }

    fun poke() {
        lastInput = System.currentTimeMillis()
    }

    fun show() {
        controlsVisible = true
        poke()
    }

    // Hide the controls after 5 s without input while playing.
    LaunchedEffect(controlsVisible, menu, state.playing, lastInput) {
        if (controlsVisible && menu == Menu.None && state.playing) {
            delay(5_000)
            controlsVisible = false
        }
    }
    LaunchedEffect(controlsVisible, menu) {
        runCatching {
            when {
                menu != Menu.None -> menuFocus.requestFocus()
                controlsVisible -> playFocus.requestFocus()
                else -> rootFocus.requestFocus()
            }
        }
    }

    BackHandler {
        when {
            menu != Menu.None -> menu = Menu.None
            controlsVisible && state.error == null -> controlsVisible = false
            else -> onExit()
        }
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black)
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                poke()
                when (event.key.nativeKeyCode) {
                    AndroidKeyEvent.KEYCODE_MEDIA_PLAY_PAUSE -> { vm.togglePlay(); show(); true }
                    AndroidKeyEvent.KEYCODE_MEDIA_PLAY -> { vm.player.play(); true }
                    AndroidKeyEvent.KEYCODE_MEDIA_PAUSE -> { vm.player.pause(); show(); true }
                    AndroidKeyEvent.KEYCODE_MEDIA_FAST_FORWARD -> { vm.seekBy(30_000); show(); true }
                    AndroidKeyEvent.KEYCODE_MEDIA_REWIND -> { vm.seekBy(-30_000); show(); true }
                    else -> false
                }
            },
    ) {
        AndroidView(
            factory = { context ->
                PlayerView(context).apply {
                    useController = false
                    setShowBuffering(PlayerView.SHOW_BUFFERING_WHEN_PLAYING)
                    keepScreenOn = true
                    player = vm.player
                }
            },
            modifier = Modifier.fillMaxSize(),
        )

        // Invisible target that owns the remote while the controls are hidden.
        Box(
            Modifier
                .fillMaxSize()
                .focusRequester(rootFocus)
                .onKeyEvent { event ->
                    if (controlsVisible || event.type != KeyEventType.KeyDown) return@onKeyEvent false
                    when (event.key.nativeKeyCode) {
                        AndroidKeyEvent.KEYCODE_DPAD_CENTER, AndroidKeyEvent.KEYCODE_ENTER -> { vm.togglePlay(); show(); true }
                        AndroidKeyEvent.KEYCODE_DPAD_LEFT -> { vm.seekBy(-10_000); show(); true }
                        AndroidKeyEvent.KEYCODE_DPAD_RIGHT -> { vm.seekBy(10_000); show(); true }
                        AndroidKeyEvent.KEYCODE_DPAD_UP, AndroidKeyEvent.KEYCODE_DPAD_DOWN -> { show(); true }
                        else -> false
                    }
                }
                .focusable(),
        )

        if (state.loading && state.error == null) {
            Text("Loading…", modifier = Modifier.align(Alignment.Center), style = MaterialTheme.typography.titleLarge, color = Color.White)
        }

        state.error?.let { error ->
            ErrorPanel(error, onTranscode = vm::retryWithTranscode, onExit = onExit, focus = playFocus)
            return@Box
        }

        if (controlsVisible) {
            Controls(
                state = state,
                playFocus = playFocus,
                onTogglePlay = { vm.togglePlay(); poke() },
                onSeek = { vm.seekBy(it); poke() },
                onMenu = { menu = it },
            )
        }

        when (menu) {
            Menu.Audio -> OptionsPanel("Audio", state.audioOptions, menuFocus) { vm.selectAudio(it); menu = Menu.None }
            Menu.Subtitles -> OptionsPanel("Subtitles", state.subtitleOptions, menuFocus) { vm.selectSubtitle(it); menu = Menu.None }
            Menu.Info -> InfoPanel(state, menuFocus)
            Menu.None -> {}
        }
    }
}

@Composable
private fun Controls(
    state: PlayerUiState,
    playFocus: FocusRequester,
    onTogglePlay: () -> Unit,
    onSeek: (Long) -> Unit,
    onMenu: (Menu) -> Unit,
) {
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxWidth().height(320.dp).align(Alignment.BottomCenter)
                .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(alpha = 0.85f)))),
        )
        Column(Modifier.align(Alignment.BottomStart).fillMaxWidth().padding(horizontal = 48.dp, vertical = 32.dp)) {
            Text(state.title, style = MaterialTheme.typography.headlineMedium, color = Color.White)
            state.subtitle?.let { Text(it, style = MaterialTheme.typography.titleMedium, color = Color.White.copy(alpha = 0.8f)) }
            Spacer(Modifier.height(16.dp))
            SeekBar(state, onSeek)
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Button(onClick = onTogglePlay, modifier = Modifier.focusRequester(playFocus)) {
                    Text(if (state.playing) "Pause" else "Play")
                }
                OutlinedButton(onClick = { onSeek(-10_000) }) { Text("−10s") }
                OutlinedButton(onClick = { onSeek(10_000) }) { Text("+10s") }
                OutlinedButton(onClick = { onMenu(Menu.Audio) }, enabled = state.audioOptions.size > 1) { Text("Audio") }
                OutlinedButton(onClick = { onMenu(Menu.Subtitles) }, enabled = state.subtitleOptions.size > 1) { Text("Subtitles") }
                OutlinedButton(onClick = { onMenu(Menu.Info) }) { Text("Info") }
                Spacer(Modifier.width(12.dp))
                Text(state.method, style = MaterialTheme.typography.labelLarge, color = Color.White.copy(alpha = 0.7f))
            }
        }
    }
}

/** Focusable seek bar: left/right jump 10 s while it's focused. */
@Composable
private fun SeekBar(state: PlayerUiState, onSeek: (Long) -> Unit) {
    var focused by remember { mutableStateOf(false) }
    val duration = state.durationMs.coerceAtLeast(1)
    Column {
        Box(
            Modifier
                .fillMaxWidth()
                .height(if (focused) 10.dp else 6.dp)
                .onFocusChanged { focused = it.isFocused }
                .onKeyEvent { e ->
                    if (e.type != KeyEventType.KeyDown) return@onKeyEvent false
                    when (e.key.nativeKeyCode) {
                        AndroidKeyEvent.KEYCODE_DPAD_LEFT -> { onSeek(-10_000); true }
                        AndroidKeyEvent.KEYCODE_DPAD_RIGHT -> { onSeek(10_000); true }
                        else -> false
                    }
                }
                .focusable()
                .background(Color.White.copy(alpha = 0.25f))
                .then(if (focused) Modifier.border(1.dp, MaterialTheme.colorScheme.primary) else Modifier),
        ) {
            Box(Modifier.fillMaxHeight().fillMaxWidth((state.bufferedMs.toFloat() / duration).coerceIn(0f, 1f)).background(Color.White.copy(alpha = 0.35f)))
            Box(Modifier.fillMaxHeight().fillMaxWidth((state.positionMs.toFloat() / duration).coerceIn(0f, 1f)).background(MaterialTheme.colorScheme.primary))
        }
        Spacer(Modifier.height(6.dp))
        Row {
            Text(formatClock(state.positionMs), color = Color.White, style = MaterialTheme.typography.labelLarge)
            Spacer(Modifier.weight(1f))
            Text("−" + formatClock(state.durationMs - state.positionMs), color = Color.White, style = MaterialTheme.typography.labelLarge)
        }
    }
}

@Composable
private fun SidePanel(title: String, content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.CenterEnd) {
        Column(
            Modifier.fillMaxHeight().width(480.dp).background(Color.Black.copy(alpha = 0.88f)).padding(32.dp),
        ) {
            Text(title, style = MaterialTheme.typography.headlineSmall, color = Color.White)
            Spacer(Modifier.height(16.dp))
            content()
        }
    }
}

@Composable
private fun OptionsPanel(title: String, options: List<TrackOption>, focus: FocusRequester, onPick: (String) -> Unit) {
    SidePanel(title) {
        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp), contentPadding = PaddingValues(vertical = 4.dp)) {
            val selectedIndex = options.indexOfFirst { it.selected }.coerceAtLeast(0)
            itemsIndexed(options, key = { _, o -> o.key }) { index, option ->
                val m = Modifier.fillMaxWidth().then(if (index == selectedIndex) Modifier.focusRequester(focus) else Modifier)
                if (option.selected) {
                    Button(onClick = { onPick(option.key) }, modifier = m) { Text("✓  ${option.label}") }
                } else {
                    OutlinedButton(onClick = { onPick(option.key) }, modifier = m) { Text(option.label) }
                }
            }
        }
    }
}

/** What's actually playing: handy for checking Dolby Vision, HDR and passthrough on a real TV. */
@Composable
private fun InfoPanel(state: PlayerUiState, focus: FocusRequester) {
    SidePanel("Playback info") {
        InfoLine("Method", state.method)
        InfoLine("Why", state.reasons.joinToString("\n"))
        InfoLine("Video", state.videoInfo)
        InfoLine("Audio", state.audioInfo)
        // Something focusable so Back and the D-pad behave inside the panel.
        Box(Modifier.focusRequester(focus).focusable())
    }
}

@Composable
private fun InfoLine(label: String, value: String) {
    if (value.isBlank()) return
    Text(label, style = MaterialTheme.typography.labelLarge, color = Color.White.copy(alpha = 0.6f))
    Text(value, style = MaterialTheme.typography.bodyLarge, color = Color.White)
    Spacer(Modifier.height(12.dp))
}

@Composable
private fun ErrorPanel(error: String, onTranscode: () -> Unit, onExit: () -> Unit, focus: FocusRequester) {
    LaunchedEffect(error) { runCatching { focus.requestFocus() } }
    Column(
        Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.85f)).padding(96.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("This video couldn't play", style = MaterialTheme.typography.headlineMedium, color = Color.White)
        Spacer(Modifier.height(12.dp))
        Text(error, style = MaterialTheme.typography.bodyLarge, color = Color.White.copy(alpha = 0.8f))
        Spacer(Modifier.height(24.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Button(onClick = onTranscode, modifier = Modifier.focusRequester(focus)) { Text("Try again with transcoding") }
            OutlinedButton(onClick = onExit) { Text("Back") }
        }
    }
}
