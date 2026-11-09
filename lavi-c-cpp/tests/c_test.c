#include "../include/lavi.h"
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

uint64_t c_named_delay_cb(const char* delay_name, const fsm_value_t* context, const fsm_value_t* event, void* user_data) {
    (void)context;
    (void)event;
    (void)user_data;
    assert(strcmp(delay_name, "myNamedDelay") == 0);
    return 750;
}

bool c_guard_with_params_cb(const char* guard_name, const fsm_value_t* context, const fsm_value_t* event, const fsm_value_t* params, void* user_data) {
    (void)guard_name;
    (void)event;
    (void)user_data;
    assert(params != NULL);
    const fsm_value_t* min_val = fsm_value_get_property(params, "min");
    assert(min_val != NULL && fsm_value_is_number(min_val));
    double min_num = fsm_value_get_number(min_val);
    const fsm_value_t* count_val = fsm_value_get_property(context, "count");
    assert(count_val != NULL && fsm_value_is_number(count_val));
    double count_num = fsm_value_get_number(count_val);
    return count_num >= min_num;
}

static bool c_input_verified = false;
static void inspect_input_cb(const char* action_name, const fsm_value_t* context, const fsm_value_t* event, void* user_data) {
    (void)action_name;
    (void)context;
    (void)user_data;
    assert(event != NULL);
    const fsm_value_t* inp = fsm_value_get_property(event, "input");
    assert(inp != NULL);
    assert(fsm_value_is_object(inp));
    const fsm_value_t* count_val = fsm_value_get_property(inp, "startCount");
    assert(count_val != NULL && fsm_value_is_number(count_val));
    assert(fsm_value_get_number(count_val) == 42.0);
    const fsm_value_t* user_val = fsm_value_get_property(inp, "user");
    assert(user_val != NULL && fsm_value_is_string(user_val));
    char user_buf[16];
    fsm_value_get_string(user_val, user_buf, sizeof(user_buf));
    assert(strcmp(user_buf, "alice") == 0);
    c_input_verified = true;
}

static int c_inspect_event_count = 0;
static bool c_inspect_saw_microstep = false;
static bool c_inspect_saw_snapshot = false;
static void c_inspector_cb(const char* inspect_json, void* user_data) {
    assert(inspect_json != NULL);
    int* counter = (int*)user_data;
    if (counter != NULL) {
        (*counter)++;
    }
    c_inspect_event_count++;
    if (strstr(inspect_json, "@xstate.microstep") != NULL) {
        c_inspect_saw_microstep = true;
    }
    if (strstr(inspect_json, "@xstate.snapshot") != NULL) {
        c_inspect_saw_snapshot = true;
    }
}

static uint64_t dynamic_delay_calc_cb(const char* delay_name, const fsm_value_t* context, const fsm_value_t* event, void* user_data) {
    (void)event;
    (void)user_data;
    assert(strcmp(delay_name, "dynamicTimeout") == 0);
    const fsm_value_t* base_v = fsm_value_get_property(context, "base");
    const fsm_value_t* factor_v = fsm_value_get_property(context, "factor");
    assert(base_v != NULL && fsm_value_is_number(base_v));
    assert(factor_v != NULL && fsm_value_is_number(factor_v));
    return (uint64_t)(fsm_value_get_number(base_v) * fsm_value_get_number(factor_v)); // 200 * 3 = 600 ms
}

static bool c_json_event_payload_verified = false;
static void inspect_json_event_cb(const char* action_name, const fsm_value_t* context, const fsm_value_t* event, void* user_data) {
    (void)action_name;
    (void)context;
    (void)user_data;
    assert(event != NULL);
    const fsm_value_t* score_v = fsm_value_get_property(event, "score");
    assert(score_v != NULL && fsm_value_is_number(score_v));
    assert(fsm_value_get_number(score_v) == 95.0);
    c_json_event_payload_verified = true;
}

static bool c_action_params_verified = false;
static void inspect_action_params_cb(const char* action_name, const fsm_value_t* context, const fsm_value_t* event, const fsm_value_t* params, void* user_data) {
    (void)action_name;
    (void)context;
    (void)event;
    (void)user_data;
    assert(params != NULL);
    const fsm_value_t* tag_v = fsm_value_get_property(params, "actionTag");
    assert(tag_v != NULL && fsm_value_is_string(tag_v));
    char tag_buf[16];
    fsm_value_get_string(tag_v, tag_buf, sizeof(tag_buf));
    assert(strcmp(tag_buf, "critical") == 0);
    c_action_params_verified = true;
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

    // Milestone 3: Named delay test via fsm_register_delay
    const char* NAMED_DELAY_JSON = "{"
      "\"id\": \"namedDelayFsm\","
      "\"initial\": \"waiting\","
      "\"states\": {"
        "\"waiting\": {"
          "\"after\": { \"myNamedDelay\": \"expired\" }"
        "},"
        "\"expired\": {}"
      "}"
    "}";

    SimulatedClock* delay_clock = fsm_simulated_clock_create(0);
    MachineContext* delay_fsm = fsm_create(NAMED_DELAY_JSON);
    assert(delay_fsm != NULL);
    assert(fsm_register_delay(delay_fsm, "myNamedDelay", c_named_delay_cb, NULL) == 0);
    assert(fsm_register_simulated_clock(delay_fsm, delay_clock) == 0);
    assert(fsm_start(delay_fsm) == 0);
    assert(fsm_matches(delay_fsm, "waiting"));
    uint64_t dl = 0;
    assert(fsm_get_next_deadline(delay_fsm, &dl) == true);
    assert(dl == 750);
    assert(fsm_simulated_clock_set(delay_clock, 750) == 0);
    assert(fsm_matches(delay_fsm, "expired"));
    fsm_simulated_clock_free(delay_clock);
    fsm_free(delay_fsm);

    // Milestone 3: Higher-order guards and guard with params via fsm_register_guard_with_params
    const char* GUARD_PARAMS_JSON = "{"
      "\"id\": \"guardParamsFsm\","
      "\"initial\": \"low\","
      "\"context\": { \"count\": 3 },"
      "\"states\": {"
        "\"low\": {"
          "\"on\": {"
            "\"CHECK\": {"
              "\"target\": \"high\","
              "\"guard\": { \"type\": \"checkMin\", \"params\": { \"min\": 5 } }"
            "}"
          "}"
        "},"
        "\"high\": {}"
      "}"
    "}";

    MachineContext* gp_fsm = fsm_create(GUARD_PARAMS_JSON);
    assert(gp_fsm != NULL);
    assert(fsm_register_guard_with_params(gp_fsm, "checkMin", c_guard_with_params_cb, NULL) == 0);
    assert(fsm_start(gp_fsm) == 0);
    // count is 3, min is 5 -> transition disabled
    fsm_send_event(gp_fsm, "CHECK");
    assert(fsm_matches(gp_fsm, "low"));
    fsm_free(gp_fsm);

    // ==========================================
    // Milestone 5: Input & Context Initialization
    // ==========================================
    const char* INPUT_MACHINE_JSON = "{"
      "\"id\": \"inputTestFsm\","
      "\"initial\": \"init\","
      "\"context\": { \"cycles\": 0 },"
      "\"states\": {"
        "\"init\": {"
          "\"entry\": [\"inspectInput\"],"
          "\"on\": { \"NEXT\": \"done\" }"
        "},"
        "\"done\": { \"type\": \"final\" }"
      "}"
    "}";

    // Error & null parameter handling
    assert(fsm_create_with_input(NULL, "{\"startCount\": 42}") == NULL);
    assert(fsm_create_with_input(INPUT_MACHINE_JSON, "{ invalid json") == NULL);

    // Null input fallback -> equivalent to fsm_create
    MachineContext* m_null_inp = fsm_create_with_input(INPUT_MACHINE_JSON, NULL);
    assert(m_null_inp != NULL);
    fsm_destroy(m_null_inp);

    // Valid creation with input
    MachineContext* m_inp = fsm_create_with_input(INPUT_MACHINE_JSON, "{\"startCount\": 42, \"user\": \"alice\"}");
    assert(m_inp != NULL);
    fsm_register_action(m_inp, "inspectInput", inspect_input_cb, NULL);
    assert(fsm_start(m_inp) == 0);
    assert(c_input_verified == true);
    fsm_destroy(m_inp);

    // ==========================================
    // Milestone 5: Machine Output Retrieval
    // ==========================================
    const char* OUTPUT_MACHINE_JSON = "{"
      "\"id\": \"outputFsm\","
      "\"initial\": \"working\","
      "\"output\": { \"result\": \"success\", \"code\": 200 },"
      "\"states\": {"
        "\"working\": {"
          "\"on\": { \"FINISH\": \"completed\" }"
        "},"
        "\"completed\": {"
          "\"type\": \"final\""
        "}"
      "}"
    "}";

    MachineContext* out_m = fsm_create(OUTPUT_MACHINE_JSON);
    assert(out_m != NULL);
    assert(fsm_start(out_m) == 0);

    // Uncompleted machine must return -4
    char temp_buf[64];
    assert(fsm_get_output_json(out_m, temp_buf, sizeof(temp_buf)) == -4);
    assert(fsm_get_output_json(NULL, temp_buf, sizeof(temp_buf)) == -1);

    // Transition to final state
    fsm_send_event(out_m, "FINISH");
    assert(fsm_is_in_state(out_m, "completed"));
    assert(fsm_is_done(out_m) == 1);

    // Sizing query when done
    int32_t out_req = fsm_get_output_json(out_m, NULL, 0);
    assert(out_req > 0);

    // Buffer too small test with canary validation
    char small_out[4] = { 0x55, 0x55, 0x55, 0x55 };
    assert(fsm_get_output_json(out_m, small_out, sizeof(small_out)) == -2);
    assert(small_out[3] == 0x55); // No canary clobbering

    // Buffer retrieval
    char* out_buf = (char*)malloc((size_t)out_req);
    assert(out_buf != NULL);
    int32_t out_written = fsm_get_output_json(out_m, out_buf, (size_t)out_req);
    assert(out_written > 0 && out_written == out_req - 1);
    assert(strstr(out_buf, "\"result\":\"success\"") != NULL);
    assert(strstr(out_buf, "\"code\":200") != NULL);
    free(out_buf);
    fsm_destroy(out_m);

    // ==========================================
    // Milestone 5: Metadata Aggregation & Inspection
    // ==========================================
    const char* META_MACHINE_JSON = "{"
      "\"id\": \"metaFsm\","
      "\"initial\": \"phaseA\","
      "\"meta\": { \"version\": \"2.1\" },"
      "\"states\": {"
        "\"phaseA\": {"
          "\"meta\": { \"tag\": \"A\", \"limit\": 10 },"
          "\"on\": { \"NEXT\": \"phaseB\" }"
        "},"
        "\"phaseB\": {"
          "\"meta\": { \"tag\": \"B\", \"limit\": 20 }"
        "}"
      "}"
    "}";

    MachineContext* meta_m = fsm_create(META_MACHINE_JSON);
    assert(meta_m != NULL);
    assert(fsm_start(meta_m) == 0);

    // Null checks
    assert(fsm_get_meta_json(NULL, temp_buf, sizeof(temp_buf)) == -1);
    assert(fsm_get_state_meta_json(NULL, "phaseA", temp_buf, sizeof(temp_buf)) == -1);
    assert(fsm_get_state_meta_json(meta_m, NULL, temp_buf, sizeof(temp_buf)) == -2);

    // Aggregated meta query (root + active state phaseA)
    int32_t meta_req = fsm_get_meta_json(meta_m, NULL, 0);
    assert(meta_req > 0);
    char* meta_buf = (char*)malloc((size_t)meta_req);
    int32_t meta_written = fsm_get_meta_json(meta_m, meta_buf, (size_t)meta_req);
    assert(meta_written > 0);
    assert(strstr(meta_buf, "\"version\":\"2.1\"") != NULL);
    assert(strstr(meta_buf, "\"tag\":\"A\"") != NULL);
    free(meta_buf);

    // State-specific meta query for phaseA
    int32_t sm_req = fsm_get_state_meta_json(meta_m, "phaseA", NULL, 0);
    assert(sm_req > 0);
    char* sm_buf = (char*)malloc((size_t)sm_req);
    int32_t sm_written = fsm_get_state_meta_json(meta_m, "phaseA", sm_buf, (size_t)sm_req);
    assert(sm_written > 0);
    assert(strstr(sm_buf, "\"limit\":10") != NULL);
    free(sm_buf);

    // State-specific meta query for inactive state phaseB
    int32_t sm_b_req = fsm_get_state_meta_json(meta_m, "phaseB", NULL, 0);
    assert(sm_b_req > 0);
    char* sm_b_buf = (char*)malloc((size_t)sm_b_req);
    int32_t sm_b_written = fsm_get_state_meta_json(meta_m, "phaseB", sm_b_buf, (size_t)sm_b_req);
    assert(sm_b_written > 0);
    assert(strstr(sm_b_buf, "\"limit\":20") != NULL);
    free(sm_b_buf);

    // Non-existent state meta returns -4
    assert(fsm_get_state_meta_json(meta_m, "does_not_exist", temp_buf, sizeof(temp_buf)) == -4);

    // Buffer too small returns -3 per lavi.h contract
    assert(fsm_get_state_meta_json(meta_m, "phaseA", small_out, sizeof(small_out)) == -3);
    fsm_destroy(meta_m);

    // ==========================================
    // Milestone 5: Dynamic Delay Calculation in C
    // ==========================================
    const char* DYN_DELAY_JSON = "{"
      "\"id\": \"dynDelayC\","
      "\"initial\": \"waiting\","
      "\"context\": { \"base\": 200, \"factor\": 3 },"
      "\"states\": {"
        "\"waiting\": {"
          "\"after\": { \"dynamicTimeout\": \"expired\" }"
        "},"
        "\"expired\": {}"
      "}"
    "}";

    SimulatedClock* dyn_clk = fsm_simulated_clock_create(0);
    MachineContext* dyn_m = fsm_create(DYN_DELAY_JSON);
    assert(dyn_m != NULL);
    assert(fsm_register_delay(dyn_m, "dynamicTimeout", dynamic_delay_calc_cb, NULL) == 0);
    assert(fsm_register_simulated_clock(dyn_m, dyn_clk) == 0);
    assert(fsm_start(dyn_m) == 0);
    assert(fsm_is_in_state(dyn_m, "waiting"));

    uint64_t dyn_dl = 0;
    assert(fsm_get_next_deadline(dyn_m, &dyn_dl) == true);
    assert(dyn_dl == 600);

    // Advance to 599 ms -> still waiting
    assert(fsm_simulated_clock_set(dyn_clk, 599) == 0);
    assert(fsm_is_in_state(dyn_m, "waiting"));

    // Advance to 600 ms -> transitions to expired
    assert(fsm_simulated_clock_set(dyn_clk, 600) == 0);
    assert(fsm_is_in_state(dyn_m, "expired"));

    fsm_simulated_clock_free(dyn_clk);
    fsm_destroy(dyn_m);

    // ==========================================
    // Milestone 5: Structured JSON Events & Action Params
    // ==========================================
    const char* JSON_EVENT_FSM = "{"
      "\"id\": \"jsonEventFsm\","
      "\"initial\": \"ready\","
      "\"states\": {"
        "\"ready\": {"
          "\"on\": {"
            "\"UPDATE_SCORE\": {"
              "\"target\": \"scored\","
              "\"actions\": ["
                "\"inspectEvent\","
                "{ \"type\": \"paramAction\", \"params\": { \"actionTag\": \"critical\" } }"
              "]"
            "}"
          "}"
        "},"
        "\"scored\": {}"
      "}"
    "}";

    MachineContext* je_m = fsm_create(JSON_EVENT_FSM);
    assert(je_m != NULL);
    fsm_register_action(je_m, "inspectEvent", inspect_json_event_cb, NULL);
    fsm_register_action_with_params(je_m, "paramAction", inspect_action_params_cb, NULL);

    // Pre-start event is dropped without buffering
    assert(fsm_send_json_event(je_m, "{\"type\": \"UPDATE_SCORE\", \"score\": 95}") == 0);
    assert(c_json_event_payload_verified == false);

    assert(fsm_start(je_m) == 0);
    assert(fsm_is_in_state(je_m, "ready"));

    // Active state serialization
    char state_buf[32];
    int32_t st_len = fsm_get_state(je_m, state_buf, sizeof(state_buf));
    assert(st_len > 0);
    assert(strcmp(state_buf, "\"ready\"") == 0);

    // Send valid JSON event with payload
    assert(fsm_send_json_event(je_m, "{\"type\": \"UPDATE_SCORE\", \"score\": 95}") == 0);
    assert(c_json_event_payload_verified == true);
    assert(c_action_params_verified == true);
    assert(fsm_is_in_state(je_m, "scored"));

    fsm_destroy(je_m);

    // ==========================================
    // Milestone 6: Inspection & Telemetry Hook
    // ==========================================
    MachineContext* insp_m = fsm_create(STOPLIGHT_JSON);
    assert(insp_m != NULL);
    int user_inspect_count = 0;
    assert(fsm_register_inspector(insp_m, c_inspector_cb, &user_inspect_count) == 0);
    assert(fsm_start(insp_m) == 0);
    assert(fsm_send_event(insp_m, "TIMER") == 0);
    assert(fsm_is_in_state(insp_m, "yellow"));
    assert(c_inspect_event_count > 0);
    assert(user_inspect_count == c_inspect_event_count);
    assert(c_inspect_saw_microstep == true);
    assert(c_inspect_saw_snapshot == true);
    fsm_destroy(insp_m);

    printf("C Test Passed!\n");
    return 0;
}

