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

    // State query tests: fsm_can, fsm_has_tag, fsm_is_done, fsm_stop
    const char* QUERIES_JSON = "{"
      "\"id\": \"queriesC\","
      "\"initial\": \"idle\","
      "\"states\": {"
        "\"idle\": {"
          "\"tags\": [\"ready\", \"standby\"],"
          "\"on\": {"
            "\"START\": \"working\""
          "}"
        "},"
        "\"working\": {"
          "\"initial\": \"step1\","
          "\"tags\": \"busy\","
          "\"states\": {"
            "\"step1\": {"
              "\"tags\": [\"phase1\"],"
              "\"on\": {"
                "\"FINISH\": \"#queriesC.done\""
              "}"
            "}"
          "}"
        "},"
        "\"done\": {"
          "\"type\": \"final\","
          "\"tags\": [\"completed\"]"
        "}"
      "}"
    "}";

    MachineContext* q1 = fsm_create(QUERIES_JSON);
    assert(q1 != NULL);
    fsm_start(q1);

    assert(fsm_can(NULL, "START") < 0);
    assert(fsm_can(q1, NULL) < 0);
    assert(fsm_has_tag(NULL, "ready") < 0);
    assert(fsm_has_tag(q1, NULL) < 0);
    assert(fsm_is_done(NULL) < 0);
    assert(fsm_stop(NULL) < 0);

    assert(fsm_is_done(q1) == 0);
    assert(fsm_has_tag(q1, "ready") == 1);
    assert(fsm_has_tag(q1, "standby") == 1);
    assert(fsm_has_tag(q1, "busy") == 0);
    assert(fsm_can(q1, "START") == 1);
    assert(fsm_can(q1, "UNKNOWN") == 0);

    fsm_send_event(q1, "START");
    assert(fsm_matches(q1, "working.step1"));
    assert(fsm_is_done(q1) == 0);
    assert(fsm_has_tag(q1, "ready") == 0);
    assert(fsm_has_tag(q1, "busy") == 1);
    assert(fsm_has_tag(q1, "phase1") == 1);
    assert(fsm_can(q1, "FINISH") == 1);
    assert(fsm_can(q1, "START") == 0);

    fsm_send_event(q1, "FINISH");
    assert(fsm_matches(q1, "done"));
    assert(fsm_is_done(q1) == 1);
    assert(fsm_has_tag(q1, "completed") == 1);
    assert(fsm_can(q1, "FINISH") == 0);
    fsm_free(q1);

    MachineContext* q2 = fsm_create(QUERIES_JSON);
    assert(q2 != NULL);
    fsm_start(q2);
    assert(fsm_can(q2, "START") == 1);
    assert(fsm_stop(q2) == 0);
    assert(fsm_can(q2, "START") == 0);
    assert(fsm_is_done(q2) == 0);
    fsm_send_event(q2, "START");
    assert(fsm_matches(q2, "idle"));
    fsm_free(q2);

    printf("C Test Passed!\n");
    return 0;
}
