package app;
import probe.ThrowableEntryGuest;
public final class ThrowableUncaught {
    public static int before, after;
    public static void main(String[] args) {
        before = 7;
        ThrowableEntryGuest.idiv(1, 0);
        after = 9;
    }
}
