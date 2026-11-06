package app.kinora.playback

import android.content.Context
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.okhttp.OkHttpDataSource
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import okhttp3.OkHttpClient

/**
 * Builds the ExoPlayer used for every video. It shares the app's OkHttp
 * client, so streams, HLS segments and subtitles carry the same bearer token
 * as API calls.
 */
@UnstableApi
object PlayerFactory {
    fun create(context: Context, http: OkHttpClient): ExoPlayer {
        val dataSource = OkHttpDataSource.Factory(http).setUserAgent("Kinora-Android")
        val renderers = DefaultRenderersFactory(context)
            // If the device has no decoder for the preferred one (e.g. Dolby Vision
            // profile 8 on a phone), fall back to the HEVC base layer instead of failing.
            .setEnableDecoderFallback(true)
            .setExtensionRendererMode(DefaultRenderersFactory.EXTENSION_RENDERER_MODE_OFF)
        return ExoPlayer.Builder(context, renderers)
            .setMediaSourceFactory(DefaultMediaSourceFactory(dataSource))
            .setAudioAttributes(
                AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(),
                /* handleAudioFocus = */ true,
            )
            .setHandleAudioBecomingNoisy(true)
            .setSeekBackIncrementMs(10_000)
            .setSeekForwardIncrementMs(10_000)
            .build()
    }
}
