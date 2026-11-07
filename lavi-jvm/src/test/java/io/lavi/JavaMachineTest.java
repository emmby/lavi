package io.lavi;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.jupiter.api.Assertions.*;

class JavaMachineTest {
    private static final String MACHINE_JSON = "{\n" +
        "  \"id\": \"traffic\",\n" +
        "  \"initial\": \"green\",\n" +
        "  \"context\": { \"cycles\": 2, \"ratio\": 1.5, \"label\": \"main\", \"ok\": true, \"items\": [\"x\", \"y\"] },\n" +
        "  \"states\": {\n" +
        "    \"green\": {\n" +
        "      \"tags\": [\"go\"],\n" +
        "      \"after\": { \"500\": \"yellow\" },\n" +
        "      \"on\": {\n" +
        "        \"TIMER\": { \"target\": \"yellow\", \"guard\": \"canAdvance\", \"actions\": [\"onAdvance\"] }\n" +
        "      }\n" +
        "    },\n" +
        "    \"yellow\": { \"on\": { \"FINISH\": \"red\" } },\n" +
        "    \"red\": { \"type\": \"final\" }\n" +
        "  }\n" +
        "}";

    @Test
    void invalidJsonThrowsIllegalArgumentException() {
        assertThrows(IllegalArgumentException.class, () -> new Machine("{bad json"));
    }

    @Test
    void unstartedMachineDropsEventsAndMatchesNothing() {
        try (Machine m = new Machine(MACHINE_JSON)) {
            assertFalse(m.matches("green"));
            assertFalse(m.can("TIMER"));
            assertFalse(m.hasTag("go"));
            assertFalse(m.isDone());
            m.send("TIMER");
            m.start();
            assertTrue(m.matches("green"));
        }
    }

    @Test
    void callbacksValueInspectionReentrancyAndRetainedValueSafety() {
        AtomicReference<Value> savedCtx = new AtomicReference<>();
        AtomicReference<Value> savedEvt = new AtomicReference<>();
        try (SimulatedClock clock = new SimulatedClock(100);
             Machine m = new Machine(MACHINE_JSON, clock)) {
            m.registerGuard("canAdvance", (ctx, evt) -> {
                assertEquals(2L, ctx.getProperty("cycles").asLong().orElse(-1L));
                assertEquals(1.5, ctx.getProperty("ratio").asNumber().orElse(0.0));
                assertEquals("main", ctx.getProperty("label").asString().orElse(""));
                assertTrue(ctx.getProperty("ok").asBoolean().orElse(false));
                assertEquals("y", ctx.getProperty("items").getElement(1).asString().orElse(""));
                assertTrue(ctx.getProperty("items").getElement(99).isNull());
                assertTrue(ctx.getProperty("missing").getProperty("deep").isNull());
                assertEquals(2, ctx.getProperty("items").size());
                assertEquals(7L, evt.getProperty("step").asLong().orElse(-1L));
                savedCtx.set(ctx);
                savedEvt.set(evt);
                return true;
            });
            m.registerAction("onAdvance", (ctx, evt) -> {
                assertTrue(m.matches("yellow"));
                m.send("FINISH");
            });
            m.start();
            assertEquals(600L, m.getNextDeadline().orElse(-1L));
            m.send("TIMER", "{\"step\": 7}");
            assertTrue(m.matches("red"));
            assertTrue(m.isDone());
        }
        assertEquals("main", savedCtx.get().getProperty("label").asString().orElse(""));
        assertEquals(7L, savedEvt.get().getProperty("step").asLong().orElse(-1L));
    }

    @Test
    void callbackExceptionShortCircuitsSimulatedClockAndPropagates() {
        AtomicBoolean secondTickCalled = new AtomicBoolean(false);
        String timerJson = "{\n" +
            "  \"initial\": \"a\",\n" +
            "  \"states\": {\n" +
            "    \"a\": { \"after\": { \"100\": { \"target\": \"b\", \"actions\": [\"boom\"] } } },\n" +
            "    \"b\": { \"after\": { \"100\": { \"target\": \"c\", \"actions\": [\"second\"] } } },\n" +
            "    \"c\": {}\n" +
            "  }\n" +
            "}";
        try (SimulatedClock clock = new SimulatedClock(0);
             Machine m = new Machine(timerJson, clock)) {
            m.registerAction("boom", (ctx, evt) -> { throw new IllegalStateException("fail at 100"); });
            m.registerAction("second", (ctx, evt) -> secondTickCalled.set(true));
            m.start();
            IllegalStateException ex = assertThrows(IllegalStateException.class, () -> clock.set(500));
            assertEquals("fail at 100", ex.getMessage());
            assertEquals(100L, clock.now());
            assertFalse(secondTickCalled.get());
        }
    }

    @Test
    void closedMachineThrowsIllegalStateException() {
        Machine m = new Machine(MACHINE_JSON);
        m.close();
        assertThrows(IllegalStateException.class, m::start);
    }

    @Test
    void nestedCallbackCanInsideAction() {
        String testJson = "{\n" +
            "  \"initial\": \"s1\",\n" +
            "  \"states\": {\n" +
            "    \"s1\": {\n" +
            "      \"on\": {\n" +
            "        \"GO\": { \"target\": \"s2\", \"actions\": [\"checkCan\"] }\n" +
            "      }\n" +
            "    },\n" +
            "    \"s2\": {\n" +
            "      \"on\": {\n" +
            "        \"CHECK\": { \"guard\": \"alwaysTrue\" }\n" +
            "      }\n" +
            "    }\n" +
            "  }\n" +
            "}";

        AtomicBoolean canResult = new AtomicBoolean(false);
        AtomicBoolean guardCalled = new AtomicBoolean(false);

        try (Machine m = new Machine(testJson)) {
            m.registerGuard("alwaysTrue", (ctx, evt) -> {
                guardCalled.set(true);
                return true;
            });
            m.registerAction("checkCan", (ctx, evt) -> {
                // Nested invocation of m.can while executing an action callback!
                boolean canCheck = m.can("CHECK");
                canResult.set(canCheck);
            });
            m.start();
            m.send("GO");
            assertTrue(m.matches("s2"));
            assertTrue(canResult.get(), "m.can('CHECK') should evaluate to true inside action");
            assertTrue(guardCalled.get(), "guard should have been called during m.can('CHECK')");
        }
    }

    @Test
    void nativeLoaderResolveTargetArch() {
        // macOS tests
        assertEquals("darwin-aarch64", NativeLoader.resolveTargetArch("Mac OS X", "aarch64", false));
        assertEquals("darwin-aarch64", NativeLoader.resolveTargetArch("Mac OS X", "arm64", false));
        assertEquals("darwin-x86_64", NativeLoader.resolveTargetArch("Mac OS X", "x86_64", false));

        // Linux tests (gnu vs musl)
        assertEquals("linux-x86_64-gnu", NativeLoader.resolveTargetArch("Linux", "x86_64", false));
        assertEquals("linux-x86_64-musl", NativeLoader.resolveTargetArch("Linux", "x86_64", true));
        assertEquals("linux-aarch64-gnu", NativeLoader.resolveTargetArch("Linux", "aarch64", false));
        assertEquals("linux-aarch64-musl", NativeLoader.resolveTargetArch("Linux", "aarch64", true));

        // Windows tests
        assertEquals("windows-x86_64", NativeLoader.resolveTargetArch("Windows 11", "amd64", false));
        assertEquals("windows-x86_64", NativeLoader.resolveTargetArch("Windows 10", "x86_64", false));
        assertEquals("windows-aarch64", NativeLoader.resolveTargetArch("Windows 11", "aarch64", false));
    }

    @Test
    void nativeLoaderExtractOrReuse(@TempDir Path tempDir) throws IOException {
        String content = "dummy-native-library-bytes";
        String hash = "abc12345";
        String fileName = "libtest.dylib";

        // First extraction
        ByteArrayInputStream stream1 = new ByteArrayInputStream(content.getBytes(StandardCharsets.UTF_8));
        Path extracted1 = NativeLoader.extractOrReuse(stream1, hash, tempDir, fileName);
        assertTrue(Files.exists(extracted1));
        assertEquals(content, Files.readString(extracted1));

        // Second extraction with same hash should reuse existing file without reading stream
        ByteArrayInputStream stream2 = new ByteArrayInputStream("different-content".getBytes(StandardCharsets.UTF_8));
        Path extracted2 = NativeLoader.extractOrReuse(stream2, hash, tempDir, fileName);
        assertEquals(extracted1, extracted2);
        assertEquals(content, Files.readString(extracted2));
    }

    @Test
    void simulatedClockWithClosedMachineDoesNotCauseUseAfterFree() {
        try (SimulatedClock clock = new SimulatedClock(0)) {
            Machine m1 = new Machine(MACHINE_JSON, clock);
            Machine m2 = new Machine(MACHINE_JSON, clock);
            m1.start();
            m2.start();

            // Close m1 while m2 remains active and registered
            m1.close();
            assertTrue(m1.isClosed());
            assertFalse(m2.isClosed());

            // Stepping clock must not dereference m1 or crash
            assertDoesNotThrow(() -> clock.set(1000));
            assertEquals(1000L, clock.now());
            assertTrue(m2.matches("yellow"));

            m2.close();
        }
    }

    @Test
    void deepValueToJsonIterativeNoStackOverflow() {
        // Build deeply nested JSON
        StringBuilder sb = new StringBuilder();
        int depth = 500;
        for (int i = 0; i < depth; i++) {
            sb.append("{\"child\":");
        }
        sb.append("42");
        for (int i = 0; i < depth; i++) {
            sb.append("}");
        }
        Value val = Value.fromJson(sb.toString());
        assertFalse(val.isNull());
        String serialized = val.toJson();
        assertNotNull(serialized);
        assertTrue(serialized.contains("42"));
    }
}
