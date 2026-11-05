package fixtures;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.TreeMap;

/** Reference-JDK metadata capture only. Application classes and projected bodies are never executed. */
public final class TypeVerificationFacts {
    private static final String[] CORE = {"java/lang/Object", "java/lang/Cloneable", "java/io/Serializable",
        "java/lang/Throwable", "java/lang/String", "java/lang/Class", "java/lang/invoke/MethodType",
        "java/lang/invoke/MethodHandle"};

    private record Member(int flags, String name, String descriptor, boolean code) {}
    private record Metadata(int minor, int major, int flags, String name, String parent,
                            List<String> interfaces, List<Member> fields, List<Member> methods,
                            LinkedHashSet<String> references) {}

    private static String utf(String[] strings, int index) {
        if (index <= 0 || index >= strings.length || strings[index] == null)
            throw new IllegalArgumentException("expected Utf8 at " + index);
        return strings[index];
    }
    private static String className(String[] strings, int[] classes, int index) {
        if (index <= 0 || index >= classes.length || classes[index] == 0)
            throw new IllegalArgumentException("expected Class at " + index);
        return utf(strings, classes[index]);
    }
    private static void descriptorNames(String descriptor, LinkedHashSet<String> names) {
        for (int index = 0; index < descriptor.length(); index++) {
            if (descriptor.charAt(index) != 'L') continue;
            int end = descriptor.indexOf(';', index);
            if (end < 0) throw new IllegalArgumentException("unterminated descriptor");
            names.add(descriptor.substring(index + 1, end)); index = end;
        }
    }
    private static boolean attributes(DataInputStream input, String[] strings) throws Exception {
        boolean code = false;
        int count = input.readUnsignedShort();
        for (int index = 0; index < count; index++) {
            String name = utf(strings, input.readUnsignedShort());
            long length = Integer.toUnsignedLong(input.readInt());
            if (length > input.available()) throw new IllegalArgumentException("truncated attribute");
            input.skipNBytes(length); code |= name.equals("Code");
        }
        return code;
    }
    private static List<Member> members(DataInputStream input, String[] strings,
                                         LinkedHashSet<String> references) throws Exception {
        List<Member> result = new ArrayList<>();
        int count = input.readUnsignedShort();
        for (int index = 0; index < count; index++) {
            int flags = input.readUnsignedShort();
            String name = utf(strings, input.readUnsignedShort());
            String descriptor = utf(strings, input.readUnsignedShort());
            descriptorNames(descriptor, references);
            result.add(new Member(flags, name, descriptor, attributes(input, strings)));
        }
        return result;
    }
    private static Metadata read(byte[] bytes) throws Exception {
        // This small tooling reader extracts only facts; Rust independently checks the projection.
        if (bytes.length > 16 * 1024 * 1024) throw new IllegalArgumentException("oversized class resource");
        try (DataInputStream input = new DataInputStream(new ByteArrayInputStream(bytes))) {
            if (input.readInt() != 0xcafebabe) throw new IllegalArgumentException("class magic");
            int minor = input.readUnsignedShort(), major = input.readUnsignedShort();
            if (major < 51 || major > 65 || minor != 0) throw new IllegalArgumentException("fixture version");
            int count = input.readUnsignedShort();
            String[] strings = new String[count]; int[] classes = new int[count];
            List<Integer> descriptors = new ArrayList<>();
            for (int index = 1; index < count; index++) {
                int tag = input.readUnsignedByte();
                switch (tag) {
                    case 1: strings[index] = input.readUTF(); break;
                    case 3: case 4: input.readInt(); break;
                    case 5: case 6: input.readLong(); index++; break;
                    case 7: classes[index] = input.readUnsignedShort(); break;
                    case 8: case 19: case 20: input.readUnsignedShort(); break;
                    case 9: case 10: case 11: case 17: case 18: input.readInt(); break;
                    case 12: input.readUnsignedShort(); descriptors.add(input.readUnsignedShort()); break;
                    case 15: input.readUnsignedByte(); input.readUnsignedShort(); break;
                    case 16: descriptors.add(input.readUnsignedShort()); break;
                    default: throw new IllegalArgumentException("constant tag " + tag);
                }
            }
            LinkedHashSet<String> references = new LinkedHashSet<>();
            for (int index : classes) if (index != 0) {
                String name = utf(strings, index);
                if (name.startsWith("[")) descriptorNames(name, references); else references.add(name);
            }
            for (int index : descriptors) descriptorNames(utf(strings, index), references);
            int flags = input.readUnsignedShort();
            String name = className(strings, classes, input.readUnsignedShort());
            int parentIndex = input.readUnsignedShort();
            String parent = parentIndex == 0 ? "" : className(strings, classes, parentIndex);
            List<String> interfaces = new ArrayList<>();
            int interfaceCount = input.readUnsignedShort();
            for (int index = 0; index < interfaceCount; index++)
                interfaces.add(className(strings, classes, input.readUnsignedShort()));
            List<Member> fields = members(input, strings, references);
            List<Member> methods = members(input, strings, references);
            attributes(input, strings);
            if (input.available() != 0) throw new IllegalArgumentException("trailing class bytes");
            return new Metadata(minor, major, flags, name, parent, interfaces, fields, methods, references);
        }
    }

    private static final class Pool {
        final List<byte[]> entries = new ArrayList<>();
        final LinkedHashMap<String, Integer> keys = new LinkedHashMap<>();
        int add(String key, byte[] bytes) {
            Integer existing = keys.get(key);
            if (existing != null) return existing;
            if (entries.size() >= 65534) throw new IllegalArgumentException("projection constant pool limit");
            entries.add(bytes); keys.put(key, entries.size()); return entries.size();
        }
        int utf(String value) throws Exception {
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream data = new DataOutputStream(bytes); data.writeByte(1); data.writeUTF(value);
            return add("U:" + value, bytes.toByteArray());
        }
        int type(String value) throws Exception {
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream data = new DataOutputStream(bytes); data.writeByte(7); data.writeShort(utf(value));
            return add("C:" + value, bytes.toByteArray());
        }
    }
    private static int parameterSlots(Member member) {
        String descriptor = member.descriptor();
        if (!descriptor.startsWith("(")) throw new IllegalArgumentException("method descriptor");
        int slots = (member.flags() & 8) == 0 ? 1 : 0;
        for (int index = 1; descriptor.charAt(index) != ')'; index++) {
            boolean array = false;
            while (descriptor.charAt(index) == '[') { array = true; index++; }
            char kind = descriptor.charAt(index);
            if (kind == 'L') {
                index = descriptor.indexOf(';', index);
                if (index < 0) throw new IllegalArgumentException("method reference descriptor");
            }
            slots += !array && (kind == 'J' || kind == 'D') ? 2 : 1;
        }
        if (slots > 255) throw new IllegalArgumentException("method parameter slots");
        return slots;
    }
    private static byte[] projectedMembers(Pool pool, List<Member> members, boolean methods) throws Exception {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream(); DataOutputStream data = new DataOutputStream(bytes);
        data.writeShort(members.size());
        for (Member member : members) {
            data.writeShort(member.flags()); data.writeShort(pool.utf(member.name()));
            data.writeShort(pool.utf(member.descriptor()));
            boolean body = methods && member.code();
            data.writeShort(body ? 1 : 0);
            if (body) {
                // Inert synthetic Code solely for parser/registry admission. NEVER selected or executed.
                data.writeShort(pool.utf("Code")); data.writeInt(14);
                data.writeShort(1); data.writeShort(parameterSlots(member)); data.writeInt(2);
                data.writeByte(1); data.writeByte(0xbf); data.writeShort(0); data.writeShort(0);
            }
        }
        return bytes.toByteArray();
    }
    private static byte[] project(Metadata metadata) throws Exception {
        Pool pool = new Pool(); int self = pool.type(metadata.name());
        int parent = metadata.parent().isEmpty() ? 0 : pool.type(metadata.parent());
        List<Integer> interfaces = new ArrayList<>();
        for (String name : metadata.interfaces()) interfaces.add(pool.type(name));
        byte[] fields = projectedMembers(pool, metadata.fields(), false);
        byte[] methods = projectedMembers(pool, metadata.methods(), true);
        ByteArrayOutputStream bytes = new ByteArrayOutputStream(); DataOutputStream data = new DataOutputStream(bytes);
        data.writeInt(0xcafebabe); data.writeShort(metadata.minor()); data.writeShort(metadata.major());
        data.writeShort(pool.entries.size() + 1); for (byte[] entry : pool.entries) data.write(entry);
        data.writeShort(metadata.flags()); data.writeShort(self); data.writeShort(parent);
        data.writeShort(interfaces.size()); for (int type : interfaces) data.writeShort(type);
        data.write(fields); data.write(methods); data.writeShort(0); return bytes.toByteArray();
    }
    private static String units(String text) {
        StringBuilder result = new StringBuilder();
        for (int index = 0; index < text.length(); index++)
            result.append(String.format("%04x", (int) text.charAt(index)));
        return result.toString();
    }
    private static String digest(byte[] bytes) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
    }
    private static void write(Path root, String name, byte[] bytes) throws Exception {
        Path destination = root.resolve(name + ".class").normalize();
        if (!destination.startsWith(root.normalize())) throw new IllegalArgumentException("class path escapes root");
        Files.createDirectories(destination.getParent()); Files.write(destination, bytes);
    }
    public static void main(String[] args) throws Exception {
        if (args.length < 2) throw new IllegalArgumentException("OUTPUT_DIRECTORY INPUT_CLASS_DIRECTORY...");
        Path output = Path.of(args[0]); Files.createDirectories(output);
        LinkedHashSet<String> targetNames = new LinkedHashSet<>(), requested = new LinkedHashSet<>();
        for (String name : CORE) requested.add(name);
        for (int index = 1; index < args.length; index++) {
            try (var paths = Files.walk(Path.of(args[index]))) {
                for (Path path : paths.filter(p -> p.toString().endsWith(".class")).sorted().toList()) {
                    Metadata metadata = read(Files.readAllBytes(path));
                    targetNames.add(metadata.name()); requested.addAll(metadata.references());
                }
            }
        }
        requested.removeAll(targetNames);
        Files.writeString(output.resolve("requested.txt"), String.join("\n", requested.stream().sorted()
            .map(TypeVerificationFacts::units).toList()) + "\n", StandardCharsets.UTF_8);
        ArrayDeque<String> pending = new ArrayDeque<>(requested.stream().sorted().toList());
        TreeMap<String, String> records = new TreeMap<>(), missing = new TreeMap<>();
        while (!pending.isEmpty()) {
            String name = pending.removeFirst();
            if (records.containsKey(name) || missing.containsKey(name)) continue;
            if (records.size() + missing.size() >= 4096) throw new IllegalArgumentException("capture class bound");
            Class<?> type;
            try { type = Class.forName(name.replace('/', '.'), false, ClassLoader.getPlatformClassLoader()); }
            catch (ClassNotFoundException | LinkageError failure) {
                missing.put(name, units(name) + "\t" + failure.getClass().getSimpleName()); continue;
            }
            String loader = type.getClassLoader() == null ? "bootstrap"
                : type.getClassLoader() == ClassLoader.getPlatformClassLoader() ? "platform" : "unsupported";
            if (loader.equals("unsupported")) throw new IllegalArgumentException("unexpected defining loader: " + name);
            byte[] original;
            var resource = type.getResource("/" + name + ".class");
            if (resource == null || !resource.getProtocol().equals("jrt"))
                throw new IllegalArgumentException("not a reference JDK resource: " + name);
            try (var stream = resource.openStream()) { original = stream.readNBytes(16 * 1024 * 1024 + 1); }
            Metadata metadata = read(original);
            if (!metadata.name().equals(name)) throw new IllegalArgumentException("resource identity mismatch");
            if (!metadata.parent().isEmpty()) pending.add(metadata.parent());
            pending.addAll(metadata.interfaces());
            byte[] projected = project(metadata);
            write(output.resolve("original"), name, original); // Temporary evidence only, never distributed.
            write(output.resolve("projected"), name, projected);
            StringBuilder record = new StringBuilder("C\t" + loader + "\t" + units(name) + "\t"
                + metadata.major() + "\t" + metadata.minor() + "\t" + metadata.flags() + "\t"
                + units(metadata.parent()) + "\t" + digest(original) + "\t" + resource + "\n");
            for (String implemented : metadata.interfaces()) record.append("I\t").append(units(implemented)).append('\n');
            for (Member member : metadata.fields()) record.append("F\t").append(member.flags()).append('\t')
                .append(units(member.name())).append('\t').append(units(member.descriptor())).append('\n');
            for (Member member : metadata.methods()) record.append("M\t").append(member.flags()).append('\t')
                .append(units(member.name())).append('\t').append(units(member.descriptor())).append('\t')
                .append(member.code() ? "1" : "0").append('\n');
            records.put(name, record.toString());
        }
        for (String name : CORE) if (!records.containsKey(name)) throw new IllegalArgumentException("missing core role " + name);
        Files.writeString(output.resolve("facts.tsv"), String.join("", records.values()), StandardCharsets.UTF_8);
        Files.writeString(output.resolve("missing.tsv"), String.join("\n", missing.values())
            + (missing.isEmpty() ? "" : "\n"), StandardCharsets.UTF_8);
        System.out.println("Captured " + records.size() + " JDK class fact sets; " + missing.size() + " unavailable names; "
            + targetNames.size() + " target classes excluded from projection");
    }
}
