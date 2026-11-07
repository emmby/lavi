package io.lavi;

@FunctionalInterface
public interface ParameterizedGuardCallback {
    boolean evaluate(Value context, Value event, Value params);
}
