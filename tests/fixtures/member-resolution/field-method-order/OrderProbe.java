import java.lang.reflect.InvocationTargetException;
import java.nio.file.Path;
import java.util.List;
public class OrderProbe {
    static String trigger(Class<?> target, String name) throws Exception {
        try {
            return "I:" + target.getDeclaredMethod(name).invoke(null);
        } catch (InvocationTargetException e) {
            return e.getCause().getClass().getSimpleName();
        } catch (LinkageError e) {
            return e.getClass().getSimpleName();
        }
    }
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        for (String name : List.of("getStatic", "getStaticWrong", "getInstance", "getInstanceWrong",
                 "privateField", "finalWrite", "missingField", "callStatic", "callStaticWrong",
                 "callInstance", "callInstanceWrong", "privateCall", "missingCall")) {
            for (String timing : List.of("split", "late", "same")) {
                var a = new LoadingConstraintProbe.Domain("A", dir);
                var b = new LoadingConstraintProbe.Domain("B", dir);
                var c = new LoadingConstraintProbe.Domain("C", dir);
                b.routes.put("owner.Base", a);
                c.routes.put("sym.Sub", b);
                if (timing.equals("same"))
                    c.routes.put("value.K", a);
                if (!timing.equals("late")) {
                    a.value("K");
                    c.value("K");
                }
                Class<?> target = c.load("call.Steps", true);
                String first = trigger(target, name);
                String aa = a.value("K"), cc = c.value("K"), bb = b.value("K");
                String second = trigger(target, name);
                String next = switch (name) {
                    case "getStaticWrong" -> "getStatic";
                    case "getInstanceWrong" -> "getInstance";
                    case "callStaticWrong" -> "callStatic";
                    case "callInstanceWrong" -> "callInstance";
                    default -> null;
                };
                System.out.println(String.join("\t", name, timing, first, aa + "/" + cc + "/" + bb,
                    second, next == null ? "-" : trigger(target, next)));
            }
        }
    }
}
