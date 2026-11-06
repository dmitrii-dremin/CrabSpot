package app;

public class BootstrapStatus {
    public static int status, phase;
    public static void main(String[] ignored) {
        phase = 1;
        if (String.class.desiredAssertionStatus()) status |= 1;
        if (Object.class.desiredAssertionStatus()) status |= 2;
        phase = 2;
    }
}
