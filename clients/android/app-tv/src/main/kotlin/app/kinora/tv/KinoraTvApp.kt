package app.kinora.tv

import android.app.Application
import android.content.Context
import android.os.Build
import app.kinora.api.AuthInterceptor
import app.kinora.api.KinoraApi
import app.kinora.api.sameServer
import app.kinora.model.ClientCaps
import app.kinora.model.Theme
import app.kinora.playback.DeviceCaps
import app.kinora.tv.data.Session
import app.kinora.tv.data.SessionStore
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import coil3.network.okhttp.OkHttpNetworkFetcherFactory
import coil3.request.crossfade
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

class KinoraTvApp : Application(), SingletonImageLoader.Factory {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
    }

    override fun newImageLoader(context: PlatformContext): ImageLoader =
        ImageLoader.Builder(context)
            .components { add(OkHttpNetworkFetcherFactory(callFactory = { container.http })) }
            .crossfade(true)
            .build()
}

/**
 * Everything screens need, built by hand (no DI framework). One OkHttp client
 * is shared by API calls, images and video so all of them carry the session
 * token, and only to the signed-in server.
 */
class AppContainer(private val context: Context) {
    private val store = SessionStore(context)

    private val _session = MutableStateFlow<Session?>(null)
    /** Null until loaded from disk. */
    val session: StateFlow<Session?> = _session.asStateFlow()

    private val _theme = MutableStateFlow<Theme?>(null)
    /** The server's default theme, once fetched. Null means the built-in Kinora theme. */
    val theme: StateFlow<Theme?> = _theme.asStateFlow()

    /** For work that must finish even when a screen closes (saving progress, stopping a stream). */
    val appScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    val http: OkHttpClient = OkHttpClient.Builder()
        .addInterceptor(
            AuthInterceptor { url ->
                val s = _session.value ?: return@AuthInterceptor null
                val base = s.baseUrl?.toHttpUrlOrNull() ?: return@AuthInterceptor null
                s.token.takeIf { sameServer(url, base) }
            },
        )
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .build()

    val deviceName: String = "${Build.MANUFACTURER.replaceFirstChar { it.uppercase() }} ${Build.MODEL} (Kinora TV)"

    /** What this TV can play. Re-read on every playback start, since the TV or receiver may have changed. */
    fun caps(): ClientCaps = DeviceCaps.detect(context)

    /** API client for the current server. Only call once a server is set. */
    fun api(): KinoraApi = KinoraApi(requireNotNull(_session.value?.baseUrl) { "no server" }, http)

    /** API client for a server that isn't saved yet (the connect screen). */
    fun apiFor(baseUrl: String): KinoraApi = KinoraApi(baseUrl, http)

    suspend fun load() {
        _session.value = store.load()
    }

    suspend fun update(session: Session) {
        store.save(session)
        _session.value = session
    }

    suspend fun refreshTheme() {
        val list = runCatching { api().themes() }.getOrNull() ?: return
        _theme.value = list.themes.firstOrNull { it.id == list.defaultTheme }
    }

    suspend fun signOut() {
        runCatching { api().logout() }
        update((_session.value ?: Session()).copy(token = null))
    }
}

val Context.container: AppContainer get() = (applicationContext as KinoraTvApp).container
