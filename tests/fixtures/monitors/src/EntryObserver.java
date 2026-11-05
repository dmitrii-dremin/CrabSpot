import java.lang.reflect.*;
public class EntryObserver {
    public static void main(String[] ignored) throws Exception {
        Class<?> entry = Class.forName("locks.RawEntry");
        String[] args = new String[0];
        String result = "returned";
        try { entry.getMethod("main", String[].class).invoke(null, (Object)args); }
        catch (InvocationTargetException x) { result = x.getCause().getClass().getSimpleName(); }
        Object other = entry.getField("other").get(null);
        System.out.print(result + " " + entry.getField("phase").getInt(null)
                         + " " + Thread.holdsLock(args) + "/" + Thread.holdsLock(other));
    }
}
