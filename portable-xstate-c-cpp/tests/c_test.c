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

void verify_assign_cb(const char* action_name, const fsm_value_t* context, const fsm_value_t* event, void* user_data) {
    (void)action_name;
    (void)event;
    bool* verified_flag = (bool*)user_data;
    const fsm_value_t* count_val = fsm_value_get_property(context, "count");
    const fsm_value_t* foo_val = fsm_value_get_property(context, "foo");
    assert(count_val != NULL && fsm_value_is_number(count_val));
    assert(fsm_value_get_number(count_val) == 100.0);
    assert(foo_val != NULL && fsm_value_is_string(foo_val));
    char foo_buf[16];
    fsm_value_get_string(foo_val, foo_buf, sizeof(foo_buf));
    assert(strcmp(foo_buf, "win") == 0);
    *verified_flag = true;
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

    // Virtual time advancement test using SimulatedClock*
    const char* APRIL_JSON = "{"
      "\"id\": \"april\","
      "\"initial\": \"ConnectToNetwork\","
      "\"states\": {"
        "\"ConnectToNetwork\": {"
          "\"initial\": \"Polling\","
          "\"after\": { \"300000\": \"Wait\" },"
          "\"states\": {"
            "\"Polling\": {"
              "\"after\": { \"500\": { \"target\": \"Polling\", \"reenter\": true } }"
            "}"
          "}"
        "},"
        "\"Wait\": {"
          "\"after\": { \"60000\": \"Sleep\" }"
        "},"
        "\"Sleep\": {"
          "\"after\": { \"3600000\": \"Wake\" }"
        "},"
        "\"Wake\": {}"
      "}"
    "}";

    SimulatedClock* sim_clock = fsm_simulated_clock_create(0);
    assert(sim_clock != NULL);
    MachineContext* april_fsm = fsm_create(APRIL_JSON);
    assert(april_fsm != NULL);
    assert(fsm_register_simulated_clock(april_fsm, sim_clock) == 0);
    assert(fsm_start(april_fsm) == 0);

    uint64_t next_deadline = 0;
    assert(fsm_get_next_deadline(april_fsm, &next_deadline) == true);
    assert(next_deadline == 500);

    // Advance 300,000 ms -> parent timeout transitions ConnectToNetwork -> Wait at t = 300,000
    assert(fsm_simulated_clock_increment(sim_clock, 300000) == 0);
    assert(fsm_simulated_clock_now(sim_clock) == 300000);
    assert(fsm_matches(april_fsm, "Wait"));
    assert(fsm_get_next_deadline(april_fsm, &next_deadline) == true);
    assert(next_deadline == 360000);

    // Advance 24 hours (86,400,000 ms) in one call -> Wait -> Sleep at 360,000, Sleep -> Wake at 3,960,000
    assert(fsm_simulated_clock_set(sim_clock, 86700000) == 0);
    assert(fsm_simulated_clock_now(sim_clock) == 86700000);
    assert(fsm_matches(april_fsm, "Wake"));
    assert(fsm_get_next_deadline(april_fsm, &next_deadline) == false);

    fsm_simulated_clock_free(sim_clock);
    fsm_free(april_fsm);

    // Wildcard and partial wildcard transition test
    const char* WILDCARD_JSON = "{"
      "\"initial\": \"A\","
      "\"states\": {"
        "\"A\": {"
          "\"on\": {"
            "\"*\": \"wild\","
            "\"models.*\": \"modelWild\","
            "\"models.user\": \"exactUser\""
          "}"
        "},"
        "\"wild\": {},"
        "\"modelWild\": {},"
        "\"exactUser\": {}"
      "}"
    "}";

    MachineContext* w1 = fsm_create(WILDCARD_JSON);
    assert(w1 != NULL);
    fsm_start(w1);
    fsm_send_event(w1, "models.user");
    assert(fsm_matches(w1, "exactUser"));
    fsm_free(w1);

    MachineContext* w2 = fsm_create(WILDCARD_JSON);
    assert(w2 != NULL);
    fsm_start(w2);
    fsm_send_event(w2, "models.post.deleted");
    assert(fsm_matches(w2, "modelWild"));
    fsm_free(w2);

    MachineContext* w3 = fsm_create(WILDCARD_JSON);
    assert(w3 != NULL);
    fsm_start(w3);
    fsm_send_event(w3, "OTHER_EVENT");
    assert(fsm_matches(w3, "wild"));
    fsm_free(w3);

    // Declarative xstate.assign multi-property context update test
    const char* ASSIGN_JSON = "{"
      "\"initial\": \"counting\","
      "\"context\": { \"count\": 0, \"foo\": \"bar\" },"
      "\"states\": {"
        "\"counting\": {"
          "\"on\": {"
            "\"WIN\": {"
              "\"target\": \"won\","
              "\"actions\": ["
                "{ \"type\": \"xstate.assign\", \"assignment\": { \"count\": 100, \"foo\": \"win\" } },"
                "\"verifyAssigned\""
              "]"
            "}"
          "}"
        "},"
        "\"won\": {}"
      "}"
    "}";

    bool assign_verified = false;
    MachineContext* assign_fsm = fsm_create(ASSIGN_JSON);
    assert(assign_fsm != NULL);
    fsm_register_action(assign_fsm, "verifyAssigned", verify_assign_cb, &assign_verified);
    fsm_start(assign_fsm);
    fsm_send_event(assign_fsm, "WIN");
    assert(fsm_matches(assign_fsm, "won"));
    assert(assign_verified == true);
    fsm_free(assign_fsm);

    printf("C Test Passed!\n");
    return 0;
}
