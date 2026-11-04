package fixtures;
import java.lang.reflect.InvocationTargetException;
interface SpecialChildDefault extends SpecialRootDefault {
    default int number(int value) { return value * 31 - 7; }
    default int rootNumber(int value) { return SpecialRootDefault.super.number(value); }
}
class SpecialCallLeaf extends SpecialCallCases {
    public int value(int value) { return value * 101; }
    protected int hook(int value) { return value * -13; }
    public long wide(long value) { return value + 0x1020304050607080L; }
    public int number(int value) { return value * 103; }
}
public class SpecialCallCases extends SpecialCallMiddle implements SpecialChildDefault {
    int hidden;
    public int value(int value) { hidden = value; return value * -5; }
    protected int hook(int value) { return value + 17; }
    public long wide(long value) { return value + 0x2030405060708090L; }
    public int number(int value) { return value * 101; }
    public int callSuper(int value) { return super.value(value); }
    public int callGrand(int value) { return super.parentValue(value); }
    public int callFinal(int value) { return super.finalBody(value); }
    public int callSuperStatic(int value) { return super.parentStatic(value); }
    public int callBeforeDivide(int value) { hidden = 99; return super.divide(value); }
    public void callSuperVoid(int value) { super.store(value); }
    public int interfaceSuper(int value) { return SpecialChildDefault.super.number(value); }
    public int classDefaultSuper(int value) { return super.number(value); }
    public int interfaceInherited(int value) { return SpecialChildDefault.super.inherited(value); }
    public int interfaceChain(int value) { return SpecialChildDefault.super.chain(value); }
    public int interfaceRootSuper(int value) { return SpecialChildDefault.super.rootNumber(value); }
    public int interfacePrivate(int value) { return SpecialChildDefault.super.privateBody(value); }
    public int callSuperArray(int length) { return super.array(length); }
    public int callSuperRecursive(int depth) { super.hidden = 7; return super.recurse(depth); }
    public long callSuperLong(long value) { return super.wide(value); }
    public float callSuperFloat(float value) { return super.single(value); }
    public double callSuperDouble(double value) { return super.precise(value); }
    public double callSuperMixed(int bias, long wide, float single, double precise) {
        return super.mixed(bias, wide, single, precise);
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
            Object receiver = leaf ? new SpecialCallLeaf() : new SpecialCallCases();
            result = value(SpecialCallCases.class.getMethod(method, types).invoke(receiver, inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println((leaf ? "leaf/" : "child/") + method + "\t" + descriptor
            + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (boolean leaf : new boolean[] {false, true}) {
            for (int value : new int[] {Integer.MIN_VALUE, -65537, -32768, -129, -1, 0, 1,
                    2, 127, 128, 255, 32767, 65536, Integer.MAX_VALUE}) {
                for (String method : new String[] {"callSuper", "callGrand", "callFinal", "callSuperStatic",
                        "callBeforeDivide", "interfaceSuper", "classDefaultSuper", "interfaceInherited",
                        "interfaceChain", "interfaceRootSuper", "interfacePrivate"}) emit(leaf, method, "(I)I", value);
                emit(leaf, "callSuperVoid", "(I)V", value);
            }
            for (int depth : new int[] {-1, 0, 1, 2, 3, 7, 12, 64}) emit(leaf, "callSuperRecursive", "(I)I", depth);
            for (int length : new int[] {-2, -1, 0, 1, 2, 17, 64}) emit(leaf, "callSuperArray", "(I)I", length);
            for (long value : new long[] {Long.MIN_VALUE, Long.MIN_VALUE + 1, -4294967296L,
                    -1, 0, 1, 4294967296L, Long.MAX_VALUE}) emit(leaf, "callSuperLong", "(J)J", value);
            for (int bits : new int[] {0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000,
                    0x3f800000, 0xbf800000, 0x7f7fffff, 0xff7fffff, 0x7f800000, 0xff800000,
                    0x7fc01234, 0xffa05678}) emit(leaf, "callSuperFloat", "(F)F", Float.intBitsToFloat(bits));
            for (long bits : new long[] {0, 0x8000000000000000L, 1, 0x8000000000000001L,
                    0x000fffffffffffffL, 0x0010000000000000L, 0x3ff0000000000000L,
                    0xbff0000000000000L, 0x7fefffffffffffffL, 0xffefffffffffffffL,
                    0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000001234L,
                    0xfff0000000005678L}) emit(leaf, "callSuperDouble", "(D)D", Double.longBitsToDouble(bits));
            emit(leaf, "callSuperMixed", "(IJFD)D", 7, 19L, 1.25f, -2.5);
            emit(leaf, "callSuperMixed", "(IJFD)D", Integer.MIN_VALUE, Long.MAX_VALUE, Float.MIN_VALUE, -0.0);
            emit(leaf, "callSuperMixed", "(IJFD)D", Integer.MAX_VALUE, Long.MIN_VALUE, -Float.MAX_VALUE, Double.MIN_VALUE);
            emit(leaf, "callSuperMixed", "(IJFD)D", -1, 0L, Float.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY);
            emit(leaf, "callSuperMixed", "(IJFD)D", 0, -1L, -0.0f, 0.0);
            emit(leaf, "callSuperMixed", "(IJFD)D", 1, 4294967296L, Float.intBitsToFloat(0x7fc01234), 17.5);
        }
    }
}
