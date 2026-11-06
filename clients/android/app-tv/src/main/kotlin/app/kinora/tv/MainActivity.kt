// Media3 marks parts of its API as unstable; this app opts in deliberately.
@file:androidx.annotation.OptIn(markerClass = [androidx.media3.common.util.UnstableApi::class])

package app.kinora.tv

import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavHostController
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import androidx.tv.material3.MaterialTheme
import app.kinora.model.ItemCard
import app.kinora.tv.ui.screens.ConnectScreen
import app.kinora.tv.ui.screens.HomeScreen
import app.kinora.tv.ui.screens.ItemScreen
import app.kinora.tv.ui.screens.LibraryScreen
import app.kinora.tv.ui.screens.PlayerScreen
import app.kinora.tv.ui.screens.SettingsScreen
import app.kinora.tv.ui.screens.SignInScreen
import app.kinora.tv.ui.theme.KinoraTheme
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { KinoraTvRoot() }
    }
}

private object Routes {
    const val CONNECT = "connect"
    const val SIGN_IN = "sign-in"
    const val HOME = "home"
    const val SETTINGS = "settings"
    const val LIBRARY = "library/{id}?name={name}"
    const val ITEM = "item/{id}"
    const val PLAYER = "player/{id}?start={start}"

    fun library(id: String, name: String) = "library/$id?name=${Uri.encode(name)}"
    fun item(id: String) = "item/$id"
    fun player(id: String, startMs: Long) = "player/$id?start=$startMs"
}

@Composable
private fun KinoraTvRoot() {
    val container = androidx.compose.ui.platform.LocalContext.current.container
    val session by container.session.collectAsStateWithLifecycle()
    val theme by container.theme.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { if (container.session.value == null) container.load() }

    KinoraTheme(theme) {
        Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
            val s = session ?: return@Box // still reading from disk
            // Decided once; later changes (signing in or out) navigate explicitly.
            val start = remember {
                when {
                    s.baseUrl == null -> Routes.CONNECT
                    s.token == null -> Routes.SIGN_IN
                    else -> Routes.HOME
                }
            }
            AppNav(rememberNavController(), start)
        }
    }
}

@Composable
private fun AppNav(nav: NavHostController, start: String) {
    val container = androidx.compose.ui.platform.LocalContext.current.container
    val scope = rememberCoroutineScope()

    fun resetTo(route: String) = nav.navigate(route) { popUpTo(0) { inclusive = true } }
    val signOut: () -> Unit = { scope.launch { container.signOut(); resetTo(Routes.SIGN_IN) } }
    val openItem: (ItemCard) -> Unit = { nav.navigate(Routes.item(it.id)) }

    NavHost(navController = nav, startDestination = start) {
        composable(Routes.CONNECT) { ConnectScreen(onConnected = { resetTo(Routes.SIGN_IN) }) }
        composable(Routes.SIGN_IN) {
            SignInScreen(onSignedIn = { resetTo(Routes.HOME) }, onChangeServer = { resetTo(Routes.CONNECT) })
        }
        composable(Routes.HOME) {
            HomeScreen(
                onOpenItem = openItem,
                onOpenLibrary = { id, name -> nav.navigate(Routes.library(id, name)) },
                onOpenSettings = { nav.navigate(Routes.SETTINGS) },
                onSignOut = signOut,
            )
        }
        composable(Routes.SETTINGS) {
            SettingsScreen(onChangeServer = { resetTo(Routes.CONNECT) }, onSignedOut = { resetTo(Routes.SIGN_IN) })
        }
        composable(
            Routes.LIBRARY,
            arguments = listOf(navArgument("id") { type = NavType.StringType }, navArgument("name") { type = NavType.StringType; defaultValue = "" }),
        ) { entry ->
            LibraryScreen(
                libraryId = entry.arguments?.getString("id").orEmpty(),
                name = entry.arguments?.getString("name").orEmpty(),
                onOpenItem = openItem,
                onSignOut = signOut,
            )
        }
        composable(Routes.ITEM, arguments = listOf(navArgument("id") { type = NavType.StringType })) { entry ->
            ItemScreen(
                itemId = entry.arguments?.getString("id").orEmpty(),
                onOpenItem = openItem,
                onPlay = { id, startMs -> nav.navigate(Routes.player(id, startMs)) },
                onSignOut = signOut,
            )
        }
        composable(
            Routes.PLAYER,
            arguments = listOf(navArgument("id") { type = NavType.StringType }, navArgument("start") { type = NavType.LongType; defaultValue = -1L }),
        ) { entry ->
            PlayerScreen(
                itemId = entry.arguments?.getString("id").orEmpty(),
                startMs = entry.arguments?.getLong("start") ?: -1L,
                onExit = { nav.popBackStack() },
            )
        }
    }
}
