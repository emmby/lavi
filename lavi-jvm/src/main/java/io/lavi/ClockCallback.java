package io.lavi;

@FunctionalInterface
public interface ClockCallback {
    long nowMs();
}
