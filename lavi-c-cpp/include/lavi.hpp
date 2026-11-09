#ifndef LAVI_HPP
#define LAVI_HPP

#include "lavi.h"
#include <string>
#include <stdexcept>
#include <functional>
#include <type_traits>
#include <map>
#include <vector>

namespace lavi {

class Value {
    const fsm_value_t* ptr;
public:
    explicit Value(const fsm_value_t* p) : ptr(p) {}
    
    const fsm_value_t* raw() const noexcept { return ptr; }

    bool isNull() const noexcept { return ptr == nullptr; }
    bool is_null() const noexcept { return isNull(); }

    bool isObject() const noexcept { return ptr && fsm_value_is_object(ptr); }
    bool is_object() const noexcept { return isObject(); }

    bool isString() const noexcept { return ptr && fsm_value_is_string(ptr); }
    bool is_string() const noexcept { return isString(); }

    bool isNumber() const noexcept { return ptr && fsm_value_is_number(ptr); }
    bool is_number() const noexcept { return isNumber(); }

    bool isBool() const noexcept { return ptr && fsm_value_is_bool(ptr); }
    bool is_bool() const noexcept { return isBool(); }
    
    bool getString(std::string& out) const {
        if (!ptr || !fsm_value_is_string(ptr)) return false;
        size_t req_len = fsm_value_get_string(ptr, nullptr, 0);
        if (req_len == 0) {
            out.clear();
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
    bool get_string(std::string& out) const { return getString(out); }

    bool getNumber(double& out) const noexcept {
        if (!ptr || !fsm_value_is_number(ptr)) return false;
        out = fsm_value_get_number(ptr);
        return true;
    }
    bool get_number(double& out) const noexcept { return getNumber(out); }

    bool getBool(bool& out) const noexcept {
        if (!ptr || !fsm_value_is_bool(ptr)) return false;
        out = fsm_value_get_bool(ptr);
        return true;
    }
    bool get_bool(bool& out) const noexcept { return getBool(out); }

    Value getProperty(const char* key) const {
        if (!ptr || !key) return Value(nullptr);
        return Value(fsm_value_get_property(ptr, key));
    }
    Value getProperty(const std::string& key) const {
        return getProperty(key.c_str());
    }
    Value get_property(const char* key) const { return getProperty(key); }
    Value get_property(const std::string& key) const { return getProperty(key); }
};


using ActionCallback = std::function<void(const Value& context, const Value& event)>;
using ActionWithParamsCallback = std::function<void(const Value& context, const Value& event, const Value& params)>;
using GuardCallback = std::function<bool(const Value& context, const Value& event)>;
using DelayCallback = std::function<uint64_t(const Value& context, const Value& event)>;
using GuardWithParamsCallback = std::function<bool(const Value& context, const Value& event, const Value& params)>;
using ClockCallback = std::function<uint64_t()>;
using InspectCallback = std::function<void(const std::string& inspect_json)>;

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
#if defined(__cpp_exceptions) || defined(__EXCEPTIONS) || defined(_CPPUNWIND)
            throw std::runtime_error("Failed to create SimulatedClock.");
#else
            return;
#endif
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
 * @brief C++ Wrapper for the Lavi state machine.
 * 
 * Handles memory management (RAII) and provides a cleaner C++ API.
 */
class Machine {
private:
    MachineContext* machine;
    const char* m_json;
    const char* m_input;
    std::string m_json_str;
    std::string m_input_str;

    std::map<std::string, ActionCallback> m_callbacks;
    std::map<std::string, ActionWithParamsCallback> m_actionsWithParams;
    std::map<std::string, GuardCallback> m_guards;
    std::map<std::string, DelayCallback> m_delays;
    std::map<std::string, GuardWithParamsCallback> m_guardsWithParams;

    struct RawAction { fsm_action_callback_t cb; void* user_data; };
    struct RawActionWithParams { fsm_action_with_params_callback_t cb; void* user_data; };
    struct RawGuard { fsm_guard_callback_t cb; void* user_data; };
    struct RawDelay { fsm_delay_callback_t cb; void* user_data; };
    struct RawGuardWithParams { fsm_guard_with_params_callback_t cb; void* user_data; };

    std::map<std::string, RawAction> m_rawActions;
    std::map<std::string, RawActionWithParams> m_rawActionsWithParams;
    std::map<std::string, RawGuard> m_rawGuards;
    std::map<std::string, RawDelay> m_rawDelays;
    std::map<std::string, RawGuardWithParams> m_rawGuardsWithParams;

    fsm_clock_callback_t m_clockCallback = nullptr;
    ClockCallback m_clockFn;
    ::SimulatedClock* m_simClock = nullptr;
    fsm_inspect_callback_t m_rawInspectCallback = nullptr;
    void* m_rawInspectUserData = nullptr;
    InspectCallback m_inspectFn;

    const char* get_json_ptr() const noexcept {
        if (m_json) return m_json;
        if (!m_json_str.empty()) return m_json_str.c_str();
        return "";
    }

    const char* get_input_ptr() const noexcept {
        if (m_input) return m_input;
        if (!m_input_str.empty()) return m_input_str.c_str();
        return nullptr;
    }
    
    static void c_callback_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, void* user_data) {
        if (!user_data) return;
        auto* instance = static_cast<Machine*>(user_data);
        auto it = instance->m_callbacks.find(name ? name : "");
        if (it != instance->m_callbacks.end() && it->second) {
            it->second(Value(ctx), Value(evt));
        }
    }

    static void c_action_with_params_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, const fsm_value_t* params, void* user_data) {
        if (!user_data) return;
        auto* instance = static_cast<Machine*>(user_data);
        auto it = instance->m_actionsWithParams.find(name ? name : "");
        if (it != instance->m_actionsWithParams.end() && it->second) {
            it->second(Value(ctx), Value(evt), Value(params));
        }
    }

    static bool c_guard_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, void* user_data) {
        if (!user_data) return false;
        auto* instance = static_cast<Machine*>(user_data);
        auto it = instance->m_guards.find(name ? name : "");
        if (it != instance->m_guards.end() && it->second) {
            return it->second(Value(ctx), Value(evt));
        }
        return false;
    }

    static uint64_t c_delay_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, void* user_data) {
        if (!user_data) return 0;
        auto* instance = static_cast<Machine*>(user_data);
        auto it = instance->m_delays.find(name ? name : "");
        if (it != instance->m_delays.end() && it->second) {
            return it->second(Value(ctx), Value(evt));
        }
        return 0;
    }

    static bool c_guard_with_params_thunk(const char* name, const fsm_value_t* ctx, const fsm_value_t* evt, const fsm_value_t* params, void* user_data) {
        if (!user_data) return false;
        auto* instance = static_cast<Machine*>(user_data);
        auto it = instance->m_guardsWithParams.find(name ? name : "");
        if (it != instance->m_guardsWithParams.end() && it->second) {
            return it->second(Value(ctx), Value(evt), Value(params));
        }
        return false;
    }

    static uint64_t c_clock_thunk(void* user_data) {
        if (!user_data) return 0;
        auto* instance = static_cast<Machine*>(user_data);
        if (instance->m_clockFn) {
            return instance->m_clockFn();
        }
        return 0;
    }

    static void c_inspect_thunk(const char* inspect_json, void* user_data) {
        if (!user_data || !inspect_json) return;
        auto* instance = static_cast<Machine*>(user_data);
        if (instance->m_inspectFn) {
            instance->m_inspectFn(std::string(inspect_json));
        }
    }

    void rebind_this_pointers() {
        if (!machine) return;
        for (const auto& pair : m_callbacks) {
            fsm_register_action(machine, pair.first.c_str(), c_callback_thunk, this);
        }
        for (const auto& pair : m_actionsWithParams) {
            fsm_register_action_with_params(machine, pair.first.c_str(), c_action_with_params_thunk, this);
        }
        for (const auto& pair : m_guards) {
            fsm_register_guard(machine, pair.first.c_str(), c_guard_thunk, this);
        }
        for (const auto& pair : m_delays) {
            fsm_register_delay(machine, pair.first.c_str(), c_delay_thunk, this);
        }
        for (const auto& pair : m_guardsWithParams) {
            fsm_register_guard_with_params(machine, pair.first.c_str(), c_guard_with_params_thunk, this);
        }
        if (!m_simClock && m_clockFn) {
            fsm_register_clock_with_data(machine, c_clock_thunk, this);
        }
        if (m_inspectFn) {
            fsm_register_inspector(machine, c_inspect_thunk, this);
        }
    }

    void bind_registered_handlers() {
        if (!machine) return;
        rebind_this_pointers();
        for (const auto& pair : m_rawActions) {
            fsm_register_action(machine, pair.first.c_str(), pair.second.cb, pair.second.user_data);
        }
        for (const auto& pair : m_rawActionsWithParams) {
            fsm_register_action_with_params(machine, pair.first.c_str(), pair.second.cb, pair.second.user_data);
        }
        for (const auto& pair : m_rawGuards) {
            fsm_register_guard(machine, pair.first.c_str(), pair.second.cb, pair.second.user_data);
        }
        for (const auto& pair : m_rawDelays) {
            fsm_register_delay(machine, pair.first.c_str(), pair.second.cb, pair.second.user_data);
        }
        for (const auto& pair : m_rawGuardsWithParams) {
            fsm_register_guard_with_params(machine, pair.first.c_str(), pair.second.cb, pair.second.user_data);
        }
        if (m_simClock) {
            fsm_register_simulated_clock(machine, m_simClock);
        } else if (!m_clockFn && m_clockCallback) {
            fsm_register_clock(machine, m_clockCallback);
        }
        if (!m_inspectFn && m_rawInspectCallback) {
            fsm_register_inspector(machine, m_rawInspectCallback, m_rawInspectUserData);
        }
    }

public:
    /**
     * @brief Construct a new Lavi Machine object from raw JSON string pointers (zero-copy).
     */
    Machine(const char* json_config, const char* input_json = nullptr)
        : machine(nullptr), m_json(json_config), m_input(input_json) {}

    Machine(const char* json_config, SimulatedClock& clock)
        : machine(nullptr), m_json(json_config), m_input(nullptr), m_simClock(clock.raw()) {}

    Machine(const char* json_config, const char* input_json, SimulatedClock& clock)
        : machine(nullptr), m_json(json_config), m_input(input_json), m_simClock(clock.raw()) {}

    /**
     * @brief Construct a new Lavi Machine object from owned std::string buffers.
     */
    Machine(const std::string& json_config)
        : machine(nullptr), m_json(nullptr), m_input(nullptr), m_json_str(json_config) {}

    Machine(const std::string& json_config, const std::string& input_json)
        : machine(nullptr), m_json(nullptr), m_input(nullptr), m_json_str(json_config), m_input_str(input_json) {}

    Machine(const std::string& json_config, SimulatedClock& clock)
        : machine(nullptr), m_json(nullptr), m_input(nullptr), m_json_str(json_config), m_simClock(clock.raw()) {}

    Machine(const std::string& json_config, const std::string& input_json, SimulatedClock& clock)
        : machine(nullptr), m_json(nullptr), m_input(nullptr), m_json_str(json_config), m_input_str(input_json), m_simClock(clock.raw()) {}

    /**
     * @brief Destroy the Lavi Machine object and free underlying Rust context.
     */
    ~Machine() {
        if (machine) {
            fsm_free(machine);
            machine = nullptr;
        }
    }

    // Disable copying
    Machine(const Machine&) = delete;
    Machine& operator=(const Machine&) = delete;

    // Allow moving
    Machine(Machine&& other) noexcept
        : machine(other.machine),
          m_json(other.m_json),
          m_input(other.m_input),
          m_json_str(std::move(other.m_json_str)),
          m_input_str(std::move(other.m_input_str)),
          m_callbacks(std::move(other.m_callbacks)),
          m_actionsWithParams(std::move(other.m_actionsWithParams)),
          m_guards(std::move(other.m_guards)),
          m_delays(std::move(other.m_delays)),
          m_guardsWithParams(std::move(other.m_guardsWithParams)),
          m_rawActions(std::move(other.m_rawActions)),
          m_rawActionsWithParams(std::move(other.m_rawActionsWithParams)),
          m_rawGuards(std::move(other.m_rawGuards)),
          m_rawDelays(std::move(other.m_rawDelays)),
          m_rawGuardsWithParams(std::move(other.m_rawGuardsWithParams)),
          m_clockCallback(other.m_clockCallback),
          m_clockFn(std::move(other.m_clockFn)),
          m_simClock(other.m_simClock),
          m_rawInspectCallback(other.m_rawInspectCallback),
          m_rawInspectUserData(other.m_rawInspectUserData),
          m_inspectFn(std::move(other.m_inspectFn)) {
        other.machine = nullptr;
        other.m_json = nullptr;
        other.m_input = nullptr;
        other.m_clockCallback = nullptr;
        other.m_simClock = nullptr;
        other.m_rawInspectCallback = nullptr;
        other.m_rawInspectUserData = nullptr;
        rebind_this_pointers();
    }

    Machine& operator=(Machine&& other) noexcept {
        if (this != &other) {
            if (machine) fsm_free(machine);
            machine = other.machine;
            m_json = other.m_json;
            m_input = other.m_input;
            m_json_str = std::move(other.m_json_str);
            m_input_str = std::move(other.m_input_str);
            other.machine = nullptr;
            other.m_json = nullptr;
            other.m_input = nullptr;
            m_callbacks = std::move(other.m_callbacks);
            m_actionsWithParams = std::move(other.m_actionsWithParams);
            m_guards = std::move(other.m_guards);
            m_delays = std::move(other.m_delays);
            m_guardsWithParams = std::move(other.m_guardsWithParams);
            m_rawActions = std::move(other.m_rawActions);
            m_rawActionsWithParams = std::move(other.m_rawActionsWithParams);
            m_rawGuards = std::move(other.m_rawGuards);
            m_rawDelays = std::move(other.m_rawDelays);
            m_rawGuardsWithParams = std::move(other.m_rawGuardsWithParams);
            m_clockCallback = other.m_clockCallback;
            other.m_clockCallback = nullptr;
            m_clockFn = std::move(other.m_clockFn);
            m_simClock = other.m_simClock;
            other.m_simClock = nullptr;
            m_rawInspectCallback = other.m_rawInspectCallback;
            other.m_rawInspectCallback = nullptr;
            m_rawInspectUserData = other.m_rawInspectUserData;
            other.m_rawInspectUserData = nullptr;
            m_inspectFn = std::move(other.m_inspectFn);
            rebind_this_pointers();
        }
        return *this;
    }

    /**
     * @brief Starts the state machine and processes initial actions.
     */
    void start() {
        if (machine) return;
        const char* j = get_json_ptr();
        const char* inp = get_input_ptr();
        if (inp) {
            machine = fsm_create_with_input(j, inp);
        } else {
            machine = fsm_create(j);
        }
        if (!machine) {
#if defined(__cpp_exceptions) || defined(__EXCEPTIONS) || defined(_CPPUNWIND)
            throw std::runtime_error("Failed to create FSM machine from provided JSON.");
#else
            return;
#endif
        }
        bind_registered_handlers();
        if (fsm_start(machine) != 0) {
#if defined(__cpp_exceptions) || defined(__EXCEPTIONS) || defined(_CPPUNWIND)
            throw std::runtime_error("Failed to start FSM machine.");
#else
            return;
#endif
        }
    }

    /**
     * @brief Returns true if the underlying machine context pointer is non-null.
     */
    bool isValid() const noexcept {
        return machine != nullptr;
    }
    bool is_valid() const noexcept {
        return isValid();
    }

    /**
     * @brief Retrieve final machine output serialized as JSON.
     * @return Serialized JSON string of machine output, or empty string if not done.
     */
    std::string get_output_json() const {
        if (!machine) return "";
        int32_t req = fsm_get_output_json(machine, nullptr, 0);
        if (req <= 1) return "";
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_output_json(machine, buf.data(), buf.size());
        if (written < 0) return "";
        return std::string(buf.data(), static_cast<size_t>(written));
    }
    std::string getOutputJson() const { return get_output_json(); }

    /**
     * @brief Populate output string with final machine output JSON.
     * @param out Output string populated on success.
     * @return true if machine completed and has output; false otherwise.
     */
    bool get_output(std::string& out) const {
        if (!machine) return false;
        int32_t req = fsm_get_output_json(machine, nullptr, 0);
        if (req <= 0) return false;
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_output_json(machine, buf.data(), buf.size());
        if (written < 0) return false;
        out.assign(buf.data(), static_cast<size_t>(written));
        return true;
    }
    bool getOutput(std::string& out) const { return get_output(out); }

    /**
     * @brief Retrieve active states aggregated metadata as a JSON object string.
     */
    std::string get_meta_json() const {
        if (!machine) return "";
        int32_t req = fsm_get_meta_json(machine, nullptr, 0);
        if (req <= 1) return "";
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_meta_json(machine, buf.data(), buf.size());
        if (written < 0) return "";
        return std::string(buf.data(), static_cast<size_t>(written));
    }
    std::string getMetaJson() const { return get_meta_json(); }

    /**
     * @brief Retrieve metadata for a specific state path serialized as JSON.
     */
    std::string get_state_meta_json(const char* state_path) const {
        if (!machine || !state_path) return "";
        int32_t req = fsm_get_state_meta_json(machine, state_path, nullptr, 0);
        if (req <= 1) return "";
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_state_meta_json(machine, state_path, buf.data(), buf.size());
        if (written < 0) return "";
        return std::string(buf.data(), static_cast<size_t>(written));
    }
    std::string get_state_meta_json(const std::string& state_path) const {
        return get_state_meta_json(state_path.c_str());
    }
    std::string getStateMetaJson(const char* state_path) const {
        return get_state_meta_json(state_path);
    }
    std::string getStateMetaJson(const std::string& state_path) const {
        return get_state_meta_json(state_path.c_str());
    }

    /**
     * @brief Retrieve active state configuration serialized as JSON.
     */
    std::string get_state() const {
        if (!machine) return "";
        int32_t req = fsm_get_state(machine, nullptr, 0);
        if (req <= 1) return "";
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_state(machine, buf.data(), buf.size());
        if (written < 0) return "";
        return std::string(buf.data(), static_cast<size_t>(written));
    }
    std::string getState() const { return get_state(); }

    /**
     * @brief Check whether the given dot-delimited state path is currently active.
     */
    bool matches(const char* state_path) const noexcept {
        if (!machine || !state_path) return false;
        return fsm_matches(machine, state_path);
    }
    bool matches(const std::string& state_path) const noexcept {
        return matches(state_path.c_str());
    }
    bool is_in_state(const char* state_path) const noexcept { return matches(state_path); }
    bool is_in_state(const std::string& state_path) const noexcept { return matches(state_path.c_str()); }

    /**
     * @brief Check whether the given event would select an enabled transition without mutating state.
     */
    bool can(const char* event) const noexcept {
        if (!machine || !event) return false;
        return fsm_can(machine, event) == 1;
    }
    bool can(const std::string& event) const noexcept {
        return can(event.c_str());
    }

    /**
     * @brief Check whether any currently active state node in the hierarchy has the given tag.
     */
    bool hasTag(const char* tag) const noexcept {
        if (!machine || !tag) return false;
        return fsm_has_tag(machine, tag) == 1;
    }
    bool hasTag(const std::string& tag) const noexcept {
        return hasTag(tag.c_str());
    }
    bool has_tag(const char* tag) const noexcept { return hasTag(tag); }
    bool has_tag(const std::string& tag) const noexcept { return hasTag(tag.c_str()); }

    /**
     * @brief Check whether the state machine has reached a final/done state.
     */
    bool isDone() const noexcept {
        if (!machine) return false;
        return fsm_is_done(machine) == 1;
    }
    bool is_done() const noexcept { return isDone(); }

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
     */
    std::string getSnapshot() const {
        if (!machine) return "";
        int32_t req = fsm_get_snapshot_json(machine, nullptr, 0);
        if (req <= 1) return "";
        std::vector<char> buf(static_cast<size_t>(req));
        int32_t written = fsm_get_snapshot_json(machine, buf.data(), buf.size());
        if (written < 0) return "";
        return std::string(buf.data(), static_cast<size_t>(written));
    }
    std::string get_snapshot_json() const { return getSnapshot(); }

    /**
     * @brief Restore machine state from a serialized JSON snapshot string.
     */
    bool restoreSnapshot(const char* snapshot_json) {
        if (!snapshot_json) return false;
        bool created_here = (machine == nullptr);
        if (created_here) {
            const char* j = get_json_ptr();
            const char* inp = get_input_ptr();
            if (inp) {
                machine = fsm_create_with_input(j, inp);
            } else {
                machine = fsm_create(j);
            }
            if (!machine) {
                return false;
            }
            bind_registered_handlers();
        }
        if (fsm_restore_snapshot_json(machine, snapshot_json) != 0) {
            if (created_here) {
                fsm_free(machine);
                machine = nullptr;
            }
            return false;
        }
        return true;
    }
    bool restoreSnapshot(const std::string& snapshot_json) {
        return restoreSnapshot(snapshot_json.c_str());
    }
    bool restore_snapshot_json(const char* snapshot_json) {
        return restoreSnapshot(snapshot_json);
    }
    bool restore_snapshot_json(const std::string& snapshot_json) {
        return restoreSnapshot(snapshot_json.c_str());
    }

    /**
     * @brief Send an event to the state machine.
     */
    void send(const char* event) {
        if (machine) {
            fsm_send_event(machine, event);
        }
    }
    void send(const std::string& event) { send(event.c_str()); }

    /**
     * @brief Send a structured JSON event to the state machine.
     */
    void send_json(const char* json_event) {
        if (machine && json_event) {
            fsm_send_json_event(machine, json_event);
        }
    }
    void send_json(const std::string& json_event) { send_json(json_event.c_str()); }
    void sendJson(const char* json_event) { send_json(json_event); }
    void sendJson(const std::string& json_event) { send_json(json_event.c_str()); }

    /**
     * @brief Register a callback for an XState action (lambda overload).
     */
    void registerAction(const std::string& name, ActionCallback callback) {
        m_callbacks[name] = std::move(callback);
        m_rawActions.erase(name);
        if (machine) {
            fsm_register_action(machine, name.c_str(), c_callback_thunk, this);
        }
    }

    /**
     * @brief Register a zero-allocation C function pointer callback for an XState action.
     */
    void registerAction(const std::string& name, fsm_action_callback_t callback, void* user_data = nullptr) {
        m_rawActions[name] = {callback, user_data};
        m_callbacks.erase(name);
        if (machine) {
            fsm_register_action(machine, name.c_str(), callback, user_data);
        }
    }
    void register_action(const std::string& name, ActionCallback callback) { registerAction(name, std::move(callback)); }
    void register_action(const std::string& name, fsm_action_callback_t callback, void* user_data = nullptr) { registerAction(name, callback, user_data); }

    /**
     * @brief Register a callback for a parameterized XState action (lambda overload).
     */
    void registerActionWithParams(const std::string& name, ActionWithParamsCallback callback) {
        m_actionsWithParams[name] = std::move(callback);
        m_rawActionsWithParams.erase(name);
        if (machine) {
            fsm_register_action_with_params(machine, name.c_str(), c_action_with_params_thunk, this);
        }
    }

    /**
     * @brief Register a zero-allocation C function pointer callback for a parameterized action.
     */
    void registerActionWithParams(const std::string& name, fsm_action_with_params_callback_t callback, void* user_data = nullptr) {
        m_rawActionsWithParams[name] = {callback, user_data};
        m_actionsWithParams.erase(name);
        if (machine) {
            fsm_register_action_with_params(machine, name.c_str(), callback, user_data);
        }
    }
    void register_action_with_params(const std::string& name, ActionWithParamsCallback callback) { registerActionWithParams(name, std::move(callback)); }
    void register_action_with_params(const std::string& name, fsm_action_with_params_callback_t callback, void* user_data = nullptr) { registerActionWithParams(name, callback, user_data); }
    void register_action(const std::string& name, ActionWithParamsCallback callback) { registerActionWithParams(name, std::move(callback)); }
    void register_action(const std::string& name, fsm_action_with_params_callback_t callback, void* user_data = nullptr) { registerActionWithParams(name, callback, user_data); }

    /**
     * @brief Register a callback for an XState guard (lambda overload).
     */
    void registerGuard(const std::string& name, GuardCallback callback) {
        m_guards[name] = std::move(callback);
        m_rawGuards.erase(name);
        if (machine) {
            fsm_register_guard(machine, name.c_str(), c_guard_thunk, this);
        }
    }

    /**
     * @brief Register a zero-allocation C function pointer callback for an XState guard.
     */
    void registerGuard(const std::string& name, fsm_guard_callback_t callback, void* user_data = nullptr) {
        m_rawGuards[name] = {callback, user_data};
        m_guards.erase(name);
        if (machine) {
            fsm_register_guard(machine, name.c_str(), callback, user_data);
        }
    }
    void register_guard(const std::string& name, GuardCallback callback) { registerGuard(name, std::move(callback)); }
    void register_guard(const std::string& name, fsm_guard_callback_t callback, void* user_data = nullptr) { registerGuard(name, callback, user_data); }

    /**
     * @brief Register a callback for a dynamic named delay (lambda overload).
     */
    void registerDelay(const std::string& name, DelayCallback callback) {
        m_delays[name] = std::move(callback);
        m_rawDelays.erase(name);
        if (machine) {
            fsm_register_delay(machine, name.c_str(), c_delay_thunk, this);
        }
    }

    /**
     * @brief Register a zero-allocation C function pointer callback for a dynamic named delay.
     */
    void registerDelay(const std::string& name, fsm_delay_callback_t callback, void* user_data = nullptr) {
        m_rawDelays[name] = {callback, user_data};
        m_delays.erase(name);
        if (machine) {
            fsm_register_delay(machine, name.c_str(), callback, user_data);
        }
    }
    void register_delay(const std::string& name, DelayCallback callback) { registerDelay(name, std::move(callback)); }
    void register_delay(const std::string& name, fsm_delay_callback_t callback, void* user_data = nullptr) { registerDelay(name, callback, user_data); }

    /**
     * @brief Register a callback for a parameterized XState guard (lambda overload).
     */
    void registerGuardWithParams(const std::string& name, GuardWithParamsCallback callback) {
        m_guardsWithParams[name] = std::move(callback);
        m_rawGuardsWithParams.erase(name);
        if (machine) {
            fsm_register_guard_with_params(machine, name.c_str(), c_guard_with_params_thunk, this);
        }
    }

    /**
     * @brief Register a zero-allocation C function pointer callback for a parameterized guard.
     */
    void registerGuardWithParams(const std::string& name, fsm_guard_with_params_callback_t callback, void* user_data = nullptr) {
        m_rawGuardsWithParams[name] = {callback, user_data};
        m_guardsWithParams.erase(name);
        if (machine) {
            fsm_register_guard_with_params(machine, name.c_str(), callback, user_data);
        }
    }
    void register_guard_with_params(const std::string& name, GuardWithParamsCallback callback) { registerGuardWithParams(name, std::move(callback)); }
    void register_guard_with_params(const std::string& name, fsm_guard_with_params_callback_t callback, void* user_data = nullptr) { registerGuardWithParams(name, callback, user_data); }
    void register_guard(const std::string& name, GuardWithParamsCallback callback) { registerGuardWithParams(name, std::move(callback)); }
    void register_guard(const std::string& name, fsm_guard_with_params_callback_t callback, void* user_data = nullptr) { registerGuardWithParams(name, callback, user_data); }

    /**
     * @brief Register a callback for the system clock (raw function pointer).
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
     * @brief Disambiguate captureless lambdas convertible to both fsm_clock_callback_t
     *        and ClockCallback, routing them to the zero-allocation C function pointer overload.
     */
    template <
        typename F,
        typename Decayed = typename std::decay<F>::type,
        typename std::enable_if<
            !std::is_same<Decayed, fsm_clock_callback_t>::value &&
            !std::is_same<Decayed, std::nullptr_t>::value &&
            !std::is_integral<Decayed>::value &&
            std::is_convertible<Decayed, fsm_clock_callback_t>::value,
            int>::type = 0>
    void registerClock(F&& callback) {
        registerClock(static_cast<fsm_clock_callback_t>(callback));
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
    void register_clock(fsm_clock_callback_t callback) { registerClock(callback); }
    void register_clock(ClockCallback callback) { registerClock(std::move(callback)); }
    void register_clock(SimulatedClock& clock) { registerClock(clock); }

    /**
     * @brief Register an inspection callback receiving serialized JSON inspection events (lambda overload).
     */
    void registerInspector(InspectCallback callback) {
        m_inspectFn = std::move(callback);
        m_rawInspectCallback = nullptr;
        m_rawInspectUserData = nullptr;
        if (machine) {
            fsm_register_inspector(machine, c_inspect_thunk, this);
        }
    }

    /**
     * @brief Register a zero-allocation C function pointer inspection callback.
     */
    void registerInspector(fsm_inspect_callback_t callback, void* user_data = nullptr) {
        m_rawInspectCallback = callback;
        m_rawInspectUserData = user_data;
        m_inspectFn = nullptr;
        if (machine) {
            fsm_register_inspector(machine, callback, user_data);
        }
    }
    void register_inspector(InspectCallback callback) { registerInspector(std::move(callback)); }
    void register_inspector(fsm_inspect_callback_t callback, void* user_data = nullptr) { registerInspector(callback, user_data); }

    /**
     * @brief Query the earliest active `after` timer deadline in milliseconds.
     */
    bool getNextDeadline(uint64_t& out_deadline) const {
        if (!machine) return false;
        return fsm_get_next_deadline(machine, &out_deadline);
    }
    bool get_next_deadline(uint64_t& out_deadline) const noexcept { return getNextDeadline(out_deadline); }

    /**
     * @brief Process pending explicit timers (delayed `after` transitions).
     */
    void tick() {
        if (machine) {
            fsm_tick(machine);
        }
    }
};


} // namespace lavi

#endif // LAVI_HPP
