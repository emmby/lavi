CXX ?= c++
CXXFLAGS ?= -std=c++11 -Wall -Wextra -I../../lavi-c-cpp/include
LDFLAGS ?= -L../../target/debug -llavi_c_cpp -lpthread -ldl -lm

.PHONY: all run clean

all: build/example_cpp

build/example_cpp: main.cpp ../../lavi-c-cpp/include/lavi.hpp ../../target/debug/liblavi_c_cpp.a
	mkdir -p build
	$(CXX) $(CXXFLAGS) main.cpp $(LDFLAGS) -o build/example_cpp

run: build/example_cpp
	./build/example_cpp

clean:
	rm -rf build
