package io.lavi

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class KotlinMachineTest {
    private val machineJson = """
        {
          "id": "kotlinDemo",
          "initial": "idle",
          "context": {
            "retries": 2,
            "temperature": 21.5,
            "name": "rover",
            "armed": true,
            "tags": ["alpha", "beta"]
          },
          "states": {
            "idle": {
              "on": {
                "LAUNCH": {
                  "target": "running",
                  "guard": "canLaunch",
                  "actions": ["logLaunch"]
                }
              }
            },
            "running": {
              "after": { "250": "done" }
            },
            "done": { "type": "final" }
          }
        }
    """.trimIndent()

    @Test
    fun kotlinTrailingLambdasOperatorIndexingAndMapPayloadsWork() {
        var loggedBoost: Long? = null
        SimulatedClock(0).use { clock ->
            Machine(machineJson, clock).use { machine ->
                machine.onGuard("canLaunch") { ctx, evt ->
                    assertEquals(2L, ctx["retries"].asLongOrNull())
                    assertEquals(21.5, ctx["temperature"].asDoubleOrNull())
                    assertEquals("rover", ctx["name"].asStringOrNull())
                    assertEquals(true, ctx["armed"].asBooleanOrNull())
                    assertEquals("beta", ctx["tags"][1].asStringOrNull())
                    assertNull(ctx["missing"]["deep"].asStringOrNull())
                    assertTrue("tags" in ctx)
                    (evt["boost"].asLongOrNull() ?: 0L) > 5L
                }
                machine.onAction("logLaunch") { _, evt ->
                    loggedBoost = evt["boost"].asLongOrNull()
                }

                machine.start()
                assertFalse(machine.can("LAUNCH", mapOf("boost" to 3)))
                assertTrue(machine.can("LAUNCH", mapOf("boost" to 10)))

                machine.send("LAUNCH", mapOf("boost" to 10, "nested" to mapOf("ok" to true), "list" to listOf(1, 2)))
                assertTrue(machine.matches("running"))
                assertEquals(10L, loggedBoost)

                clock.increment(250)
                assertTrue(machine.isDone)
            }
        }
    }

    @Test
    fun negativeAndOutOfBoundsIndexingOnValueReturnsNullValue() {
        val json = """{"items": ["first", "second"]}"""
        val value = Value.fromJson(json)
        val items = value["items"]
        assertEquals("first", items[0].asStringOrNull())
        assertEquals("second", items[1].asStringOrNull())
        assertTrue(items[-1].isNull)
        assertNull(items[-1].asStringOrNull())
        assertTrue(items[2].isNull)
        assertNull(items[2].asStringOrNull())
        assertTrue(items[999].isNull)
        assertNull(items[999].asStringOrNull())
        assertFalse("items" in items)
        assertTrue("items" in value)
        assertFalse("missing" in value)
    }

    @Test
    fun onClockTrailingLambdaWorks() {
        val timerJson = """
            {
              "initial": "a",
              "states": {
                "a": { "after": { "50": "b" } },
                "b": { "type": "final" }
              }
            }
        """.trimIndent()
        var customNow = 1000L
        Machine(timerJson).use { machine ->
            machine.onClock { customNow }
            machine.start()
            assertFalse(machine.isDone)
            customNow = 1050L
            machine.tick()
            assertTrue(machine.isDone)
        }
    }

    @Test
    fun deepIterativeMapPayloadSerializationNoStackOverflow() {
        var current: Map<String, Any?> = mapOf("depth" to 0, "final" to 42)
        for (i in 1..500) {
            current = mapOf("child" to current)
        }

        var observedFinal: Long? = null
        val testJson = """
            {
              "initial": "idle",
              "states": {
                "idle": {
                  "on": {
                    "NESTED": {
                      "target": "done",
                      "actions": ["record"]
                    }
                  }
                },
                "done": { "type": "final" }
              }
            }
        """.trimIndent()

        Machine(testJson).use { machine ->
            machine.onAction("record") { _, evt ->
                var node = evt
                for (i in 1..500) {
                    node = node["child"]
                }
                observedFinal = node["final"].asLongOrNull()
            }
            machine.start()
            assertTrue(machine.can("NESTED", current))
            machine.send("NESTED", current)
            assertTrue(machine.isDone)
            assertEquals(42L, observedFinal)
        }
    }

    @Test
    fun nanAndInfinitePayloadRejection() {
        val testJson = """
            {
              "initial": "idle",
              "states": {
                "idle": { "on": { "EVT": "done" } },
                "done": {}
              }
            }
        """.trimIndent()

        Machine(testJson).use { machine ->
            machine.start()
            assertFailsWith<IllegalArgumentException> {
                machine.can("EVT", mapOf("val" to Double.NaN))
            }
            assertFailsWith<IllegalArgumentException> {
                machine.send("EVT", mapOf("val" to Double.POSITIVE_INFINITY))
            }
            assertFailsWith<IllegalArgumentException> {
                machine.can("EVT", mapOf("val" to Double.NEGATIVE_INFINITY))
            }
            assertFailsWith<IllegalArgumentException> {
                machine.send("EVT", mapOf("val" to Float.NaN))
            }
            assertFailsWith<IllegalArgumentException> {
                machine.can("EVT", mapOf("val" to Float.POSITIVE_INFINITY))
            }
        }
    }

    @Test
    fun unsupportedPayloadTypeThrowsIllegalArgumentException() {
        class CustomType

        val testJson = """
            {
              "initial": "idle",
              "states": {
                "idle": { "on": { "EVT": "done" } },
                "done": {}
              }
            }
        """.trimIndent()

        Machine(testJson).use { machine ->
            machine.start()
            assertFailsWith<IllegalArgumentException> {
                machine.send("EVT", mapOf("bad" to CustomType()))
            }
        }
    }

    @Test
    fun kotlinContextInspectionAndParameterizedCallbacks() {
        val testJson = """
            {
              "id": "ktParam",
              "initial": "idle",
              "context": { "score": 95, "user": "alice" },
              "states": {
                "idle": {
                  "on": {
                    "SUBMIT": {
                      "target": "verified",
                      "guard": { "type": "minScore", "params": { "threshold": 80 } },
                      "actions": [{ "type": "onVerified", "params": { "award": "gold" } }]
                    }
                  }
                },
                "verified": { "type": "final" }
              }
            }
        """.trimIndent()

        var observedAward: String? = null
        var guardExecuted = false

        Machine(testJson).use { machine ->
            assertTrue(machine.context.isNull)

            machine.onGuardWithParams("minScore") { ctx, _, params ->
                guardExecuted = true
                val threshold = params["threshold"].asLongOrNull() ?: 0L
                val score = ctx["score"].asLongOrNull() ?: 0L
                score >= threshold
            }

            machine.onActionWithParams("onVerified") { _, _, params ->
                observedAward = params["award"].asStringOrNull()
            }

            machine.start()
            assertFalse(machine.context.isNull)
            assertEquals(95L, machine.context["score"].asLongOrNull())
            assertEquals("alice", machine.context["user"].asStringOrNull())

            assertTrue(machine.can("SUBMIT"))
            machine.send("SUBMIT")
            assertTrue(machine.matches("verified"))
            assertTrue(guardExecuted)
            assertEquals("gold", observedAward)
        }
    }
}
