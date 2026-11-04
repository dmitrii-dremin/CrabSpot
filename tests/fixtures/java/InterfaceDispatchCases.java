package fixtures;

import java.lang.reflect.InvocationTargetException;

interface DispatchRoot {
    default int number(int value) { return value * 3 + 1; }
    default long wide(long value) { return value ^ 0x123456789abcdef0L; }
    default float single(float value) { return value; }
    default double precise(double value) { return value; }
    void write(int value);
    int read();
    int step(int depth);
    default int viaNumber(int value) { return number(value) + 17; }
    private int privateNumber(int value) { return value ^ 0x56789abc; }
    default int viaPrivate(int value) { return privateNumber(value); }
    private static int helper(int value) { return value * 19 - 3; }
    default int viaStatic(int value) { return helper(value); }
    default int writeRead(int value) { write(value); return read(); }
    default void writeOnly(int value) { write(value); }
    default int beforeDivide(int value) { write(value); return 100 / value; }
    default double mixed(int bias, long wide, float single, double precise) {
        write(bias); return read() + wide + single + precise;
    }
    default int recurse(int depth) { return depth <= 0 ? read() : 1 + recurse(depth - 1); }
    default int cross(int depth) { return depth <= 0 ? read() : 1 + step(depth - 1); }
    default int array(int length) {
        int[] values = new int[length];
        for (int index = 0; index < length; index++) values[index] = number(index);
        int sum = 0; for (int value : values) sum += value; return sum;
    }
}
interface DispatchChild extends DispatchRoot {
    default int number(int value) { return value * 31 - 7; }
    default long wide(long value) { return value * 5; }
}
interface DispatchLeft extends DispatchRoot {}
interface DispatchRight extends DispatchRoot {}
interface DispatchDiamond extends DispatchLeft, DispatchRight {}
interface DispatchSpecific extends DispatchLeft {
    default int number(int value) { return value ^ 0x3456789a; }
}
interface DispatchSpecificDiamond extends DispatchSpecific, DispatchRight {}
interface DispatchRedeclared extends DispatchRoot { int number(int value); }

class DispatchChildCases extends InterfaceDispatchCases implements DispatchChild {}
class DispatchLeafCases extends DispatchChildCases {
    public int number(int value) { return value * -13 + 5; }
    public long wide(long value) { return value + 0x1020304050607080L; }
}
class DispatchDiamondCases extends InterfaceDispatchCases implements DispatchDiamond {}
class DispatchSpecificCases extends InterfaceDispatchCases implements DispatchSpecificDiamond {}
class DispatchMaskBase extends InterfaceDispatchCases {
    public int number(int value) { return value - 29; }
}
class DispatchMaskedCases extends DispatchMaskBase implements DispatchChild {}
class DispatchRedeclaredCases extends InterfaceDispatchCases implements DispatchRedeclared {
    public int number(int value) { return value + 97; }
}

/** Original Java 21 fixtures and an explicitly development-only reference driver. */
public class InterfaceDispatchCases implements DispatchRoot {
    int state;
    public void write(int value) { state = value; }
    public int read() { return state; }
    public int step(int depth) { return cross(depth); }
    public int interfaceNumber(int value) { DispatchRoot receiver = this; return receiver.number(value); }
    public int virtualNumber(int value) { return number(value); }
    public int interfaceViaNumber(int value) { DispatchRoot receiver = this; return receiver.viaNumber(value); }
    public int interfacePrivate(int value) { DispatchRoot receiver = this; return receiver.viaPrivate(value); }
    public int interfaceStatic(int value) { DispatchRoot receiver = this; return receiver.viaStatic(value); }
    public int interfaceWrite(int value) { DispatchRoot receiver = this; return receiver.writeRead(value); }
    public void interfaceVoid(int value) { DispatchRoot receiver = this; receiver.writeOnly(value); }
    public int interfaceBeforeDivide(int value) { DispatchRoot receiver = this; return receiver.beforeDivide(value); }
    public int interfaceNull(int value) { state = value; DispatchRoot receiver = null; return receiver.number(value); }
    public int interfaceRecursive(int depth) { write(7); DispatchRoot receiver = this; return receiver.recurse(depth); }
    public int interfaceCross(int depth) { write(11); DispatchRoot receiver = this; return receiver.cross(depth); }
    public int interfaceArray(int length) { DispatchRoot receiver = this; return receiver.array(length); }
    public long interfaceLong(long value) { DispatchRoot receiver = this; return receiver.wide(value); }
    public float interfaceFloat(float value) { DispatchRoot receiver = this; return receiver.single(value); }
    public double interfaceDouble(double value) { DispatchRoot receiver = this; return receiver.precise(value); }
    public double interfaceMixed(int bias, long wide, float single, double precise) {
        DispatchRoot receiver = this; return receiver.mixed(bias, wide, single, precise);
    }
    private static Object receiver(String kind) {
        return switch (kind) {
            case "root" -> new InterfaceDispatchCases();
            case "child" -> new DispatchChildCases();
            case "leaf" -> new DispatchLeafCases();
            case "diamond" -> new DispatchDiamondCases();
            case "specific" -> new DispatchSpecificCases();
            case "masked" -> new DispatchMaskedCases();
            case "redeclared" -> new DispatchRedeclaredCases();
            default -> throw new IllegalArgumentException(kind);
        };
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
    private static void emit(String kind, String method, String descriptor, Object... inputs) throws Exception {
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
            result = value(InterfaceDispatchCases.class.getMethod(method, types).invoke(receiver(kind), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String kind : new String[] {"root", "child", "leaf", "diamond", "specific", "masked", "redeclared"}) {
            for (int value : new int[] {Integer.MIN_VALUE, -65537, -32768, -129, -1, 0, 1,
                    2, 127, 128, 255, 32767, 65536, Integer.MAX_VALUE}) {
                for (String method : new String[] {"interfaceNumber", "virtualNumber", "interfaceViaNumber",
                        "interfacePrivate", "interfaceStatic", "interfaceWrite", "interfaceBeforeDivide",
                        "interfaceNull"}) emit(kind, method, "(I)I", value);
                emit(kind, "interfaceVoid", "(I)V", value);
            }
            for (int depth : new int[] {-1, 0, 1, 2, 3, 7, 12, 64}) {
                emit(kind, "interfaceRecursive", "(I)I", depth); emit(kind, "interfaceCross", "(I)I", depth);
            }
            for (int length : new int[] {-2, -1, 0, 1, 2, 17, 64}) emit(kind, "interfaceArray", "(I)I", length);
            for (long value : new long[] {Long.MIN_VALUE, Long.MIN_VALUE + 1, -4294967296L,
                    -1, 0, 1, 4294967296L, Long.MAX_VALUE}) emit(kind, "interfaceLong", "(J)J", value);
            for (int bits : new int[] {0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000,
                    0x3f800000, 0xbf800000, 0x7f7fffff, 0xff7fffff, 0x7f800000, 0xff800000,
                    0x7fc01234, 0xffa05678}) emit(kind, "interfaceFloat", "(F)F", Float.intBitsToFloat(bits));
            for (long bits : new long[] {0, 0x8000000000000000L, 1, 0x8000000000000001L,
                    0x000fffffffffffffL, 0x0010000000000000L, 0x3ff0000000000000L,
                    0xbff0000000000000L, 0x7fefffffffffffffL, 0xffefffffffffffffL,
                    0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000001234L,
                    0xfff0000000005678L}) emit(kind, "interfaceDouble", "(D)D", Double.longBitsToDouble(bits));
            emit(kind, "interfaceMixed", "(IJFD)D", 7, 19L, 1.25f, -2.5);
            emit(kind, "interfaceMixed", "(IJFD)D", Integer.MIN_VALUE, Long.MAX_VALUE, Float.MIN_VALUE, -0.0);
            emit(kind, "interfaceMixed", "(IJFD)D", Integer.MAX_VALUE, Long.MIN_VALUE, -Float.MAX_VALUE, Double.MIN_VALUE);
            emit(kind, "interfaceMixed", "(IJFD)D", -1, 0L, Float.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY);
            emit(kind, "interfaceMixed", "(IJFD)D", 0, -1L, -0.0f, 0.0);
            emit(kind, "interfaceMixed", "(IJFD)D", 1, 4294967296L, Float.intBitsToFloat(0x7fc01234), 17.5);
        }
    }
}
