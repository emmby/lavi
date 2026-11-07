CC ?= cc
CFLAGS ?= -std=c11 -Wall -Wextra -I../../lavi-c-cpp/include
LDFLAGS ?= -L../../target/debug -llavi_c_cpp -lpthread -ldl -lm

.PHONY: all run clean

all: build/example_c

build/example_c: main.c ../../lavi-c-cpp/include/lavi.h
	mkdir -p build
	$(CC) $(CFLAGS) main.c $(LDFLAGS) -o build/example_c

run: build/example_c
	./build/example_c

clean:
	rm -rf build
