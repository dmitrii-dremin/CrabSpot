package fixtures;
import java.lang.reflect.InvocationTargetException;
interface CastRootMarker extends java.io.Serializable {
    default int defaultType(int value) { Object receiver = value == 0 ? null : this; return receiver instanceof CastChild ? 1 : 0; }
}
interface CastLeftMarker extends CastRootMarker {}
interface CastRightMarker extends CastRootMarker {}
interface CastOtherMarker {}
class CastChild extends CastCases implements CastOtherMarker, Cloneable {
    public int childOnly(int value) { return value * 3 + 1; }
}
class CastUnrelated {}
class CastNullTarget {}
public class CastCases extends CastBase implements CastLeftMarker, CastRightMarker {
    int state;
    public int instanceMask(int value) {
        state = value; Object receiver = value == 0 ? null : this; int mask = 0;
        if (receiver instanceof Object) mask |= 1;
        if (receiver instanceof CastBase) mask |= 2;
        if (receiver instanceof CastCases) mask |= 4;
        if (receiver instanceof CastChild) mask |= 8;
        if (receiver instanceof CastRootMarker) mask |= 16;
        if (receiver instanceof CastLeftMarker) mask |= 32;
        if (receiver instanceof CastRightMarker) mask |= 64;
        if (receiver instanceof CastOtherMarker) mask |= 128;
        if (receiver instanceof java.io.Serializable) mask |= 256;
        if (receiver instanceof Cloneable) mask |= 512;
        if (receiver instanceof CastUnrelated) mask |= 1024;
        if (receiver instanceof int[]) mask |= 2048;
        if (receiver instanceof Object[]) mask |= 4096;
        if (receiver instanceof int[][]) mask |= 8192;
        return mask;
    }
    public int castSelf(int value) { state = value; Object receiver = value == 0 ? null : this; CastCases cast = (CastCases) receiver; return cast == null ? 17 : cast.state; }
    public int castChild(int value) { state = value; Object receiver = value == 0 ? null : this; CastChild cast = (CastChild) receiver; return cast == null ? 17 : cast.childOnly(value); }
    public int castOther(int value) { state = value; Object receiver = value == 0 ? null : this; CastUnrelated cast = (CastUnrelated) receiver; return cast == null ? 17 : 1; }
    public int castInterface(int value) { state = value; Object receiver = value == 0 ? null : this; CastOtherMarker cast = (CastOtherMarker) receiver; return cast == null ? 17 : value; }
    public int castSerializable(int value) { state = value; Object receiver = value == 0 ? null : this; java.io.Serializable cast = (java.io.Serializable) receiver; return cast == null ? 17 : value; }
    public int castCloneable(int value) { state = value; Object receiver = value == 0 ? null : this; Cloneable cast = (Cloneable) receiver; return cast == null ? 17 : value; }
    public int castPrimitiveArray(int value) { state = value; Object receiver = value == 0 ? null : this; int[] cast = (int[]) receiver; return cast == null ? 17 : cast.length; }
    public int castReferenceArray(int value) { state = value; Object receiver = value == 0 ? null : this; Object[] cast = (Object[]) receiver; return cast == null ? 17 : cast.length; }
    public int castRankArray(int value) { state = value; Object receiver = value == 0 ? null : this; int[][] cast = (int[][]) receiver; return cast == null ? 17 : cast.length; }
    public int callBase(int value) { state = value; return baseTest(value); }
    public int callSpecialBase(int value) { state = value; return super.baseTest(value); }
    public int callInterface(int value) { state = value; CastRootMarker receiver = this; return receiver.defaultType(value); }
    public int nullUnregistered(int value) { state = value; Object receiver = null; CastNullTarget cast = (CastNullTarget) receiver; return cast == null ? value : 7; }
    public int nullTestUnregistered(int value) { state = value; Object receiver = null; return receiver instanceof CastNullTarget ? 7 : value; }
    private static int staticArray(int length) { Object receiver = new byte[length]; return receiver instanceof byte[] ? ((byte[]) receiver).length : -1; }
    public int callStaticArray(int length) { state = length; return staticArray(length); }
    public int arrayMask(int kind, int length) {
        state = kind;
        Object receiver = switch (kind) {
            case 0 -> new boolean[length];
            case 1 -> new byte[length];
            case 2 -> new char[length];
            case 3 -> new short[length];
            case 4 -> new int[length];
            case 5 -> new long[length];
            case 6 -> new float[length];
            case 7 -> new double[length];
            default -> null;
        };
        int mask = 0;
        if (receiver instanceof Object) mask |= 1;
        if (receiver instanceof Cloneable) mask |= 2;
        if (receiver instanceof java.io.Serializable) mask |= 4;
        if (receiver instanceof boolean[]) mask |= 8;
        if (receiver instanceof byte[]) mask |= 16;
        if (receiver instanceof char[]) mask |= 32;
        if (receiver instanceof short[]) mask |= 64;
        if (receiver instanceof int[]) mask |= 128;
        if (receiver instanceof long[]) mask |= 256;
        if (receiver instanceof float[]) mask |= 512;
        if (receiver instanceof double[]) mask |= 1024;
        if (receiver instanceof Object[]) mask |= 2048;
        if (receiver instanceof Cloneable[]) mask |= 4096;
        if (receiver instanceof java.io.Serializable[]) mask |= 8192;
        if (receiver instanceof Object[][]) mask |= 16384;
        if (receiver instanceof int[][]) mask |= 32768;
        if (receiver instanceof CastCases) mask |= 65536;
        return mask;
    }
    public int arrayCast(int kind, int target, int length) {
        state = kind;
        Object receiver = switch (kind) {
            case 0 -> new boolean[length];
            case 1 -> new byte[length];
            case 2 -> new char[length];
            case 3 -> new short[length];
            case 4 -> new int[length];
            case 5 -> new long[length];
            case 6 -> new float[length];
            case 7 -> new double[length];
            default -> null;
        };
        return switch (target) {
            case 0 -> { boolean[] cast = (boolean[]) receiver; yield cast == null ? 17 : cast.length; }
            case 1 -> { byte[] cast = (byte[]) receiver; yield cast == null ? 17 : cast.length; }
            case 2 -> { char[] cast = (char[]) receiver; yield cast == null ? 17 : cast.length; }
            case 3 -> { short[] cast = (short[]) receiver; yield cast == null ? 17 : cast.length; }
            case 4 -> { int[] cast = (int[]) receiver; yield cast == null ? 17 : cast.length; }
            case 5 -> { long[] cast = (long[]) receiver; yield cast == null ? 17 : cast.length; }
            case 6 -> { float[] cast = (float[]) receiver; yield cast == null ? 17 : cast.length; }
            case 7 -> { double[] cast = (double[]) receiver; yield cast == null ? 17 : cast.length; }
            case 8 -> { Object cast = (Object) receiver; yield cast == null ? 17 : cast == receiver ? 11 : 12; }
            case 9 -> { Cloneable cast = (Cloneable) receiver; yield cast == null ? 17 : cast == receiver ? 11 : 12; }
            case 10 -> { java.io.Serializable cast = (java.io.Serializable) receiver; yield cast == null ? 17 : cast == receiver ? 11 : 12; }
            case 11 -> { Object[] cast = (Object[]) receiver; yield cast == null ? 17 : cast.length; }
            case 12 -> { int[][] cast = (int[][]) receiver; yield cast == null ? 17 : cast.length; }
            default -> { CastCases cast = (CastCases) receiver; yield cast == null ? 17 : cast.state; }
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
            result = value(CastCases.class.getMethod(method, types).invoke((kind.equals("root") ? new CastCases() : new CastChild()), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String kind : new String[] {"root", "child"}) {
            for (int value : new int[] {Integer.MIN_VALUE, -65537, -32768, -129, -1, 0, 1, 2, 127, 128, 255, 32767, 65536, Integer.MAX_VALUE}) {
                for (String method : new String[] {"instanceMask", "castSelf", "castChild", "castOther", "castInterface",
                        "castSerializable", "castCloneable", "castPrimitiveArray", "castReferenceArray", "castRankArray",
                        "callBase", "callSpecialBase", "callInterface", "nullUnregistered", "nullTestUnregistered"}) emit(kind, method, "(I)I", value);
            }
            for (int length : new int[] {-1, 0, 1, 7, 17}) {
                emit(kind, "callStaticArray", "(I)I", length);
                for (int array = 0; array <= 8; array++) emit(kind, "arrayMask", "(II)I", array, length);
            }
            for (int array = 0; array <= 8; array++) for (int target = 0; target < 14; target++)
                for (int length : new int[] {-1, 0, 3}) emit(kind, "arrayCast", "(III)I", array, target, length);
        }
    }
}
