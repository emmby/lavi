#ifndef PORTABLE_XSTATE_H
#define PORTABLE_XSTATE_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MachineContext MachineContext;

MachineContext* fsm_create(const char* json_config);
int fsm_start(MachineContext* ptr);
void fsm_free(MachineContext* ptr);
bool fsm_matches(const MachineContext* ptr, const char* state_path);
int fsm_send_event(MachineContext* ptr, const char* event_name);

typedef struct fsm_value_t fsm_value_t;

// Type checks
bool fsm_value_is_object(const fsm_value_t* val);
bool fsm_value_is_string(const fsm_value_t* val);
bool fsm_value_is_number(const fsm_value_t* val);
bool fsm_value_is_bool(const fsm_value_t* val);

// Accessors
size_t fsm_value_get_string(const fsm_value_t* val, char* buf, size_t buf_len);
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

typedef bool (*fsm_guard_callback_t)(
    const char* guard_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
);

typedef uint64_t (*fsm_clock_callback_t)(void);

// Registration
int fsm_register_action(
    MachineContext* ctx,
    const char* action_name,
    fsm_action_callback_t callback,
    void* user_data
);

int fsm_register_guard(
    MachineContext* ctx,
    const char* guard_name,
    fsm_guard_callback_t callback,
    void* user_data
);

int fsm_register_clock(
    MachineContext* ctx,
    fsm_clock_callback_t callback
);

/**
 * @brief Process pending explicit timers (delayed after transitions).
 * 
 * Call this periodically or when waking from an event loop to advance delayed transitions.
 * 
 * @param ptr The machine context. The caller is responsible for ensuring the lifetime of this pointer exceeds the duration of this call.
 * @return 0 on success, or -1 if the context pointer is null.
 */
int fsm_tick(MachineContext* ptr);
#ifdef __cplusplus
}
#endif

#endif // PORTABLE_XSTATE_H
