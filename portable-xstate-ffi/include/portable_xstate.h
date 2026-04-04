#ifndef PORTABLE_XSTATE_H
#define PORTABLE_XSTATE_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MachineContext MachineContext;

MachineContext* fsm_create(const char* json_config);
void fsm_free(MachineContext* ptr);
int fsm_get_state(MachineContext* ptr, char* buf, int len);
int fsm_send_event(MachineContext* ptr, const char* event_name);

typedef struct fsm_value_t fsm_value_t;

// Type checks
bool fsm_value_is_object(const fsm_value_t* val);
bool fsm_value_is_string(const fsm_value_t* val);
bool fsm_value_is_number(const fsm_value_t* val);
bool fsm_value_is_bool(const fsm_value_t* val);

// Accessors
const char* fsm_value_get_string(const fsm_value_t* val);
double fsm_value_get_number(const fsm_value_t* val);
bool fsm_value_get_bool(const fsm_value_t* val);
const fsm_value_t* fsm_value_get_property(const fsm_value_t* val, const char* key);

// Callback signature
typedef void (*fsm_action_callback_t)(
    const char* action_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
);

// Registration
int fsm_register_action(
    MachineContext* ctx,
    const char* action_name,
    fsm_action_callback_t callback,
    void* user_data
);
#ifdef __cplusplus
}
#endif

#endif // PORTABLE_XSTATE_H
