package io.lavi;

@FunctionalInterface
public interface OutputResolverCallback {
    Value resolve(Value context, Value event);
}
