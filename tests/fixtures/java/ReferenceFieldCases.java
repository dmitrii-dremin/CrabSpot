package fixtures;
import java.lang.reflect.InvocationTargetException;
interface ReferenceFieldMarker extends java.io.Serializable { int compute(int value); }
class ReferenceFieldChild extends ReferenceFieldCases implements Cloneable {
    @Override public int compute(int value) { return value * 5 - 1; }
}
public class ReferenceFieldCases extends ReferenceFieldBase implements ReferenceFieldMarker {
    ReferenceFieldCases link;
    Object object;
    ReferenceFieldMarker marker;
    Cloneable cloneable;
    java.io.Serializable serializable;
    int state;
    boolean[] booleans;
    byte[] bytes;
    char[] chars;
    short[] shorts;
    int[] ints;
    long[] longs;
    float[] floats;
    double[] doubles;
    @Override public int compute(int value) { return value * 3 + 1; }
    public int self(int value) { state = value; link = this; return link.state; }
    public int aliases(int value) { state = value; link = this; object = link; marker = link; serializable = this; return marker.compute(value) + (object == link ? 11 : 13); }
    public int baseSelf(int value) { state = value; super.link = this; touched = value + 1; return super.link.touched; }
    public int hiddenLinks(int value) { state = value; link = this; super.link = this; touched = value + 1; return link.state + super.link.touched; }
    public int clear(int value) { state = value; link = this; object = link; link = null; return link == null && object == this ? value : 19; }
    public int replaceWithNull(int value) { state = value; object = this; object = null; return object == null ? state : 19; }
    public int storedVirtual(int value) { state = value; link = this; return link.compute(value); }
    public int storedInterface(int value) { state = value; marker = this; return marker.compute(value); }
    private int callSpecial(int value) { return super.compute(value); }
    public int specialChain(int value) { state = value; link = this; return link.callSpecial(value); }
    public int cycle(int value) { state = value; link = this; link.link = link; super.link = this; super.link.link = this; return link.link == this && super.link.link == this ? value : 19; }
    public int castAfterStore(int value) { state = value; object = this; link = this; cloneable = (Cloneable) object; return cloneable == object ? value : 19; }
    public int divideAfterStore(int value) { state = value; link = this; object = link; return 7 / value; }
    public int nullAfterStore(int value) { state = value; object = this; link = null; return link.compute(value); }
    public int hiddenType(int value) { state = value; hidden = this; object = hidden; hidden = hidden; return object == this ? value : 19; }
    public int clearHidden(int value) { state = value; hidden = this; object = hidden; hidden = null; return object == this ? value : 19; }
    public int overwrite(int length) { state = length; object = this; object = new int[length]; ints = (int[]) object; return ints.length; }
    public int arrayAfterStore(int length) { state = length; link = this; object = new int[length]; ints = (int[]) object; ints[0] = length; return ints[0]; }
    public int replaceArrayWithSelf(int length) { state = length; object = new int[length]; ints = (int[]) object; object = this; return ints.length; }
    public int storeArray(int kind, int length) {
        state = kind;
        int result;
        switch (kind) {
            case 0 -> { booleans = new boolean[length]; object = booleans; cloneable = booleans; serializable = booleans; result = booleans.length; }
            case 1 -> { bytes = new byte[length]; object = bytes; cloneable = bytes; serializable = bytes; result = bytes.length; }
            case 2 -> { chars = new char[length]; object = chars; cloneable = chars; serializable = chars; result = chars.length; }
            case 3 -> { shorts = new short[length]; object = shorts; cloneable = shorts; serializable = shorts; result = shorts.length; }
            case 4 -> { ints = new int[length]; object = ints; cloneable = ints; serializable = ints; result = ints.length; }
            case 5 -> { longs = new long[length]; object = longs; cloneable = longs; serializable = longs; result = longs.length; }
            case 6 -> { floats = new float[length]; object = floats; cloneable = floats; serializable = floats; result = floats.length; }
            case 7 -> { doubles = new double[length]; object = doubles; cloneable = doubles; serializable = doubles; result = doubles.length; }
            default -> { return -1; }
        }
        return result + (object == cloneable && cloneable == serializable ? 23 : 29);
    }
    public int arrayClear(int kind, int length) {
        int result = storeArray(kind, length);
        booleans = null;
        bytes = null;
        chars = null;
        shorts = null;
        ints = null;
        longs = null;
        floats = null;
        doubles = null;
        object = null; cloneable = null; serializable = null;
        return result;
    }
    public int arrayAlias(int kind, int length) {
        int result = storeArray(kind, length); Object alias = object; object = null;
        return cloneable == serializable && alias == cloneable ? result + 1 : 31;
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
            result = value(ReferenceFieldCases.class.getMethod(method, types).invoke((kind.equals("root") ? new ReferenceFieldCases() : new ReferenceFieldChild()), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String kind : new String[] {"root", "child"}) {
            for (int value : new int[] {Integer.MIN_VALUE, -65537, -32768, -129, -1, 0, 1, 2, 127, 128, 255, 32767, 65536, Integer.MAX_VALUE}) {
                for (String method : new String[] {"self", "aliases", "baseSelf", "hiddenLinks", "clear", "replaceWithNull", "storedVirtual", "storedInterface", "specialChain", "cycle", "castAfterStore", "divideAfterStore", "nullAfterStore", "hiddenType", "clearHidden"}) emit(kind, method, "(I)I", value);
            }
            for (int length : new int[] {-1, 0, 1, 3, 17}) {
                for (String method : new String[] {"overwrite", "arrayAfterStore", "replaceArrayWithSelf"}) emit(kind, method, "(I)I", length);
                for (int element = 0; element < 8; element++) {
                    for (String method : new String[] {"storeArray", "arrayClear", "arrayAlias"}) emit(kind, method, "(II)I", element, length);
                }
            }
        }
    }
}
