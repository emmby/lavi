#ifndef MOCK_ARDUINO_H
#define MOCK_ARDUINO_H

#include <stdint.h>

#define HIGH 1
#define LOW 0
#define OUTPUT 1

inline void pinMode(int, int) {}
inline void digitalWrite(int, int) {}
inline unsigned long millis() { return 0; }
inline void delay(unsigned long) {}

struct MockSerial {
  void begin(unsigned long) {}
  void println(const char*) {}
  void print(const char*) {}
  void printf(const char*, ...) {}
};

static MockSerial Serial;

#endif // MOCK_ARDUINO_H
