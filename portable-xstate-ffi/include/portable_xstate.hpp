#ifndef PORTABLE_XSTATE_HPP
#define PORTABLE_XSTATE_HPP

#include "portable_xstate.h"
#include <string>
#include <stdexcept>
#include <functional>
#include <map>
#include <vector>

namespace xstate {

class Value {
    const fsm_value_t* ptr;
public:
    explicit Value(const fsm_value_t* p) : ptr(p) {}
    
    bool getString(std::string& out) const {
        if (!ptr || !fsm_value_is_string(ptr)) return false;
        size_t req_len = fsm_value_get_string(ptr, nullptr, 0);
        if (req_len == 0 && fsm_value_get_string(ptr, nullptr, 0) == 0) { // Check if truly empty
            out = "";
            return true;
        }
        std::vector<char> buf(req_len + 1);
        size_t copied = fsm_value_get_string(ptr, buf.data(), buf.size());
        if (copied > 0) {
            out.assign(buf.data(), copied);
            return true;
        }
        return false;
    }
    bool getNumber(double& out) const {
        if (!ptr || !fsm_value_is_number(ptr)) return false;
        out = fsm_value_get_number(ptr);
        return true;
    }
    bool getBool(bool& out) const {
        if (!ptr || !fsm_value_is_bool(ptr)) return false;
        out = fsm_value_get_bool(ptr);
        return true;
    }
    
    bool isNull() const {
        return ptr == nullptr;
    }
    Value getProperty(const std::string& key) const {
        if (!ptr) return Value(nullptr);
        return Value(fsm_value_get_property(ptr, key.c_str()));
    }
};

using ActionCallback = std::function<void(const Value& context, const Value& event)>;
using GuardCallback = std::function<bool(const Value& context, const Value& event)>;

/**
 * @brief C++ Wrapper for the Portable XState machine.
 * 
 * Handles memory management (RAII) and provides a cleaner C++ API.
 */
class PortableXState {
private:
    MachineContext* machine;
    const char* m_json;
    std::map<std::string, ActionCallback> m_callbacks;
    std::map<std::string, GuardCallback> m_guards;
    
    static void c_callback_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, void* user_data) {
        if (!user_data) return;
        auto* instance = static_cast<PortableXState*>(user_data);
        auto it = instance->m_callbacks.find(name ? name : "");
        if (it != instance->m_callbacks.end() && it->second) {
            it->second(Value(ctx), Value(evt));
        }
    }

    static bool c_guard_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, void* user_data) {
        if (!user_data) return false;
        auto* instance = static_cast<PortableXState*>(user_data);
        auto it = instance->m_guards.find(name ? name : "");
        if (it != instance->m_guards.end() && it->second) {
            return it->second(Value(ctx), Value(evt));
        }
        return false;
    }

public:
    /**
     * @brief Construct a new Portable X State object
     * 
     * @param json_config The JSON configuration string for the state machine.
     *                    MEMORY WARNING: This object deliberately stores only the pointer (const char*)
     *                    to avoid dynamic memory allocation. The caller MUST ensure that the memory 
     *                    pointed to by json_config remains valid and unchanged until `.start()` is called.
     * @throws std::runtime_error If the machine fails to initialize.
     */
    PortableXState(const char* json_config) : machine(nullptr), m_json(json_config) {
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
    PortableXState(PortableXState&& other) noexcept : machine(other.machine), m_json(other.m_json), m_callbacks(std::move(other.m_callbacks)), m_guards(std::move(other.m_guards)) {
        other.machine = nullptr;
    }
    PortableXState& operator=(PortableXState&& other) noexcept {
        if (this != &other) {
            if (machine) fsm_free(machine);
            machine = other.machine;
            m_json = other.m_json;
            other.machine = nullptr;
            m_callbacks = std::move(other.m_callbacks);
            m_guards = std::move(other.m_guards);
            // Re-register callbacks with this instance as user_data
            if (machine) {
                for (const auto& pair : m_callbacks) {
                    fsm_register_action(machine, pair.first.c_str(), c_callback_thunk, this);
                }
                for (const auto& pair : m_guards) {
                    fsm_register_guard(machine, pair.first.c_str(), c_guard_thunk, this);
                }
            }
        }
        return *this;
    }

    /**
     * @brief Starts the state machine and processes initial actions.
     */
    void start() {
        if (machine) return;
        machine = fsm_create(m_json);
        if (!machine) {
            throw std::runtime_error("Failed to create FSM machine from provided JSON.");
        }
        for (const auto& pair : m_callbacks) {
            fsm_register_action(machine, pair.first.c_str(), c_callback_thunk, this);
        }
        for (const auto& pair : m_guards) {
            fsm_register_guard(machine, pair.first.c_str(), c_guard_thunk, this);
        }
        if (fsm_start(machine) != 0) {
            throw std::runtime_error("Failed to start FSM machine.");
        }
    }

    /**
     * @brief Get the current state as a string.
     * 
     * @return std::string The current state ID.
     */
    std::string getState() const {
        if (!machine) return "uninitialized";
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
        if (machine) {
            fsm_send_event(machine, event);
        }
    }

    /**
     * @brief Register a callback for an XState action.
     * 
     * @param name The name of the action.
     * @param callback The std::function callback.
     */
    void registerAction(const std::string& name, ActionCallback callback) {
        m_callbacks[name] = std::move(callback);
        if (machine) {
            fsm_register_action(machine, name.c_str(), c_callback_thunk, this);
        }
    }

    /**
     * @brief Register a callback for an XState guard.
     * 
     * @param name The name of the guard.
     * @param callback The std::function callback tracking boolean conditions.
     */
    void registerGuard(const std::string& name, GuardCallback callback) {
        m_guards[name] = std::move(callback);
        if (machine) {
            fsm_register_guard(machine, name.c_str(), c_guard_thunk, this);
        }
    }
};

} // namespace xstate

#endif // PORTABLE_XSTATE_HPP
