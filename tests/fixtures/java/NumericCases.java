package fixtures;

/** Pure numeric bodies for CrabSpot; main runs only as a JDK reference oracle. */
public final class NumericCases {
    public static int intMath(int a, int b) { return (a + b) * (a - b); }
    public static long longMath(long a, long b) { return (a + b) * (a - b); }
    public static int intDivide(int a, int b) { return a / b; }
    public static int intRemainder(int a, int b) { return a % b; }
    public static long longDivide(long a, long b) { return a / b; }
    public static long longRemainder(long a, long b) { return a % b; }
    public static int intShifts(int a, int n) { return (a << n) ^ (a >> n) ^ (a >>> n); }
    public static long longShifts(long a, int n) { return (a << n) ^ (a >> n) ^ (a >>> n); }
    public static int intBits(int a, int b) { return (a & b) | (a ^ ~b); }
    public static long longBits(long a, long b) { return (a & b) | (a ^ ~b); }
    public static int intNegate(int a) { return -a; }
    public static long longNegate(long a) { return -a; }
    public static byte narrowByte(int a) { return (byte) a; }
    public static short narrowShort(int a) { return (short) a; }
    public static char narrowChar(int a) { return (char) a; }
    public static boolean booleanValue(int a) { return a != 0; }
    public static int toInt(long a) { return (int) a; }
    public static long toLong(int a) { return a; }
    public static int compare(long a, long b) { return a < b ? -1 : a == b ? 0 : 1; }
    public static int table(int key) {
        return switch (key) { case -1 -> 11; case 0 -> 22; case 1 -> 33; case 2 -> 44; default -> -9; };
    }
    public static int lookup(int key) {
        return switch (key) { case -100 -> 5; case 7 -> 6; case 10000 -> 7; default -> -8; };
    }
    public static int sum(int count) {
        int value = 0;
        for (int i = 0; i < count; i++) value += i;
        return value;
    }
    public static long factorial(int count) {
        long value = 1;
        for (int i = 1; i <= count; i++) value *= i;
        return value;
    }
    public static long arguments(int a, long b, int c, long d) { return a + b * c - d; }
    public static int increment(int value) { value += 32767; return value; }
    public static int constantInt() { return 0x12345678; }
    public static long constantLong() { return 0x1020304050607080L; }
    public static void nothing() {}

    @FunctionalInterface private interface Case { Object run(); }
    private static void emit(String method, String descriptor, String arguments, Case body) {
        String result;
        try {
            Object value = body.run();
            result = value == null ? "V" : value instanceof Long ? "L:" + value
                : value instanceof Character ? "I:" + (int) (Character) value
                : value instanceof Boolean ? "I:" + ((Boolean) value ? 1 : 0)
                : "I:" + value;
        } catch (ArithmeticException error) {
            result = "E:java/lang/ArithmeticException";
        }
        System.out.println(method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }

    public static void main(String[] ignored) {
        int[] ints = {Integer.MIN_VALUE, -65537, -129, -1, 0, 1, 127, 128, 65535, Integer.MAX_VALUE};
        long[] longs = {Long.MIN_VALUE, -4294967297L, -1, 0, 1, 4294967296L, Long.MAX_VALUE};
        for (int a : ints) {
            emit("narrowByte", "(I)B", "I:" + a, () -> narrowByte(a));
            emit("narrowShort", "(I)S", "I:" + a, () -> narrowShort(a));
            emit("narrowChar", "(I)C", "I:" + a, () -> narrowChar(a));
            emit("booleanValue", "(I)Z", "I:" + a, () -> booleanValue(a));
            emit("toLong", "(I)J", "I:" + a, () -> toLong(a));
            emit("intNegate", "(I)I", "I:" + a, () -> intNegate(a));
            emit("increment", "(I)I", "I:" + a, () -> increment(a));
            for (int b : new int[] {-1, 0, 1, 7, Integer.MAX_VALUE}) {
                String arguments = "I:" + a + ",I:" + b;
                emit("intMath", "(II)I", arguments, () -> intMath(a, b));
                emit("intDivide", "(II)I", arguments, () -> intDivide(a, b));
                emit("intRemainder", "(II)I", arguments, () -> intRemainder(a, b));
                emit("intBits", "(II)I", arguments, () -> intBits(a, b));
            }
            for (int n : new int[] {-1, 0, 31, 32, 33, 63, 64})
                emit("intShifts", "(II)I", "I:" + a + ",I:" + n, () -> intShifts(a, n));
        }
        for (long a : longs) {
            emit("toInt", "(J)I", "L:" + a, () -> toInt(a));
            emit("longNegate", "(J)J", "L:" + a, () -> longNegate(a));
            for (long b : new long[] {-1, 0, 1, 7, Long.MAX_VALUE}) {
                String arguments = "L:" + a + ",L:" + b;
                emit("longMath", "(JJ)J", arguments, () -> longMath(a, b));
                emit("longDivide", "(JJ)J", arguments, () -> longDivide(a, b));
                emit("longRemainder", "(JJ)J", arguments, () -> longRemainder(a, b));
                emit("longBits", "(JJ)J", arguments, () -> longBits(a, b));
                emit("compare", "(JJ)I", arguments, () -> compare(a, b));
            }
            for (int n : new int[] {-1, 0, 31, 32, 63, 64, 65})
                emit("longShifts", "(JI)J", "L:" + a + ",I:" + n, () -> longShifts(a, n));
        }
        for (int key : new int[] {-101, -100, -2, -1, 0, 1, 2, 7, 8, 10000, 10001}) {
            emit("table", "(I)I", "I:" + key, () -> table(key));
            emit("lookup", "(I)I", "I:" + key, () -> lookup(key));
        }
        for (int count : new int[] {-1, 0, 1, 5, 20, 21}) {
            emit("sum", "(I)I", "I:" + count, () -> sum(count));
            emit("factorial", "(I)J", "I:" + count, () -> factorial(count));
        }
        emit("arguments", "(IJIJ)J", "I:-7,L:9223372036854775807,I:2,L:-3", () -> arguments(-7, Long.MAX_VALUE, 2, -3));
        emit("constantInt", "()I", "", () -> constantInt());
        emit("constantLong", "()J", "", () -> constantLong());
        emit("nothing", "()V", "", () -> { nothing(); return null; });
    }
}
