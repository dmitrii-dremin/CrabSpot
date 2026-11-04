package fixtures;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.lang.reflect.InvocationTargetException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;

/** Original encodings and explicit JDK verification-stage oracle, never a VM dependency. */
public final class CodeValidationCases {
    private static final String[] KINDS = {
        "DeadValid", "DeadBadLdc", "DeadBadField", "DeadBadInvoke", "DeadBadLocal",
        "DeadBadWide", "DeadBadLong", "DeadInterface51", "DeadInterface52",
        "HandlerValid", "HandlerStartMiddle", "HandlerEndMiddle", "HandlerTargetMiddle",
        "FrameMiddle", "UninitValid", "UninitWrongSite", "DeadTypeMismatch", "DeadUninitializedLocal"
    };

    private static byte[] bytes(int... values) {
        byte[] result = new byte[values.length];
        for (int i = 0; i < values.length; i++) result[i] = (byte) values[i];
        return result;
    }
    private static byte[] join(byte[]... parts) throws Exception {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        for (byte[] part : parts) output.write(part);
        return output.toByteArray();
    }
    private static byte[] instruction(int opcode, int index) { return bytes(opcode, index >> 8, index); }

    private static final class Pool {
        final ArrayList<byte[]> entries = new ArrayList<>();
        final LinkedHashMap<String, Integer> indices = new LinkedHashMap<>();
        int add(String key, byte[] data) {
            Integer prior = indices.get(key);
            if (prior != null) return prior;
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

    private static byte[] rawClass(String kind) throws Exception {
        Pool pool = new Pool();
        int self = pool.type("validation/" + kind), object = pool.type("java/lang/Object");
        int major = kind.equals("DeadInterface51") ? 51 : kind.equals("DeadInterface52") ? 52 : 65;
        byte[] code, stackmap;
        int stack = 1, locals = 0;
        int[] handler = null;
        if (kind.startsWith("Handler") || kind.equals("FrameMiddle")) {
            int throwable = pool.type("java/lang/Throwable");
            code = bytes(0x10, 1, 0xac, 0x57, 5, 0xac);
            handler = new int[] {kind.equals("HandlerStartMiddle") ? 1 : 0,
                kind.equals("HandlerEndMiddle") ? 1 : 2,
                kind.equals("HandlerTargetMiddle") ? 1 : 3, throwable};
            stackmap = bytes(0, 1, 255, 0, kind.equals("FrameMiddle") ? 1 : 3,
                0, 0, 0, 1, 7, throwable >> 8, throwable);
        } else if (kind.startsWith("Uninit")) {
            code = join(instruction(0xbb, object), bytes(0x57, 4, 0xac, 0x57, 5, 0xac));
            stackmap = bytes(0, 1, 255, 0, 6, 0, 0, 0, 1, 8, 0,
                kind.equals("UninitValid") ? 0 : 3);
        } else {
            byte[] tail;
            switch (kind) {
                case "DeadValid": tail = bytes(5, 0xac); break;
                case "DeadBadLdc": tail = bytes(0x12, pool.ref(10, "java/lang/Object", "<init>", "()V"), 0xac); break;
                case "DeadBadField": tail = join(instruction(0xb2, pool.ref(10, "java/lang/Object", "<init>", "()V")), bytes(0xac)); break;
                case "DeadBadInvoke": tail = join(instruction(0xb8, object), bytes(0xac)); break;
                case "DeadBadLocal": tail = bytes(0x1a, 0xac); break;
                case "DeadBadWide": tail = bytes(0xc4, 0x15, 255, 255, 0xac); locals = 2; break;
                case "DeadBadLong": tail = bytes(0x1f, 0x88, 0xac); locals = 2; break;
                case "DeadInterface51":
                case "DeadInterface52": tail = join(instruction(0xb8, pool.ref(11, "java/lang/Runnable", "run", "()V")), bytes(5, 0xac)); break;
                case "DeadTypeMismatch": tail = bytes(1, 0xac); break;
                case "DeadUninitializedLocal": tail = bytes(0x1a, 0xac); locals = 1; break;
                default: throw new IllegalArgumentException(kind);
            }
            code = join(bytes(4, 0xac), tail); stackmap = bytes(0, 1, 2); stack = 2;
        }
        int name = pool.utf("run"), descriptor = pool.utf("()I"), codeName = pool.utf("Code"), mapName = pool.utf("StackMapTable");
        ByteArrayOutputStream output = new ByteArrayOutputStream(); DataOutputStream data = new DataOutputStream(output);
        data.writeInt(0xcafebabe); data.writeShort(0); data.writeShort(major); data.writeShort(pool.entries.size() + 1);
        for (byte[] entry : pool.entries) data.write(entry);
        data.writeShort(0x21); data.writeShort(self); data.writeShort(object);
        data.writeShort(0); data.writeShort(0); data.writeShort(1);
        data.writeShort(9); data.writeShort(name); data.writeShort(descriptor); data.writeShort(1); data.writeShort(codeName);
        data.writeInt(12 + code.length + (handler == null ? 0 : 8) + 6 + stackmap.length);
        data.writeShort(stack); data.writeShort(locals); data.writeInt(code.length); data.write(code);
        data.writeShort(handler == null ? 0 : 1);
        if (handler != null) for (int value : handler) data.writeShort(value);
        data.writeShort(1); data.writeShort(mapName); data.writeInt(stackmap.length); data.write(stackmap);
        data.writeShort(0); return output.toByteArray();
    }

    private static final class RawLoader extends ClassLoader {
        RawLoader() { super(ClassLoader.getPlatformClassLoader()); }
        Class<?> define(byte[] data) { return defineClass(null, data, 0, data.length); }
    }
    private static void probe(String kind, Path directory) throws Exception {
        // Read the emitted artifact: the reference result is bound to the exact file tested by Rust.
        String stage = "define";
        try {
            Class<?> type = new RawLoader().define(Files.readAllBytes(directory.resolve(kind + ".class")));
            stage = "lookup";
            var method = type.getDeclaredMethod("run");
            stage = "invoke";
            Object result = method.invoke(null);
            System.out.println(kind + "\t" + stage + "\tI:" + result);
        } catch (InvocationTargetException error) {
            System.out.println(kind + "\t" + stage + "\t" + error.getCause().getClass().getSimpleName());
        } catch (LinkageError error) {
            System.out.println(kind + "\t" + stage + "\t" + error.getClass().getSimpleName());
        }
    }
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("--emit-raw|--probe DIRECTORY");
        Path directory = Path.of(args[1]);
        if (args[0].equals("--emit-raw")) {
            Files.createDirectories(directory);
            for (String kind : KINDS) Files.write(directory.resolve(kind + ".class"), rawClass(kind));
        } else if (args[0].equals("--probe")) {
            for (String kind : KINDS) probe(kind, directory);
        } else throw new IllegalArgumentException(args[0]);
    }
}
