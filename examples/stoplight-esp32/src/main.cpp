#include <Arduino.h>
#include <lavi.hpp>
#include <memory>

const int PIN_RED = 13;
const int PIN_YELLOW = 12;
const int PIN_GREEN = 27;

const char* MACHINE_JSON = R"({
  "id": "stoplight",
  "initial": "red",
  "states": {
    "red": {
      "entry": "turnRed",
      "on": { "TIMER": { "target": "green" } }
    },
    "green": {
      "entry": "turnGreen",
      "on": { "TIMER": { "target": "yellow" } }
    },
    "yellow": {
      "entry": "turnYellow",
      "on": { "TIMER": { "target": "red" } }
    }
  }
})";

std::unique_ptr<lavi::Machine> machine;
unsigned long last_transition = 0;

void update_leds(const std::string& state);

void setup() {
  Serial.begin(115200);
  
  pinMode(PIN_RED, OUTPUT);
  pinMode(PIN_YELLOW, OUTPUT);
  pinMode(PIN_GREEN, OUTPUT);

  // Initialize off
  digitalWrite(PIN_RED, LOW);
  digitalWrite(PIN_YELLOW, LOW);
  digitalWrite(PIN_GREEN, LOW);

  Serial.println("Initializing Machine...");
  try {
    // PortableXState throws logic_error/runtime_error if it fails
    machine.reset(new lavi::Machine(MACHINE_JSON));
    
    // Register action callbacks to drive LEDs
    machine->registerAction("turnRed", [](const lavi::Value& ctx, const lavi::Value& evt) {
        update_leds("red");
    });
    machine->registerAction("turnGreen", [](const lavi::Value& ctx, const lavi::Value& evt) {
        update_leds("green");
    });
    machine->registerAction("turnYellow", [](const lavi::Value& ctx, const lavi::Value& evt) {
        update_leds("yellow");
    });

    machine->start();

    Serial.println("Machine Initialized.");
  } catch (const std::exception& e) {
    Serial.printf("Failed to initialize machine! Error: %s\n", e.what());
    // Halt
    while(1) { delay(1000); }
  } catch (...) {
    Serial.println("Failed to initialize machine! Unknown error.");
    while(1) { delay(1000); }
  }
}

void update_leds(const std::string& state) {
    digitalWrite(PIN_RED, LOW);
    digitalWrite(PIN_YELLOW, LOW);
    digitalWrite(PIN_GREEN, LOW);

    if (state == "red") {
        digitalWrite(PIN_RED, HIGH);
    } else if (state == "yellow") {
        digitalWrite(PIN_YELLOW, HIGH);
    } else if (state == "green") {
        digitalWrite(PIN_GREEN, HIGH);
    }
}

void loop() {
  if (!machine) return;

  unsigned long now = millis();
  
  // Transition every 1000ms for demo purposes
  if (now - last_transition > 1000) {
    last_transition = now;
    machine->send("TIMER");

    const char* currentState = machine->matches("red")
        ? "red"
        : (machine->matches("green") ? "green" : "yellow");
    Serial.print("Current State: ");
    Serial.println(currentState);
  }
  
  delay(10);
}
