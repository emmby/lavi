#ifndef LAVI_H
#define LAVI_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MachineContext MachineContext;

MachineContext* fsm_create(const char* json_config);
MachineContext* fsm_create_with_input(const char* json_config, const char* input_json);
int fsm_start(MachineContext* ptr);
void fsm_free(MachineContext* ptr);
void fsm_destroy(MachineContext* ptr);

/**
 * @brief Check whether the given dot-delimited state path is currently active.
 *
 * @param ptr The machine context.
 * @param state_path Dot-delimited state path (e.g. "green", "Display", "Display.Power.On").
 * @return true if the state path is active, false otherwise.
 */
bool fsm_matches(const MachineContext* ptr, const char* state_path);
bool fsm_is_in_state(const MachineContext* ptr, const char* state_path);
int fsm_can(const MachineContext* ctx, const char* event);
int fsm_has_tag(const MachineContext* ctx, const char* tag);
int fsm_is_done(const MachineContext* ctx);
int fsm_stop(MachineContext* ctx);
int fsm_send_event(MachineContext* ptr, const char* event_name);
int fsm_send_json_event(MachineContext* ptr, const char* json_event);
int32_t fsm_get_state(const MachineContext* ctx, char* buffer, size_t buffer_len);


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

typedef void (*fsm_action_with_params_callback_t)(
    const char* action_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    const fsm_value_t* params,
    void* user_data
);


typedef bool (*fsm_guard_callback_t)(
    const char* guard_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
);

typedef uint64_t (*fsm_delay_callback_t)(
    const char* delay_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
);

typedef bool (*fsm_guard_with_params_callback_t)(
    const char* guard_name,
    const fsm_value_t* context,
    const fsm_value_t* event,
    const fsm_value_t* params,
    void* user_data
);

typedef uint64_t (*fsm_clock_callback_t)(void);
typedef uint64_t (*fsm_clock_with_data_callback_t)(void* user_data);
typedef void (*fsm_inspect_callback_t)(
    const char* inspect_json,
    void* user_data
);

typedef int32_t (*fsm_output_resolver_callback_t)(
    const char* state_path,
    const fsm_value_t* context,
    const fsm_value_t* event,
    char* out_json_buffer,
    size_t out_buffer_len,
    void* user_data
);

typedef void (*fsm_invoke_start_callback_t)(
    const char* invoke_id,
    const char* src_json,
    const fsm_value_t* input,
    const fsm_value_t* context,
    const fsm_value_t* event,
    void* user_data
);

typedef void (*fsm_invoke_stop_callback_t)(
    const char* invoke_id,
    void* user_data
);

typedef struct SimulatedClock SimulatedClock;

// Registration
int fsm_register_action(
    MachineContext* ctx,
    const char* action_name,
    fsm_action_callback_t callback,
    void* user_data
);

int fsm_register_action_with_params(
    MachineContext* ctx,
    const char* action_name,
    fsm_action_with_params_callback_t callback,
    void* user_data
);


int fsm_register_guard(
    MachineContext* ctx,
    const char* guard_name,
    fsm_guard_callback_t callback,
    void* user_data
);

int fsm_register_delay(
    MachineContext* ctx,
    const char* delay_name,
    fsm_delay_callback_t callback,
    void* user_data
);

int fsm_register_guard_with_params(
    MachineContext* ctx,
    const char* guard_name,
    fsm_guard_with_params_callback_t callback,
    void* user_data
);

int fsm_register_clock(
    MachineContext* ctx,
    fsm_clock_callback_t callback
);

int fsm_register_clock_with_data(
    MachineContext* ctx,
    fsm_clock_with_data_callback_t callback,
    void* user_data
);

int fsm_register_inspector(
    MachineContext* ctx,
    fsm_inspect_callback_t callback,
    void* user_data
);

int fsm_register_output_resolver(
    MachineContext* ctx,
    const char* state_path,
    fsm_output_resolver_callback_t callback,
    void* user_data
);

int fsm_register_invoke_callbacks(
    MachineContext* ctx,
    fsm_invoke_start_callback_t on_start,
    fsm_invoke_stop_callback_t on_stop,
    void* user_data
);

bool fsm_get_next_deadline(
    const MachineContext* ptr,
    uint64_t* out_deadline
);

SimulatedClock* fsm_simulated_clock_create(uint64_t initial_ms);
void fsm_simulated_clock_free(SimulatedClock* clock);
int fsm_register_simulated_clock(MachineContext* ctx, SimulatedClock* clock);
uint64_t fsm_simulated_clock_now(const SimulatedClock* clock);
int fsm_simulated_clock_set(SimulatedClock* clock, uint64_t target_ms);
int fsm_simulated_clock_increment(SimulatedClock* clock, uint64_t dt_ms);

/**
 * @brief Process pending explicit timers (delayed after transitions).
 * 
 * Call this periodically or when waking from an event loop to advance delayed transitions.
 * 
 * @param ptr The machine context. The caller is responsible for ensuring the lifetime of this pointer exceeds the duration of this call.
 * @return 0 on success, or -1 if the context pointer is null.
 */
int fsm_tick(MachineContext* ptr);

/**
 * @brief Serialize current machine snapshot (state value, context, history, status) as JSON.
 *
 * Buffer-safe sizing:
 * - If buffer is NULL or buffer_len is 0, returns the required buffer capacity in bytes (including null terminator).
 * - If buffer_len is insufficient to hold the null-terminated string, returns -2 without writing to buffer.
 * - If buffer is valid and buffer_len is sufficient, writes null-terminated JSON and returns length written.
 *
 * @param ctx The machine context.
 * @param buffer Output char buffer (or NULL for size query).
 * @param buffer_len Capacity of buffer in bytes.
 * @return Number of characters written on success (>= 0), required buffer size on query (buffer == NULL or buffer_len == 0),
 *         or negative error code (-1: NULL ctx, -2: buffer too small, -3: integer overflow).
 */
int32_t fsm_get_snapshot_json(const MachineContext* ctx, char* buffer, size_t buffer_len);

/**
 * @brief Restore machine state from a serialized JSON snapshot.
 *
 * Updates current state value, context, history table, and machine status from the snapshot,
 * and revives active timer deadlines.
 *
 * @param ctx The machine context.
 * @param json_str Null-terminated JSON snapshot string.
 * @return 0 on success, or negative error code (-1: NULL ctx, -2: NULL json_str, -3: invalid UTF-8, -4: restore failure).
 */
int32_t fsm_restore_snapshot_json(MachineContext* ctx, const char* json_str);

/**
 * @brief Serialize the machine completion output as JSON.
 *
 * Buffer-safe sizing:
 * - Returns required buffer size (including null terminator) if buffer is NULL or buffer_len is 0.
 * - Returns -1 if ctx is NULL.
 * - Returns -2 if buffer_len is insufficient.
 * - Returns -3 on integer overflow.
 * - Returns -4 if machine has not reached a final state or no output is defined.
 * - Returns number of characters written on success (>= 0).
 */
int32_t fsm_get_output_json(const MachineContext* ctx, char* buffer, size_t buffer_len);

/**
 * @brief Serialize the output of the currently active final leaf state as JSON.
 *
 * Useful inside a root output resolver (`state_path == ""`) to inspect the output produced
 * by the active final child state.
 *
 * Buffer-safe sizing:
 * - Returns required buffer size (including null terminator) if buffer is NULL or buffer_len is 0.
 * - Returns -1 if ctx is NULL.
 * - Returns -2 if buffer_len is insufficient.
 * - Returns -3 on integer overflow.
 * - Returns -4 if no active final leaf state has output defined.
 * - Returns number of characters written on success (>= 0).
 */
int32_t fsm_get_final_leaf_output_json(const MachineContext* ctx, char* buffer, size_t buffer_len);

/**
 * @brief Compute the pure initial transition without starting or mutating the machine.
 *
 * Serializes `{"value": ..., "context": ..., "actions": [...]}` as JSON.
 *
 * @param ctx The machine context.
 * @param input_json Optional JSON input string (or NULL to use default input).
 * @param buffer Output buffer (or NULL for size query).
 * @param buffer_len Capacity of buffer in bytes.
 * @return Number of characters written on success (>= 0), required buffer size on query,
 *         or negative error code (-1: NULL ctx, -2: buffer too small, -3: integer overflow, -4: invalid input JSON).
 */
int32_t fsm_initial_transition_json(
    const MachineContext* ctx,
    const char* input_json,
    char* buffer,
    size_t buffer_len
);

/**
 * @brief Compute a pure transition from a state value or snapshot and event without mutating the machine.
 *
 * Serializes `{"value": ..., "context": ..., "actions": [...], "historyValue": {...}}` as JSON.
 *
 * @param ctx The machine context.
 * @param state_or_snapshot_json State value (e.g. `"green"` or `{"a":"b"}`) or snapshot JSON.
 * @param event_json Event name or JSON event object (`{"type":"TIMER"}`).
 * @param buffer Output buffer (or NULL for size query).
 * @param buffer_len Capacity of buffer in bytes.
 * @return Number of characters written on success (>= 0), required buffer size on query,
 *         or negative error code (-1: NULL ctx/args, -2: buffer too small, -3: integer overflow, -4: invalid state/event JSON).
 */
int32_t fsm_transition_pure_json(
    const MachineContext* ctx,
    const char* state_or_snapshot_json,
    const char* event_json,
    char* buffer,
    size_t buffer_len
);

/**
 * @brief Serialize aggregated metadata for all currently active states as a JSON object.
 *
 * Buffer-safe sizing:
 * - Returns required buffer size (including null terminator) if buffer is NULL or buffer_len is 0.
 * - Returns -1 if ctx is NULL.
 * - Returns -2 if buffer_len is insufficient.
 * - Returns -3 on integer overflow.
 * - Returns number of characters written on success (>= 0).
 */
int32_t fsm_get_meta_json(const MachineContext* ctx, char* buffer, size_t buffer_len);

/**
 * @brief Serialize metadata defined on a specific state node as JSON.
 *
 * @param ctx The machine context.
 * @param state_path Dot-delimited path ("a.b"), state ID ("#myState"), machine ID, or "" for root machine meta.
 * @param buffer Output buffer (or NULL for size query).
 * @param buffer_len Capacity of buffer in bytes.
 * @return Number of characters written on success (>= 0), required buffer size on query,
 *         or negative error code (-1: NULL ctx, -2: NULL state_path/UTF-8 error, -3: buffer too small,
 *         -4: state not found, -5: no meta defined on specified state).
 */
int32_t fsm_get_state_meta_json(const MachineContext* ctx, const char* state_path, char* buffer, size_t buffer_len);

#ifdef __cplusplus
}
#endif

#endif // LAVI_H
