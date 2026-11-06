#include "../include/portable_xstate.h"
#include <stdio.h>
#include <stdlib.h>
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

    // Snapshot serialization and restoration test
    const char* SNAPSHOT_TEST_JSON = "{"
      "\"id\": \"snapshotTestMachine\","
      "\"initial\": \"idle\","
      "\"context\": { \"count\": 0, \"flag\": \"initial\" },"
      "\"states\": {"
        "\"idle\": {"
          "\"tags\": [\"dormant\"],"
          "\"on\": {"
            "\"START\": {"
              "\"target\": \"active.subA\","
              "\"actions\": ["
                "{ \"type\": \"xstate.assign\", \"assignment\": { \"count\": 42, \"flag\": \"started\" } }"
              "]"
            "}"
          "}"
        "},"
        "\"active\": {"
          "\"initial\": \"subA\","
          "\"tags\": [\"working\"],"
          "\"states\": {"
            "\"hist\": { \"type\": \"history\", \"history\": \"shallow\" },"
            "\"subA\": {"
              "\"on\": { \"STEP\": \"subB\" }"
            "},"
            "\"subB\": {"
              "\"on\": {"
                "\"PAUSE\": \"#snapshotTestMachine.paused\","
                "\"FINISH\": \"#snapshotTestMachine.done\""
              "}"
            "}"
          "}"
        "},"
        "\"paused\": {"
          "\"on\": {"
            "\"RESUME\": \"active.hist\""
          "}"
        "},"
        "\"done\": {"
          "\"type\": \"final\","
          "\"tags\": [\"completed\"]"
        "}"
      "}"
    "}";

    MachineContext* m1 = fsm_create(SNAPSHOT_TEST_JSON);
    assert(m1 != NULL);
    fsm_start(m1);
    assert(fsm_matches(m1, "idle"));
    assert(fsm_has_tag(m1, "dormant") == 1);

    // Transition idle -> active.subA (with context assign: count=42, flag="started")
    fsm_send_event(m1, "START");
    assert(fsm_matches(m1, "active.subA"));
    assert(fsm_has_tag(m1, "working") == 1);

    // Transition active.subA -> active.subB
    fsm_send_event(m1, "STEP");
    assert(fsm_matches(m1, "active.subB"));

    // Transition active.subB -> paused (records active's history as subB)
    fsm_send_event(m1, "PAUSE");
    assert(fsm_matches(m1, "paused"));

    // 1. Buffer size query test
    int32_t req_len = fsm_get_snapshot_json(m1, NULL, 0);
    assert(req_len > 0);

    // 2. Buffer too small test
    char small_buf[8];
    assert(fsm_get_snapshot_json(m1, small_buf, sizeof(small_buf)) == -2);

    // 3. Serialize snapshot
    char* snapshot_buf = (char*)malloc((size_t)req_len);
    assert(snapshot_buf != NULL);
    int32_t written = fsm_get_snapshot_json(m1, snapshot_buf, (size_t)req_len);
    assert(written > 0);
    assert((size_t)written < (size_t)req_len);
    assert(strlen(snapshot_buf) == (size_t)written);

    // 4. Null & error checks
    assert(fsm_get_snapshot_json(NULL, snapshot_buf, (size_t)req_len) == -1);
    assert(fsm_restore_snapshot_json(NULL, snapshot_buf) == -1);
    assert(fsm_restore_snapshot_json(m1, NULL) == -2);
    assert(fsm_restore_snapshot_json(m1, "{ invalid json ]") < 0);

    // 5. Restore into a fresh machine context m2
    MachineContext* m2 = fsm_create(SNAPSHOT_TEST_JSON);
    assert(m2 != NULL);
    assert(fsm_restore_snapshot_json(m2, snapshot_buf) == 0);

    // Verify m2 resumed directly in "paused" state
    assert(fsm_matches(m2, "paused"));
    assert(!fsm_matches(m2, "idle"));

    // 6. Resume via history state active.hist -> must enter active.subB (NOT default initial subA)
    fsm_send_event(m2, "RESUME");
    assert(fsm_matches(m2, "active.subB"));
    assert(!fsm_matches(m2, "active.subA"));
    assert(fsm_has_tag(m2, "working") == 1);

    // 7. Transition to final state done
    fsm_send_event(m2, "FINISH");
    assert(fsm_matches(m2, "done"));
    assert(fsm_is_done(m2) == 1);
    assert(fsm_has_tag(m2, "completed") == 1);

    free(snapshot_buf);
    fsm_free(m1);
    fsm_free(m2);

    // Snapshot status parity test across active, done, and stopped states
    const char* STATUS_TEST_JSON = "{"
      "\"id\": \"statusFsm\","
      "\"initial\": \"s1\","
      "\"states\": {"
        "\"s1\": { \"on\": { \"STEP\": \"s2\", \"FINISH\": \"doneState\" } },"
        "\"s2\": { \"on\": { \"FINISH\": \"doneState\" } },"
        "\"doneState\": { \"type\": \"final\" }"
      "}"
    "}";

    // 1. ACTIVE
    MachineContext* sm_act = fsm_create(STATUS_TEST_JSON);
    assert(sm_act != NULL);
    fsm_start(sm_act);
    fsm_send_event(sm_act, "STEP");
    assert(fsm_matches(sm_act, "s2"));
    assert(fsm_is_done(sm_act) == 0);

    int32_t act_req = fsm_get_snapshot_json(sm_act, NULL, 0);
    char* act_buf = (char*)malloc((size_t)act_req);
    fsm_get_snapshot_json(sm_act, act_buf, (size_t)act_req);
    assert(strstr(act_buf, "\"status\":\"active\"") != NULL);

    MachineContext* sm_act_res = fsm_create(STATUS_TEST_JSON);
    assert(fsm_restore_snapshot_json(sm_act_res, act_buf) == 0);
    assert(fsm_matches(sm_act_res, "s2"));
    assert(fsm_is_done(sm_act_res) == 0);
    fsm_send_event(sm_act_res, "FINISH");
    assert(fsm_matches(sm_act_res, "doneState"));
    assert(fsm_is_done(sm_act_res) == 1);
    free(act_buf);
    fsm_free(sm_act);
    fsm_free(sm_act_res);

    // 2. DONE
    MachineContext* sm_done = fsm_create(STATUS_TEST_JSON);
    assert(sm_done != NULL);
    fsm_start(sm_done);
    fsm_send_event(sm_done, "FINISH");
    assert(fsm_matches(sm_done, "doneState"));
    assert(fsm_is_done(sm_done) == 1);

    int32_t done_req = fsm_get_snapshot_json(sm_done, NULL, 0);
    char* done_buf = (char*)malloc((size_t)done_req);
    fsm_get_snapshot_json(sm_done, done_buf, (size_t)done_req);
    assert(strstr(done_buf, "\"status\":\"done\"") != NULL);

    MachineContext* sm_done_res = fsm_create(STATUS_TEST_JSON);
    assert(fsm_restore_snapshot_json(sm_done_res, done_buf) == 0);
    assert(fsm_matches(sm_done_res, "doneState"));
    assert(fsm_is_done(sm_done_res) == 1);
    int32_t done_res_req = fsm_get_snapshot_json(sm_done_res, NULL, 0);
    char* done_res_buf = (char*)malloc((size_t)done_res_req);
    fsm_get_snapshot_json(sm_done_res, done_res_buf, (size_t)done_res_req);
    assert(strstr(done_res_buf, "\"status\":\"done\"") != NULL);
    fsm_send_event(sm_done_res, "STEP");
    assert(fsm_matches(sm_done_res, "doneState"));
    assert(fsm_is_done(sm_done_res) == 1);
    free(done_buf);
    free(done_res_buf);
    fsm_free(sm_done);
    fsm_free(sm_done_res);

    // 3. STOPPED
    MachineContext* sm_stop = fsm_create(STATUS_TEST_JSON);
    assert(sm_stop != NULL);
    fsm_start(sm_stop);
    fsm_send_event(sm_stop, "STEP");
    fsm_stop(sm_stop);
    assert(fsm_matches(sm_stop, "s2"));

    int32_t stop_req = fsm_get_snapshot_json(sm_stop, NULL, 0);
    char* stop_buf = (char*)malloc((size_t)stop_req);
    fsm_get_snapshot_json(sm_stop, stop_buf, (size_t)stop_req);
    assert(strstr(stop_buf, "\"status\":\"stopped\"") != NULL);

    MachineContext* sm_stop_res = fsm_create(STATUS_TEST_JSON);
    assert(fsm_restore_snapshot_json(sm_stop_res, stop_buf) == 0);
    assert(fsm_matches(sm_stop_res, "s2"));
    assert(fsm_is_done(sm_stop_res) == 0);
    fsm_send_event(sm_stop_res, "FINISH");
    assert(fsm_matches(sm_stop_res, "s2"));
    assert(!fsm_matches(sm_stop_res, "doneState"));
    int32_t stop_res_req = fsm_get_snapshot_json(sm_stop_res, NULL, 0);
    char* stop_res_buf = (char*)malloc((size_t)stop_res_req);
    fsm_get_snapshot_json(sm_stop_res, stop_res_buf, (size_t)stop_res_req);
    assert(strstr(stop_res_buf, "\"status\":\"stopped\"") != NULL);
    free(stop_buf);
    free(stop_res_buf);
    fsm_free(sm_stop);
    fsm_free(sm_stop_res);

    printf("C Test Passed!\n");
    return 0;
}
