#include "../include/portable_xstate.hpp"
#include <iostream>
#include <cassert>

const char* STOPLIGHT_JSON = R"({
  "id": "trafficLight",
  "initial": "green",
  "states": {
    "green": { "on": { "TIMER": [ { "target": "yellow", "actions": ["doTimerAction"] } ] } },
    "yellow": { "on": { "TIMER": [ { "target": "red" } ] } },
    "red": { "on": { "TIMER": [ { "target": "green" } ] } }
  },
  "context": { "cycles": 0 }
})";

int main() {
    std::cout << "Starting C++ Integration Test..." << std::endl;

    try {
        xstate::PortableXState machine(STOPLIGHT_JSON);

        bool cpp_action_called = false;
        machine.registerAction("doTimerAction", [&cpp_action_called](const xstate::Value& context, const xstate::Value& event) {
            std::cout << "C++ Action called!" << std::endl;
            auto cycles = context.getProperty("cycles");
            assert(!cycles.isNull());
            double cycles_val = -1;
            assert(cycles.getNumber(cycles_val) && cycles_val == 0.0);
            
            auto missing = context.getProperty("missing_key");
            assert(missing.isNull());

            auto ev_type = event.getProperty("type");
            assert(!ev_type.isNull());
            std::string ev_name;
            assert(ev_type.getString(ev_name) && ev_name == "TIMER");

            cpp_action_called = true;
        });

        // Unstarted machine matches nothing
        assert(!machine.matches("green"));

        machine.start();

        assert(machine.matches("green"));
        assert(machine.matches(std::string("green")));
        assert(!machine.matches("yellow"));

        machine.send("TIMER");
        assert(machine.matches("yellow"));
        assert(!machine.matches("green"));
        assert(cpp_action_called == true);

        const char* PARALLEL_JSON = R"({
          "id": "displayMachine",
          "initial": "Display",
          "states": {
            "Display": {
              "type": "parallel",
              "states": {
                "Theme": {
                  "initial": "Light",
                  "states": { "Light": {}, "Dark": {} }
                },
                "Power": {
                  "initial": "On",
                  "states": { "On": {}, "Off": {} }
                }
              }
            }
          }
        })";
        xstate::PortableXState parallelMachine(PARALLEL_JSON);
        parallelMachine.start();
        assert(parallelMachine.matches("Display"));
        assert(parallelMachine.matches("Display.Theme"));
        assert(parallelMachine.matches("Display.Theme.Light"));
        assert(parallelMachine.matches("Display.Power.On"));
        assert(!parallelMachine.matches("Display.Power.Off"));

    } catch (const std::exception& e) {
        std::cerr << "Test Failed with exception: " << e.what() << std::endl;
        return 1;
    }

    std::cout << "C++ Test Passed!" << std::endl;
    return 0;
}
