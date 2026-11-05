import java.nio.file.*;
import java.net.*;
import java.lang.reflect.InvocationTargetException;
public class Split {
    public static void main(String[] args) throws Exception {
        URL root = Path.of(args[0]).toUri().toURL();
        try (URLClassLoader family = new URLClassLoader(new URL[]{root}, null);
             URLClassLoader split = new URLClassLoader(new URL[]{root}, family) {
                 protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
                     if (!name.equals("split.Parent$Child")) return super.loadClass(name, resolve);
                     Class<?> known = findLoadedClass(name);
                     Class<?> found = known == null ? findClass(name) : known;
                     if (resolve) resolveClass(found);
                     return found;
                 }
             }) {
            Class<?> host = family.loadClass("split.Parent");
            for (ClassLoader loader : new ClassLoader[]{family, split}) {
                Class<?> child = loader.loadClass("split.Parent$Child");
                System.out.print((child.getNestHost() == host) + "/" + host.isNestmateOf(child) + "/");
                try { System.out.println(child.getMethod("read").invoke(null)); }
                catch (InvocationTargetException e) { System.out.println(e.getCause().getClass().getSimpleName()); }
            }
        }
    }
}
