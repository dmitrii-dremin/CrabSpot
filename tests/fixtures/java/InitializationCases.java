package fixtures;
import java.lang.reflect.InvocationTargetException;
import java.net.URL;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.net.URLClassLoader;
class InitProbeTrace { static int trace; static int input; static Error first; static Error second; static Throwable saved; static Throwable savedSecond; }
class InitProbeNumeric {
    static final int FIXED = 17;
    static final byte SMALL = -2;
    static final long WIDE = -4294967297L;
    static final float SINGLE = -0.0f;
    static final double DOUBLE = Double.NaN;
    static final int dynamic;
    static int value;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 1; dynamic = InitProbeTrace.input + 31; value = InitProbeTrace.input + FIXED; }
}
interface InitProbeDefaultA {
    int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 1);
    default int a() { return 1; }
}
interface InitProbeDefaultB extends InitProbeDefaultA {
    int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 2);
    default int b() { return 2; }
}
interface InitProbeDefaultC extends InitProbeDefaultA {
    int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 5);
    default int c() { return 5; }
}
class InitProbeDiamond implements InitProbeDefaultB, InitProbeDefaultC {
    static int value = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 6);
}
interface InitProbeInherited extends InitProbeDefaultA {
    int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 8);
}
class InitProbeInheritedChild implements InitProbeInherited {
    static int value = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 6);
}
class InitProbeEntry {
    static int value;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 9; value = InitProbeTrace.input + 9; }
    public static int entry(int ignored) { return value * 100 + InitProbeTrace.trace; }
}
interface InitProbePlain { int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 9); }
class InitProbeParent { static int value = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 3); }
class InitProbeChild extends InitProbeParent implements InitProbeDefaultB, InitProbePlain {
    static int value = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 4);
}
interface InitProbePrivate {
    int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 5);
    private int hidden() { return 7; }
}
interface InitProbeStaticOnly {
    int MARK = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 6);
    private static int hidden() { return 9; }
}
class InitProbePrivateChild implements InitProbePrivate, InitProbeStaticOnly {
    static int value = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 7);
}
class InitProbeOwner { static int value = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 1); }
class InitProbeSymbolic extends InitProbeOwner { static int other = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 2); }
class InitProbeCycleA {
    static int before = InitProbeTrace.input;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 1; }
    static int value = InitProbeCycleB.value + 3;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 3; }
}
class InitProbeCycleB {
    static int value;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 2; value = InitProbeCycleA.before + 2; }
}
class InitProbeOnce { static int count; static int value = ++count; }
class InitProbeWrite {
    static int value;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 8; value = 11; }
}
class InitProbeError extends Error { }
class InitProbeFailure {
    static int value;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 4; value = 19; if (InitProbeTrace.input != 0) throw InitProbeTrace.first; value = 23; }
}
class InitProbeFailureSecond {
    static int value;
    static { InitProbeTrace.trace = InitProbeTrace.trace * 10 + 6; value = 27; if (InitProbeTrace.input != 0) throw InitProbeTrace.second; value = 29; }
}
class InitProbeFailureChild extends InitProbeFailure {
    static int childValue = (InitProbeTrace.trace = InitProbeTrace.trace * 10 + 5);
}
class InitProbeRecover {
    static int value;
    static { try { value = InitProbeFailure.value; } catch (Error failure) { InitProbeTrace.saved = failure; value = 43; } }
}
public class InitializationCases {
    public static int constants(int ignored) { return InitProbeNumeric.value + InitProbeNumeric.dynamic; }
    public static int order(int ignored) { int value = InitProbeChild.value; return InitProbeTrace.trace; }
    public static int interfaceThenClass(int ignored) { int a = InitProbeDefaultB.MARK; int b = InitProbeChild.value; return InitProbeTrace.trace; }
    public static int classThenInterface(int ignored) { int a = InitProbeChild.value; int b = InitProbePlain.MARK; return InitProbeTrace.trace; }
    public static int twoFailures(int ignored) {
        int a; int b;
        try { a = InitProbeFailureSecond.value; }
        catch (Error failure) { InitProbeTrace.savedSecond = failure; a = failure == InitProbeTrace.second ? 107 : 109; }
        try { b = InitProbeFailure.value; }
        catch (Error failure) { InitProbeTrace.saved = failure; b = failure == InitProbeTrace.first ? 101 : 103; }
        return a * 1000 + b;
    }
    public static int recoveredError(int ignored) { return InitProbeRecover.value; }
    public static int dependencyError(int ignored) {
        try { return InitProbeFailureChild.childValue; }
        catch (Error failure) { InitProbeTrace.saved = failure; return failure == InitProbeTrace.first ? 101 : 103; }
    }
    public static int interfaceOnly(int ignored) { int value = InitProbeDefaultB.MARK; return InitProbeTrace.trace; }
    public static int diamond(int ignored) { int value = InitProbeDiamond.value; return InitProbeTrace.trace; }
    public static int inheritedInterface(int ignored) { int value = InitProbeInheritedChild.value; return InitProbeTrace.trace; }
    public static int privateInterface(int ignored) { int value = InitProbePrivateChild.value; return InitProbeTrace.trace; }
    public static int declaringOwner(int ignored) { int value = InitProbeSymbolic.value; return InitProbeTrace.trace; }
    public static int recursive(int ignored) { return InitProbeCycleA.value * 1000 + InitProbeTrace.trace; }
    public static int once(int ignored) { return InitProbeOnce.value + InitProbeOnce.value + InitProbeOnce.count * 10; }
    public static int writeTrigger(int value) { InitProbeWrite.value = value; return InitProbeWrite.value + InitProbeTrace.trace * 100; }
    public static int caughtError(int ignored) {
        try { return InitProbeFailure.value; }
        catch (Error failure) { InitProbeTrace.saved = failure; return failure == InitProbeTrace.first ? 101 : 103; }
    }
    public static int uncaughtError(int ignored) { return InitProbeFailure.value; }
    private static void emit(String method, int input) throws Exception {
        URL location = InitializationCases.class.getProtectionDomain().getCodeSource().getLocation();
        try (URLClassLoader loader = new URLClassLoader(new URL[] {location}, null)) {
            Class<?> root = Class.forName(method.equals("entry") ? "fixtures.InitProbeEntry" : "fixtures.InitializationCases", false, loader);
            Class<?> trace = Class.forName("fixtures.InitProbeTrace", true, loader);
            var inputField = trace.getDeclaredField("input"); inputField.setAccessible(true); inputField.setInt(null, input);
            var constructor = Class.forName("fixtures.InitProbeError", true, loader).getDeclaredConstructor(); constructor.setAccessible(true);
            Object first = constructor.newInstance();
            Object second = constructor.newInstance();
            var firstField = trace.getDeclaredField("first"); firstField.setAccessible(true); firstField.set(null, first);
            var secondField = trace.getDeclaredField("second"); secondField.setAccessible(true); secondField.set(null, second);
            var selected = root.getMethod(method, int.class); selected.setAccessible(true);
            String result;
            try { result = "I:" + selected.invoke(null, input); }
            catch (InvocationTargetException error) {
                if (error.getCause() != first) throw new IllegalStateException("unexpected exception", error.getCause());
                result = "T:first";
            }
            if (!method.equals("caughtError") && !method.equals("uncaughtError") && !method.equals("dependencyError") && !method.equals("twoFailures")) {
                String repeated = "I:" + selected.invoke(null, input);
                if (!repeated.equals(result)) throw new IllegalStateException("initialization repeated");
            }
            System.out.println(method + "\t(I)I\tI:" + input + "\t" + result);
        }
    }
    // Original reference-only emitter: javac cannot attach ConstantValue to a
    // mutable static, or emit out-of-range narrow constants without conversion.
    private static class RawLoader extends ClassLoader {
        Class<?> define(byte[] bytes) { return defineClass("fixtures.InitProbeRawConstant", bytes, 0, bytes.length); }
    }
    private static void utf(DataOutputStream out, String text) throws Exception { out.writeByte(1); out.writeUTF(text); }
    private static void pair(DataOutputStream out, int tag, int first, int second) throws Exception {
        out.writeByte(tag); out.writeShort(first); if (tag != 7) out.writeShort(second);
    }
    private static byte[] rawConstant(String descriptor, int value) throws Exception {
        var bytes = new ByteArrayOutputStream(); var out = new DataOutputStream(bytes);
        out.writeInt(0xcafebabe); out.writeShort(0); out.writeShort(65); out.writeShort(14);
        utf(out, "fixtures/InitProbeRawConstant"); pair(out, 7, 1, 0);
        utf(out, "java/lang/Object"); pair(out, 7, 3, 0);
        utf(out, "value"); utf(out, descriptor); utf(out, "ConstantValue");
        out.writeByte(3); out.writeInt(value);
        utf(out, "get"); utf(out, "(I)I"); utf(out, "Code");
        pair(out, 12, 5, 6); pair(out, 9, 2, 12);
        out.writeShort(0x21); out.writeShort(2); out.writeShort(4); out.writeShort(0);
        out.writeShort(1); out.writeShort(9); out.writeShort(5); out.writeShort(6);
        out.writeShort(1); out.writeShort(7); out.writeInt(2); out.writeShort(8);
        out.writeShort(1); out.writeShort(9); out.writeShort(9); out.writeShort(10);
        out.writeShort(1); out.writeShort(11); out.writeInt(16);
        out.writeShort(1); out.writeShort(1); out.writeInt(4);
        out.write(new byte[] {(byte)0xb2, 0, 13, (byte)0xac});
        out.writeShort(0); out.writeShort(0); out.writeShort(0);
        return bytes.toByteArray();
    }
    private static void emitRaw(String descriptor, int input) throws Exception {
        Class<?> raw = new RawLoader().define(rawConstant(descriptor, input));
        Object result = raw.getMethod("get", int.class).invoke(null, input);
        System.out.println("raw/" + descriptor + "\t(I)I\tI:" + input + "\tI:" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String method : new String[] {"constants", "order", "interfaceThenClass", "classThenInterface", "recoveredError", "dependencyError", "twoFailures", "interfaceOnly", "diamond", "inheritedInterface", "entry", "privateInterface", "declaringOwner", "recursive", "once", "writeTrigger", "caughtError", "uncaughtError"})
            for (int input : new int[] {-7, -1, 0, 1, 7}) emit(method, input);
        for (String descriptor : new String[] {"Z", "B", "C", "S", "I"})
            for (int input : new int[] {Integer.MIN_VALUE, -65537, -32769, -1, 0, 1, 2, 3, 255, 65534, Integer.MAX_VALUE}) emitRaw(descriptor, input);
    }
}
