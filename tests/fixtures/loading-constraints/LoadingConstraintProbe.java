import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Reference-only JVM observations. Every scenario starts with fresh loader identities. */
public final class LoadingConstraintProbe {
    static final class Domain extends ClassLoader {
        final String identity;
        final Path classes;
        final Map<String, Domain> routes = new HashMap<>();

        Domain(String identity, Path classes) {
            super(ClassLoader.getPlatformClassLoader());
            this.identity = identity;
            this.classes = classes;
        }

        @Override
        protected Class<?> findClass(String name) throws ClassNotFoundException {
            Domain delegate = routes.get(name);
            if (delegate != null) return delegate.loadClass(name);
            try {
                byte[] bytes = Files.readAllBytes(classes.resolve(name.replace('.', '/') + ".class"));
                return defineClass(name, bytes, 0, bytes.length);
            } catch (IOException e) {
                throw new ClassNotFoundException(name, e);
            }
        }

        Class<?> load(String name, boolean initialize) throws ClassNotFoundException {
            return Class.forName(name, initialize, this);
        }

        String value(String leaf) throws ClassNotFoundException {
            try {
                return ((Domain) load("value." + leaf, false).getClassLoader()).identity;
            } catch (LinkageError e) {
                return e.getClass().getSimpleName();
            }
        }
    }

    record Stage(String name, String outcome) {}

    static Stage trigger(Domain loader, String name) throws ClassNotFoundException {
        String stage = "define";
        try {
            loader.load(name, false);
            stage = "initialize";
            loader.load(name, true);
            return new Stage(stage, "ok");
        } catch (LinkageError e) {
            return new Stage(stage, e.getClass().getSimpleName());
        }
    }

    static void row(String... columns) {
        System.out.println(String.join("\t", columns));
    }

    static void timing(Path dir) throws Exception {
        for (String family : List.of("Argument", "Return", "Array", "Primitive", "Private",
                                      "Static", "Package", "Inherit", "Declare")) {
            for (String order : List.of("both-before", "left-before", "right-before", "both-after", "same-before")) {
                var a = new Domain("A", dir);
                var b = new Domain("B", dir);
                var c = new Domain("C", dir);
                String target = "child.Sub" + family;
                Domain left = a, right = b;
                switch (family) {
                    case "Inherit" -> {
                        target = "child.Inherit";
                        b.routes.put("parent.Impl", a);
                        b.routes.put("iface.Service", c);
                        right = c;
                    }
                    case "Declare" -> {
                        target = "child.Declare";
                        b.routes.put("iface.Service", c);
                        left = c;
                    }
                    default -> b.routes.put("parent.Base" + family, a);
                }
                if (order.equals("same-before")) right.routes.put("value.K", left);
                boolean both = order.equals("same-before") || order.equals("both-before");
                String priorLeft = both || order.equals("left-before") ? left.value("K") : "-";
                String priorRight = both || order.equals("right-before") ? right.value("K") : "-";
                Stage stage = trigger(b, target);
                String afterLeft = "-", afterRight = "-", repair = "-";
                if (stage.outcome().equals("ok")) {
                    afterLeft = left.value("K");
                    afterRight = right.value("K");
                    if (afterRight.equals("LinkageError")) {
                        right.routes.put("value.K", left);
                        repair = right.value("K");
                    } else if (afterLeft.equals("LinkageError")) {
                        left.routes.put("value.K", right);
                        repair = left.value("K");
                    }
                }
                row("timing", family, order, priorLeft, priorRight, stage.name(), stage.outcome(),
                    afterLeft, afterRight, repair);
            }
        }
    }

    static void selection(Path dir) throws Exception {
        for (String[] kinds : new String[][] {
                {"Abstract", "Abstract"}, {"Default", "Abstract"}, {"Abstract", "Default"},
                {"Default", "Default"}, {"Private", "Default"}, {"Default", "Private"}}) {
            for (String order : List.of("before", "after", "same")) {
                var left = new Domain("A", dir);
                var child = new Domain("B", dir);
                var right = new Domain("C", dir);
                String target = "child." + kinds[0] + kinds[1];
                child.routes.put("iface.Left" + kinds[0], left);
                child.routes.put("iface.Right" + kinds[1], right);
                if (order.equals("same")) right.routes.put("value.K", left);
                if (!order.equals("after")) {
                    left.value("K");
                    right.value("K");
                }
                String stage = "define", outcome, after = "-";
                try {
                    child.load(target, false);
                    stage = "initialize";
                    Class<?> type = child.load(target, true);
                    after = left.value("K") + "/" + right.value("K");
                    // Later invocation resolves its signature under the selected CP owner.
                    child.routes.put("value.K", kinds[0].equals("Private") ? right : left);
                    stage = "lookup";
                    var method = type.getDeclaredMethod("run");
                    stage = "invoke";
                    outcome = "I:" + method.invoke(null);
                } catch (InvocationTargetException e) {
                    outcome = e.getCause().getClass().getSimpleName();
                } catch (LinkageError e) {
                    outcome = e.getClass().getSimpleName();
                }
                row("selection", target, order, stage, outcome, after);
            }
        }
    }

    static void packages(Path dir) throws Exception {
        for (String kind : List.of("SameSpelling", "Public", "PrivateChild", "StaticChild",
                                   "Protected", "StaticParent", "Bridge")) {
            var a = new Domain("A", dir);
            var b = new Domain("B", dir);
            boolean bridge = kind.equals("Bridge");
            b.routes.put(bridge ? "shared.Bridge" : "shared.Base" + kind, a);
            a.value("K");
            b.value("K");
            Stage stage = trigger(b, bridge ? "other.Leaf" : "shared.Child" + kind);
            row("packages", kind, stage.name(), stage.outcome());
        }
    }

    static void transitive(Path dir) throws Exception {
        for (boolean reverse : List.of(false, true)) {
            for (boolean prime : List.of(false, true)) {
                var a = new Domain("A", dir);
                var b = new Domain("B", dir);
                var c = new Domain("C", dir);
                b.routes.put("parent.First", a);
                c.routes.put("parent.Second", b);
                if (prime) {
                    a.value("K");
                    c.value("K");
                }
                Stage first = reverse ? trigger(c, "child.Second") : trigger(b, "child.First");
                Stage second = reverse ? trigger(b, "child.First") : trigger(c, "child.Second");
                b.routes.put("value.K", reverse ? c : a);
                row("transitive", "transitive", Boolean.toString(reverse), Boolean.toString(prime),
                    first.outcome(), second.outcome(), a.value("K") + "/" + b.value("K") + "/" + c.value("K"),
                    first.name(), second.name());
            }
        }
        for (String kind : List.of("PQ", "QP", "PRet", "QRet")) {
            var a = new Domain("A", dir);
            var b = new Domain("B", dir);
            b.routes.put("parent." + kind, a);
            a.value("Q");
            b.value("Q");
            Stage failure = trigger(b, "child." + kind);
            String prior = a.value("P") + "/" + b.value("P");
            b.routes.put("value.P", a);
            row("transitive", kind, failure.outcome(), prior, b.value("P"), failure.name());
        }
    }

    static void additional(Path dir) throws Exception {
        var a = new Domain("A", dir);
        var b = new Domain("B", dir);
        var c = new Domain("C", dir);
        b.routes.put("parent.Impl", a);
        b.routes.put("iface.Service", c);
        c.routes.put("value.K", a);
        String owners = a.value("K") + "/" + b.value("K") + "/" + c.value("K");
        Stage inherited = trigger(b, "child.Inherit");
        row("inherited", "distinct-child", owners, inherited.name(), inherited.outcome());

        for (String kind : List.of("Order00", "Order01", "Order10", "Order11", "Order20", "Order21")) {
            var parent = new Domain("A", dir);
            var child = new Domain("B", dir);
            child.routes.put("parent." + kind, parent);
            if (!parent.value("Q").equals("A") || !child.value("Q").equals("B")) throw new AssertionError();
            Stage failure = trigger(child, "child." + kind);
            row("method-order", kind, failure.name(), failure.outcome(), parent.value("P"), child.value("P"));
        }
    }

    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        timing(dir);
        selection(dir);
        transitive(dir);
        packages(dir);
        additional(dir);
    }
}
