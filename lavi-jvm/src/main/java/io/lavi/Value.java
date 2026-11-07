package io.lavi;

import java.util.*;

public final class Value {
    public enum Type {
        NULL,
        BOOLEAN,
        NUMBER,
        STRING,
        ARRAY,
        OBJECT
    }

    public static final Value NULL = new Value(Type.NULL, null, null, null, null, null);

    private final Type type;
    private final Boolean boolVal;
    private final Double numVal;
    private final String strVal;
    private final List<Value> arrayVal;
    private final Map<String, Value> objVal;

    private Value(Type type, Boolean b, Double num, String s, List<Value> arr, Map<String, Value> obj) {
        this.type = type;
        this.boolVal = b;
        this.numVal = num;
        this.strVal = s;
        this.arrayVal = arr;
        this.objVal = obj;
    }

    public static Value of(boolean b) {
        return new Value(Type.BOOLEAN, b, null, null, null, null);
    }

    public static Value of(double n) {
        return new Value(Type.NUMBER, null, n, null, null, null);
    }

    public static Value of(long n) {
        return new Value(Type.NUMBER, null, (double) n, null, null, null);
    }

    public static Value of(String s) {
        if (s == null) return NULL;
        return new Value(Type.STRING, null, null, s, null, null);
    }

    public static Value ofArray(List<Value> elements) {
        if (elements == null) return NULL;
        return new Value(Type.ARRAY, null, null, null, Collections.unmodifiableList(new ArrayList<>(elements)), null);
    }

    public static Value ofObject(Map<String, Value> properties) {
        if (properties == null) return NULL;
        return new Value(Type.OBJECT, null, null, null, null, Collections.unmodifiableMap(new LinkedHashMap<>(properties)));
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
        return type == Type.BOOLEAN ? Optional.of(boolVal) : Optional.empty();
    }

    public Optional<Double> asNumber() {
        return type == Type.NUMBER ? Optional.of(numVal) : Optional.empty();
    }

    public Optional<Long> asLong() {
        return type == Type.NUMBER ? Optional.of(numVal.longValue()) : Optional.empty();
    }

    public Optional<String> asString() {
        return type == Type.STRING ? Optional.of(strVal) : Optional.empty();
    }

    public Value getProperty(String key) {
        if (type == Type.OBJECT && objVal != null && key != null) {
            Value val = objVal.get(key);
            return val != null ? val : NULL;
        }
        return NULL;
    }

    public Value getElement(int index) {
        if (type == Type.ARRAY && arrayVal != null && index >= 0 && index < arrayVal.size()) {
            return arrayVal.get(index);
        }
        return NULL;
    }

    public int size() {
        if (type == Type.ARRAY && arrayVal != null) {
            return arrayVal.size();
        }
        if (type == Type.OBJECT && objVal != null) {
            return objVal.size();
        }
        return 0;
    }

    public boolean hasProperty(String key) {
        if (type == Type.OBJECT && objVal != null && key != null) {
            return objVal.containsKey(key);
        }
        return false;
    }

    public Map<String, Value> asObjectMap() {
        return type == Type.OBJECT && objVal != null ? objVal : Collections.emptyMap();
    }

    public List<Value> asArrayList() {
        return type == Type.ARRAY && arrayVal != null ? arrayVal : Collections.emptyList();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Value other)) return false;
        if (type != other.type) return false;
        return switch (type) {
            case NULL -> true;
            case BOOLEAN -> Objects.equals(boolVal, other.boolVal);
            case NUMBER -> Objects.equals(numVal, other.numVal);
            case STRING -> Objects.equals(strVal, other.strVal);
            case ARRAY -> Objects.equals(arrayVal, other.arrayVal);
            case OBJECT -> Objects.equals(objVal, other.objVal);
        };
    }

    @Override
    public int hashCode() {
        return switch (type) {
            case NULL -> 0;
            case BOOLEAN -> Objects.hashCode(boolVal);
            case NUMBER -> Objects.hashCode(numVal);
            case STRING -> Objects.hashCode(strVal);
            case ARRAY -> Objects.hashCode(arrayVal);
            case OBJECT -> Objects.hashCode(objVal);
        };
    }

    @Override
    public String toString() {
        return toJson();
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder();
        toJson(sb);
        return sb.toString();
    }

    private void toJson(StringBuilder sb) {
        switch (type) {
            case NULL -> sb.append("null");
            case BOOLEAN -> sb.append(boolVal);
            case NUMBER -> {
                if (numVal == Math.floor(numVal) && !Double.isInfinite(numVal) && numVal >= Long.MIN_VALUE && numVal <= Long.MAX_VALUE) {
                    sb.append(numVal.longValue());
                } else {
                    sb.append(numVal);
                }
            }
            case STRING -> escapeJsonString(strVal, sb);
            case ARRAY -> {
                sb.append("[");
                for (int i = 0; i < arrayVal.size(); i++) {
                    if (i > 0) sb.append(",");
                    arrayVal.get(i).toJson(sb);
                }
                sb.append("]");
            }
            case OBJECT -> {
                sb.append("{");
                boolean first = true;
                for (Map.Entry<String, Value> entry : objVal.entrySet()) {
                    if (!first) sb.append(",");
                    first = false;
                    escapeJsonString(entry.getKey(), sb);
                    sb.append(":");
                    entry.getValue().toJson(sb);
                }
                sb.append("}");
            }
        }
    }

    private static void escapeJsonString(String s, StringBuilder sb) {
        sb.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\b' -> sb.append("\\b");
                case '\f' -> sb.append("\\f");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                    if (c < ' ') {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
                }
            }
        }
        sb.append('"');
    }

    // -----------------------------------------------------------------------
    // Iterative JSON Parser (Zero CPU Call-Stack Recursion)
    // -----------------------------------------------------------------------

    private enum ContainerType { ARRAY, OBJECT }

    private static final class ParseFrame {
        final ContainerType type;
        final List<Value> list;
        final Map<String, Value> map;
        String pendingKey;

        ParseFrame(ContainerType type) {
            this.type = type;
            if (type == ContainerType.ARRAY) {
                this.list = new ArrayList<>();
                this.map = null;
            } else {
                this.list = null;
                this.map = new LinkedHashMap<>();
            }
        }
    }

    public static Value fromJson(String json) {
        if (json == null) return NULL;
        String s = json.trim();
        if (s.isEmpty() || s.equals("null")) return NULL;
        if (s.equals("true")) return Value.of(true);
        if (s.equals("false")) return Value.of(false);

        Deque<ParseFrame> stack = new ArrayDeque<>();
        Value root = null;
        int idx = 0;
        int len = s.length();
        boolean expectValue = true;

        while (idx < len) {
            while (idx < len && Character.isWhitespace(s.charAt(idx))) idx++;
            if (idx >= len) break;

            char c = s.charAt(idx);

            if (expectValue) {
                if (c == '{') {
                    stack.push(new ParseFrame(ContainerType.OBJECT));
                    idx++;
                    expectValue = false;
                } else if (c == '[') {
                    stack.push(new ParseFrame(ContainerType.ARRAY));
                    idx++;
                    expectValue = true;
                } else if (c == ']') {
                    if (!stack.isEmpty() && stack.peek().type == ContainerType.ARRAY) {
                        ParseFrame frame = stack.pop();
                        Value arrVal = Value.ofArray(frame.list);
                        if (stack.isEmpty()) {
                            root = arrVal;
                        } else {
                            attachToTop(stack.peek(), arrVal);
                        }
                        idx++;
                        expectValue = false;
                    } else {
                        idx++;
                    }
                } else if (c == '}') {
                    if (!stack.isEmpty() && stack.peek().type == ContainerType.OBJECT) {
                        ParseFrame frame = stack.pop();
                        Value objVal = Value.ofObject(frame.map);
                        if (stack.isEmpty()) {
                            root = objVal;
                        } else {
                            attachToTop(stack.peek(), objVal);
                        }
                        idx++;
                        expectValue = false;
                    } else {
                        idx++;
                    }
                } else if (c == '"') {
                    int[] next = new int[1];
                    String str = parseStringToken(s, idx, next);
                    idx = next[0];
                    Value val = Value.of(str);
                    if (stack.isEmpty()) {
                        root = val;
                    } else {
                        attachToTop(stack.peek(), val);
                    }
                    expectValue = false;
                } else if (c == 't' && s.startsWith("true", idx)) {
                    idx += 4;
                    Value val = Value.of(true);
                    if (stack.isEmpty()) root = val; else attachToTop(stack.peek(), val);
                    expectValue = false;
                } else if (c == 'f' && s.startsWith("false", idx)) {
                    idx += 5;
                    Value val = Value.of(false);
                    if (stack.isEmpty()) root = val; else attachToTop(stack.peek(), val);
                    expectValue = false;
                } else if (c == 'n' && s.startsWith("null", idx)) {
                    idx += 4;
                    if (stack.isEmpty()) root = NULL; else attachToTop(stack.peek(), NULL);
                    expectValue = false;
                } else if (c == '-' || Character.isDigit(c)) {
                    int start = idx;
                    while (idx < len) {
                        char ch = s.charAt(idx);
                        if (Character.isDigit(ch) || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E') {
                            idx++;
                        } else {
                            break;
                        }
                    }
                    double num = Double.parseDouble(s.substring(start, idx));
                    Value val = Value.of(num);
                    if (stack.isEmpty()) root = val; else attachToTop(stack.peek(), val);
                    expectValue = false;
                } else {
                    idx++;
                }
            } else {
                if (c == ',') {
                    idx++;
                    if (!stack.isEmpty() && stack.peek().type == ContainerType.ARRAY) {
                        expectValue = true;
                    } else {
                        expectValue = false;
                    }
                } else if (c == ']') {
                    if (!stack.isEmpty() && stack.peek().type == ContainerType.ARRAY) {
                        ParseFrame frame = stack.pop();
                        Value arrVal = Value.ofArray(frame.list);
                        if (stack.isEmpty()) {
                            root = arrVal;
                        } else {
                            attachToTop(stack.peek(), arrVal);
                        }
                        idx++;
                        expectValue = false;
                    } else {
                        idx++;
                    }
                } else if (c == '}') {
                    if (!stack.isEmpty() && stack.peek().type == ContainerType.OBJECT) {
                        ParseFrame frame = stack.pop();
                        Value objVal = Value.ofObject(frame.map);
                        if (stack.isEmpty()) {
                            root = objVal;
                        } else {
                            attachToTop(stack.peek(), objVal);
                        }
                        idx++;
                        expectValue = false;
                    } else {
                        idx++;
                    }
                } else if (!stack.isEmpty() && stack.peek().type == ContainerType.OBJECT && stack.peek().pendingKey == null) {
                    if (c == '"') {
                        int[] next = new int[1];
                        String key = parseStringToken(s, idx, next);
                        idx = next[0];
                        stack.peek().pendingKey = key;
                        while (idx < len && Character.isWhitespace(s.charAt(idx))) idx++;
                        if (idx < len && s.charAt(idx) == ':') {
                            idx++;
                        }
                        expectValue = true;
                    } else {
                        idx++;
                    }
                } else {
                    idx++;
                }
            }
        }

        return root != null ? root : NULL;
    }

    private static void attachToTop(ParseFrame top, Value val) {
        if (top.type == ContainerType.ARRAY) {
            top.list.add(val);
        } else if (top.type == ContainerType.OBJECT) {
            if (top.pendingKey != null) {
                top.map.put(top.pendingKey, val);
                top.pendingKey = null;
            }
        }
    }

    private static String parseStringToken(String s, int startIdx, int[] nextIdx) {
        StringBuilder sb = new StringBuilder();
        int i = startIdx + 1; // skip leading "
        int len = s.length();
        while (i < len) {
            char c = s.charAt(i);
            if (c == '"') {
                i++;
                break;
            } else if (c == '\\' && i + 1 < len) {
                i++;
                char esc = s.charAt(i);
                switch (esc) {
                    case '"' -> sb.append('"');
                    case '\\' -> sb.append('\\');
                    case '/' -> sb.append('/');
                    case 'b' -> sb.append('\b');
                    case 'f' -> sb.append('\f');
                    case 'n' -> sb.append('\n');
                    case 'r' -> sb.append('\r');
                    case 't' -> sb.append('\t');
                    case 'u' -> {
                        if (i + 4 < len) {
                            String hex = s.substring(i + 1, i + 5);
                            sb.append((char) Integer.parseInt(hex, 16));
                            i += 4;
                        }
                    }
                    default -> sb.append(esc);
                }
            } else {
                sb.append(c);
            }
            i++;
        }
        nextIdx[0] = i;
        return sb.toString();
    }
}
