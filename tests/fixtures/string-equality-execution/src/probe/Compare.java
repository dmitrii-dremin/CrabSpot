package probe;

public class Compare {
    public static int phase, same;
    public static void main(String[] args) {
        phase = 1;
        Object left = args[0];
        Object right = args.length == 1 ? left : args[1];
        same = left.equals(right) ? 1 : 0;
        phase = 2;
    }
}
