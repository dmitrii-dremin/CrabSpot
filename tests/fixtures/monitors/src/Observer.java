import java.lang.reflect.*;

public class Observer {

    public static void main(String[] args) throws Exception {
        String stage = "load";
        String result = "";
        Class<?> c = null;
        Object a = null;
        Object b = new int[1];
        RuntimeException supplied = new RuntimeException("sentinel");
        try {
            c = Class.forName("locks.Raw", false, Observer.class.getClassLoader());
            stage = "lookup";
            Method run = c.getMethod("run", Object.class, Object.class, RuntimeException.class);
            a = args[0].startsWith("sync-") ? c : new Object();
            stage = "invoke";
            result = "I:" + run.invoke(null, a, b, supplied);
        } catch (Throwable t) {
            Throwable cause = t instanceof InvocationTargetException ? t.getCause() : t;
            result = cause == supplied ? "supplied" : cause.getClass().getSimpleName();
        }
        String phase = stage.equals("invoke") ? "" + c.getField("phase").getInt(null) : "-";
        System.out.println(stage + " " + result + " " + phase + " " + (a != null && Thread.holdsLock(a)) + " " + Thread.holdsLock(b));
    }
}
