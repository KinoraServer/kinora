package app.kinora.model

import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonTransformingSerializer

/**
 * A list of strings that also accepts the list encoded as a JSON string
 * (`"[\"Action\"]"`), as older servers sent for `genres`, or null. Anything
 * else unreadable becomes an empty list rather than failing the whole page.
 */
object LenientStringList : JsonTransformingSerializer<List<String>>(ListSerializer(String.serializer())) {
    override fun transformDeserialize(element: JsonElement): JsonElement = when {
        element is JsonArray -> JsonArray(element.filterIsInstance<JsonPrimitive>().filter { it.isString })
        element is JsonPrimitive && element.isString ->
            runCatching { Json.parseToJsonElement(element.content) }.getOrNull()
                ?.let { if (it is JsonArray) transformDeserialize(it) else null }
                ?: JsonArray(emptyList())
        else -> JsonArray(emptyList())
    }
}
