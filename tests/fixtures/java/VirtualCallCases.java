package fixtures;

import java.lang.reflect.InvocationTargetException;

// Original body-kernel oracle. Constructors only leave default field values.
class VirtualCallLeaf extends VirtualCallCases {
    @Override protected int target(int value) { return value * -3 + 17; }
    @Override protected long target(long value) { return value ^ 0x7f10203040506070L; }
}

public class VirtualCallCases extends VirtualCallBase {
    int hidden;
    long wide;
    float single;
    double precise;
    @Override protected int target(int value) { hidden = value + 1; return hidden * 31; }
    @Override protected long target(long value) { wide = value * 5; return wide; }
    @Override protected int peer(int depth) { return depth <= 0 ? 17 : parity(depth - 1) + 1; }
    public int callInt(int value) { return target(value); }
    public long callLong(long value) { return target(value); }
    public int callInherited(int value) { return inheritedBody(value); }
    public int callFinal(int value) { return finalBody(value); }
    public int callBaseBody(int value) { return dispatchFromBase(value); }
    public int callBaseStatic(int value) { return baseCallsStatic(value); }
    public int callOverloaded(int value) { return target(value) + (int)target((long)value); }
    public int callRecursive(int depth) { return parity(depth); }
    public int recurse(int depth) { return depth <= 0 ? hidden : 1 + recurse(depth - 1); }
    public int callSelfRecursive(int depth) { hidden = 7; return recurse(depth); }
    private int privateBody(int value) { return value ^ 0x456789ab; }
    public int callPrivate(int value) { return privateBody(value); }
    public int nullCall(int value) { VirtualCallBase missing = null; return missing.inheritedBody(value); }
    public int divide(int value) { hidden = value; return 100 / value; }
    public int callBeforeDivide(int value) { super.hidden = 99; return divide(value); }
    public float floatBody(float value) { single = value; return single; }
    public float callFloat(float value) { return floatBody(value); }
    public double doubleBody(double value) { precise = value; return precise; }
    public double callDouble(double value) { return doubleBody(value); }
    public double mixed(int bias, long wide, float single, double precise) {
        hidden = bias; this.wide = wide; this.single = single; this.precise = precise;
        return hidden + this.wide + this.single + this.precise;
    }
    public double callMixed(int bias, long wide, float single, double precise) {
        return mixed(bias, wide, single, precise);
    }
    public void write(int value) { hidden = value; }
    public int callVoid(int value) { write(value); return hidden; }
    public int arrayBody(int length) {
        int[] data = new int[length];
        for (int i = 0; i < data.length; i++) data[i] = target(i);
        int sum = 0; for (int value : data) sum += value; return sum;
    }
    public int callArray(int length) { return arrayBody(length); }

    private static String value(Object value, boolean result) {
        if (value == null) return "V";
        if (value instanceof Integer integer) return "I:" + integer;
        if (value instanceof Long wide) return "L:" + wide;
        if (value instanceof Float single) return result && Float.isNaN(single) ? "F:NaN"
            : String.format("F:%08x", Float.floatToRawIntBits(single));
        if (value instanceof Double precise) return result && Double.isNaN(precise) ? "D:NaN"
            : String.format("D:%016x", Double.doubleToRawLongBits(precise));
        throw new IllegalArgumentException("unexpected oracle value");
    }
    private static void emit(boolean leaf, String method, String descriptor, Object... inputs) throws Exception {
        Class<?>[] types = new Class<?>[inputs.length];
        var arguments = new StringBuilder();
        for (int index = 0; index < inputs.length; index++) {
            Object input = inputs[index];
            types[index] = input instanceof Integer ? int.class : input instanceof Long ? long.class
                : input instanceof Float ? float.class : double.class;
            if (index != 0) arguments.append(','); arguments.append(value(input, false));
        }
        String result;
        try {
            Object receiver = leaf ? new VirtualCallLeaf() : new VirtualCallCases();
            result = value(VirtualCallCases.class.getMethod(method, types).invoke(receiver, inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println((leaf ? "leaf/" : "child/") + method + "\t" + descriptor
            + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (int value : new int[] {Integer.MIN_VALUE, -65537, -32768, -129, -1, 0, 1,
                2, 127, 128, 255, 32767, 65536, Integer.MAX_VALUE}) {
            fixtures.virtualp.PackageCallBase[] receivers = {
                new fixtures.virtualp.PackageCallBase(), new fixtures.virtualq.PackageCallMiddle(),
                new fixtures.virtualp.PackageCallBridge(), new fixtures.virtualq.PackageCallLeaf()
            };
            String[] names = {"package-base", "package-middle", "package-bridge", "package-leaf"};
            for (int index = 0; index < receivers.length; index++) {
                System.out.println(names[index] + "/dispatch\t(I)I\tI:" + value
                    + "\tI:" + receivers[index].dispatch(value));
            }
        }
        for (boolean leaf : new boolean[] {false, true}) {
            for (int value : new int[] {Integer.MIN_VALUE, -65537, -32768, -129, -1, 0, 1,
                    2, 127, 128, 255, 32767, 65536, Integer.MAX_VALUE}) {
                for (String method : new String[] {"dispatchFromBase", "callInt", "callInherited",
                        "callFinal", "callBaseBody", "callBaseStatic", "callOverloaded",
                        "callPrivate", "nullCall", "callBeforeDivide", "callVoid"}) emit(leaf, method, "(I)I", value);
            }
            for (int depth : new int[] {-1, 0, 1, 2, 3, 7, 12, 64}) {
                emit(leaf, "callRecursive", "(I)I", depth); emit(leaf, "callSelfRecursive", "(I)I", depth);
            }
            for (long value : new long[] {Long.MIN_VALUE, Long.MIN_VALUE + 1, -4294967296L,
                    -1, 0, 1, 4294967296L, Long.MAX_VALUE}) emit(leaf, "callLong", "(J)J", value);
            for (int bits : new int[] {0, 0x80000000, 1, 0x80000001, 0x007fffff,
                    0x00800000, 0x3f800000, 0xbf800000, 0x7f7fffff, 0xff7fffff,
                    0x7f800000, 0xff800000, 0x7fc01234, 0xffa05678}) emit(leaf, "callFloat", "(F)F", Float.intBitsToFloat(bits));
            for (long bits : new long[] {0, 0x8000000000000000L, 1, 0x8000000000000001L,
                    0x000fffffffffffffL, 0x0010000000000000L, 0x3ff0000000000000L,
                    0xbff0000000000000L, 0x7fefffffffffffffL, 0xffefffffffffffffL,
                    0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000001234L,
                    0xfff0000000005678L}) emit(leaf, "callDouble", "(D)D", Double.longBitsToDouble(bits));
            for (int length : new int[] {-2, -1, 0, 1, 2, 17, 64}) emit(leaf, "callArray", "(I)I", length);
            emit(leaf, "callMixed", "(IJFD)D", 7, 19L, 1.25f, -2.5);
            emit(leaf, "callMixed", "(IJFD)D", Integer.MIN_VALUE, Long.MAX_VALUE, Float.MIN_VALUE, -0.0);
            emit(leaf, "callMixed", "(IJFD)D", Integer.MAX_VALUE, Long.MIN_VALUE, -Float.MAX_VALUE, Double.MIN_VALUE);
            emit(leaf, "callMixed", "(IJFD)D", -1, 0L, Float.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY);
            emit(leaf, "callMixed", "(IJFD)D", 0, -1L, -0.0f, 0.0);
            emit(leaf, "callMixed", "(IJFD)D", 1, 4294967296L, Float.intBitsToFloat(0x7fc01234), 17.5);
        }
    }
}
