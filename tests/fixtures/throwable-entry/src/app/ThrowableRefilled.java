package app;
import probe.ThrowableEntryGuest;
public final class ThrowableRefilled {
    public static void main(String[] args) {
        RuntimeException user = ThrowableEntryGuest.makeUser();
        ThrowableEntryGuest.refill(user);
        throw user;
    }
}
