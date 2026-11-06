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
using ClockCallback = std::function<uint64_t()>;

/**
 * @brief Virtual clock modeled on XState 5's SimulatedClock.
 *
 * Owns a virtual millisecond timestamp and advances registered state machines
 * chronologically through intermediate `after` timer deadlines via `set()` and `increment()`.
 */
class SimulatedClock {
private:
    ::SimulatedClock* m_ptr;

public:
    explicit SimulatedClock(uint64_t initial_ms = 0)
        : m_ptr(fsm_simulated_clock_create(initial_ms)) {
        if (!m_ptr) {
            throw std::runtime_error("Failed to create SimulatedClock.");
        }
    }

    ~SimulatedClock() {
        if (m_ptr) {
            fsm_simulated_clock_free(m_ptr);
            m_ptr = nullptr;
        }
    }

    SimulatedClock(const SimulatedClock&) = delete;
    SimulatedClock& operator=(const SimulatedClock&) = delete;

    SimulatedClock(SimulatedClock&& other) noexcept : m_ptr(other.m_ptr) {
        other.m_ptr = nullptr;
    }

    SimulatedClock& operator=(SimulatedClock&& other) noexcept {
        if (this != &other) {
            if (m_ptr) {
                fsm_simulated_clock_free(m_ptr);
            }
            m_ptr = other.m_ptr;
            other.m_ptr = nullptr;
        }
        return *this;
    }

    uint64_t now() const noexcept {
        return m_ptr ? fsm_simulated_clock_now(m_ptr) : 0;
    }

    void set(uint64_t target_ms) {
        if (m_ptr) {
            fsm_simulated_clock_set(m_ptr, target_ms);
        }
    }

    void increment(uint64_t dt_ms) {
        if (m_ptr) {
            fsm_simulated_clock_increment(m_ptr, dt_ms);
        }
    }

    ::SimulatedClock* raw() const noexcept {
        return m_ptr;
    }
};

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
    fsm_clock_callback_t m_clockCallback = nullptr;
    ClockCallback m_clockFn;
    ::SimulatedClock* m_simClock = nullptr;
    
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

    static uint64_t c_clock_thunk(void* user_data) {
        if (!user_data) return 0;
        auto* instance = static_cast<PortableXState*>(user_data);
        if (instance->m_clockFn) {
            return instance->m_clockFn();
        }
        return 0;
    }

    void rebind_this_pointers() {
        if (!machine) return;
        for (const auto& pair : m_callbacks) {
            fsm_register_action(machine, pair.first.c_str(), c_callback_thunk, this);
        }
        for (const auto& pair : m_guards) {
            fsm_register_guard(machine, pair.first.c_str(), c_guard_thunk, this);
        }
        if (!m_simClock && m_clockFn) {
            fsm_register_clock_with_data(machine, c_clock_thunk, this);
        }
    }

    void bind_registered_handlers() {
        if (!machine) return;
        rebind_this_pointers();
        if (m_simClock) {
            fsm_register_simulated_clock(machine, m_simClock);
        } else if (!m_clockFn && m_clockCallback) {
            fsm_register_clock(machine, m_clockCallback);
        }
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

    PortableXState(const char* json_config, SimulatedClock& clock)
        : machine(nullptr), m_json(json_config), m_simClock(clock.raw()) {
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
    PortableXState(PortableXState&& other) noexcept
        : machine(other.machine),
          m_json(other.m_json),
          m_callbacks(std::move(other.m_callbacks)),
          m_guards(std::move(other.m_guards)),
          m_clockCallback(other.m_clockCallback),
          m_clockFn(std::move(other.m_clockFn)),
          m_simClock(other.m_simClock) {
        other.machine = nullptr;
        other.m_clockCallback = nullptr;
        other.m_simClock = nullptr;
        rebind_this_pointers();
    }
    PortableXState& operator=(PortableXState&& other) noexcept {
        if (this != &other) {
            if (machine) fsm_free(machine);
            machine = other.machine;
            m_json = other.m_json;
            other.machine = nullptr;
            m_callbacks = std::move(other.m_callbacks);
            m_guards = std::move(other.m_guards);
            m_clockCallback = other.m_clockCallback;
            other.m_clockCallback = nullptr;
            m_clockFn = std::move(other.m_clockFn);
            m_simClock = other.m_simClock;
            other.m_simClock = nullptr;
            rebind_this_pointers();
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
        bind_registered_handlers();
        if (fsm_start(machine) != 0) {
            throw std::runtime_error("Failed to start FSM machine.");
        }
    }

    /**
     * @brief Check whether the given dot-delimited state path is currently active.
     * 
     * @param state_path Dot-delimited state path (e.g. "green", "Display", "Display.Power.On").
     * @return true if the state path is active, false otherwise.
     */
    bool matches(const char* state_path) const noexcept {
        if (!machine || !state_path) return false;
        return fsm_matches(machine, state_path);
    }

    /**
     * @brief Check whether the given dot-delimited state path is currently active.
     * 
     * @param state_path Dot-delimited state path (e.g. "green", "Display", "Display.Power.On").
     * @return true if the state path is active, false otherwise.
     */
    bool matches(const std::string& state_path) const noexcept {
        return matches(state_path.c_str());
    }

    /**
     * @brief Check whether the given event would select an enabled transition without mutating state.
     *
     * @param event The event name to check.
     * @return true if an enabled transition exists for the event, false otherwise.
     */
    bool can(const char* event) const noexcept {
        if (!machine || !event) return false;
        return fsm_can(machine, event) == 1;
    }

    /**
     * @brief Check whether the given event would select an enabled transition without mutating state.
     *
     * @param event The event name to check.
     * @return true if an enabled transition exists for the event, false otherwise.
     */
    bool can(const std::string& event) const noexcept {
        return can(event.c_str());
    }

    /**
     * @brief Check whether any currently active state node in the hierarchy has the given tag.
     *
     * @param tag The tag string to check.
     * @return true if the active state configuration has the tag, false otherwise.
     */
    bool hasTag(const char* tag) const noexcept {
        if (!machine || !tag) return false;
        return fsm_has_tag(machine, tag) == 1;
    }

    /**
     * @brief Check whether any currently active state node in the hierarchy has the given tag.
     *
     * @param tag The tag string to check.
     * @return true if the active state configuration has the tag, false otherwise.
     */
    bool hasTag(const std::string& tag) const noexcept {
        return hasTag(tag.c_str());
    }

    /**
     * @brief Check whether the state machine has reached a final/done state.
     *
     * @return true if the machine is in a final/done state, false otherwise.
     */
    bool isDone() const noexcept {
        if (!machine) return false;
        return fsm_is_done(machine) == 1;
    }

    /**
     * @brief Stop the state machine. A stopped machine ignores subsequent events and timers.
     */
    void stop() noexcept {
        if (machine) {
            fsm_stop(machine);
        }
    }

    /**
     * @brief Serialize current machine state to a JSON snapshot string.
     *
     * @return Serialized JSON snapshot, or empty string if machine is uninitialized or serialization fails.
     */
    std::string get_snapshot() const {
        if (!machine) return "";
        int32_t req = fsm_get_snapshot_json(machine, nullptr, 0);
        if (req <= 1) return "";
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_snapshot_json(machine, buf.data(), buf.size());
        if (written < 0) return "";
        return std::string(buf.data(), static_cast<size_t>(written));
    }

    /**
     * @brief Restore machine state from a serialized JSON snapshot string.
     *
     * If the machine has not yet been started, initializes the underlying FSM and binds
     * registered handlers before restoring the snapshot.
     *
     * @param snapshot_json Serialized JSON snapshot string.
     * @return true if snapshot was successfully restored, false otherwise.
     */
    bool restore_snapshot(const char* snapshot_json) {
        if (!snapshot_json) return false;
        if (!machine) {
            machine = fsm_create(m_json);
            if (!machine) {
                return false;
            }
            bind_registered_handlers();
        }
        return fsm_restore_snapshot_json(machine, snapshot_json) == 0;
    }

    bool restore_snapshot(const std::string& snapshot_json) {
        return restore_snapshot(snapshot_json.c_str());
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

    /**
     * @brief Register a callback for the system clock (used for delayed transitions).
     * 
     * @param callback The callback that returns the current absolute time in milliseconds.
     */
    void registerClock(fsm_clock_callback_t callback) {
        m_clockCallback = callback;
        m_clockFn = nullptr;
        m_simClock = nullptr;
        if (machine) {
            fsm_register_clock(machine, callback);
        }
    }

    /**
     * @brief Register a C++ callable (lambda/std::function) as the external clock.
     */
    void registerClock(ClockCallback callback) {
        m_clockFn = std::move(callback);
        m_clockCallback = nullptr;
        m_simClock = nullptr;
        if (machine) {
            fsm_register_clock_with_data(machine, c_clock_thunk, this);
        }
    }

    /**
     * @brief Bind a SimulatedClock to this state machine.
     */
    void registerClock(SimulatedClock& clock) {
        m_simClock = clock.raw();
        m_clockCallback = nullptr;
        m_clockFn = nullptr;
        if (machine && m_simClock) {
            fsm_register_simulated_clock(machine, m_simClock);
        }
    }

    /**
     * @brief Query the earliest active `after` timer deadline in milliseconds.
     *
     * @param out_deadline Populated with the earliest deadline if an active timer exists.
     * @return true if an active timer deadline exists, false otherwise.
     */
    bool getNextDeadline(uint64_t& out_deadline) const {
        if (!machine) return false;
        return fsm_get_next_deadline(machine, &out_deadline);
    }

    /**
     * @brief Process pending explicit timers (delayed `after` transitions).
     * 
     * Call this inside your main event loop to ensure delayed timers trigger correctly.
     */
    void tick() {
        if (machine) {
            fsm_tick(machine);
        }
    }
};

} // namespace xstate

#endif // PORTABLE_XSTATE_HPP
