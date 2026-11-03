package fixtures;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

// Original numeric-signature method bodies. Only the development oracle runs main.
public class ArrayCases {
    public static int booleanRoundTrip(int value) {
        boolean[] array = new boolean[1];
        array[0] = (value & 1) != 0;
        return array[0] ? 1 : 0;
    }
    public static int byteRoundTrip(int value) {
        byte[] array = new byte[1]; array[0] = (byte)value; return array[0];
    }
    public static int charRoundTrip(int value) {
        char[] array = new char[1]; array[0] = (char)value; return array[0];
    }
    public static int shortRoundTrip(int value) {
        short[] array = new short[1]; array[0] = (short)value; return array[0];
    }
    public static int intRoundTrip(int value) {
        int[] array = new int[1]; array[0] = value; return array[0];
    }
    public static long longRoundTrip(long value) {
        long[] array = new long[1]; array[0] = value; return array[0];
    }
    public static float floatRoundTrip(float value) {
        float[] array = new float[1]; array[0] = value; return array[0];
    }
    public static double doubleRoundTrip(double value) {
        double[] array = new double[1]; array[0] = value; return array[0];
    }
    public static long defaults(int kind) {
        switch (kind) {
            case 0: return (new boolean[2])[1] ? 1 : 0;
            case 1: return (new byte[2])[1];
            case 2: return (new char[2])[1];
            case 3: return (new short[2])[1];
            case 4: return (new int[2])[1];
            case 5: return (new long[2])[1];
            case 6: return (long)(new float[2])[1];
            default: return (long)(new double[2])[1];
        }
    }
    public static int arrayLength(int length) { return new int[length].length; }
    public static int indexedStore(int length, int index, int value) {
        int[] array = new int[length]; array[index] = value; return array[index];
    }
    public static int indexedLoad(int length, int index) {
        byte[] array = new byte[length]; return array[index];
    }
    public static int nullLength() { int[] array = null; return array.length; }
    public static int nullLoad(int index) { int[] array = null; return array[index]; }
    public static int nullStore(int index, int value) {
        int[] array = null; array[index] = value; return 1;
    }
    public static int aliases(int value) {
        int[] first = new int[2]; int[] alias = first; int[] distinct = new int[2];
        alias[1] = value;
        if (first != alias) return -100;
        if (first == distinct) return -200;
        return first[1];
    }
    public static int referenceBranches(int selector) {
        int[] array = selector == 0 ? null : new int[1];
        if (array == null) return -1;
        int result = 1;
        if (array != null) result += array.length;
        return result;
    }
    public static int filledSum(int length) {
        int[] array = new int[length];
        for (int i = 0; i < array.length; i++) array[i] = i * 7 - 13;
        int total = 0;
        for (int i = 0; i < array.length; i++) total += array[i];
        return total;
    }
    public static int literals(int index) {
        short[] array = {Short.MIN_VALUE, -1, 0, 1, Short.MAX_VALUE};
        return array[index];
    }
    public static int nestedAllocations(int length) {
        int[] before = new int[1]; before[0] = 11;
        return before[0] + filledSum(length) + intRoundTrip(19);
    }

    private static String value(Object value, boolean result) {
        if (value == null) return "V";
        if (value instanceof Integer integer) return "I:" + integer;
        if (value instanceof Long wide) return "L:" + wide;
        if (value instanceof Float single) {
            return result && Float.isNaN(single) ? "F:NaN"
                : String.format("F:%08x", Float.floatToRawIntBits(single));
        }
        if (value instanceof Double precise) {
            return result && Double.isNaN(precise) ? "D:NaN"
                : String.format("D:%016x", Double.doubleToRawLongBits(precise));
        }
        throw new IllegalArgumentException("unexpected oracle value " + value);
    }
    private static void emit(String name, String descriptor, Object... arguments) throws Exception {
        Method selected = null;
        for (Method method : ArrayCases.class.getDeclaredMethods()) {
            if (method.getName().equals(name)) selected = method;
        }
        if (selected == null) throw new IllegalArgumentException(name);
        StringBuilder input = new StringBuilder();
        for (Object argument : arguments) {
            if (!input.isEmpty()) input.append(',');
            input.append(value(argument, false));
        }
        String result;
        try {
            result = value(selected.invoke(null, arguments), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(name + "\t" + descriptor + "\t" + input + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        int[] integers = {Integer.MIN_VALUE, -65537, -65536, -32769, -32768, -257,
            -256, -129, -128, -2, -1, 0, 1, 2, 127, 128, 255, 256, 32767, 32768,
            65535, 65536, Integer.MAX_VALUE};
        for (int value : integers) {
            for (String name : new String[] {"booleanRoundTrip", "byteRoundTrip",
                    "charRoundTrip", "shortRoundTrip", "intRoundTrip"}) emit(name, "(I)I", value);
            emit("aliases", "(I)I", value);
        }
        for (long value : new long[] {Long.MIN_VALUE, Long.MIN_VALUE + 1, -4294967296L,
                -1, 0, 1, 4294967296L, Long.MAX_VALUE}) emit("longRoundTrip", "(J)J", value);
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
        for (int kind = 0; kind < 8; kind++) emit("defaults", "(I)J", kind);
        for (int length : new int[] {-2, -1, 0, 1, 2, 17, 64}) {
            emit("arrayLength", "(I)I", length);
            emit("filledSum", "(I)I", length);
            emit("nestedAllocations", "(I)I", length);
        }
        for (int length : new int[] {-1, 0, 1, 3}) {
            for (int index : new int[] {Integer.MIN_VALUE, -1, 0, 1, 2, 3, Integer.MAX_VALUE}) {
                emit("indexedStore", "(III)I", length, index, 0x12345678);
                emit("indexedLoad", "(II)I", length, index);
            }
        }
        emit("nullLength", "()I");
        for (int index : new int[] {Integer.MIN_VALUE, -1, 0, 1, Integer.MAX_VALUE}) {
            emit("nullLoad", "(I)I", index);
            emit("nullStore", "(II)I", index, 7);
        }
        for (int selector : new int[] {-1, 0, 1}) emit("referenceBranches", "(I)I", selector);
        for (int index : new int[] {-1, 0, 1, 2, 3, 4, 5}) emit("literals", "(I)I", index);
    }
}
