package app.kinora.tv.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** The server this app talks to and who is signed in. */
data class Session(
    /** Base URL ending in "/", e.g. "http://192.168.1.20:8420/". */
    val baseUrl: String? = null,
    val token: String? = null,
    val username: String? = null,
) {
    val signedIn get() = baseUrl != null && token != null
}

private val Context.sessionStore by preferencesDataStore(name = "session")

/**
 * Persists the server address and session token. The token is encrypted with
 * a key kept in the Android Keystore, so it can't be read from a backup or a
 * copied data folder.
 */
class SessionStore(private val context: Context) {
    private val baseUrlKey = stringPreferencesKey("base_url")
    private val tokenKey = stringPreferencesKey("token")
    private val usernameKey = stringPreferencesKey("username")

    suspend fun load(): Session {
        val prefs: Preferences = context.sessionStore.data.first()
        return Session(
            baseUrl = prefs[baseUrlKey],
            token = prefs[tokenKey]?.let(TokenCipher::decrypt),
            username = prefs[usernameKey],
        )
    }

    suspend fun save(session: Session) {
        context.sessionStore.edit { prefs ->
            fun set(key: Preferences.Key<String>, value: String?) {
                if (value == null) prefs.remove(key) else prefs[key] = value
            }
            set(baseUrlKey, session.baseUrl)
            set(tokenKey, session.token?.let(TokenCipher::encrypt))
            set(usernameKey, session.username)
        }
    }
}

private object TokenCipher {
    private const val ALIAS = "kinora-session-token"
    private const val TRANSFORM = "AES/GCM/NoPadding"

    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }

    fun encrypt(plain: String): String {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val data = cipher.doFinal(plain.toByteArray())
        return b64(cipher.iv) + ":" + b64(data)
    }

    /** Null if the stored value can't be decrypted (e.g. the key was reset); the user signs in again. */
    fun decrypt(stored: String): String? = runCatching {
        val (iv, data) = stored.split(":", limit = 2)
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, unb64(iv)))
        String(cipher.doFinal(unb64(data)))
    }.getOrNull()

    private fun b64(bytes: ByteArray) = Base64.encodeToString(bytes, Base64.NO_WRAP)
    private fun unb64(s: String) = Base64.decode(s, Base64.NO_WRAP)
}
