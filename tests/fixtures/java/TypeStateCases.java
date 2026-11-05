package fixtures;

import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.nio.file.Files;
import java.nio.file.Path;

/** JDK reference only. Record the first failing stage in a fresh loader for each graph. */
public final class TypeStateCases {
    private static final class GraphLoader extends ClassLoader {
        private final Path directory;

        GraphLoader(Path directory) {
            super(ClassLoader.getPlatformClassLoader());
            this.directory = directory;
        }

        @Override
        protected Class<?> findClass(String name) throws ClassNotFoundException {
            Path path = directory.resolve(name.replace('.', '/') + ".class").normalize();
            if (!path.startsWith(directory)) throw new ClassNotFoundException(name);
            try {
                byte[] bytes = Files.readAllBytes(path);
                return defineClass(name, bytes, 0, bytes.length);
            } catch (IOException error) {
                throw new ClassNotFoundException(name, error);
            }
        }
    }

    public static void main(String[] args) throws Exception {
        Path directory = Path.of(args[0]).toAbsolutePath().normalize();
        for (int index = 1; index < args.length; index++) {
            String name = args[index];
            String stage = "define";
            String outcome;
            try {
                Class<?> type = new GraphLoader(directory).findClass(name.replace('/', '.'));
                stage = "lookup";
                var method = type.getDeclaredMethod("run");
                stage = "invoke";
                Object value = method.invoke(null);
                if (method.getReturnType() == void.class) outcome = "void";
                else if (value == null) outcome = "null";
                else if (value instanceof Integer) outcome = "I:" + value;
                else throw new IllegalStateException("unexpected result type " + value.getClass());
            } catch (InvocationTargetException error) {
                outcome = error.getCause().getClass().getSimpleName();
            } catch (LinkageError error) {
                outcome = error.getClass().getSimpleName();
            }
            System.out.println(name + "\t" + stage + "\t" + outcome);
        }
    }
}
