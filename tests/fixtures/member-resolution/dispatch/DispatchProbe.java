import java.lang.reflect.InvocationTargetException;
import java.nio.file.Path;
import java.util.List;
public class DispatchProbe {
    static String call(Class<?> caller, int n, int port) throws Exception {
        try {
            return "I:" + caller.getMethod("run", int.class, int.class).invoke(null, n, port);
        } catch (InvocationTargetException e) {
            return e.getCause().getClass().getSimpleName();
        } catch (LinkageError e) {
            return e.getClass().getSimpleName();
        }
    }
    public static void main(String[] args) throws Exception {
        Path path = Path.of(args[0]);
        for (String mode : List.of("common", "caller-split", "override-split", "plain-split")) {
            for (int port = 0; port <= 1; port++) {
                var a = new LoadingConstraintProbe.Domain("A", path);
                var b = new LoadingConstraintProbe.Domain("B", path);
                var c = new LoadingConstraintProbe.Domain("C", path);
                var d = new LoadingConstraintProbe.Domain("D", path);
                var e = new LoadingConstraintProbe.Domain("E", path);
                b.routes.put("base.Owner", a);
                for (var loader : List.of(b, c, d, e)) loader.routes.put("api.Port", a);
                for (var loader : List.of(c, d, e)) loader.routes.put("symbolic.Sub", b);
                c.routes.put("base.Owner", a);
                c.routes.put("dyn.First", d);
                c.routes.put("dyn.Second", d);
                c.routes.put("dyn.Plain", e);
                if (!mode.equals("caller-split"))
                    c.routes.put("value.K", a);
                if (!mode.equals("override-split"))
                    d.routes.put("value.K", a);
                if (!mode.equals("plain-split"))
                    e.routes.put("value.K", a);
                String owners = a.value("K") + "/" + b.value("K") + "/" + c.value("K") + "/"
                    + d.value("K") + "/" + e.value("K");
                Class<?> caller;
                try {
                    caller = c.load("caller.Dispatch", true);
                } catch (LinkageError failure) {
                    for (int receiver : new int[] {1, 2, 3, 1})
                        System.out.println(String.join("\t", mode, "" + port, "" + receiver, owners,
                            "caller-load", failure.getClass().getSimpleName(), "-", "-", "-"));
                    continue;
                }
                for (int receiver : new int[] {1, 2, 3, 1}) {
                    String result = call(caller, receiver, port);
                    var held = caller.getField("held").get(null);
                    String actual = held == null ? "null" : held.getClass().getSimpleName();
                    Class<?> owner = a.load("base.Owner", true);
                    Object value = held == null ? null : owner.getField("slot").get(held);
                    String token = value == null
                        ? "null"
                        : ((LoadingConstraintProbe.Domain) value.getClass().getClassLoader()).identity;
                    String counts = caller.getField("entered").getInt(null) + "/"
                        + caller.getField("returned").getInt(null) + "/"
                        + owner.getField("calls").getInt(null);
                    System.out.println(String.join("\t", mode, "" + port, "" + receiver, owners,
                        "invoke", result, actual, token, counts));
                }
            }
        }
    }
}
