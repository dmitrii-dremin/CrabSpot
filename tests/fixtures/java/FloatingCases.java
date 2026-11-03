package fixtures;

/** Primitive floating method bodies, with a JDK-only reference-output harness. */
public final class FloatingCases {
    public static float fAdd(float a, float b) { return a + b; }
    public static float fSub(float a, float b) { return a - b; }
    public static float fMul(float a, float b) { return a * b; }
    public static float fDiv(float a, float b) { return a / b; }
    public static float fRem(float a, float b) { return a % b; }
    public static float fNeg(float a) { return -a; }
    public static int fLess(float a, float b) { return a < b ? 1 : 0; }
    public static int fGreater(float a, float b) { return a > b ? 1 : 0; }
    public static int fEqual(float a, float b) { return a == b ? 1 : 0; }
    public static double dAdd(double a, double b) { return a + b; }
    public static double dSub(double a, double b) { return a - b; }
    public static double dMul(double a, double b) { return a * b; }
    public static double dDiv(double a, double b) { return a / b; }
    public static double dRem(double a, double b) { return a % b; }
    public static double dNeg(double a) { return -a; }
    public static int dLess(double a, double b) { return a < b ? 1 : 0; }
    public static int dGreater(double a, double b) { return a > b ? 1 : 0; }
    public static int dEqual(double a, double b) { return a == b ? 1 : 0; }
    public static int fToInt(float a) { return (int) a; }
    public static long fToLong(float a) { return (long) a; }
    public static int dToInt(double a) { return (int) a; }
    public static long dToLong(double a) { return (long) a; }
    public static float intToFloat(int a) { return a; }
    public static double intToDouble(int a) { return a; }
    public static float longToFloat(long a) { return a; }
    public static double longToDouble(long a) { return a; }
    public static double fToDouble(float a) { return a; }
    public static float dToFloat(double a) { return (float) a; }
    public static float fConstant() { return 3.1415927f; }
    public static double dConstant() { return 3.141592653589793; }
    public static float fSmall() { return 2.0f; }
    public static double dSmall() { return 1.0; }
    public static float fUnfused(float a, float b, float c) { return a * b + c; }
    public static double dUnfused(double a, double b, double c) { return a * b + c; }
    public static double mixed(int a, float b, long c, double d) { return a + b + c + d; }

    @FunctionalInterface private interface Case { Object run(); }
    private static String f(float value) { return "F:" + String.format(java.util.Locale.ROOT, "%08x", Float.floatToRawIntBits(value)); }
    private static String d(double value) { return "D:" + String.format(java.util.Locale.ROOT, "%016x", Double.doubleToRawLongBits(value)); }
    private static void emit(String method, String descriptor, String arguments, Case body) {
        Object value = body.run();
        String result = value instanceof Float ? (Float.isNaN((Float) value) ? "F:NaN" : f((Float) value))
            : value instanceof Double ? (Double.isNaN((Double) value) ? "D:NaN" : d((Double) value))
            : value instanceof Long ? "L:" + value : "I:" + value;
        System.out.println(method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) {
        int[] floats = {0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000,
            0x3f000000, 0x3f800000, 0xbf800000, 0x3f800001, 0x4b800000,
            0x7f7fffff, 0xff7fffff, 0x7f800000, 0xff800000, 0x7fc00001, 0x7f800001,
            0x4f000000, 0xcf000000, 0x5f000000, 0xdf000000};
        long[] doubles = {0, 0x8000000000000000L, 1, 0x8000000000000001L,
            0x000fffffffffffffL, 0x0010000000000000L, 0x3fe0000000000000L,
            0x3ff0000000000000L, 0xbff0000000000000L, 0x3ff0000000000001L,
            0x4340000000000000L, 0x7fefffffffffffffL, 0xffefffffffffffffL,
            0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000000001L,
            0x7ff0000000000001L, 0x41e0000000000000L, 0xc1e0000000000000L,
            0x43e0000000000000L, 0xc3e0000000000000L};
        for (int bits : floats) {
            float a = Float.intBitsToFloat(bits);
            emit("fNeg", "(F)F", f(a), () -> fNeg(a));
            emit("fToInt", "(F)I", f(a), () -> fToInt(a));
            emit("fToLong", "(F)J", f(a), () -> fToLong(a));
            emit("fToDouble", "(F)D", f(a), () -> fToDouble(a));
            for (int other : floats) {
                float b = Float.intBitsToFloat(other);
                String arguments = f(a) + "," + f(b);
                emit("fAdd", "(FF)F", arguments, () -> fAdd(a, b));
                emit("fSub", "(FF)F", arguments, () -> fSub(a, b));
                emit("fMul", "(FF)F", arguments, () -> fMul(a, b));
                emit("fDiv", "(FF)F", arguments, () -> fDiv(a, b));
                emit("fRem", "(FF)F", arguments, () -> fRem(a, b));
                emit("fLess", "(FF)I", arguments, () -> fLess(a, b));
                emit("fGreater", "(FF)I", arguments, () -> fGreater(a, b));
                emit("fEqual", "(FF)I", arguments, () -> fEqual(a, b));
            }
        }
        for (long bits : doubles) {
            double a = Double.longBitsToDouble(bits);
            emit("dNeg", "(D)D", d(a), () -> dNeg(a));
            emit("dToInt", "(D)I", d(a), () -> dToInt(a));
            emit("dToLong", "(D)J", d(a), () -> dToLong(a));
            emit("dToFloat", "(D)F", d(a), () -> dToFloat(a));
            for (long other : doubles) {
                double b = Double.longBitsToDouble(other);
                String arguments = d(a) + "," + d(b);
                emit("dAdd", "(DD)D", arguments, () -> dAdd(a, b));
                emit("dSub", "(DD)D", arguments, () -> dSub(a, b));
                emit("dMul", "(DD)D", arguments, () -> dMul(a, b));
                emit("dDiv", "(DD)D", arguments, () -> dDiv(a, b));
                emit("dRem", "(DD)D", arguments, () -> dRem(a, b));
                emit("dLess", "(DD)I", arguments, () -> dLess(a, b));
                emit("dGreater", "(DD)I", arguments, () -> dGreater(a, b));
                emit("dEqual", "(DD)I", arguments, () -> dEqual(a, b));
            }
        }
        for (int a : new int[] {Integer.MIN_VALUE, -16777217, -1, 0, 1, 16777217, Integer.MAX_VALUE}) {
            emit("intToFloat", "(I)F", "I:" + a, () -> intToFloat(a));
            emit("intToDouble", "(I)D", "I:" + a, () -> intToDouble(a));
        }
        long rounding = (1L << 62) + (1L << 38) + 1;
        for (long a : new long[] {Long.MIN_VALUE, -rounding, -9007199254740993L, -1, 0, 1, 9007199254740993L, rounding, Long.MAX_VALUE}) {
            emit("longToFloat", "(J)F", "L:" + a, () -> longToFloat(a));
            emit("longToDouble", "(J)D", "L:" + a, () -> longToDouble(a));
        }
        emit("fConstant", "()F", "", () -> fConstant());
        emit("dConstant", "()D", "", () -> dConstant());
        emit("fSmall", "()F", "", () -> fSmall());
        emit("dSmall", "()D", "", () -> dSmall());
        float fa = Float.intBitsToFloat(0x3f800001), fc = Float.intBitsToFloat(0xbf800002);
        emit("fUnfused", "(FFF)F", f(fa) + "," + f(fa) + "," + f(fc), () -> fUnfused(fa, fa, fc));
        double da = Double.longBitsToDouble(0x3ff0000000000001L), dc = Double.longBitsToDouble(0xbff0000000000002L);
        emit("dUnfused", "(DDD)D", d(da) + "," + d(da) + "," + d(dc), () -> dUnfused(da, da, dc));
        emit("mixed", "(IFJD)D", "I:7," + f(1.25f) + ",L:9007199254740993," + d(-2.5), () -> mixed(7, 1.25f, 9007199254740993L, -2.5));
    }
}
