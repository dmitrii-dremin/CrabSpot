package fixtures;

import java.lang.reflect.InvocationTargetException;

// Original development fixture. Its constructor leaves all instance fields at
// defaults; matching body inputs do not assert that CrabSpot ran a constructor.
class InstanceBodyBase {
    int hidden;
    protected long inherited;
    public int inheritedBody(int value) { hidden = value; return hidden + 3; }
}

public class InstanceBodyCases extends InstanceBodyBase {
    boolean flag;
    byte small;
    char letter;
    short narrow;
    int integer;
    long wide;
    float single;
    double precise;
    Object link;
    int hidden;

    public int booleanRoundTrip(int value) { flag = (value & 1) != 0; return flag ? 1 : 0; }
    public int byteRoundTrip(int value) { small = (byte)value; return small; }
    public int charRoundTrip(int value) { letter = (char)value; return letter; }
    public int shortRoundTrip(int value) { narrow = (short)value; return narrow; }
    public int intRoundTrip(int value) { integer = value; return integer; }
    public long longRoundTrip(long value) { wide = value; return wide; }
    public float floatRoundTrip(float value) { single = value; return single; }
    public double doubleRoundTrip(double value) { precise = value; return precise; }
    public int hiddenOwners(int value) {
        super.hidden = value; hidden = value + 1; return super.hidden * 31 + hidden;
    }
    public long inheritedField(long value) { inherited = value; return inherited; }
    public int repeatedUpdates(int value) {
        integer = value;
        for (int index = 0; index < 7; index++) integer = integer * 17 + index;
        return integer;
    }
    public int aliasReceiver(int value) {
        InstanceBodyCases alias = this; alias.integer = value;
        return this == alias ? integer : -1;
    }
    public int nullReferenceField(int value) { link = null; return link == null ? value : -1; }
    public int nullRead(int ignored) { InstanceBodyCases missing = null; return missing.integer; }
    public int nullWrite(int value) { InstanceBodyCases missing = null; missing.integer = value; return 1; }
    public int storeBeforeDivide(int value) { integer = value; return 100 / value; }
    public int arraysAndFields(int length) {
        int[] values = new int[length];
        integer = 0;
        for (int index = 0; index < values.length; index++) {
            values[index] = index * 7; integer += values[index];
        }
        return integer;
    }
    private static int numericHelper(int value) { return value * 3 + 1; }
    public int staticHelper(int value) { integer = numericHelper(value); return integer; }
    public void writeOnly(int value) { integer = value; }
    public double mixedWidths(int bias, long wide, float single, double precise) {
        integer = bias; this.wide = wide; this.single = single; this.precise = precise;
        return integer + this.wide + this.single + this.precise;
    }

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
    private static void emit(String method, String descriptor, Object... inputs) throws Exception {
        Class<?>[] types = new Class<?>[inputs.length];
        var arguments = new StringBuilder();
        for (int index = 0; index < inputs.length; index++) {
            Object input = inputs[index];
            types[index] = input instanceof Integer ? int.class : input instanceof Long ? long.class
                : input instanceof Float ? float.class : double.class;
            if (index != 0) arguments.append(',');
            arguments.append(value(input, false));
        }
        String result;
        try {
            result = value(InstanceBodyCases.class.getMethod(method, types)
                .invoke(new InstanceBodyCases(), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (int value : new int[] {Integer.MIN_VALUE, -65537, -65536, -32769, -32768,
                -257, -256, -129, -128, -2, -1, 0, 1, 2, 127, 128, 255, 256,
                32767, 32768, 65535, 65536, Integer.MAX_VALUE}) {
            for (String method : new String[] {"booleanRoundTrip", "byteRoundTrip",
                    "charRoundTrip", "shortRoundTrip", "intRoundTrip", "hiddenOwners",
                    "repeatedUpdates", "aliasReceiver", "nullReferenceField",
                    "nullRead", "nullWrite", "storeBeforeDivide", "staticHelper", "inheritedBody"}) {
                emit(method, "(I)I", value);
            }
            emit("writeOnly", "(I)V", value);
        }
        for (long value : new long[] {Long.MIN_VALUE, Long.MIN_VALUE + 1, -4294967296L,
                -1, 0, 1, 4294967296L, Long.MAX_VALUE}) {
            emit("longRoundTrip", "(J)J", value); emit("inheritedField", "(J)J", value);
        }
        for (int bits : new int[] {0, 0x80000000, 1, 0x80000001, 0x007fffff,
                0x00800000, 0x3f800000, 0xbf800000, 0x7f7fffff, 0xff7fffff,
                0x7f800000, 0xff800000, 0x7fc01234, 0xffa05678}) {
            emit("floatRoundTrip", "(F)F", Float.intBitsToFloat(bits));
        }
        for (long bits : new long[] {0, 0x8000000000000000L, 1, 0x8000000000000001L,
                0x000fffffffffffffL, 0x0010000000000000L, 0x3ff0000000000000L,
                0xbff0000000000000L, 0x7fefffffffffffffL, 0xffefffffffffffffL,
                0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000001234L,
                0xfff0000000005678L}) emit("doubleRoundTrip", "(D)D", Double.longBitsToDouble(bits));
        for (int length : new int[] {-2, -1, 0, 1, 2, 17, 64}) emit("arraysAndFields", "(I)I", length);
        emit("mixedWidths", "(IJFD)D", 7, 19L, 1.25f, -2.5);
        emit("mixedWidths", "(IJFD)D", Integer.MIN_VALUE, Long.MAX_VALUE, Float.MIN_VALUE, -0.0);
        emit("mixedWidths", "(IJFD)D", Integer.MAX_VALUE, Long.MIN_VALUE, -Float.MAX_VALUE, Double.MIN_VALUE);
        emit("mixedWidths", "(IJFD)D", -1, 0L, Float.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY);
        emit("mixedWidths", "(IJFD)D", 0, -1L, -0.0f, 0.0);
        emit("mixedWidths", "(IJFD)D", 1, 4294967296L, Float.intBitsToFloat(0x7fc01234), 17.5);
    }
}
