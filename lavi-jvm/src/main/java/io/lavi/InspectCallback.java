package io.lavi;

@FunctionalInterface
public interface InspectCallback {
    void onInspect(Value event);
}
