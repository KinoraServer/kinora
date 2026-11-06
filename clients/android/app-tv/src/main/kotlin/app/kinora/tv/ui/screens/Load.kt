package app.kinora.tv.ui.screens

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import app.kinora.api.ApiException
import app.kinora.tv.ui.components.CenteredMessage
import kotlinx.coroutines.CancellationException

/** Loading state for a screen's data. */
sealed interface Load<out T> {
    data object Loading : Load<Nothing>
    data class Failed(val message: String, val unauthorized: Boolean) : Load<Nothing>
    data class Ready<T>(val value: T) : Load<T>
}

class Loader<T>(val state: Load<T>, val reload: () -> Unit)

/** Runs [block] when the screen opens (and again on [Loader.reload]). */
@Composable
fun <T> rememberLoader(vararg keys: Any?, block: suspend () -> T): Loader<T> {
    var attempt by remember { mutableIntStateOf(0) }
    var state by remember(*keys) { mutableStateOf<Load<T>>(Load.Loading) }
    LaunchedEffect(attempt, *keys) {
        state = try {
            Load.Ready(block())
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            Load.Failed(e.message ?: "Server error", e.isUnauthorized)
        } catch (e: Exception) {
            Load.Failed("Couldn't reach the server (${e.message ?: e.javaClass.simpleName}).", false)
        }
    }
    return Loader(state) { attempt++ }
}

/** Shows a spinner-ish message, or an error with Retry / Sign in again, or the content. */
@Composable
fun <T> LoadContent(loader: Loader<T>, onSignOut: () -> Unit, content: @Composable (T) -> Unit) {
    when (val s = loader.state) {
        Load.Loading -> CenteredMessage("Loading…")
        is Load.Failed ->
            if (s.unauthorized) {
                CenteredMessage("Your session has ended. Please sign in again.", action = "Sign in" to onSignOut)
            } else {
                CenteredMessage(s.message, action = "Retry" to loader.reload)
            }
        is Load.Ready -> content(s.value)
    }
}
