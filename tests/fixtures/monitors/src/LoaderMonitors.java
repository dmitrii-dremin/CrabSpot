import java.net.*;
import java.nio.file.*;
import java.lang.reflect.*;

public class LoaderMonitors {
    static StringBuilder phases = new StringBuilder();

    static void inspect(Class<?>... locks) {
        if (phases.length() != 0) phases.append("/");
        for (Class<?> lock : locks) phases.append(Thread.holdsLock(lock) ? "1" : "0");
    }

    static void invoke(Class<?> child, Runnable query) {
        try {
            child.getMethod("visit", Runnable.class).invoke(null, query);
        } catch (ReflectiveOperationException e) {
            throw new AssertionError(e);
        }
    }

    public static void main(String[] args) throws Exception {
        URL[] url = {Path.of(args[0]).toUri().toURL()};
        try (URLClassLoader a = new URLClassLoader(url, null);
URLClassLoader b = new URLClassLoader(url, null);) {
                Class<?> ca = a.loadClass("own.Child");
                Class<?> cb = b.loadClass("own.Child");
                Class<?>[] locks = {ca.getSuperclass(), ca, cb.getSuperclass(), cb};
                inspect(locks);
                invoke(ca, ()->{
                    inspect(locks);
                    invoke(cb, ()->{
                        inspect(locks);
                        invoke(ca, ()->inspect(locks));
                        inspect(locks);
                    });
                    inspect(locks);
                });
                inspect(locks);
                System.out.println(phases);
            }
        }
    }
