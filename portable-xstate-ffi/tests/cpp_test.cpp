#include "../include/portable_xstate.hpp"
#include <iostream>
#include <cassert>

const char* STOPLIGHT_JSON = R"({
  "id": "trafficLight",
  "initial": "green",
  "states": {
    "green": { "on": { "TIMER": [ { "target": "yellow" } ] } },
    "yellow": { "on": { "TIMER": [ { "target": "red" } ] } },
    "red": { "on": { "TIMER": [ { "target": "green" } ] } }
  },
  "context": { "cycles": 0 }
})";

int main() {
    std::cout << "Starting C++ Integration Test..." << std::endl;

    try {
        PortableXState machine(STOPLIGHT_JSON);

        std::string state = machine.getState();
        std::cout << "Initial State: " << state << std::endl;
        assert(state == "green");

        machine.send("TIMER");
        state = machine.getState();
        std::cout << "New State: " << state << std::endl;
        assert(state == "yellow");

    } catch (const std::exception& e) {
        std::cerr << "Test Failed with exception: " << e.what() << std::endl;
        return 1;
    }

    std::cout << "C++ Test Passed!" << std::endl;
    return 0;
}
