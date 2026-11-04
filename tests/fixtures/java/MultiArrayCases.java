package fixtures;
import java.lang.reflect.InvocationTargetException;
class MultiArrayChild extends MultiArrayCases implements ReferenceArrayOther, Cloneable {
    @Override public int compute(int value) { return value * 5 - 1; }
}
public class MultiArrayCases extends ReferenceArrayBase implements ReferenceArrayMarker {
    int state;
    Object graph;
    Object leaf;
    Object extra;
    @Override public int compute(int value) { return value * 3 + 1; }
    private int primitiveLength(int kind) {
        return switch (kind) {
            case 0 -> ((boolean[]) leaf).length;
            case 1 -> ((byte[]) leaf).length;
            case 2 -> ((char[]) leaf).length;
            case 3 -> ((short[]) leaf).length;
            case 4 -> ((int[]) leaf).length;
            case 5 -> ((long[]) leaf).length;
            case 6 -> ((float[]) leaf).length;
            case 7 -> ((double[]) leaf).length;
            default -> -1;
        };
    }
    private int primitiveValue(int kind, int index) {
        return switch (kind) {
            case 0 -> ((boolean[]) leaf)[index] ? 1 : 0;
            case 1 -> (int) ((byte[]) leaf)[index];
            case 2 -> (int) ((char[]) leaf)[index];
            case 3 -> (int) ((short[]) leaf)[index];
            case 4 -> (int) ((int[]) leaf)[index];
            case 5 -> (int) ((long[]) leaf)[index];
            case 6 -> (int) ((float[]) leaf)[index];
            case 7 -> (int) ((double[]) leaf)[index];
            default -> -1;
        };
    }
    private int inspect2(int kind) {
        int sum = ((Object[]) graph).length;
        for (Object row : (Object[]) graph) {
            leaf = row; int length = primitiveLength(kind); sum = sum * 31 + length;
            for (int index = 0; index < length; index++) sum += primitiveValue(kind, index);
        }
        return sum;
    }
    private int inspect3(int kind) {
        int sum = ((Object[]) graph).length;
        for (Object plane : (Object[]) graph) {
            sum = sum * 31 + ((Object[]) plane).length;
            for (Object row : (Object[]) plane) {
                leaf = row; int length = primitiveLength(kind); sum = sum * 31 + length;
                for (int index = 0; index < length; index++) sum += primitiveValue(kind, index);
            }
        }
        return sum;
    }
    private int inspectPartial2() {
        int sum = ((Object[]) graph).length;
        for (Object row : (Object[]) graph) {
            leaf = row; sum = sum * 31 + ((Object[]) row).length;
            for (Object value : (Object[]) row) if (value != null) sum += 1;
        }
        return sum;
    }
    private int inspectPartial3() {
        int sum = ((Object[]) graph).length;
        for (Object plane : (Object[]) graph) {
            sum = sum * 31 + ((Object[]) plane).length;
            for (Object row : (Object[]) plane) {
                leaf = row; sum = sum * 31 + ((Object[]) row).length;
                for (Object value : (Object[]) row) if (value != null) sum += 1;
            }
        }
        return sum;
    }
    public int full2(int kind, int a, int b) {
        state = kind;
        graph = switch (kind) {
            case 0 -> new boolean[a][b];
            case 1 -> new byte[a][b];
            case 2 -> new char[a][b];
            case 3 -> new short[a][b];
            case 4 -> new int[a][b];
            case 5 -> new long[a][b];
            case 6 -> new float[a][b];
            case 7 -> new double[a][b];
            default -> new int[a][b];
        };
        return inspect2(kind);
    }
    public int partial2(int kind, int a, int b) {
        state = kind;
        graph = switch (kind) {
            case 0 -> new boolean[a][b][];
            case 1 -> new byte[a][b][];
            case 2 -> new char[a][b][];
            case 3 -> new short[a][b][];
            case 4 -> new int[a][b][];
            case 5 -> new long[a][b][];
            case 6 -> new float[a][b][];
            case 7 -> new double[a][b][];
            default -> new int[a][b][];
        };
        return inspectPartial2();
    }
    public int full3(int kind, int a, int b, int c) {
        state = kind;
        graph = switch (kind) {
            case 0 -> new boolean[a][b][c];
            case 1 -> new byte[a][b][c];
            case 2 -> new char[a][b][c];
            case 3 -> new short[a][b][c];
            case 4 -> new int[a][b][c];
            case 5 -> new long[a][b][c];
            case 6 -> new float[a][b][c];
            case 7 -> new double[a][b][c];
            default -> new int[a][b][c];
        };
        return inspect3(kind);
    }
    public int partial3(int kind, int a, int b, int c) {
        state = kind;
        graph = switch (kind) {
            case 0 -> new boolean[a][b][c][];
            case 1 -> new byte[a][b][c][];
            case 2 -> new char[a][b][c][];
            case 3 -> new short[a][b][c][];
            case 4 -> new int[a][b][c][];
            case 5 -> new long[a][b][c][];
            case 6 -> new float[a][b][c][];
            case 7 -> new double[a][b][c][];
            default -> new int[a][b][c][];
        };
        return inspectPartial3();
    }
    public int full4(int a, int b, int c, int d) {
        state = a; graph = new int[a][b][c][d]; int sum = ((Object[]) graph).length;
        for (Object cube : (Object[]) graph) {
            sum = sum * 31 + ((Object[]) cube).length;
            for (Object plane : (Object[]) cube) {
                sum = sum * 31 + ((Object[]) plane).length;
                for (Object row : (Object[]) plane) {
                    leaf = row; sum = sum * 31 + ((int[]) row).length;
                    for (int value : (int[]) row) sum += value;
                }
            }
        }
        return sum;
    }
    public int objects4(int a, int b, int c, int d) {
        state = a; graph = new Object[a][b][c][d]; int sum = ((Object[]) graph).length;
        for (Object cube : (Object[]) graph) {
            sum = sum * 31 + ((Object[]) cube).length;
            for (Object plane : (Object[]) cube) {
                sum = sum * 31 + ((Object[]) plane).length;
                for (Object row : (Object[]) plane) {
                    leaf = row; sum = sum * 31 + ((Object[]) row).length;
                    for (Object value : (Object[]) row) if (value != null) sum += 1;
                }
            }
        }
        return sum;
    }
    private int allocateReferences(int kind, int a, int b) {
        state = kind; graph = switch (kind) {
            case 0 -> new Object[a][b];
            case 1 -> new ReferenceArrayBase[a][b];
            case 2 -> new MultiArrayCases[a][b];
            case 3 -> new MultiArrayChild[a][b];
            case 4 -> new ReferenceArrayMarker[a][b];
            case 5 -> new ReferenceArrayOther[a][b];
            case 6 -> new ReferenceArrayUnrelated[a][b];
            case 7 -> new Cloneable[a][b];
            case 8 -> new java.io.Serializable[a][b];
            default -> new Object[a][b];
        };
        return ((Object[]) graph).length;
    }
    public int references2(int kind, int a, int b) { allocateReferences(kind, a, b); return inspectPartial2(); }
    public int storeReference(int kind, int a, int b, int i, int j) {
        allocateReferences(kind, a, b); Object[] row = (Object[]) ((Object[]) graph)[i];
        row[j] = this; leaf = row[j]; return leaf == this ? compute(kind) : -1;
    }
    public int touch2(int kind, int a, int b, int i, int j, int value) {
        state = kind; graph = switch (kind) {
            case 0 -> new boolean[a][b];
            case 1 -> new byte[a][b];
            case 2 -> new char[a][b];
            case 3 -> new short[a][b];
            case 4 -> new int[a][b];
            case 5 -> new long[a][b];
            case 6 -> new float[a][b];
            case 7 -> new double[a][b];
            default -> new int[a][b];
        };
        leaf = ((Object[]) graph)[i];
        switch (kind) {
            case 0 -> ((boolean[]) leaf)[j] = (value & 1) != 0;
            case 1 -> ((byte[]) leaf)[j] = (byte) value;
            case 2 -> ((char[]) leaf)[j] = (char) value;
            case 3 -> ((short[]) leaf)[j] = (short) value;
            case 4 -> ((int[]) leaf)[j] = (int) value;
            case 5 -> ((long[]) leaf)[j] = (long) value;
            case 6 -> ((float[]) leaf)[j] = (float) value;
            case 7 -> ((double[]) leaf)[j] = (double) value;
            default -> { return -1; }
        }
        return primitiveValue(kind, j);
    }
    public int negativeAfterPrior(int a, int b) {
        state = a; graph = new Object[1]; ((Object[]) graph)[0] = this;
        leaf = this; extra = graph; graph = new int[a][b]; return ((Object[]) graph).length;
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
            result = value(MultiArrayCases.class.getMethod(method, types).invoke((kind.equals("root") ? new MultiArrayCases() : new MultiArrayChild()), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String receiver : new String[] {"root", "child"}) {
            for (int a : new int[] {-1,0,1,2,3}) for (int b : new int[] {-1,0,1,2,3}) {
                emit(receiver,"negativeAfterPrior","(II)I",a,b);
                for (int kind=0;kind<8;kind++) {
                    emit(receiver,"full2","(III)I",kind,a,b); emit(receiver,"partial2","(III)I",kind,a,b);
                }
                for (int kind=0;kind<9;kind++) emit(receiver,"references2","(III)I",kind,a,b);
            }
            for (int a : new int[] {-1,0,1,2}) for (int b : new int[] {-1,0,1,2}) for (int c : new int[] {-1,0,1,2}) {
                for (int kind=0;kind<8;kind++) {
                    emit(receiver,"full3","(IIII)I",kind,a,b,c); emit(receiver,"partial3","(IIII)I",kind,a,b,c);
                }
                for (int d : new int[] {-1,0,1,2}) {
                    emit(receiver,"full4","(IIII)I",a,b,c,d); emit(receiver,"objects4","(IIII)I",a,b,c,d);
                }
            }
            for (int[] counts : new int[][] {{0,Integer.MAX_VALUE,Integer.MAX_VALUE},{1,0,Integer.MAX_VALUE},{0,Integer.MAX_VALUE,-1},{0,-1,Integer.MAX_VALUE}}) {
                for (int kind=0;kind<8;kind++) emit(receiver,"full3","(IIII)I",kind,counts[0],counts[1],counts[2]);
            }
            for (int[] counts : new int[][] {{0,Integer.MAX_VALUE,Integer.MAX_VALUE,Integer.MAX_VALUE},{1,0,Integer.MAX_VALUE,Integer.MAX_VALUE},{1,1,0,Integer.MAX_VALUE}}) {
                for (String method : new String[] {"full4","objects4"}) emit(receiver,method,"(IIII)I",counts[0],counts[1],counts[2],counts[3]);
            }
            for (int[] sizes : new int[][] {{1,1},{1,0},{0,1},{-1,1},{1,-1},{2,3}}) {
                for (int[] indices : new int[][] {{0,0},{-1,0},{0,-1},{1,2},{2,3}}) {
                    for (int kind=0;kind<9;kind++) emit(receiver,"storeReference","(IIIII)I",kind,sizes[0],sizes[1],indices[0],indices[1]);
                    for (int kind=0;kind<8;kind++) for (int value : new int[] {Integer.MIN_VALUE,-1,0,1,Integer.MAX_VALUE}) emit(receiver,"touch2","(IIIIII)I",kind,sizes[0],sizes[1],indices[0],indices[1],value);
                }
            }
        }
    }
}
