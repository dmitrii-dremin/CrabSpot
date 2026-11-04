package fixtures;

import java.lang.reflect.InvocationTargetException;
import java.net.URL;
import java.net.URLClassLoader;

class CrossStaticTrace {
    static int trace, input, calls;
    static Error first, second;
    static Throwable saved;
    static Object object;
    static long wide;
    static float single;
    static double precise;
}
class CrossStaticMath {
    static int bias;
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 1; bias = CrossStaticTrace.input + 7; }
    static int value(int value) { CrossStaticTrace.calls++; return value * 3 + bias; }
    static long widen(long value, int extra) { CrossStaticTrace.wide = value; return value * 5 + extra; }
    static float flip(float value) { CrossStaticTrace.single = value; return -value; }
    static double negate(double value) { CrossStaticTrace.precise = value; return -value; }
}
class CrossStaticParent {
    static int base;
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 2; base = CrossStaticTrace.input + 11; }
    static int inherited(int value) { return value + base; }
    static int hidden(int value) { return value * 2 + base; }
}
class CrossStaticChild extends CrossStaticParent {
    static int own;
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 3; own = CrossStaticTrace.input + 13; }
    static int hidden(int value) { return value * 5 + own; }
}
interface CrossStaticSuperContract {
    int MARK = (CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 9);
    static int untouched(int value) { return value + MARK; }
}
interface CrossStaticContract extends CrossStaticSuperContract {
    int MARK = (CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 4);
    static int value(int value) { return helper(value) + MARK; }
    private static int helper(int value) { return CrossStaticMath.value(value); }
}
class CrossStaticEntry {
    static int value = CrossStaticMath.value(CrossStaticTrace.input);
    public static int entry(int input) { return value + input; }
}
class CrossStaticCycleA {
    static int before = CrossStaticTrace.input;
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 5; }
    static int value = CrossStaticCycleB.read() + 3;
    static int seed() { return before; }
    static int read() { return value; }
    static int recurse(int depth) { CrossStaticTrace.calls++; return depth == 0 ? value : CrossStaticCycleB.recurse(depth - 1) + 1; }
}
class CrossStaticCycleB {
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 6; }
    static int value = CrossStaticCycleA.seed() + 2;
    static int read() { return value; }
    static int recurse(int depth) { CrossStaticTrace.calls++; return depth == 0 ? value : CrossStaticCycleA.recurse(depth - 1) + 1; }
}
class CrossStaticReference {
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 7; }
    static Object select(long wide, Object first, double precise, Object second, int choice) {
        CrossStaticTrace.wide = wide; CrossStaticTrace.precise = precise;
        CrossStaticTrace.object = choice == 0 ? first : second;
        return CrossStaticTrace.object;
    }
    static int[] array(int[] value) { CrossStaticTrace.object = value; return value; }
}
class CrossStaticError extends Error { }
class CrossStaticThrower {
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 8; }
    static int raise(Error value) { CrossStaticTrace.calls++; CrossStaticTrace.saved = value; throw value; }
}
class CrossStaticBad {
    static int before;
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 8; before = 41; if (CrossStaticTrace.input != 0) throw CrossStaticTrace.first; before = 43; }
    static int target(int value) { CrossStaticTrace.calls++; return before + value; }
}
class CrossStaticRecover {
    static int value;
    static { try { value = CrossStaticBad.target(CrossStaticTrace.input); } catch (Error error) { CrossStaticTrace.saved = error; value = 53; } }
    static int read() { return value; }
}
class CrossStaticPlain {
    // The JDK executes this; explicit host-body profiles intentionally leave it unapplied.
    static { CrossStaticTrace.trace = CrossStaticTrace.trace * 10 + 9; }
    static int value(int value) { return value * 3 + 7; }
    static int recurse(int depth) { return depth == 0 ? 11 : CrossStaticPlainTwo.recurse(depth - 1) + 1; }
}
class CrossStaticPlainChild extends CrossStaticPlain { }
class CrossStaticPlainTwo {
    static int value(int value) { return value * 5 - 3; }
    static int recurse(int depth) { return depth == 0 ? 13 : CrossStaticPlain.recurse(depth - 1) + 1; }
}
interface CrossStaticPlainInterface {
    static int value(int value) { return helper(value) + CrossStaticPlainTwo.value(value); }
    private static int helper(int value) { return CrossStaticPlain.value(value); }
}
public class CrossStaticCases {
    public static int arithmetic(int value) { return CrossStaticMath.value(value) + CrossStaticMath.value(value + 1); }
    public static long wide(int value) { return CrossStaticMath.widen(((long) value << 60) | 0x12345L, value); }
    public static float single(int value) { return CrossStaticMath.flip((float) value); }
    public static double floating(int value) { return CrossStaticMath.negate((double) value); }
    public static int inherited(int value) { return CrossStaticChild.inherited(value); }
    public static int hidden(int value) { return CrossStaticParent.hidden(value) + CrossStaticChild.hidden(value); }
    public static int interfaces(int value) { return CrossStaticContract.value(value); }
    public static int recursiveInitialization(int ignored) { return CrossStaticCycleA.read() * 100 + CrossStaticTrace.trace; }
    public static int recursion(int value) { return CrossStaticCycleA.recurse(value & 3); }
    public static int mixed(int value) {
        Object selected = CrossStaticReference.select((long) value - 4294967296L, CrossStaticTrace.first, value / 2.0, CrossStaticTrace.second, value & 1);
        return selected == CrossStaticTrace.first ? 17 : 19;
    }
    public static int nullMixed(int value) {
        Object selected = CrossStaticReference.select((long) value, null, value / 2.0, CrossStaticTrace.second, value & 1);
        return selected == null ? 23 : 29;
    }
    public static int arrays(int value) {
        int[] array = new int[value & 3];
        if (array.length != 0) array[0] = value;
        int[] result = CrossStaticReference.array(array);
        return result.length + (result == array ? 31 : 37) + (result.length == 0 ? 0 : result[0]);
    }
    public static int caughtBody(int value) {
        try { return CrossStaticThrower.raise((value & 1) == 0 ? CrossStaticTrace.first : CrossStaticTrace.second); }
        catch (Error error) { return error == CrossStaticTrace.first ? 41 : 43; }
    }
    public static int uncaughtBody(int value) { return CrossStaticThrower.raise((value & 1) == 0 ? CrossStaticTrace.first : CrossStaticTrace.second); }
    public static int replacement(int ignored) {
        try { return CrossStaticThrower.raise(CrossStaticTrace.first); }
        catch (Error error) { return CrossStaticThrower.raise(CrossStaticTrace.second); }
    }
    public static int caughtInitialization(int value) {
        try { return CrossStaticBad.target(value); }
        catch (Error error) { CrossStaticTrace.saved = error; return error == CrossStaticTrace.first ? 47 : 49; }
    }
    public static int uncaughtInitialization(int value) { return CrossStaticBad.target(value); }
    public static int recoveredInitialization(int ignored) { return CrossStaticRecover.read(); }
    public int bodyArithmetic(int value) { return CrossStaticPlain.value(value) + CrossStaticPlainChild.value(value + 1); }
    public int bodyRecursion(int value) { return CrossStaticPlain.recurse(value & 3); }
    public int bodyInterfaces(int value) { return CrossStaticPlainInterface.value(value); }

    private static String result(Object value) {
        if (value instanceof Long number) return "L:" + number;
        if (value instanceof Float number) return "F:" + String.format("%08x", Float.floatToRawIntBits(number));
        if (value instanceof Double number) return "D:" + String.format("%016x", Double.doubleToRawLongBits(number));
        return "I:" + value;
    }
    private static void emit(String name, int input) throws Exception {
        URL location = CrossStaticCases.class.getProtectionDomain().getCodeSource().getLocation();
        try (URLClassLoader loader = new URLClassLoader(new URL[] {location}, null)) {
            Class<?> root = Class.forName(name.equals("entry") ? "fixtures.CrossStaticEntry" : "fixtures.CrossStaticCases", false, loader);
            Class<?> trace = Class.forName("fixtures.CrossStaticTrace", true, loader);
            var inputField = trace.getDeclaredField("input"); inputField.setAccessible(true); inputField.setInt(null, input);
            var constructor = Class.forName("fixtures.CrossStaticError", true, loader).getDeclaredConstructor(); constructor.setAccessible(true);
            Object first = constructor.newInstance(); Object second = constructor.newInstance();
            var firstField = trace.getDeclaredField("first"); firstField.setAccessible(true); firstField.set(null, first);
            var secondField = trace.getDeclaredField("second"); secondField.setAccessible(true); secondField.set(null, second);
            var selected = root.getMethod(name, int.class); selected.setAccessible(true);
            Object receiver = name.startsWith("body") ? root.getDeclaredConstructor().newInstance() : null;
            String result;
            try { result = result(selected.invoke(receiver, input)); }
            catch (InvocationTargetException error) {
                if (error.getCause() == first) result = "T:first";
                else if (error.getCause() == second) result = "T:second";
                else throw new IllegalStateException("unexpected exception", error.getCause());
            }
            if (!result.startsWith("T:") && !name.equals("caughtInitialization")) {
                String repeated = result(selected.invoke(receiver, input));
                if (!repeated.equals(result)) throw new IllegalStateException("result changed on repeat");
            }
            String descriptor = name.equals("wide") ? "(I)J" : name.equals("single") ? "(I)F" : name.equals("floating") ? "(I)D" : "(I)I";
            System.out.println(name + "\t" + descriptor + "\tI:" + input + "\t" + result);
        }
    }
    public static void main(String[] ignored) throws Exception {
        for (String method : new String[] {"arithmetic", "wide", "single", "floating", "inherited", "hidden", "interfaces", "entry", "recursiveInitialization", "recursion", "mixed", "nullMixed", "arrays", "caughtBody", "uncaughtBody", "replacement", "caughtInitialization", "uncaughtInitialization", "recoveredInitialization", "bodyArithmetic", "bodyRecursion", "bodyInterfaces"})
            for (int input : new int[] {-7, -2, -1, 0, 1, 2, 7}) emit(method, input);
    }
}
