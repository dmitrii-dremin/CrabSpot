import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
public class StickyProbe {
    static final class Gate extends ClassLoader {
        final Path classes;
        boolean enabled;
        int attempts;
        Gate(Path classes) {
            super(ClassLoader.getPlatformClassLoader());
            this.classes = classes;
        }
        @Override
        protected Class<?> findClass(String name) throws ClassNotFoundException {
            if (name.equals("owner.Late")) {
                attempts++;
                if (!enabled)
                    throw new ClassNotFoundException(name);
            }
            try {
                var bytes = Files.readAllBytes(classes.resolve(name.replace('.', '/') + ".class"));
                return defineClass(name, bytes, 0, bytes.length);
            } catch (IOException e) {
                throw new ClassNotFoundException(name, e);
            }
        }
    }
    static String invoke(Class<?> c, String member) throws Exception {
        try {
            return "I:" + c.getDeclaredMethod(member).invoke(null);
        } catch (InvocationTargetException e) {
            return e.getCause().getClass().getSimpleName();
        }
    }
    public static void main(String[] args) throws Exception {
        var path = Path.of(args[0]);
        for (String first : List.of("field1", "method")) {
            for (boolean explicitlyLoad : List.of(false, true)) {
                var g = new Gate(path);
                var stale = Class.forName("caller.Sticky", true, g);
                var fresh = Class.forName("caller.Fresh", true, g);
                var fail = invoke(stale, first);
                int before = g.attempts;
                g.enabled = true;
                if (explicitlyLoad)
                    Class.forName("owner.Late", false, g);
                int admitted = g.attempts;
                var again = invoke(stale, first);
                int repeated = g.attempts;
                var sibling = invoke(stale, "field2");
                var other = invoke(stale, first.equals("method") ? "field1" : "method");
                int previous = g.attempts;
                var independent = invoke(fresh, first);
                System.out.println(String.join("\t", first, Boolean.toString(explicitlyLoad), fail,
                    again, sibling, other, independent,
                    before + "/" + admitted + "/" + repeated + "/" + previous + "/" + g.attempts));
            }
        }
    }
}
