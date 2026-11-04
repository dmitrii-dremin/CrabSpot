package fixtures;
import java.lang.reflect.InvocationTargetException;
interface ReferenceArrayMarker extends java.io.Serializable { int compute(int value); }
interface ReferenceArrayOther { int compute(int value); }
class ReferenceArrayChild extends ReferenceArrayCases implements ReferenceArrayOther, Cloneable {
    @Override public int compute(int value) { return value * 5 - 1; }
}
class ReferenceArrayUnrelated {}
public class ReferenceArrayCases extends ReferenceArrayBase implements ReferenceArrayMarker {
    int state;
    Object[] saved;
    Object item;
    Object auxiliary;
    @Override public int compute(int value) { return value * 3 + 1; }
    private int special(int value) { return super.compute(value); }
    public int allocateNominal(int kind, int length) {
        state = kind;
        saved = switch (kind) {
            case 0 -> new Object[length];
            case 1 -> new ReferenceArrayBase[length];
            case 2 -> new ReferenceArrayCases[length];
            case 3 -> new ReferenceArrayChild[length];
            case 4 -> new ReferenceArrayMarker[length];
            case 5 -> new ReferenceArrayOther[length];
            case 6 -> new ReferenceArrayUnrelated[length];
            case 7 -> new Cloneable[length];
            case 8 -> new java.io.Serializable[length];
            default -> new Object[length];
        };
        int zeros = 0;
        for (int index = 0; index < saved.length; index++) if (saved[index] == null) zeros++;
        int mask = 0;
        if (saved instanceof Object[]) mask |= 1;
        if (saved instanceof ReferenceArrayBase[]) mask |= 2;
        if (saved instanceof ReferenceArrayCases[]) mask |= 4;
        if (saved instanceof ReferenceArrayChild[]) mask |= 8;
        if (saved instanceof ReferenceArrayMarker[]) mask |= 16;
        if (saved instanceof ReferenceArrayOther[]) mask |= 32;
        if (saved instanceof ReferenceArrayUnrelated[]) mask |= 64;
        if (saved instanceof Cloneable[]) mask |= 128;
        if (saved instanceof java.io.Serializable[]) mask |= 256;
        return saved.length * 31 + zeros + mask;
    }
    private int primitive(int kind, int length) {
        auxiliary = switch (kind) {
            case 0 -> new boolean[length];
            case 1 -> new byte[length];
            case 2 -> new char[length];
            case 3 -> new short[length];
            case 4 -> new int[length];
            case 5 -> new long[length];
            case 6 -> new float[length];
            case 7 -> new double[length];
            default -> new int[length];
        };
        return length;
    }
    public int storeSelf(int kind, int length, int index) {
        allocateNominal(kind, length); saved[index] = this; item = saved[index];
        return item == this ? compute(state) + saved.length : -1;
    }
    public int storeNull(int kind, int length, int index) {
        allocateNominal(kind, length); saved[index] = null; item = saved[index];
        return item == null ? saved.length : -1;
    }
    public int storePrimitive(int kind, int element, int length, int index, int width) {
        allocateNominal(kind, length); primitive(element, width); saved[index] = auxiliary;
        item = saved[index]; return item == auxiliary ? length + width : -1;
    }
    public int storeArray(int kind, int length, int index, int width) {
        allocateNominal(kind, length); auxiliary = new Object[width]; saved[index] = auxiliary;
        item = saved[index]; return ((Object[]) item).length + saved.length;
    }
    public int cycle(int length, int index) {
        state = length; saved = new Object[length]; saved[index] = saved; item = saved[index];
        return item == saved ? saved.length : -1;
    }
    public int aliases(int length) {
        state = length; saved = new Object[length]; saved[0] = this; saved[1] = this;
        item = saved[0]; saved[0] = null; return item == this && saved[1] == this ? length : -1;
    }
    public int overwrite(int length, int index) {
        state = length; saved = new Object[length]; saved[index] = this; primitive(4, 1);
        saved[index] = auxiliary; item = saved[index]; saved[index] = null;
        return ((int[]) item).length;
    }
    public int failedOverwrite(int length, int index) {
        state = length; saved = new ReferenceArrayCases[length]; saved[index] = this;
        item = saved[index]; primitive(4, 1); saved[index] = auxiliary; return -1;
    }
    public int nullStore(int index) {
        state = index; saved = null; primitive(4, 1); saved[index] = auxiliary; return -1;
    }
    public int nullLoad(int index) { state = index; saved = null; return saved[index] == null ? 1 : 2; }
    public int negativeAfterStore(int length) {
        state = length; saved = new Object[1]; saved[0] = this; item = saved[0];
        auxiliary = new Object[length]; return ((Object[]) auxiliary).length;
    }
    public int allocateNested(int kind, int length) {
        state = kind;
        saved = switch (kind) {
            case 0 -> new boolean[length][];
            case 1 -> new byte[length][];
            case 2 -> new char[length][];
            case 3 -> new short[length][];
            case 4 -> new int[length][];
            case 5 -> new long[length][];
            case 6 -> new float[length][];
            case 7 -> new double[length][];
            default -> new int[length][];
        };
        int zeros = 0;
        for (int index = 0; index < saved.length; index++) if (saved[index] == null) zeros++;
        return zeros + (saved instanceof Object[][] ? 2 : 0)
            + (saved instanceof Cloneable[] ? 4 : 0) + (saved instanceof java.io.Serializable[] ? 8 : 0);
    }
    public int nestedPrimitive(int kind, int element, int length, int index, int width) {
        allocateNested(kind, length); primitive(element, width); saved[index] = auxiliary;
        item = saved[index];
        return switch (kind) {
            case 0 -> ((boolean[]) item).length;
            case 1 -> ((byte[]) item).length;
            case 2 -> ((char[]) item).length;
            case 3 -> ((short[]) item).length;
            case 4 -> ((int[]) item).length;
            case 5 -> ((long[]) item).length;
            case 6 -> ((float[]) item).length;
            case 7 -> ((double[]) item).length;
            default -> -1;
        };
    }
    public int nestedObjects(int length, int index, int width) {
        state = length; saved = new Object[length][]; auxiliary = new Object[width];
        saved[index] = auxiliary; item = saved[index]; return ((Object[]) item).length;
    }
    public int primitiveIntoObjects(int length, int index, int width) {
        state = length; saved = new Object[length][]; primitive(4, width);
        saved[index] = auxiliary; item = saved[index]; return -1;
    }
    public int deepArray(int length, int index, int width) {
        state = length; saved = new int[length][][]; auxiliary = new int[width][];
        saved[index] = auxiliary; item = saved[index]; return ((Object[]) item).length;
    }
    public int callInterface(int length, int index, int value) {
        state = length; saved = new ReferenceArrayMarker[length]; saved[index] = this;
        item = saved[index]; return ((ReferenceArrayMarker) item).compute(value);
    }
    public int callVirtual(int length, int index, int value) {
        state = length; saved = new ReferenceArrayCases[length]; saved[index] = this;
        item = saved[index]; return ((ReferenceArrayCases) item).compute(value);
    }
    public int callSpecial(int length, int index, int value) {
        state = length; saved = new ReferenceArrayCases[length]; saved[index] = this;
        item = saved[index]; return ((ReferenceArrayCases) item).special(value);
    }
    public int clearAfterCast(int length, int index) {
        state = length; saved = new ReferenceArrayCases[length]; saved[index] = this;
        Object[] alias = (Object[]) saved; item = alias[index]; alias[index] = null;
        return item == this ? length : -1;
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
            result = value(ReferenceArrayCases.class.getMethod(method, types).invoke((kind.equals("root") ? new ReferenceArrayCases() : new ReferenceArrayChild()), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String receiver : new String[] {"root", "child"}) {
            for (int length : new int[] {-1, 0, 1, 3}) {
                for (int kind = 0; kind < 9; kind++) {
                    emit(receiver, "allocateNominal", "(II)I", kind, length);
                    for (int index : new int[] {-1, 0, 2, 3}) {
                        emit(receiver, "storeSelf", "(III)I", kind, length, index);
                        emit(receiver, "storeNull", "(III)I", kind, length, index);
                        for (int width : new int[] {-1, 0, 2}) emit(receiver, "storeArray", "(IIII)I", kind, length, index, width);
                    }
                }
                for (int kind = 0; kind < 8; kind++) emit(receiver, "allocateNested", "(II)I", kind, length);
                for (String method : new String[] {"aliases", "negativeAfterStore"}) emit(receiver, method, "(I)I", length);
                for (int index : new int[] {-1, 0, 2, 3}) {
                    for (String method : new String[] {"cycle", "overwrite", "failedOverwrite", "clearAfterCast"}) emit(receiver, method, "(II)I", length, index);
                    for (int width : new int[] {-1, 0, 2}) {
                        for (String method : new String[] {"nestedObjects", "primitiveIntoObjects", "deepArray"}) emit(receiver, method, "(III)I", length, index, width);
                    }
                    for (int value : new int[] {Integer.MIN_VALUE, 0, Integer.MAX_VALUE}) {
                        for (String method : new String[] {"callInterface", "callVirtual", "callSpecial"}) emit(receiver, method, "(III)I", length, index, value);
                    }
                }
            }
            for (int index : new int[] {Integer.MIN_VALUE, -1, 0, 1, Integer.MAX_VALUE}) {
                emit(receiver, "nullStore", "(I)I", index); emit(receiver, "nullLoad", "(I)I", index);
            }
            for (int kind = 0; kind < 9; kind++) for (int element = 0; element < 8; element++) {
                for (int[] args : new int[][] {{1,0,2},{1,-1,2},{1,1,2},{0,0,0},{1,0,-1},{-1,0,2}}) emit(receiver,"storePrimitive","(IIIII)I",kind,element,args[0],args[1],args[2]);
            }
            for (int kind = 0; kind < 8; kind++) for (int element = 0; element < 8; element++) {
                for (int[] args : new int[][] {{1,0,2},{1,-1,2},{1,1,2},{0,0,0},{1,0,-1},{-1,0,2}}) emit(receiver,"nestedPrimitive","(IIIII)I",kind,element,args[0],args[1],args[2]);
            }
        }
    }
}
