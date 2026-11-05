import java.lang.reflect.InvocationTargetException;
import java.nio.file.Path;
import java.util.List;

/** Keep observable JDK stage outcomes separate from the required preparation relation. */
public final class LoadingSelectionEdges {
    static LoadingConstraintProbe.Stage invoke(String kind, Path dir, LoadingConstraintProbe.Domain child,
                                                LoadingConstraintProbe.Domain service) throws Exception {
        String stage = "reflect";
        try {
            Class<?> root = child.load("child." + kind, false);
            root.getMethods();
            root.getDeclaredMethods();
            Class<?> owner = service.load("iface.Service", false);
            var method = owner.getDeclaredMethod("take", service.load("value.K", false));
            Class<?> concrete = root;
            if (root.isInterface()) {
                var helper = new LoadingConstraintProbe.Domain("D", dir);
                helper.routes.put("child." + kind, child);
                stage = "helper-init";
                concrete = helper.load("helper." + kind, true);
            }
            stage = "construct";
            Object receiver = concrete.getDeclaredConstructor().newInstance();
            stage = "invoke";
            method.invoke(receiver, new Object[]{null});
            return new LoadingConstraintProbe.Stage(stage, "ok");
        } catch (InvocationTargetException e) {
            return new LoadingConstraintProbe.Stage(stage, e.getCause().getClass().getSimpleName());
        } catch (LinkageError e) {
            return new LoadingConstraintProbe.Stage(stage, e.getClass().getSimpleName());
        }
    }

    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        boolean withInvocation = args[1].equals("invoke");
        for (String kind : List.of("Abs", "Prot", "Package", "Private", "Static", "Bridge",
                                   "Suppress", "RootInterface", "DeclaredInterface")) {
            for (String mode : withInvocation ? List.of("split", "shared") : List.of("split", "late", "shared")) {
                var a = new LoadingConstraintProbe.Domain("A", dir);
                var b = new LoadingConstraintProbe.Domain("B", dir);
                var c = new LoadingConstraintProbe.Domain("C", dir);
                var left = a;
                b.routes.put("iface.Service", c);
                switch (kind) {
                    case "Suppress" -> b.routes.put("iface.Reabstract", a);
                    case "RootInterface" -> b.routes.put("iface.Default", a);
                    case "DeclaredInterface" -> left = b;
                    default -> b.routes.put("parent." + kind, a);
                }
                if (mode.equals("shared")) c.routes.put("value.K", left);
                if (!mode.equals("late")) {
                    left.value("K");
                    c.value("K");
                }
                var first = LoadingConstraintProbe.trigger(b, "child." + kind);
                if (withInvocation) {
                    var last = first.outcome().equals("ok") ? invoke(kind, dir, b, c) : first;
                    LoadingConstraintProbe.row(kind, mode, first.name(), first.outcome(), last.name(), last.outcome(),
                                               left.value("K"), c.value("K"));
                } else {
                    LoadingConstraintProbe.row(kind, mode, first.name(), first.outcome(), left.value("K"), c.value("K"));
                }
            }
        }
    }
}
