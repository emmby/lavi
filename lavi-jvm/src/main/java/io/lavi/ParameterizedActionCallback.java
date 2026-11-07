package io.lavi;

@FunctionalInterface
public interface ParameterizedActionCallback {
    void execute(Value context, Value event, Value params);
}
