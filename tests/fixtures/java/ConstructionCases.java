package fixtures;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.lang.reflect.InvocationTargetException;
import java.net.URL;
import java.net.URLClassLoader;
import java.util.ArrayList;
import java.util.LinkedHashMap;

class ConstructionTrace {
    static int input, trace, count, observed;
    static Error first;
    static Object escaped, saved;
}
class ConstructionBase {
    final int base;
    int hidden;
    ConstructionBase(int value) {
        ConstructionTrace.escaped = this;
        ConstructionTrace.observed = read();
        base = value + 5;
        hidden = value + 6;
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 1;
    }
    int read() { return base; }
}
class ConstructionChild extends ConstructionBase {
    final int own;
    int hidden = 7;
    ConstructionChild(int value) {
        super(value);
        own = value + 2;
        hidden = value;
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 2;
        ConstructionTrace.count++;
        if (value < 0) throw ConstructionTrace.first;
    }
    ConstructionChild() {
        this(4);
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 3;
    }
    int read() { return hidden; }
}
class ConstructionValues {
    final boolean flag;
    final byte tiny;
    final char character;
    final short small;
    final int integer;
    final long wide;
    final float single;
    final double precise;
    final Object object;
    final int[] array;
    ConstructionValues(boolean flag, byte tiny, char character, short small, int integer,
                       long wide, float single, double precise, Object object, int[] array) {
        this.flag = flag; this.tiny = tiny; this.character = character; this.small = small;
        this.integer = integer; this.wide = wide; this.single = single; this.precise = precise;
        this.object = object; this.array = array;
        ConstructionTrace.escaped = this;
        ConstructionTrace.count++;
    }
}
class ConstructionNode {
    final int value;
    ConstructionNode next;
    Object[] links;
    ConstructionNode(int value) { this.value = value; ConstructionTrace.count++; }
    ConstructionNode(int value, int depth) {
        this(value);
        if (depth > 0) next = new ConstructionNode(value + 1, depth - 1);
    }
}
class ConstructionInner {
    final int value;
    ConstructionInner(int value) { this.value = value; ConstructionTrace.count++; }
}
class ConstructionOuter extends ConstructionBase {
    final ConstructionInner inner;
    ConstructionOuter(ConstructionInner inner) {
        super(inner.value);
        this.inner = inner;
        ConstructionTrace.count++;
        ConstructionTrace.escaped = this;
    }
    ConstructionOuter(int value) { this(new ConstructionInner(value)); }
}
class ConstructionActive {
    static int early = ConstructionTrace.input;
    static ConstructionActive singleton = new ConstructionActive(early);
    static int late = 13;
    final int seen, value;
    ConstructionActive(int value) {
        this.value = value; seen = late;
        ConstructionTrace.count++;
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 4;
    }
}
class ConstructionFailBefore extends ConstructionBase {
    static int argument(int value) {
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 5;
        if (value < 0) throw ConstructionTrace.first;
        return value;
    }
    ConstructionFailBefore(int value) { super(argument(value)); ConstructionTrace.count++; }
}
class ConstructionBadInitialization {
    static {
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 6;
        if (ConstructionTrace.input < 0) throw ConstructionTrace.first;
    }
    ConstructionBadInitialization() { ConstructionTrace.count++; }
}
class ConstructionError extends Error { }

public class ConstructionCases {
    public static int delegated(int ignored) {
        ConstructionChild child = new ConstructionChild();
        return child.base + child.own + child.hidden + ((ConstructionBase) child).hidden
            + ConstructionTrace.observed + ConstructionTrace.trace;
    }
    public static int caught(int value) {
        try { ConstructionTrace.saved = new ConstructionChild(value); }
        catch (Error error) { if (error != ConstructionTrace.first) return -999; }
        ConstructionChild escaped = (ConstructionChild) ConstructionTrace.escaped;
        return escaped.base + escaped.own + escaped.hidden + ((ConstructionBase) escaped).hidden
            + ConstructionTrace.observed + ConstructionTrace.trace + (ConstructionTrace.saved == null ? 31 : 37);
    }
    public static int uncaught(int value) {
        ConstructionChild child = new ConstructionChild(value);
        ConstructionTrace.saved = child;
        return child.own;
    }
    private static ConstructionValues values(int value) {
        int[] array = new int[] {value, value + 1};
        return new ConstructionValues((value & 1) != 0, (byte) (value * 129), (char) (value - 1),
            (short) (value * 16385), value, ((long) value << 60) | 0x12345L,
            -(float) value, -(double) value, ConstructionTrace.first, array);
    }
    public static long primitives(int value) {
        ConstructionValues result = values(value);
        return result.wide + result.integer + result.tiny + result.character + result.small
            + (result.flag ? 1 : 0) + result.array[0] + (result.object == ConstructionTrace.first ? 17 : 19);
    }
    public static float single(int value) { return values(value).single; }
    public static double floating(int value) { return values(value).precise; }
    public static int nullArguments(int value) {
        ConstructionValues result = new ConstructionValues(false, (byte) 0, (char) 0, (short) 0,
            value, 0L, -0.0f, -0.0d, null, null);
        return result.object == null && result.array == null ? result.integer : -999;
    }
    public static int chain(int value) {
        ConstructionNode node = new ConstructionNode(value, value & 3);
        ConstructionTrace.escaped = node;
        int sum = 0;
        for (ConstructionNode cursor = node; cursor != null; cursor = cursor.next) sum += cursor.value;
        return sum;
    }
    public static int loop(int value) {
        ConstructionNode head = null, tail = null;
        int sum = 0;
        for (int index = 0; index <= (value & 3); index++) {
            ConstructionNode node = new ConstructionNode(value + index);
            if (tail == null) head = node; else tail.next = node;
            tail = node; sum += node.value;
        }
        ConstructionTrace.escaped = head;
        return sum;
    }
    public static int cycle(int value) {
        ConstructionNode first = new ConstructionNode(value);
        ConstructionNode second = new ConstructionNode(value + 1);
        first.next = second; second.next = first;
        first.links = new Object[] {first, second, first};
        second.links = first.links;
        ConstructionTrace.escaped = first; ConstructionTrace.saved = second;
        return first.next.next == first && first.links[0] == first.links[2]
            ? first.value + second.value + first.links.length : -999;
    }
    private static ConstructionNode create(int value) { return new ConstructionNode(value); }
    public static int sameSite(int value) {
        ConstructionNode first = create(value), second = create(value + 1);
        ConstructionTrace.escaped = first; ConstructionTrace.saved = second;
        return first != second ? first.value + second.value : -999;
    }
    public static int beforeDelegation(int value) {
        ConstructionOuter result = new ConstructionOuter(value);
        return result.base + result.inner.value;
    }
    public static int activeInitialization(int value) {
        ConstructionActive result = new ConstructionActive(value + 1);
        ConstructionTrace.escaped = result; ConstructionTrace.saved = ConstructionActive.singleton;
        return result.value + result.seen + ConstructionActive.singleton.value + ConstructionActive.singleton.seen;
    }
    public static int caughtBefore(int value) {
        try { ConstructionTrace.saved = new ConstructionFailBefore(value); }
        catch (Error error) { if (error != ConstructionTrace.first) return -999; }
        return ConstructionTrace.trace + (ConstructionTrace.saved == null ? 31 : 37);
    }
    public static int caughtInitialization(int value) {
        try { ConstructionTrace.saved = new ConstructionBadInitialization(); }
        catch (Error error) { if (error != ConstructionTrace.first) return -999; }
        return ConstructionTrace.trace + (ConstructionTrace.saved == null ? 31 : 37);
    }
    public static Object object(int value) {
        Object result = new Object(); ConstructionTrace.escaped = result; return result;
    }

    public static void failBeforeChain(int value) {
        ConstructionTrace.trace = ConstructionTrace.trace * 10 + 8;
        if (value < 0) throw ConstructionTrace.first;
    }
    public static void save(Object value) { ConstructionTrace.escaped = value; }

    // Original Java65 encodings: lawful cases javac21 does not normally emit.
    private static byte[] bytes(int... values) {
        byte[] result = new byte[values.length];
        for (int index = 0; index < values.length; index++) result[index] = (byte) values[index];
        return result;
    }
    private static byte[] join(byte[]... parts) throws Exception {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        for (byte[] part : parts) output.write(part);
        return output.toByteArray();
    }
    private static byte[] instruction(int opcode, int index) { return bytes(opcode, index >> 8, index); }
    private static class Pool {
        final ArrayList<byte[]> entries = new ArrayList<>();
        final LinkedHashMap<String, Integer> indices = new LinkedHashMap<>();
        int add(String key, byte[] data) {
            Integer prior = indices.get(key); if (prior != null) return prior;
            entries.add(data); indices.put(key, entries.size()); return entries.size();
        }
        int utf(String text) throws Exception {
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            DataOutputStream data = new DataOutputStream(output); data.writeByte(1); data.writeUTF(text);
            return add("U:" + text, output.toByteArray());
        }
        int type(String name) throws Exception { return add("C:" + name, instruction(7, utf(name))); }
        int ref(int tag, String owner, String name, String descriptor) throws Exception {
            int type = type(owner), ni = utf(name), di = utf(descriptor);
            int nt = add("N:" + name + descriptor, bytes(12, ni >> 8, ni, di >> 8, di));
            return add(tag + ":" + owner + ":" + name + descriptor, bytes(tag, type >> 8, type, nt >> 8, nt));
        }
    }
    private static byte[] rawMethod(Pool pool, int flags, String name, String descriptor,
                                    byte[] code, int stack, int locals, int[] handler, byte[] stackmap) throws Exception {
        ByteArrayOutputStream output = new ByteArrayOutputStream(); DataOutputStream data = new DataOutputStream(output);
        data.writeShort(flags); data.writeShort(pool.utf(name)); data.writeShort(pool.utf(descriptor)); data.writeShort(1);
        data.writeShort(pool.utf("Code"));
        data.writeInt(12 + code.length + (handler == null ? 0 : 8) + (stackmap == null ? 0 : 6 + stackmap.length));
        data.writeShort(stack); data.writeShort(locals); data.writeInt(code.length); data.write(code);
        data.writeShort(handler == null ? 0 : 1); if (handler != null) for (int value : handler) data.writeShort(value);
        data.writeShort(stackmap == null ? 0 : 1);
        if (stackmap != null) { data.writeShort(pool.utf("StackMapTable")); data.writeInt(stackmap.length); data.write(stackmap); }
        return output.toByteArray();
    }
    private static byte[] rawClass(String kind) throws Exception {
        String name = "fixtures/Construction" + kind;
        Pool pool = new Pool(); int self = pool.type(name), object = pool.type("java/lang/Object");
        ArrayList<byte[]> methods = new ArrayList<>();
        ByteArrayOutputStream fieldBytes = new ByteArrayOutputStream(); DataOutputStream fields = new DataOutputStream(fieldBytes);
        int fieldCount = 0;
        if (kind.startsWith("rawToken")) {
            int opcode = kind.equals("rawTokenNull") ? 0xc6 : kind.equals("rawTokenNonNull") ? 0xc7
                : kind.equals("rawTokenEqual") ? 0xa5 : 0xa6;
            boolean unary = opcode >= 0xc6;
            byte[] code = join(instruction(0xbb, object), unary ? bytes(0x59, opcode, 0, 6, 0x57, 3, 0xac, 0x57, 4, 0xac)
                : bytes(0x59, opcode, 0, 5, 3, 0xac, 4, 0xac));
            byte[] stackmap = unary ? bytes(0, 1, 255, 0, 10, 0, 1, 1, 0, 1, 8, 0, 0) : bytes(0, 1, 9);
            methods.add(rawMethod(pool, 9, "run", "(I)I", code, 2, 1, null, stackmap));
        } else {
            String[] descriptors = kind.equals("rawNarrow") ? new String[] {"B", "C", "S", "Z"}
                : new String[] {kind.equals("rawLong") ? "J" : kind.equals("rawNull") ? "Ljava/lang/Object;" : "I"};
            ArrayList<Integer> references = new ArrayList<>();
            byte[] stores = bytes();
            for (int index = 0; index < descriptors.length; index++) {
                String field = "value" + index, descriptor = descriptors[index];
                fields.writeShort(0x11); fields.writeShort(pool.utf(field)); fields.writeShort(pool.utf(descriptor)); fields.writeShort(0);
                int reference = pool.ref(9, name, field, descriptor); references.add(reference); fieldCount++;
                byte[] value = kind.equals("rawNull") ? bytes(1) : kind.equals("rawLong") ? bytes(0x1b, 0x85) : bytes(0x1b);
                stores = join(stores, bytes(0x2a), value, instruction(0xb5, reference));
            }
            byte[] chain = join(bytes(0x2a), instruction(0xb7, pool.ref(10, "java/lang/Object", "<init>", "()V")));
            byte[] constructor = join(stores, chain, bytes(0xb1));
            int[] handler = null; byte[] stackmap = null;
            if (kind.equals("rawCaughtBefore")) {
                byte[] finish = join(chain, stores, bytes(0xb1));
                byte[] normal = join(bytes(0x1b), instruction(0xb8, pool.ref(10, "fixtures/ConstructionCases", "failBeforeChain", "(I)V")), finish);
                int target = normal.length, error = pool.type("java/lang/Error");
                constructor = join(normal, bytes(0x4d), finish); handler = new int[] {0, 4, target, error};
                stackmap = bytes(0, 1, 255, target >> 8, target, 0, 2, 6, 1, 0, 1, 7, error >> 8, error);
            }
            methods.add(rawMethod(pool, 1, "<init>", "(I)V", constructor, kind.equals("rawLong") ? 3 : 2,
                handler == null ? 2 : 3, handler, stackmap));
            byte[] run = join(instruction(0xbb, self), bytes(0x59, 0x1a), instruction(0xb7, pool.ref(10, name, "<init>", "(I)V")));
            run = join(run, bytes(0x59), instruction(0xb8, pool.ref(10, "fixtures/ConstructionCases", "save", "(Ljava/lang/Object;)V")));
            String result = descriptors.length == 1 ? descriptors[0] : "I";
            if (kind.equals("rawNarrow")) {
                run = join(run, bytes(0x4c));
                for (int index = 0; index < references.size(); index++)
                    run = join(run, bytes(0x2b), instruction(0xb4, references.get(index)), index == 0 ? bytes() : bytes(0x60));
            } else run = join(run, instruction(0xb4, references.get(0)));
            run = join(run, bytes(result.equals("J") ? 0xad : result.startsWith("L") ? 0xb0 : 0xac));
            methods.add(rawMethod(pool, 9, "run", "(I)" + result, run, 3, 2, null, null));
        }
        ByteArrayOutputStream output = new ByteArrayOutputStream(); DataOutputStream data = new DataOutputStream(output);
        data.writeInt(0xcafebabe); data.writeShort(0); data.writeShort(65); data.writeShort(pool.entries.size() + 1);
        for (byte[] entry : pool.entries) data.write(entry);
        data.writeShort(0x21); data.writeShort(self); data.writeShort(object); data.writeShort(0);
        data.writeShort(fieldCount); data.write(fieldBytes.toByteArray()); data.writeShort(methods.size());
        for (byte[] method : methods) data.write(method);
        data.writeShort(0); return output.toByteArray();
    }
    private static class RawLoader extends ClassLoader {
        RawLoader(ClassLoader parent) { super(parent); }
        Class<?> define(byte[] bytes) { return defineClass(null, bytes, 0, bytes.length); }
    }

    private static String result(Object value, Object escaped) {
        if (value == null) return "R:null";
        if (value instanceof Long number) return "L:" + number;
        if (value instanceof Float number) return "F:" + String.format("%08x", Float.floatToRawIntBits(number));
        if (value instanceof Double number) return "D:" + String.format("%016x", Double.doubleToRawLongBits(number));
        if (value instanceof Integer number) return "I:" + number;
        if (value == escaped && value != null) return "R:escaped";
        throw new IllegalStateException("unexpected result");
    }
    private static void emit(String name, int input) throws Exception {
        URL location = ConstructionCases.class.getProtectionDomain().getCodeSource().getLocation();
        try (URLClassLoader loader = new URLClassLoader(new URL[] {location}, null)) {
            Class<?> root = Class.forName("fixtures.ConstructionCases", false, loader);
            Class<?> trace = Class.forName("fixtures.ConstructionTrace", true, loader);
            var inputField = trace.getDeclaredField("input"); inputField.setAccessible(true); inputField.setInt(null, input);
            var constructor = Class.forName("fixtures.ConstructionError", true, loader).getDeclaredConstructor(); constructor.setAccessible(true);
            Object first = constructor.newInstance();
            var firstField = trace.getDeclaredField("first"); firstField.setAccessible(true); firstField.set(null, first);
            var escapedField = trace.getDeclaredField("escaped"); escapedField.setAccessible(true);
            boolean raw = name.startsWith("raw");
            var selected = raw ? new RawLoader(loader).define(rawClass(name)).getMethod("run", int.class) : root.getMethod(name, int.class);
            String outcome;
            try { outcome = result(selected.invoke(null, input), escapedField.get(null)); }
            catch (InvocationTargetException error) {
                if (error.getCause() == first) outcome = "T:first";
                else throw new IllegalStateException("unexpected exception", error.getCause());
            }
            String descriptor = name.equals("primitives") || name.equals("rawLong") ? "(I)J" : name.equals("single") ? "(I)F"
                : name.equals("floating") ? "(I)D" : name.equals("object") || name.equals("rawNull") ? "(I)Ljava/lang/Object;" : "(I)I";
            System.out.println(name + "\t" + descriptor + "\tI:" + input + "\t" + outcome);
        }
    }
    public static void main(String[] args) throws Exception {
        if (args.length == 2 && args[0].equals("--emit-raw")) {
            java.nio.file.Path directory = java.nio.file.Path.of(args[1]);
            java.nio.file.Files.createDirectories(directory);
            for (String kind : new String[] {"rawOwn", "rawLong", "rawNull", "rawNarrow", "rawCaughtBefore",
                    "rawTokenNull", "rawTokenNonNull", "rawTokenEqual", "rawTokenNotEqual"})
                java.nio.file.Files.write(directory.resolve(kind + ".class"), rawClass(kind));
            return;
        }
        for (String method : new String[] {"delegated", "caught", "uncaught", "primitives", "single", "floating",
                "nullArguments", "chain", "loop", "cycle", "sameSite", "beforeDelegation", "activeInitialization",
                "caughtBefore", "caughtInitialization", "object", "rawOwn", "rawLong", "rawNull", "rawNarrow",
                "rawCaughtBefore", "rawTokenNull", "rawTokenNonNull", "rawTokenEqual", "rawTokenNotEqual"})
            for (int input : new int[] {-7, -2, -1, 0, 1, 2, 7}) emit(method, input);
        for (int input : new int[] {-65537, -32769, -129, 127, 128, 32768, 65536, Integer.MAX_VALUE, Integer.MIN_VALUE})
            emit("rawNarrow", input);
    }
}
