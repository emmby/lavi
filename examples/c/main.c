#include "lavi.h"
#include <assert.h>
#include <stdio.h>

static const char* MACHINE_JSON =
  "{"
  "  \"id\": \"stoplight\","
  "  \"initial\": \"red\","
  "  \"context\": { \"cycles\": 1 },"
  "  \"states\": {"
  "    \"red\": {"
  "      \"entry\": [\"onRed\"],"
  "      \"after\": { \"1000\": \"green\" }"
  "    },"
  "    \"green\": {"
  "      \"entry\": [\"onGreen\"],"
  "      \"on\": {"
  "        \"PEDESTRIAN_WAIT\": {"
  "          \"target\": \"yellow\","
  "          \"guard\": \"canCross\""
  "        }"
  "      },"
  "      \"after\": { \"1000\": \"yellow\" }"
  "    },"
  "    \"yellow\": {"
  "      \"entry\": [\"onYellow\"],"
  "      \"after\": { \"1000\": \"red\" }"
  "    }"
  "  }"
  "}";

typedef struct {
  int transitions;
} StoplightState;

static void on_light_entry(
    const char* action_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
) {
  (void)action_name;
  (void)context;
  (void)event;
  StoplightState* state = (StoplightState*)user_data;
  state->transitions += 1;
}

static bool can_cross(
    const char* guard_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
) {
  (void)guard_name;
  (void)event;
  (void)user_data;
  const fsm_value_t* cycles = fsm_value_get_property(context, "cycles");
  return cycles != NULL && fsm_value_is_number(cycles) && fsm_value_get_number(cycles) >= 1.0;
}

int main(void) {
  StoplightState state = { .transitions = 0 };

  SimulatedClock* clock = fsm_simulated_clock_create(0);
  assert(clock != NULL);

  MachineContext* machine = fsm_create(MACHINE_JSON);
  assert(machine != NULL);

  assert(fsm_register_simulated_clock(machine, clock) == 0);
  assert(fsm_register_action(machine, "onRed", on_light_entry, &state) == 0);
  assert(fsm_register_action(machine, "onGreen", on_light_entry, &state) == 0);
  assert(fsm_register_action(machine, "onYellow", on_light_entry, &state) == 0);
  assert(fsm_register_guard(machine, "canCross", can_cross, NULL) == 0);

  // 1. Start in "red"
  assert(fsm_start(machine) == 0);
  assert(fsm_matches(machine, "red"));
  assert(state.transitions == 1);

  // 2. Advance 1000ms -> "green" (via `after: 1000`)
  assert(fsm_simulated_clock_increment(clock, 1000) == 0);
  assert(fsm_matches(machine, "green"));
  assert(state.transitions == 2);

  // 3. Send guarded "PEDESTRIAN_WAIT" -> "yellow"
  assert(fsm_send_event(machine, "PEDESTRIAN_WAIT") == 0);
  assert(fsm_matches(machine, "yellow"));
  assert(state.transitions == 3);

  // 4. Advance 1000ms -> "red" (via `after: 1000`)
  assert(fsm_simulated_clock_increment(clock, 1000) == 0);
  assert(fsm_matches(machine, "red"));
  assert(state.transitions == 4);

  fsm_free(machine);
  fsm_simulated_clock_free(clock);

  printf("C example passed (transitions: %d)\n", state.transitions);
  return 0;
}
