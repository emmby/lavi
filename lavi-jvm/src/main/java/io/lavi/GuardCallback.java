package io.lavi;

@FunctionalInterface
public interface GuardCallback {
    boolean evaluate(Value context, Value event);
}
