import java.lang.reflect.InvocationTargetException;
import java.nio.file.Path;
import java.util.List;
public class Resolution {
    static String invoke(Class<?> type, String name) throws Exception {
        try {
            return "I:" + type.getDeclaredMethod(name).invoke(null);
        } catch (InvocationTargetException e) {
            return e.getCause().getClass().getSimpleName();
        } catch (LinkageError e) {
            return e.getClass().getSimpleName();
        }
    }
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        for (String method : List.of("read", "write", "call", "combined")) {
            for (String order :
                List.of("split", "same", "symbolic-only", "late", "left-only", "right-only")) {
                var a = new LoadingConstraintProbe.Domain("A", dir);
                var b = new LoadingConstraintProbe.Domain("B", dir);
                var c = new LoadingConstraintProbe.Domain("C", dir);
                b.routes.put("base.Declarer", a);
                c.routes.put("base.Declarer", a);
                c.routes.put("symbolic.Sub", b);
                if (order.equals("same") || order.equals("symbolic-only"))
                    c.routes.put("value.K", a);
                if (order.equals("split") || order.equals("same") || order.equals("left-only")
                    || order.equals("symbolic-only"))
                    a.value("K");
                if (order.equals("split") || order.equals("same") || order.equals("right-only")
                    || order.equals("symbolic-only"))
                    c.value("K");
                if (order.equals("symbolic-only"))
                    b.value("K");
                Class<?> type = c.load("caller.Probe", true);
                String first = invoke(type, method);
                String aa = a.value("K"), cc = c.value("K"), bb = b.value("K");
                String repair = "-";
                if (aa.equals("LinkageError")) {
                    a.routes.put("value.K", c);
                    repair = a.value("K");
                }
                if (cc.equals("LinkageError")) {
                    c.routes.put("value.K", a);
                    repair = c.value("K");
                }
                String second = invoke(type, method);
                int entered = type.getDeclaredField("entered").getInt(null);
                int returned = type.getDeclaredField("returned").getInt(null);
                Class<?> base = a.load("base.Declarer", true);
                int calls = base.getDeclaredField("calls").getInt(null);
                Object value = base.getDeclaredField("slot").get(null);
                System.out.println(String.join("\t", method, order, first, aa + "/" + cc + "/" + bb,
                    repair, second, entered + "/" + returned + "/" + calls,
                    value == null
                        ? "null"
                        : ((LoadingConstraintProbe.Domain) value.getClass().getClassLoader()).identity));
            }
        }
    }
}
