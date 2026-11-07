package io.lavi;

import java.util.*;

public final class Value {
    static {
        NativeLoader.load();
    }

    public enum Type {
        NULL,
        BOOLEAN,
        NUMBER,
        STRING,
        ARRAY,
        OBJECT
    }

    public static final Value NULL = new Value(0L, Type.NULL);

    private final long nativeHandle;
    private final Type type;

    private Value(long nativeHandle, Type type) {
        this.nativeHandle = nativeHandle;
        this.type = type;
    }

    public static Value fromNativeHandle(long handle) {
        if (handle == 0L) {
            return NULL;
        }
        int typeOrd = nativeGetType(handle);
        if (typeOrd < 0 || typeOrd >= Type.values().length) {
            typeOrd = 0;
        }
        Type t = Type.values()[typeOrd];
        if (t == Type.NULL) {
            nativeFree(handle);
            return NULL;
        }
        Value v = new Value(handle, t);
        NativeCleaner.register(v, new ValueFreeAction(handle));
        return v;
    }

    private static final class ValueFreeAction implements Runnable {
        private final long handle;

        ValueFreeAction(long handle) {
            this.handle = handle;
        }

        @Override
        public void run() {
            if (handle != 0L) {
                nativeFree(handle);
            }
        }
    }

    public static Value of(boolean b) {
        return fromNativeHandle(nativeOfBoolean(b));
    }

    public static Value of(double n) {
        return fromNativeHandle(nativeOfNumber(n));
    }

    public static Value of(long n) {
        return fromNativeHandle(nativeOfNumber((double) n));
    }

    public static Value of(String s) {
        if (s == null) return NULL;
        return fromNativeHandle(nativeOfString(s));
    }

    public static Value ofArray(List<Value> elements) {
        if (elements == null) return NULL;
        long[] handles = new long[elements.size()];
        for (int i = 0; i < elements.size(); i++) {
            Value el = elements.get(i);
            handles[i] = el != null ? el.nativeHandle : 0L;
        }
        return fromNativeHandle(nativeOfArray(handles));
    }

    public static Value ofObject(Map<String, Value> properties) {
        if (properties == null) return NULL;
        int size = properties.size();
        String[] keys = new String[size];
        long[] handles = new long[size];
        int idx = 0;
        for (Map.Entry<String, Value> entry : properties.entrySet()) {
            keys[idx] = entry.getKey();
            Value val = entry.getValue();
            handles[idx] = val != null ? val.nativeHandle : 0L;
            idx++;
        }
        return fromNativeHandle(nativeOfObject(keys, handles));
    }

    public static Value fromJson(String json) {
        if (json == null) return NULL;
        String s = json.trim();
        if (s.isEmpty() || s.equals("null")) return NULL;
        long h = nativeFromJson(s);
        return h != 0L ? fromNativeHandle(h) : NULL;
    }

    public Type getType() {
        return type;
    }

    public boolean isNull() {
        return type == Type.NULL;
    }

    public boolean isBoolean() {
        return type == Type.BOOLEAN;
    }

    public boolean isNumber() {
        return type == Type.NUMBER;
    }

    public boolean isString() {
        return type == Type.STRING;
    }

    public boolean isArray() {
        return type == Type.ARRAY;
    }

    public boolean isObject() {
        return type == Type.OBJECT;
    }

    public Optional<Boolean> asBoolean() {
        if (type != Type.BOOLEAN || nativeHandle == 0L) {
            return Optional.empty();
        }
        return Optional.of(nativeAsBoolean(nativeHandle));
    }

    public Optional<Double> asNumber() {
        if (type != Type.NUMBER || nativeHandle == 0L) {
            return Optional.empty();
        }
        return Optional.of(nativeAsNumber(nativeHandle));
    }

    public Optional<Long> asLong() {
        if (type != Type.NUMBER || nativeHandle == 0L) {
            return Optional.empty();
        }
        return Optional.of((long) nativeAsNumber(nativeHandle));
    }

    public Optional<String> asString() {
        if (type != Type.STRING || nativeHandle == 0L) {
            return Optional.empty();
        }
        String s = nativeAsString(nativeHandle);
        return Optional.ofNullable(s);
    }

    public Value getProperty(String key) {
        if (type != Type.OBJECT || nativeHandle == 0L || key == null) {
            return NULL;
        }
        long child = nativeGetProperty(nativeHandle, key);
        return child != 0L ? fromNativeHandle(child) : NULL;
    }

    public Value getElement(int index) {
        if (type != Type.ARRAY || nativeHandle == 0L || index < 0) {
            return NULL;
        }
        long child = nativeGetElement(nativeHandle, index);
        return child != 0L ? fromNativeHandle(child) : NULL;
    }

    public int size() {
        if ((type != Type.ARRAY && type != Type.OBJECT) || nativeHandle == 0L) {
            return 0;
        }
        return nativeSize(nativeHandle);
    }

    public boolean hasProperty(String key) {
        if (type != Type.OBJECT || nativeHandle == 0L || key == null) {
            return false;
        }
        return nativeHasProperty(nativeHandle, key);
    }

    public Map<String, Value> asObjectMap() {
        if (type != Type.OBJECT || nativeHandle == 0L) {
            return Collections.emptyMap();
        }
        String[] keys = nativeGetObjectKeys(nativeHandle);
        long[] handles = nativeGetObjectValues(nativeHandle);
        if (keys == null || handles == null || keys.length != handles.length) {
            return Collections.emptyMap();
        }
        Map<String, Value> map = new LinkedHashMap<String, Value>(keys.length);
        for (int i = 0; i < keys.length; i++) {
            map.put(keys[i], fromNativeHandle(handles[i]));
        }
        return Collections.unmodifiableMap(map);
    }

    public List<Value> asArrayList() {
        if (type != Type.ARRAY || nativeHandle == 0L) {
            return Collections.emptyList();
        }
        long[] handles = nativeGetArrayElements(nativeHandle);
        if (handles == null) {
            return Collections.emptyList();
        }
        List<Value> list = new ArrayList<Value>(handles.length);
        for (int i = 0; i < handles.length; i++) {
            list.add(fromNativeHandle(handles[i]));
        }
        return Collections.unmodifiableList(list);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Value)) return false;
        Value other = (Value) o;
        if (this.type != other.type) return false;
        if (this.type == Type.NULL) return true;
        if (this.nativeHandle != 0L && other.nativeHandle != 0L) {
            return nativeEquals(this.nativeHandle, other.nativeHandle);
        }
        return false;
    }

    @Override
    public int hashCode() {
        if (type == Type.NULL || nativeHandle == 0L) return 0;
        return nativeHashCode(nativeHandle);
    }

    @Override
    public String toString() {
        return toJson();
    }

    public String toJson() {
        if (type == Type.NULL || nativeHandle == 0L) {
            return "null";
        }
        String s = nativeToJson(nativeHandle);
        return s != null ? s : "null";
    }

    public void toJson(StringBuilder sb) {
        if (sb != null) {
            sb.append(toJson());
        }
    }

    long getNativeHandle() {
        return nativeHandle;
    }

    // Native declarations
    static native void nativeFree(long handle);
    static native int nativeGetType(long handle);
    static native boolean nativeAsBoolean(long handle);
    static native double nativeAsNumber(long handle);
    static native String nativeAsString(long handle);
    static native long nativeGetProperty(long handle, String key);
    static native long nativeGetElement(long handle, int index);
    static native int nativeSize(long handle);
    static native boolean nativeHasProperty(long handle, String key);
    static native String[] nativeGetObjectKeys(long handle);
    static native long[] nativeGetObjectValues(long handle);
    static native long[] nativeGetArrayElements(long handle);
    static native boolean nativeEquals(long h1, long h2);
    static native int nativeHashCode(long handle);
    static native String nativeToJson(long handle);
    static native long nativeFromJson(String json);
    static native long nativeOfBoolean(boolean b);
    static native long nativeOfNumber(double n);
    static native long nativeOfString(String s);
    static native long nativeOfArray(long[] handles);
    static native long nativeOfObject(String[] keys, long[] handles);
}
