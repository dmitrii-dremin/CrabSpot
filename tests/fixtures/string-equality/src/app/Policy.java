package app;

public class Policy {
    public static int effects;
    public static int status;
    public static void main(String[] ignored) {
        status = Policy.class.desiredAssertionStatus() ? 1 : 0;
        assert ++effects > 0;
    }
}
