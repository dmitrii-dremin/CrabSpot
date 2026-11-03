package fixtures;

/** Same-class body graphs; the reference harness alone uses Java library calls. */
public final class CallCases {
    private static int plusOne(int value) { return value + 1; }
    private static int twice(int value) { return value * 2; }
    private static int combine(int a, int b, int c) { return a * 100 + b * 10 + c; }
    public static int preserved(int value) { return 3 + plusOne(value) * 7; }
    public static int nestedArguments(int value) { return combine(plusOne(value), twice(value), -value); }
    public static long factorial(int count) { return count <= 1 ? 1 : count * factorial(count - 1); }
    public static int fibonacci(int count) { return count <= 1 ? count : fibonacci(count - 1) + fibonacci(count - 2); }
    public static boolean even(int count) { return count == 0 || odd(count - 1); }
    public static boolean odd(int count) { return count != 0 && even(count - 1); }
    private static int overloaded(int value) { return value + 10; }
    private static long overloaded(long value) { return value + 20; }
    public static long overloads(int value) { return overloaded(value) + overloaded((long) value); }
    private static int constant() { return 123456789; }
    public static int zeroArguments() { return constant(); }
    private static void noop() {}
    public static void rootVoid() { noop(); }
    public static int voidCall(int value) { noop(); return plusOne(value); }
    private static byte byteValue(int value) { return (byte) value; }
    private static short shortValue(int value) { return (short) value; }
    private static char charValue(int value) { return (char) value; }
    private static boolean boolValue(int value) { return value != 0; }
    public static int narrowCalls(int value) { return byteValue(value) + shortValue(value) + charValue(value) + (boolValue(value) ? 1 : 0); }
    private static int divide(int left, int right) { return left / right; }
    public static int descendantDivide(int value) { return 5 + divide(100, value); }
    private static double mix(int a, float b, long c, double d) { return a + b + c - d; }
    public static double mixed(int a, float b, long c, double d) { return 1.5 + mix(a, b, c, d); }
    private static float fHelper(float value) { return value + 1.25f; }
    public static float floatCall(float value) { return 3.5f + fHelper(value) * 2.0f; }
    private static double dHelper(double value) { return value * 1.5; }
    public static double doubleCall(double value) { return dHelper(value) + dHelper(-value) - value; }

    @FunctionalInterface private interface Case { Object run(); }
    private static String f(float value) { return "F:" + String.format(java.util.Locale.ROOT, "%08x", Float.floatToRawIntBits(value)); }
    private static String d(double value) { return "D:" + String.format(java.util.Locale.ROOT, "%016x", Double.doubleToRawLongBits(value)); }
    private static void emit(String method, String descriptor, String arguments, Case body) {
        String result;
        try {
            Object value = body.run();
            result = value == null ? "V"
                : value instanceof Float ? (Float.isNaN((Float) value) ? "F:NaN" : f((Float) value))
                : value instanceof Double ? (Double.isNaN((Double) value) ? "D:NaN" : d((Double) value))
                : value instanceof Long ? "L:" + value
                : value instanceof Boolean ? "I:" + ((Boolean) value ? 1 : 0) : "I:" + value;
        } catch (ArithmeticException error) { result = "E:java/lang/ArithmeticException"; }
        System.out.println(method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) {
        for (int value : new int[] {Integer.MIN_VALUE, -65537, -129, -1, 0, 1, 7, 127, 128, 65535, Integer.MAX_VALUE}) {
            String argument = "I:" + value;
            emit("preserved", "(I)I", argument, () -> preserved(value));
            emit("nestedArguments", "(I)I", argument, () -> nestedArguments(value));
            emit("overloads", "(I)J", argument, () -> overloads(value));
            emit("voidCall", "(I)I", argument, () -> voidCall(value));
            emit("narrowCalls", "(I)I", argument, () -> narrowCalls(value));
            emit("descendantDivide", "(I)I", argument, () -> descendantDivide(value));
        }
        for (int count : new int[] {0, 1, 2, 5, 10, 15, 20, 21}) {
            emit("factorial", "(I)J", "I:" + count, () -> factorial(count));
            emit("even", "(I)Z", "I:" + count, () -> even(count));
            emit("odd", "(I)Z", "I:" + count, () -> odd(count));
        }
        for (int count : new int[] {0, 1, 2, 5, 10, 15})
            emit("fibonacci", "(I)I", "I:" + count, () -> fibonacci(count));
        for (float value : new float[] {-0.0f, 0.0f, 1.0f, -1.0f, Float.MIN_VALUE, Float.MAX_VALUE, Float.POSITIVE_INFINITY, Float.NaN})
            emit("floatCall", "(F)F", f(value), () -> floatCall(value));
        for (double value : new double[] {-0.0, 0.0, 1.0, -1.0, Double.MIN_VALUE, Double.MAX_VALUE, Double.POSITIVE_INFINITY, Double.NaN})
            emit("doubleCall", "(D)D", d(value), () -> doubleCall(value));
        emit("mixed", "(IFJD)D", "I:7," + f(1.25f) + ",L:9007199254740993," + d(-2.5), () -> mixed(7, 1.25f, 9007199254740993L, -2.5));
        emit("zeroArguments", "()I", "", () -> zeroArguments());
        emit("rootVoid", "()V", "", () -> { rootVoid(); return null; });
    }
}
