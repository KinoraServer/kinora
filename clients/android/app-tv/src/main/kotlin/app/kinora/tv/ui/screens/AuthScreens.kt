package app.kinora.tv.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.OutlinedButton
import androidx.tv.material3.Text
import app.kinora.api.ApiException
import app.kinora.model.isPlainHttp
import app.kinora.model.normalizeServerAddress
import app.kinora.tv.container
import app.kinora.tv.data.Session
import app.kinora.tv.ui.components.TvTextField
import kotlinx.coroutines.launch
import java.io.IOException

/** Step 1: which server? */
@Composable
fun ConnectScreen(onConnected: () -> Unit) {
    val container = LocalContext.current.container
    val scope = rememberCoroutineScope()
    var address by rememberSaveable { mutableStateOf(container.session.value?.baseUrl?.removeSuffix("/") ?: "") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val field = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { field.requestFocus() } }

    fun connect() {
        val base = normalizeServerAddress(address)
        if (base == null) {
            error = "That doesn't look like a server address."
            return
        }
        busy = true
        error = null
        scope.launch {
            try {
                val info = container.apiFor(base).systemInfo()
                if (info.setupRequired) {
                    error = "${info.serverName} isn't set up yet. Finish setup in the web app first."
                } else {
                    container.update(Session(baseUrl = base))
                    onConnected()
                }
            } catch (e: IOException) {
                error = "Couldn't reach $base (${e.message ?: e.javaClass.simpleName}). Check the address and that the server is running."
            } catch (e: Exception) {
                error = "That address answered, but it doesn't look like a Kinora server."
            } finally {
                busy = false
            }
        }
    }

    AuthLayout(title = "Connect to your Kinora server") {
        TvTextField(
            value = address,
            onValueChange = { address = it },
            label = "Server address, e.g. 192.168.1.20:8420 or https://media.example.com",
            keyboardType = KeyboardType.Uri,
            imeAction = ImeAction.Go,
            onImeAction = ::connect,
            focusRequester = field,
        )
        Spacer(Modifier.height(24.dp))
        Button(onClick = ::connect, enabled = !busy) { Text(if (busy) "Connecting…" else "Connect") }
        ErrorText(error)
    }
}

/** Step 2: who are you? */
@Composable
fun SignInScreen(onSignedIn: () -> Unit, onChangeServer: () -> Unit) {
    val container = LocalContext.current.container
    val scope = rememberCoroutineScope()
    val current by container.session.collectAsStateWithLifecycle()
    val session = current ?: Session()
    var username by rememberSaveable { mutableStateOf(session.username ?: "") }
    var password by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var serverName by remember { mutableStateOf<String?>(null) }
    val userField = remember { FocusRequester() }
    val passwordField = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        runCatching { (if (username.isEmpty()) userField else passwordField).requestFocus() }
        serverName = runCatching { container.api().systemInfo().serverName }.getOrNull()
    }

    fun signIn() {
        if (username.isBlank() || password.isEmpty()) {
            error = "Enter your username and password."
            return
        }
        busy = true
        error = null
        scope.launch {
            try {
                val res = container.api().login(username.trim(), password, container.deviceName)
                container.update(session.copy(token = res.token, username = res.user.username))
                onSignedIn()
            } catch (e: ApiException) {
                error = e.message
            } catch (e: IOException) {
                error = "Couldn't reach the server (${e.message ?: e.javaClass.simpleName})."
            } finally {
                busy = false
            }
        }
    }

    AuthLayout(title = "Sign in to ${serverName ?: "Kinora"}") {
        Text(
            session.baseUrl.orEmpty() + if (isPlainHttp(session.baseUrl.orEmpty())) "  ·  not encrypted (fine on your home network or Tailscale)" else "",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(24.dp))
        TvTextField(
            value = username,
            onValueChange = { username = it },
            label = "Username",
            focusRequester = userField,
            onImeAction = { runCatching { passwordField.requestFocus() } },
        )
        Spacer(Modifier.height(16.dp))
        TvTextField(
            value = password,
            onValueChange = { password = it },
            label = "Password",
            password = true,
            imeAction = ImeAction.Done,
            onImeAction = ::signIn,
            focusRequester = passwordField,
        )
        Spacer(Modifier.height(24.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Button(onClick = ::signIn, enabled = !busy) { Text(if (busy) "Signing in…" else "Sign in") }
            OutlinedButton(onClick = onChangeServer) { Text("Change server") }
        }
        ErrorText(error)
    }
}

@Composable
private fun AuthLayout(title: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize().padding(horizontal = 96.dp, vertical = 64.dp)) {
        Text("kinora", style = MaterialTheme.typography.headlineSmall, color = MaterialTheme.colorScheme.primary)
        Spacer(Modifier.height(32.dp))
        Text(title, style = MaterialTheme.typography.displaySmall)
        Spacer(Modifier.height(32.dp))
        content()
    }
}

@Composable
private fun ErrorText(error: String?) {
    if (error != null) {
        Spacer(Modifier.height(20.dp))
        Text(error, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.width(720.dp))
    }
}
