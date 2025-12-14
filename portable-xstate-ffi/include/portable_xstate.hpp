#ifndef PORTABLE_XSTATE_HPP
#define PORTABLE_XSTATE_HPP

#include "portable_xstate.h"
#include <string>
#include <stdexcept>

/**
 * @brief C++ Wrapper for the Portable XState machine.
 * 
 * Handles memory management (RAII) and provides a cleaner C++ API.
 */
class PortableXState {
private:
    MachineContext* machine;

public:
    /**
     * @brief Construct a new Portable X State object
     * 
     * @param json_config The JSON configuration string for the state machine.
     * @throws std::runtime_error If the machine fails to initialize.
     */
    PortableXState(const char* json_config) {
        machine = fsm_create(json_config);
        if (!machine) {
            throw std::runtime_error("Failed to create FSM machine from provided JSON.");
        }
    }

    /**
     * @brief Destroy the Portable X State object
     * 
     * Automatically frees the underlying Rust memory.
     */
    ~PortableXState() {
        if (machine) {
            fsm_free(machine);
            machine = nullptr;
        }
    }

    // Disable copying to prevent double-free issues
    PortableXState(const PortableXState&) = delete;
    PortableXState& operator=(const PortableXState&) = delete;

    // Allow moving
    PortableXState(PortableXState&& other) noexcept : machine(other.machine) {
        other.machine = nullptr;
    }
    PortableXState& operator=(PortableXState&& other) noexcept {
        if (this != &other) {
            if (machine) fsm_free(machine);
            machine = other.machine;
            other.machine = nullptr;
        }
        return *this;
    }

    /**
     * @brief Get the current state as a string.
     * 
     * @return std::string The current state ID.
     */
    std::string getState() const {
        char buf[64];
        if (fsm_get_state(machine, buf, sizeof(buf)) == 0) {
            return std::string(buf);
        }
        return "error";
    }

    /**
     * @brief Send an event to the state machine.
     * 
     * @param event The event name to send.
     */
    void send(const char* event) {
        fsm_send_event(machine, event);
    }
};

#endif // PORTABLE_XSTATE_HPP
