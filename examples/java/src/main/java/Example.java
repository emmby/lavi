import io.lavi.ActionCallback;
import io.lavi.Machine;
import io.lavi.SimulatedClock;

public final class Example {
    private static final String MACHINE_JSON =
        "{\n" +
        "  \"id\": \"stoplight\",\n" +
        "  \"initial\": \"red\",\n" +
        "  \"context\": { \"cycles\": 1 },\n" +
        "  \"states\": {\n" +
        "    \"red\": {\n" +
        "      \"entry\": [\"onRed\"],\n" +
        "      \"after\": { \"1000\": \"green\" }\n" +
        "    },\n" +
        "    \"green\": {\n" +
        "      \"entry\": [\"onGreen\"],\n" +
        "      \"on\": {\n" +
        "        \"PEDESTRIAN_WAIT\": {\n" +
        "          \"target\": \"yellow\",\n" +
        "          \"guard\": \"canCross\"\n" +
        "        }\n" +
        "      },\n" +
        "      \"after\": { \"1000\": \"yellow\" }\n" +
        "    },\n" +
        "    \"yellow\": {\n" +
        "      \"entry\": [\"onYellow\"],\n" +
        "      \"after\": { \"1000\": \"red\" }\n" +
        "    }\n" +
        "  }\n" +
        "}";

    private static void check(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }

    public static void main(String[] args) {
        final int[] transitions = {0};
        ActionCallback onEntry = (ctx, evt) -> transitions[0] += 1;

        try (SimulatedClock clock = new SimulatedClock(0);
             Machine machine = new Machine(MACHINE_JSON, clock)
                 .registerAction("onRed", onEntry)
                 .registerAction("onGreen", onEntry)
                 .registerAction("onYellow", onEntry)
                 .registerGuard("canCross", (ctx, evt) ->
                     ctx.getProperty("cycles").asLong().orElse(0L) >= 1L)) {

            // 1. Start in "red"
            machine.start();
            check(machine.matches("red"), "expected state 'red'");
            check(transitions[0] == 1, "expected 1 transition");

            // 2. Advance 1000ms -> "green" (via `after: 1000`)
            clock.increment(1000);
            check(machine.matches("green"), "expected state 'green'");
            check(transitions[0] == 2, "expected 2 transitions");

            // 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
            machine.send("PEDESTRIAN_WAIT");
            check(machine.matches("yellow"), "expected state 'yellow'");
            check(transitions[0] == 3, "expected 3 transitions");

            // 4. Advance 1000ms -> "red" (via `after: 1000`)
            clock.increment(1000);
            check(machine.matches("red"), "expected state 'red'");
            check(transitions[0] == 4, "expected 4 transitions");
        }

        System.out.println("Java example passed (transitions: " + transitions[0] + ")");
    }
}
