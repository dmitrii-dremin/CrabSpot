import app.BootstrapStatus;
import app.Policy;
import java.lang.reflect.Array;
import java.lang.reflect.Field;
import java.net.URLClassLoader;
import java.nio.file.Path;

public class ReadPolicy {
    static int bit(boolean b) { return b ? 1 : 0; }
    static String arrayField(String name) throws Exception {
        try {
            Field field = Class.class.getDeclaredField(name);
            field.setAccessible(true);
            Object value = field.get(null);
            return value.getClass().getName() + "/" + Array.getLength(value);
        } catch (NoSuchFieldException absent) {
            return "not-reflectively-present";
        }
    }
    static void child(Path dir, boolean first) throws Exception {
        try (var loader = new URLClassLoader(new java.net.URL[]{dir.toUri().toURL()}, null)) {
            loader.setDefaultAssertionStatus(!first);
            loader.setClassAssertionStatus("app.Policy", first);
            var c = Class.forName("app.Policy", false, loader);
            var call = c.getMethod("main", String[].class);
            int initial = bit(c.desiredAssertionStatus());
            call.invoke(null, (Object)new String[0]);
            int effects1 = c.getField("effects").getInt(null);
            loader.setClassAssertionStatus("app.Policy", !first);
            int later = bit(c.desiredAssertionStatus());
            call.invoke(null, (Object)new String[0]);
            System.out.println("child=" + bit(c != Policy.class && c.getClassLoader() == loader) + "/" +
                               initial + "/" + effects1 + "/" + later + "/" + c.getField("effects").getInt(null));
        }
    }
    public static void main(String[] argv) throws Exception {
        boolean portable = argv.length == 2 && argv[1].equals("portable");
        // Complete application operations before the implementation-field observer touches a helper.
        BootstrapStatus.main(new String[0]);
        Policy.main(new String[0]);
        ReadEquality.main(new String[0]);
        String policy = "policy=" + BootstrapStatus.status + "/" + BootstrapStatus.phase + "/" +
                        Policy.status + "/" + Policy.effects;
        if (!portable) {
            Class<?> helper = Class.forName("java.lang.StringLatin1", false, null);
            Field disabled = helper.getDeclaredField("$assertionsDisabled");
            disabled.setAccessible(true);
            policy += "/" + bit(helper.desiredAssertionStatus()) + "/" + bit(disabled.getBoolean(null));
        }
        System.out.println(policy);
        System.out.println("loaders=" + bit(String.class.getClassLoader() == null) + "/" +
                           bit(Policy.class.getClassLoader() != null) + "/" +
                           bit(String[].class.getClassLoader() == null) + "/" +
                           bit(Policy[].class.getClassLoader() == Policy.class.getClassLoader()));
        if (!portable) {
            System.out.println("arrays=" + bit(String[].class.desiredAssertionStatus()) + "/" +
                               bit(Policy[].class.desiredAssertionStatus()) + "/" +
                               bit(int[].class.desiredAssertionStatus()));
            System.out.println("class-fields=" + arrayField("EMPTY_CLASS_ARRAY") + "," +
                               arrayField("serialPersistentFields"));
        }
        child(Path.of(argv[0]), true);
        child(Path.of(argv[0]), false);
    }
}
