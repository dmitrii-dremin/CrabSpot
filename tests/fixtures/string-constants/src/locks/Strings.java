package locks;

public class Strings {
    public static int phase;

    public static void main(String[] args) {
        String left = args[0];
        String right = args[1];
        if (args.length == 3) {
            left = left.intern();
            right = right.intern();
        }
        phase = 1;
        synchronized (left) {
            phase = 2;
            synchronized (right) { phase = 3; }
        }
        phase = 4;
    }
}
