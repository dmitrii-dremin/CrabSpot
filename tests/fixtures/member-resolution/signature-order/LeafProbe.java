import java.lang.reflect.InvocationTargetException;
import java.nio.file.Path;
import java.util.List;
public class LeafProbe {
    static String invoke(Class<?> caller, String name) throws Exception {
        try {
            return "I:" + caller.getDeclaredMethod(name).invoke(null);
        } catch (InvocationTargetException e) {
            return e.getCause().getClass().getSimpleName();
        }
    }
    public static void main(String[] args) throws Exception {
        Path path = Path.of(args[0]);
        for (String kind : List.of("PQ", "QP", "PRet", "QRet", "Array")) {
            for (String mode : List.of("split-Q", "split-P", "both-absent", "shared")) {
                var a = new LoadingConstraintProbe.Domain("A", path);
                var b = new LoadingConstraintProbe.Domain("B", path);
                var c = new LoadingConstraintProbe.Domain("C", path);
                b.routes.put("owner.Base", a);
                c.routes.put("sym.Sub", b);
                if (mode.startsWith("split-")) {
                    String split = mode.substring(6);
                    a.value(split);
                    c.value(split);
                }
                if (mode.equals("shared")) {
                    for (String value : List.of("P", "Q")) {
                        c.routes.put("value." + value, a);
                        a.value(value);
                        c.value(value);
                    }
                }
                var caller = c.load("call.Steps", true);
                String first = invoke(caller, kind);
                String pa = a.value("P"), pc = c.value("P"), qa = a.value("Q"), qc = c.value("Q");
                String second = invoke(caller, kind);
                System.out.println(String.join("\t", kind, mode, first, pa + "/" + pc, qa + "/" + qc,
                    second, b.value("P") + "/" + b.value("Q")));
            }
        }
    }
}
