package io.lavi;

@FunctionalInterface
public interface InvokeStartCallback {
    void onStart(String id, Value src, Value input, Value context, Value event);
}
