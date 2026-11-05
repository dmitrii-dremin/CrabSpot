package fixtures;

import java.lang.reflect.InvocationTargetException;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;

/** Reference JDK only. A fresh loader isolates each definition and lookup. */
public final class ClassProofCases {
    public static void main(String[] args) throws Exception {
        URL directory = Path.of(args[0]).toUri().toURL();
        for (int i = 1; i < args.length; i++) {
            String name = args[i];
            String stage = "define";
            String outcome;
            try (var loader = new URLClassLoader(new URL[] {directory}, ClassLoader.getPlatformClassLoader())) {
                Class<?> type = Class.forName("classprobe." + name, false, loader);
                stage = "methods";
                var methods = type.getDeclaredMethods();
                outcome = Integer.toString(methods.length);
                for (var method : methods) {
                    if (method.getName().equals("run")) {
                        stage = "invoke";
                        outcome = "I:" + method.invoke(null);
                    }
                }
            } catch (InvocationTargetException error) {
                outcome = error.getCause().getClass().getSimpleName();
            } catch (LinkageError error) {
                outcome = error.getClass().getSimpleName();
            }
            System.out.println(name + "\t" + stage + "\t" + outcome);
        }
    }
}
