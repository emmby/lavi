#ifndef PORTABLE_XSTATE_H
#define PORTABLE_XSTATE_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MachineContext MachineContext;

MachineContext* fsm_create(const char* json_config);
void fsm_free(MachineContext* ptr);
int fsm_get_state(MachineContext* ptr, char* buf, int len);
int fsm_send_event(MachineContext* ptr, const char* event_name);

#ifdef __cplusplus
}
#endif

#endif // PORTABLE_XSTATE_H
