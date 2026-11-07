#include <Arduino.h>
#include <lavi.hpp>
#include <memory>

const int PIN_RED = 13;
const int PIN_YELLOW = 12;
const int PIN_GREEN = 27;

const char* MACHINE_JSON = R"({
  "id": "stoplight",
  "initial": "red",
  "context": { "cycles": 1 },
  "states": {
    "red": {
      "entry": ["onRed"],
      "after": { "1000": "green" }
    },
    "green": {
      "entry": ["onGreen"],
      "on": {
        "PEDESTRIAN_WAIT": {
          "target": "yellow",
          "guard": "canCross"
        }
      },
      "after": { "1000": "yellow" }
    },
    "yellow": {
      "entry": ["onYellow"],
      "after": { "1000": "red" }
    }
  }
})";

std::unique_ptr<lavi::Machine> machine;

void set_leds(bool red, bool yellow, bool green) {
  digitalWrite(PIN_RED, red ? HIGH : LOW);
  digitalWrite(PIN_YELLOW, yellow ? HIGH : LOW);
  digitalWrite(PIN_GREEN, green ? HIGH : LOW);
}

void setup() {
  Serial.begin(115200);

  pinMode(PIN_RED, OUTPUT);
  pinMode(PIN_YELLOW, OUTPUT);
  pinMode(PIN_GREEN, OUTPUT);
  set_leds(false, false, false);

  Serial.println("Initializing Machine...");
  try {
    machine.reset(new lavi::Machine(MACHINE_JSON));

    machine->registerAction("onRed", [](const lavi::Value&, const lavi::Value&) {
      set_leds(true, false, false);
      Serial.println("State: red");
    });
    machine->registerAction("onGreen", [](const lavi::Value&, const lavi::Value&) {
      set_leds(false, false, true);
      Serial.println("State: green");
    });
    machine->registerAction("onYellow", [](const lavi::Value&, const lavi::Value&) {
      set_leds(false, true, false);
      Serial.println("State: yellow");
    });

    machine->registerGuard("canCross", [](const lavi::Value& ctx, const lavi::Value&) {
      double cycles = 0.0;
      ctx.getProperty("cycles").getNumber(cycles);
      return cycles >= 1.0;
    });

    machine->registerClock([]() -> uint64_t { return millis(); });

    machine->start();
    Serial.println("Machine Initialized.");
  } catch (const std::exception& e) {
    Serial.printf("Failed to initialize machine! Error: %s\n", e.what());
    while (1) { delay(1000); }
  } catch (...) {
    Serial.println("Failed to initialize machine! Unknown error.");
    while (1) { delay(1000); }
  }
}

void loop() {
  if (!machine) return;

  machine->tick();
  delay(10);
}
