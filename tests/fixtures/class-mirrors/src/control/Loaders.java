import java.net.*;
import java.nio.file.*;
public class Loaders {
    public static void main(String[] args) throws Exception {
        var urls = new URL[]{Path.of(args[0]).toUri().toURL()};
        try (var a = new URLClassLoader(urls,null); var b = new URLClassLoader(urls,null)) {
            Class<?> ca=a.loadClass("mirrors.Child"), cb=b.loadClass("mirrors.Child");
            Class<?> aa=java.lang.reflect.Array.newInstance(ca,0).getClass();
            Class<?> ab=java.lang.reflect.Array.newInstance(cb,0).getClass();
            System.out.print((ca != cb)+"/"+(ca.getClass()==cb.getClass())+"/"+(aa != ab)+"/"+
                (aa.getComponentType()==ca)+"/"+(ab.getComponentType()==cb));
        }
    }
}
