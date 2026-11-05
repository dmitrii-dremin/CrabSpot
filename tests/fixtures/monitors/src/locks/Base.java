package locks;

public class Base {

    public static synchronized int call(Job j, int[] a, Object l) {
        return j.step(a, l);
    }
}
