import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;
public class First {
    public static void main(String[] args) throws Exception {
        char[] units = {67, 114, 97, 98, 83, 112, 111, 116, 0, 8364, 57343, 52, 50};
        String original = new String(units);
        String canonical = original.intern();
        units[0] = 97;
        URL[] path = {Path.of(args[0]).toUri().toURL()};
        try (var a = new URLClassLoader(path, null); var b = new URLClassLoader(path, null)) {
            Class<?> ca = a.loadClass("split.Late");
            Class<?> cb = b.loadClass("split.Late");
            Object other = cb.getMethod("literal").invoke(null);
            Object first = ca.getMethod("literal").invoke(null);
            Object constant = a.loadClass("split.Read").getMethod("read").invoke(null);
            String copy = new String(original);
            System.out.print((canonical == original) + "/" + (first == original) + "/" +
                (canonical == first) + "/" + (other == first) + "/" + (constant == first) + "/" + (ca != cb) + "/" +
                (copy != first && copy.intern() == first) + "/" + (original.charAt(0) == 67));
        }
    }
}
