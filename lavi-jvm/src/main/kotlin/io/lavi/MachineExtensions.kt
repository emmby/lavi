package io.lavi

import java.util.ArrayDeque

/**
 * Idiomatic Kotlin extensions for [Machine] and [Value].
 */

fun Machine.onAction(name: String, block: (context: Value, event: Value) -> Unit): Machine =
    registerAction(name, ActionCallback { ctx, evt -> block(ctx, evt) })

fun Machine.onActionWithParams(name: String, block: (context: Value, event: Value, params: Value) -> Unit): Machine =
    registerAction(name, ParameterizedActionCallback { ctx, evt, params -> block(ctx, evt, params) })

fun Machine.onGuard(name: String, block: (context: Value, event: Value) -> Boolean): Machine =
    registerGuard(name, GuardCallback { ctx, evt -> block(ctx, evt) })

fun Machine.onGuardWithParams(name: String, block: (context: Value, event: Value, params: Value) -> Boolean): Machine =
    registerGuard(name, ParameterizedGuardCallback { ctx, evt, params -> block(ctx, evt, params) })

fun Machine.onClock(block: () -> Long): Machine =
    registerClock(ClockCallback { block() })

fun Machine.onInspect(block: (event: Value) -> Unit): Machine =
    registerInspector(InspectCallback { evt -> block(evt) })

fun Machine.onOutputResolver(statePath: String = "", block: (context: Value, event: Value) -> Value): Machine =
    registerOutputResolver(statePath, OutputResolverCallback { ctx, evt -> block(ctx, evt) })

fun Machine.onInvoke(
    onStart: ((id: String, src: Value, input: Value, context: Value, event: Value) -> Unit)? = null,
    onStop: ((id: String) -> Unit)? = null
): Machine =
    registerInvokeCallbacks(
        onStart?.let { cb -> InvokeStartCallback { id, src, inp, ctx, evt -> cb(id, src, inp, ctx, evt) } },
        onStop?.let { cb -> InvokeStopCallback { id -> cb(id) } }
    )

fun Machine.send(event: String, payload: Map<String, Any?>) {
    send(event, serializePayload(payload))
}

fun Machine.can(event: String, payload: Map<String, Any?>): Boolean =
    can(event, serializePayload(payload))

operator fun Value.get(key: String): Value = getProperty(key)

operator fun Value.get(index: Int): Value = getElement(index)

operator fun Value.contains(key: String): Boolean = hasProperty(key)

fun Value.asStringOrNull(): String? = asString().orElse(null)

fun Value.asDoubleOrNull(): Double? = asNumber().orElse(null)

fun Value.asLongOrNull(): Long? = asLong().orElse(null)

fun Value.asBooleanOrNull(): Boolean? = asBoolean().orElse(null)

private sealed class SerializeFrame {
    class ObjectFrame(val iterator: Iterator<Map.Entry<*, *>>) : SerializeFrame() {
        var isFirst: Boolean = true
    }

    class ArrayFrame(val iterator: Iterator<*>) : SerializeFrame() {
        var isFirst: Boolean = true
    }
}

internal fun serializePayload(payload: Map<String, Any?>): String {
    val sb = StringBuilder()
    sb.append('{')
    val stack = ArrayDeque<SerializeFrame>()
    stack.push(SerializeFrame.ObjectFrame(payload.entries.iterator()))

    while (!stack.isEmpty()) {
        val top = stack.peek()!!
        when (top) {
            is SerializeFrame.ObjectFrame -> {
                if (top.iterator.hasNext()) {
                    if (!top.isFirst) {
                        sb.append(',')
                    } else {
                        top.isFirst = false
                    }
                    val entry = top.iterator.next()
                    val key = entry.key as? String
                        ?: throw IllegalArgumentException("Map key must be String, got: ${entry.key}")
                    escapeJsonString(key, sb)
                    sb.append(':')
                    processValue(entry.value, sb, stack)
                } else {
                    sb.append('}')
                    stack.pop()
                }
            }
            is SerializeFrame.ArrayFrame -> {
                if (top.iterator.hasNext()) {
                    if (!top.isFirst) {
                        sb.append(',')
                    } else {
                        top.isFirst = false
                    }
                    val item = top.iterator.next()
                    processValue(item, sb, stack)
                } else {
                    sb.append(']')
                    stack.pop()
                }
            }
        }
    }
    return sb.toString()
}

private fun processValue(item: Any?, sb: StringBuilder, stack: ArrayDeque<SerializeFrame>) {
    when (item) {
        null -> sb.append("null")
        is Boolean -> sb.append(item.toString())
        is Double -> {
            if (item.isNaN() || item.isInfinite()) {
                throw IllegalArgumentException("Non-finite Double: $item")
            }
            if (item == Math.floor(item) && item >= Long.MIN_VALUE.toDouble() && item <= Long.MAX_VALUE.toDouble()) {
                sb.append(item.toLong())
            } else {
                sb.append(item.toString())
            }
        }
        is Float -> {
            if (item.isNaN() || item.isInfinite()) {
                throw IllegalArgumentException("Non-finite Float: $item")
            }
            val d = item.toDouble()
            if (d == Math.floor(d) && d >= Long.MIN_VALUE.toDouble() && d <= Long.MAX_VALUE.toDouble()) {
                sb.append(item.toLong())
            } else {
                sb.append(item.toString())
            }
        }
        is Byte, is Short, is Int, is Long -> sb.append(item.toString())
        is Number -> {
            val d = item.toDouble()
            if (d.isNaN() || d.isInfinite()) {
                throw IllegalArgumentException("Non-finite Number: $item")
            }
            sb.append(item.toString())
        }
        is CharSequence -> escapeJsonString(item.toString(), sb)
        is Char -> escapeJsonString(item.toString(), sb)
        is Value -> item.toJson(sb)
        is Map<*, *> -> {
            sb.append('{')
            stack.push(SerializeFrame.ObjectFrame(item.entries.iterator()))
        }
        else -> {
            val iter = toIterator(item)
            if (iter != null) {
                sb.append('[')
                stack.push(SerializeFrame.ArrayFrame(iter))
            } else {
                throw IllegalArgumentException("Unsupported JSON payload value type: ${item::class.java.name}")
            }
        }
    }
}

private fun toIterator(item: Any): Iterator<*>? {
    return when (item) {
        is Iterable<*> -> item.iterator()
        is Array<*> -> item.iterator()
        is BooleanArray -> item.iterator()
        is ByteArray -> item.iterator()
        is ShortArray -> item.iterator()
        is IntArray -> item.iterator()
        is LongArray -> item.iterator()
        is FloatArray -> item.iterator()
        is DoubleArray -> item.iterator()
        is CharArray -> item.iterator()
        else -> null
    }
}

private fun escapeJsonString(s: String, sb: StringBuilder) {
    sb.append('"')
    for (i in 0 until s.length) {
        val c = s[i]
        when (c) {
            '"' -> sb.append("\\\"")
            '\\' -> sb.append("\\\\")
            '\b' -> sb.append("\\b")
            '\u000C' -> sb.append("\\f")
            '\n' -> sb.append("\\n")
            '\r' -> sb.append("\\r")
            '\t' -> sb.append("\\t")
            else -> {
                if (c < ' ') {
                    sb.append(String.format("\\u%04x", c.code))
                } else {
                    sb.append(c)
                }
            }
        }
    }
    sb.append('"')
}
