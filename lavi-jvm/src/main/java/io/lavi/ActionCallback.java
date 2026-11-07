package io.lavi;

@FunctionalInterface
public interface ActionCallback {
    void execute(Value context, Value event);
}
