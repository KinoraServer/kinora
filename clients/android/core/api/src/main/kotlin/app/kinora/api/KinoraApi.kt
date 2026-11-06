package app.kinora.api

import app.kinora.model.HomeRow
import app.kinora.model.ItemCard
import app.kinora.model.ItemDetail
import app.kinora.model.Library
import app.kinora.model.LoginRequest
import app.kinora.model.LoginResponse
import app.kinora.model.PlaybackRequest
import app.kinora.model.PlaybackStart
import app.kinora.model.PlayedUpdate
import app.kinora.model.ProgressUpdate
import app.kinora.model.StopRequest
import app.kinora.model.SystemInfo
import app.kinora.model.ThemeList
import app.kinora.model.User
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.serializer
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** A non-2xx answer from the server, with its `{"error": "..."}` message. */
class ApiException(val status: Int, message: String) : IOException(message) {
    val isUnauthorized get() = status == 401
}

/**
 * Adds `Authorization: Bearer <token>` to requests for the Kinora server,
 * including images (Coil) and video (Media3), which share this OkHttp client.
 * [token] gets the request URL and returns null for anything that isn't the
 * signed-in server, so the token never leaks to other hosts. It's read on each
 * request, so signing in or out takes effect at once.
 */
class AuthInterceptor(private val token: (HttpUrl) -> String?) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val t = token(chain.request().url) ?: return chain.proceed(chain.request())
        return chain.proceed(chain.request().newBuilder().header("Authorization", "Bearer $t").build())
    }
}

/** True when [url] points at the same server (scheme, host, port) as [base]. */
fun sameServer(url: HttpUrl, base: HttpUrl): Boolean =
    url.scheme == base.scheme && url.host == base.host && url.port == base.port

/**
 * Hand-written typed client for the endpoints the TV app uses. Every path is
 * relative to [baseUrl] (which ends with "/"), matching how the web app works
 * behind reverse proxies with a sub-path.
 */
class KinoraApi(
    val baseUrl: HttpUrl,
    private val client: OkHttpClient,
) {
    constructor(baseUrl: String, client: OkHttpClient) : this(baseUrl.toHttpUrl(), client)

    /** Resolve a relative URL the server returned (e.g. "api/stream/123"). */
    fun resolve(path: String): HttpUrl =
        baseUrl.resolve(path.removePrefix("/")) ?: throw IllegalArgumentException("bad path: $path")

    fun imageUrl(itemId: String, kind: String): String = resolve("api/items/$itemId/images/$kind").toString()

    suspend fun systemInfo(): SystemInfo = get("api/system/info")
    suspend fun themes(): ThemeList = get("api/themes")
    suspend fun login(username: String, password: String, device: String): LoginResponse =
        post("api/auth/login", LoginRequest(username, password, device))
    suspend fun logout() = send<Unit>("POST", "api/auth/logout", "{}")
    suspend fun me(): User = get("api/auth/me")
    suspend fun home(): List<HomeRow> = get("api/home")
    suspend fun libraries(): List<Library> = get("api/libraries")

    suspend fun items(
        libraryId: String? = null,
        parentId: String? = null,
        sort: String? = null,
        query: String? = null,
        limit: Int = 500,
        offset: Int = 0,
    ): List<ItemCard> {
        val params = listOfNotNull(
            libraryId?.let { "library_id" to it },
            parentId?.let { "parent_id" to it },
            sort?.let { "sort" to it },
            query?.let { "q" to it },
            "limit" to limit.toString(),
            "offset" to offset.toString(),
        )
        val url = resolve("api/items").newBuilder().apply { params.forEach { (k, v) -> addQueryParameter(k, v) } }.build()
        return call(Request.Builder().url(url).get().build(), serializer())
    }

    suspend fun item(id: String): ItemDetail = get("api/items/$id")
    suspend fun saveProgress(itemId: String, update: ProgressUpdate) = post<Unit, ProgressUpdate>("api/items/$itemId/progress", update)
    suspend fun setPlayed(itemId: String, played: Boolean) = post<Unit, PlayedUpdate>("api/items/$itemId/played", PlayedUpdate(played))
    suspend fun startPlayback(fileId: String, request: PlaybackRequest): PlaybackStart = post("api/playback/$fileId", request)
    suspend fun stopHls(sessionId: String, stop: StopRequest) =
        send<Unit>("DELETE", "api/hls/$sessionId", json.encodeToString(StopRequest.serializer(), stop))

    // --- plumbing -------------------------------------------------------------

    private suspend inline fun <reified T> get(path: String): T =
        call(Request.Builder().url(resolve(path)).get().build(), serializer())

    private suspend inline fun <reified T, reified B> post(path: String, body: B): T =
        send("POST", path, json.encodeToString(serializer<B>(), body))

    private suspend inline fun <reified T> send(method: String, path: String, body: String): T =
        call(Request.Builder().url(resolve(path)).method(method, body.toRequestBody(JSON)).build(), serializer())

    private suspend fun <T> call(request: Request, deserializer: KSerializer<T>): T = withContext(Dispatchers.IO) {
        client.newCall(request).await().use { response ->
            val text = response.body?.string().orEmpty()
            if (!response.isSuccessful) throw ApiException(response.code, errorMessage(text) ?: "HTTP ${response.code}")
            @Suppress("UNCHECKED_CAST")
            if (deserializer.descriptor == Unit.serializer().descriptor) Unit as T else json.decodeFromString(deserializer, text)
        }
    }

    private fun errorMessage(text: String): String? = runCatching {
        json.parseToJsonElement(text).jsonObject["error"]?.jsonPrimitive?.content
    }.getOrNull()

    companion object {
        private val JSON = "application/json".toMediaType()
        val json = Json { ignoreUnknownKeys = true; explicitNulls = false; encodeDefaults = true }
    }
}

/** OkHttp call as a cancellable suspend function. */
suspend fun Call.await(): Response = suspendCancellableCoroutine { cont ->
    cont.invokeOnCancellation { cancel() }
    enqueue(object : Callback {
        override fun onResponse(call: Call, response: Response) = cont.resume(response)
        override fun onFailure(call: Call, e: IOException) = cont.resumeWithException(e)
    })
}
