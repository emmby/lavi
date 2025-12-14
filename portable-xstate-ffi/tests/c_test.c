#include "../include/portable_xstate.h"
#include <stdio.h>
#include <string.h>
#include <assert.h>

const char* STOPLIGHT_JSON = "{"
  "\"id\": \"trafficLight\","
  "\"initial\": \"green\","
  "\"states\": {"
    "\"green\": { \"on\": { \"TIMER\": [ { \"target\": \"yellow\" } ] } },"
    "\"yellow\": { \"on\": { \"TIMER\": [ { \"target\": \"red\" } ] } },"
    "\"red\": { \"on\": { \"TIMER\": [ { \"target\": \"green\" } ] } }"
  "},"
  "\"context\": { \"cycles\": 0 }"
"}";

int main() {
    printf("Starting C Integration Test...\n");

    MachineContext* machine = fsm_create(STOPLIGHT_JSON);
    assert(machine != NULL && "Failed to create FSM");

    char buf[64];
    fsm_get_state(machine, buf, sizeof(buf));
    printf("Initial State: %s\n", buf);
    assert(strcmp(buf, "green") == 0);

    fsm_send_event(machine, "TIMER");
    fsm_get_state(machine, buf, sizeof(buf));
    printf("New State: %s\n", buf);
    assert(strcmp(buf, "yellow") == 0);

    fsm_free(machine);
    printf("C Test Passed!\n");
    return 0;
}
