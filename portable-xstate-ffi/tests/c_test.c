#include "../include/portable_xstate.h"
#include <stdio.h>
#include <string.h>
#include <assert.h>

const char* STOPLIGHT_JSON = "{"
  "\"id\": \"trafficLight\","
  "\"initial\": \"green\","
  "\"states\": {"
    "\"green\": { \"on\": { \"TIMER\": [ { \"target\": \"yellow\", \"actions\": [\"doTimerAction\"] } ] } },"
    "\"yellow\": { \"on\": { \"TIMER\": [ { \"target\": \"red\" } ] } },"
    "\"red\": { \"on\": { \"TIMER\": [ { \"target\": \"green\" } ] } }"
  "},"
  "\"context\": { \"cycles\": 0 }"
"}";

bool c_action_called = false;
void my_c_callback(const char* action_name, const fsm_value_t* context, const fsm_value_t* event, void* user_data) {
    printf("Action callback triggered: %s\n", action_name);
    assert(strcmp(action_name, "doTimerAction") == 0);
    assert(user_data != NULL);
    assert(strcmp((const char*)user_data, "my_custom_data") == 0);
    
    // Check context {"cycles": 0}
    const fsm_value_t* cycles = fsm_value_get_property(context, "cycles");
    assert(cycles != NULL);
    assert(fsm_value_is_number(cycles));
    assert(fsm_value_get_number(cycles) == 0.0);
    
    // Check null properties return null
    const fsm_value_t* missing = fsm_value_get_property(context, "missing_key");
    assert(missing == NULL);

    // Check event {"type": "TIMER"}
    const fsm_value_t* ev_type = fsm_value_get_property(event, "type");
    assert(ev_type != NULL);
    assert(fsm_value_is_string(ev_type));
    char ev_buf[32];
    fsm_value_get_string(ev_type, ev_buf, sizeof(ev_buf));
    assert(strcmp(ev_buf, "TIMER") == 0);

    c_action_called = true;
}

int main() {
    printf("Starting C Integration Test...\n");

    MachineContext* machine = fsm_create(STOPLIGHT_JSON);
    assert(machine != NULL && "Failed to create FSM");

    fsm_register_action(machine, "doTimerAction", my_c_callback, (void*)"my_custom_data");
    // Should be dropped because not started
    fsm_send_event(machine, "TIMER");
    assert(fsm_matches(machine, "green"));
    assert(!fsm_matches(machine, "yellow"));
    assert(c_action_called == false);

    fsm_start(machine);

    assert(fsm_matches(machine, "green"));
    assert(!fsm_matches(machine, "yellow"));

    fsm_send_event(machine, "TIMER");
    assert(fsm_matches(machine, "yellow"));
    assert(!fsm_matches(machine, "green"));
    assert(c_action_called == true);

    fsm_free(machine);
    printf("C Test Passed!\n");
    return 0;
}
