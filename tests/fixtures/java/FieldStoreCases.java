package fixtures;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.util.ArrayList;

// Development-only JDK oracle. This original emitter deliberately omits narrowing
// instructions before putfield: javac source assignments would hide that behavior.
public class FieldStoreCases {
    private static final String[] NAMES = {
        "flag", "small", "letter", "narrow", "integer", "wide", "single", "precise"
    };
    private static final String[] TYPES = {"Z", "B", "C", "S", "I", "J", "F", "D"};

    private static class Pool {
        final ArrayList<byte[]> entries = new ArrayList<>();
        int utf(String text) throws Exception {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            out.writeByte(1); out.writeUTF(text);
            entries.add(bytes.toByteArray()); return entries.size();
        }
        int entry(int tag, int first, int second) throws Exception {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            out.writeByte(tag); out.writeShort(first);
            if (tag != 7) out.writeShort(second);
            entries.add(bytes.toByteArray()); return entries.size();
        }
    }
    private static class Loader extends ClassLoader {
        Class<?> define(byte[] bytes) {
            return defineClass("fixtures.RawInstanceStores", bytes, 0, bytes.length);
        }
    }
    private static void method(DataOutputStream out, int flags, int name, int descriptor,
            int codeName, int stack, int locals, byte[] code) throws Exception {
        out.writeShort(flags); out.writeShort(name); out.writeShort(descriptor);
        out.writeShort(1); out.writeShort(codeName); out.writeInt(12 + code.length);
        out.writeShort(stack); out.writeShort(locals); out.writeInt(code.length);
        out.write(code); out.writeShort(0); out.writeShort(0);
    }
    private static byte[] rawClass() throws Exception {
        var pool = new Pool();
        int self = pool.entry(7, pool.utf("fixtures/RawInstanceStores"), 0);
        int parent = pool.entry(7, pool.utf("java/lang/Object"), 0);
        int init = pool.utf("<init>"), voidType = pool.utf("()V");
        int initType = pool.entry(12, init, voidType);
        int parentInit = pool.entry(10, parent, initType);
        int selfInit = pool.entry(10, self, initType);
        int codeName = pool.utf("Code");
        int[] names = new int[8], types = new int[8], fields = new int[8];
        int[] methods = new int[8], signatures = new int[8];
        for (int i = 0; i < 8; i++) {
            names[i] = pool.utf(NAMES[i]); types[i] = pool.utf(TYPES[i]);
            fields[i] = pool.entry(9, self, pool.entry(12, names[i], types[i]));
            methods[i] = pool.utf(NAMES[i] + "RoundTrip");
            String computational = i < 5 ? "I" : TYPES[i];
            signatures[i] = pool.utf("(" + computational + ")" + computational);
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(0xcafebabe); out.writeShort(0); out.writeShort(65);
        out.writeShort(pool.entries.size() + 1);
        for (byte[] entry : pool.entries) out.write(entry);
        out.writeShort(0x21); out.writeShort(self); out.writeShort(parent);
        out.writeShort(0); out.writeShort(8);
        for (int i = 0; i < 8; i++) {
            out.writeShort(1); out.writeShort(names[i]); out.writeShort(types[i]); out.writeShort(0);
        }
        out.writeShort(9);
        method(out, 1, init, voidType, codeName, 1, 1,
            new byte[] {0x2a, (byte)0xb7, (byte)(parentInit >> 8), (byte)parentInit, (byte)0xb1});
        for (int i = 0; i < 8; i++) {
            int load = i < 5 ? 0x1a : new int[] {0x1e, 0x22, 0x26}[i - 5];
            int ret = i < 5 ? 0xac : new int[] {0xad, 0xae, 0xaf}[i - 5];
            method(out, 9, methods[i], signatures[i], codeName, 4, i == 5 || i == 7 ? 2 : 1,
                new byte[] {(byte)0xbb, (byte)(self >> 8), (byte)self, 0x59,
                    (byte)0xb7, (byte)(selfInit >> 8), (byte)selfInit, 0x59, (byte)load,
                    (byte)0xb5, (byte)(fields[i] >> 8), (byte)fields[i],
                    (byte)0xb4, (byte)(fields[i] >> 8), (byte)fields[i], (byte)ret});
        }
        out.writeShort(0); return bytes.toByteArray();
    }
    private static String value(Object value, boolean result) {
        if (value instanceof Integer integer) return "I:" + integer;
        if (value instanceof Long wide) return "L:" + wide;
        if (value instanceof Float single) return result && Float.isNaN(single) ? "F:NaN"
            : String.format("F:%08x", Float.floatToRawIntBits(single));
        if (value instanceof Double precise) return result && Double.isNaN(precise) ? "D:NaN"
            : String.format("D:%016x", Double.doubleToRawLongBits(precise));
        throw new IllegalArgumentException("unexpected oracle value");
    }
    private static void emit(Class<?> raw, int index, Object input) throws Exception {
        Class<?> type = index < 5 ? int.class : index == 5 ? long.class
            : index == 6 ? float.class : double.class;
        String computational = index < 5 ? "I" : TYPES[index];
        String method = NAMES[index] + "RoundTrip";
        Object result = raw.getMethod(method, type).invoke(null, input);
        System.out.println(method + "\t(" + computational + ")" + computational
            + "\t" + value(input, false) + "\t" + value(result, true));
    }
    public static void main(String[] ignored) throws Exception {
        Class<?> raw = new Loader().define(rawClass());
        for (int value : new int[] {Integer.MIN_VALUE, -65537, -65536, -32769, -32768, -257,
                -256, -129, -128, -2, -1, 0, 1, 2, 127, 128, 255, 256, 32767, 32768,
                65535, 65536, Integer.MAX_VALUE}) {
            for (int i = 0; i < 5; i++) emit(raw, i, value);
        }
        for (long value : new long[] {Long.MIN_VALUE, Long.MIN_VALUE + 1, -4294967296L,
                -1, 0, 1, 4294967296L, Long.MAX_VALUE}) emit(raw, 5, value);
        for (int bits : new int[] {0, 0x80000000, 1, 0x80000001, 0x007fffff,
                0x00800000, 0x3f800000, 0xbf800000, 0x7f7fffff, 0xff7fffff,
                0x7f800000, 0xff800000, 0x7fc01234, 0xffa05678}) {
            emit(raw, 6, Float.intBitsToFloat(bits));
        }
        for (long bits : new long[] {0, 0x8000000000000000L, 1, 0x8000000000000001L,
                0x000fffffffffffffL, 0x0010000000000000L, 0x3ff0000000000000L,
                0xbff0000000000000L, 0x7fefffffffffffffL, 0xffefffffffffffffL,
                0x7ff0000000000000L, 0xfff0000000000000L, 0x7ff8000000001234L,
                0xfff0000000005678L}) emit(raw, 7, Double.longBitsToDouble(bits));
    }
}
