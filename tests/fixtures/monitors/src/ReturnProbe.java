import java.lang.reflect.*;

public class ReturnProbe {

    public static void main(String[] args) throws Exception {
        Object a = new Object();
        RuntimeException supplied = new RuntimeException();
        Class<?> c = Class.forName("locks.Returns");
        String result;
        try {
            Object v = c.getMethod("run", Object.class, RuntimeException.class).invoke(null, a, supplied);
            result = v == a ? "same-reference" : String.valueOf(v);
        } catch (InvocationTargetException e) {
            Throwable t = e.getCause();
            result = t == supplied ? "same-throwable" : t.getClass().getSimpleName();
        }
        System.out.print(result + " " + c.getField("heldInside").getInt(null) + " " + Thread.holdsLock(a));
    }
}
