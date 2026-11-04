package fixtures;
import java.lang.reflect.InvocationTargetException;
interface StaticBodyContract {
    default int defaultTouch(int value) { StaticBodyCases.number = value; return StaticBodyCases.number; }
}
class StaticBodyBase {
    static int hidden;
    static Object baseObject;
    int baseTouch(int value) { hidden = value; return hidden; }
}
class StaticBodyChild extends StaticBodyCases {
    @Override public int compute(int value) { return value * 5 - 1; }
}
class StaticBodyThrown extends Exception { }
// These values are inspected as unapplied metadata on the CrabSpot side only.
class StaticUnapplied {
    static final int CONSTANT = 37;
    static final String TEXT = "unapplied";
    static int initialized = 91;
    static Object fresh = new Object();
}
public class StaticBodyCases extends StaticBodyBase implements StaticBodyContract, Cloneable, java.io.Serializable {
    static boolean flag;
    static byte tiny;
    static short small;
    static char letter;
    static int number;
    static long wide;
    static float single;
    static double precise;
    static int hidden;
    static Object object;
    static StaticBodyCases link;
    static StaticBodyContract contract;
    static Cloneable cloneable;
    static java.io.Serializable serializable;
    static int[] ints;
    static Object[] objects;
    static Throwable pending;
    static Throwable saved;
    Throwable first;
    Throwable second;
    public int compute(int value) { return value * 3 + 1; }
    static int touch(int value) { number = value; return number; }
    public int integral(int value) {
        flag = (value & 1) != 0; tiny = (byte) value; small = (short) value;
        letter = (char) value; number = value;
        return (flag ? 17 : 19) + tiny * 3 + small * 5 + letter * 7 + number;
    }
    public long longStore(long value) { wide = value; return wide; }
    public float floatStore(float value) { single = value; return single; }
    public double doubleStore(double value) { precise = value; return precise; }
    public int hiddenSlots(int value) {
        StaticBodyBase.hidden = value; hidden = value + 1;
        return StaticBodyBase.hidden * 3 + StaticBodyChild.hidden * 5;
    }
    public int self(int value) { number = value; object = this; link = this; return link.compute(number); }
    public int aliases(int value) {
        number = value; object = this; link = this; contract = this;
        cloneable = this; serializable = this; baseObject = this;
        return contract.defaultTouch(value) + (object == link && object == cloneable && cloneable == serializable && baseObject == this ? 23 : 29);
    }
    public int clear(int value) { object = this; link = this; link = null; number = value; return object == this && link == null ? number : 31; }
    public int overwrite(int length) { number = length; object = this; ints = new int[length]; object = ints; return ((int[]) object).length; }
    public int arrays(int length) {
        number = length; ints = new int[length]; object = ints; cloneable = ints; serializable = ints;
        objects = new StaticBodyCases[length];
        if (length != 0) { ints[0] = length + 3; objects[0] = this; }
        return ints.length + objects.length + (object == cloneable && cloneable == serializable ? 37 : 41);
    }
    public int special(int value) { return super.baseTouch(value) + StaticBodyContract.super.defaultTouch(value + 1); }
    private static int recurse(int depth) { number++; return depth <= 0 ? number : recurse(depth - 1); }
    public int recursion(int depth) { number = 7; return recurse(depth); }
    private static StaticBodyCases mixed(long a, StaticBodyCases receiver, double b) { wide = a; precise = b; link = receiver; return link; }
    public int mixedCall(int value) { return mixed(value, this, value).compute(value); }
    public int divideAfterStore(int value) { number = value; object = this; return 17 / value; }
    private static int raise(Throwable value) throws Throwable { pending = value; number += 3; throw pending; }
    public int caught(int choice) throws Throwable {
        number = 5;
        try { return raise(choice == 0 ? first : second); }
        catch (Throwable value) { saved = value; number += 7; return number + (saved == first ? 11 : 13); }
    }
    public int uncaught(int choice) throws Throwable { number = 17; return raise(choice == 0 ? first : second); }
    public int replacement(int choice) throws Throwable {
        number = 23;
        try { return raise(choice == 0 ? first : second); }
        catch (Throwable value) { saved = value; pending = second; number += 11; throw pending; }
    }
    private static void reset() {
        flag = false; tiny = 0; small = 0; letter = 0; number = 0; wide = 0;
        single = 0; precise = 0; hidden = 0; StaticBodyBase.hidden = 0;
        object = null; link = null; contract = null; cloneable = null; serializable = null;
        baseObject = null; ints = null; objects = null; pending = null; saved = null;
    }
    private static String value(Object value, boolean result) {
        if (value instanceof Integer integer) return "I:" + integer;
        if (value instanceof Long integer) return "L:" + integer;
        if (value instanceof Float single) return result && Float.isNaN(single) ? "F:NaN" : String.format("F:%08x", Float.floatToRawIntBits(single));
        if (value instanceof Double precise) return result && Double.isNaN(precise) ? "D:NaN" : String.format("D:%016x", Double.doubleToRawLongBits(precise));
        throw new IllegalArgumentException("unexpected oracle value");
    }
    private static void emit(String kind, String method, String descriptor, Object input) throws Exception {
        reset();
        StaticBodyCases receiver = kind.equals("root") ? new StaticBodyCases() : new StaticBodyChild();
        receiver.first = new StaticBodyThrown(); receiver.second = new StaticBodyThrown();
        Class<?> type = input instanceof Integer ? int.class : input instanceof Long ? long.class : input instanceof Float ? float.class : double.class;
        String result;
        try { result = value(StaticBodyCases.class.getMethod(method, type).invoke(receiver, input), true); }
        catch (InvocationTargetException error) {
            Throwable thrown = error.getCause();
            if (thrown == receiver.first) result = "T:first";
            else if (thrown == receiver.second) result = "T:second";
            else if (thrown instanceof ArithmeticException || thrown instanceof NegativeArraySizeException) result = "E:" + thrown.getClass().getName().replace('.', '/');
            else throw new IllegalStateException("unexpected exception", thrown);
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + value(input, false) + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String kind : new String[] {"root", "child"}) {
            for (int input : new int[] {Integer.MIN_VALUE, -65537, -32769, -129, -1, 0, 1, 2, 127, 128, 32767, 32768, 65535, 65536, Integer.MAX_VALUE})
                for (String method : new String[] {"integral", "hiddenSlots", "self", "aliases", "clear", "special", "mixedCall", "divideAfterStore"}) emit(kind, method, "(I)I", input);
            for (int input : new int[] {-1, 0, 1, 2, 7, 16}) {
                for (String method : new String[] {"overwrite", "arrays", "recursion"}) emit(kind, method, "(I)I", input);
                for (String method : new String[] {"caught", "uncaught", "replacement"}) emit(kind, method, "(I)I", input);
            }
            for (long input : new long[] {Long.MIN_VALUE, -1, 0, 1, 4294967296L, Long.MAX_VALUE}) emit(kind, "longStore", "(J)J", input);
            for (int bits : new int[] {0, 0x80000000, 1, 0x007fffff, 0x3f800000, 0xbf800000, 0x7f7fffff, 0x7f800000, 0xff800000, 0x7fc01234}) emit(kind, "floatStore", "(F)F", Float.intBitsToFloat(bits));
            for (long bits : new long[] {0, 0x8000000000000000L, 1, 0x000fffffffffffffL, 0x3ff0000000000000L, 0xbff0000000000000L, 0x7fefffffffffffffL, 0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000001234L}) emit(kind, "doubleStore", "(D)D", Double.longBitsToDouble(bits));
        }
    }
}
