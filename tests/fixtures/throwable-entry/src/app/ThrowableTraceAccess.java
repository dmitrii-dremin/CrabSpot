package app;
import probe.ThrowableEntryGuest;
public final class ThrowableTraceAccess {
    public static int before, after;
    public static Throwable retained;
    public static void main(String[] args) {
        retained = ThrowableEntryGuest.makeUser();
        before = 7;
        after = retained.getStackTrace().length;
    }
}
