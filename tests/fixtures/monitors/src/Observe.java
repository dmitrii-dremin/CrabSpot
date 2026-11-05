import locks.Marks;

public class Observe {

    public static void main(String[] args) throws Exception {
        Class<?> entry = Class.forName("locks." + args[0]);
        String outcome = "returned";
        String[] a = new String[0];
        try {
            entry.getMethod("main", String[].class).invoke(null, (Object)a);
        } catch (java.lang.reflect.InvocationTargetException x) {
            outcome = x.getCause().getClass().getSimpleName();
        }
        String bits = "";
        for (Object m : new Object[]{entry, a, Marks.object, Marks.array, Marks.worker, locks.Base.class}) bits += (m != null && Thread.holdsLock(m)) ? "1" : "0";
        System.out.println(outcome + " " + Marks.entries + "/" + Marks.calls + "/" + Marks.completed + "/" + Marks.sum + " " + bits);
    }
}
