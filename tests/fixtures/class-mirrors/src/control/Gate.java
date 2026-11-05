import java.lang.reflect.*;
import java.nio.file.*;
public class Gate extends ClassLoader {
    final Path path;
    boolean available;
    Gate(Path p) { super(null); path=p; }
    protected Class<?> findClass(String name) throws ClassNotFoundException {
        if (name.equals("late.Peer") && !available) throw new ClassNotFoundException(name);
        try {
            byte[] data=Files.readAllBytes(path.resolve(name.replace('.','/')+".class"));
            return defineClass(name,data,0,data.length);
        } catch (java.io.IOException e) { throw new ClassNotFoundException(name,e); }
    }
    static Object invoke(Method entry) throws Exception {
        try { return entry.invoke(null); }
        catch (InvocationTargetException failure) { return failure.getCause().getClass().getSimpleName(); }
    }
    public static void main(String[] args) throws Exception {
        Gate gate=new Gate(Path.of(args[0]));
        Method original=gate.loadClass("late.Original").getMethod("run");
        Object first=invoke(original);
        gate.available=true;
        Class<?> peer=gate.loadClass("late.Peer");
        Object same=invoke(original);
        Object fresh=invoke(gate.loadClass("late.Fresh").getMethod("run"));
        System.out.print(first+"/"+same+"/"+(fresh==peer)+"/"+(peer.getClassLoader()==gate));
    }
}
