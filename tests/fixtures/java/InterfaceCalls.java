package fixtures;

public interface InterfaceCalls {
    private static int twice(int value) { return value * 2; }
    static int chain(int value) { return 7 + twice(value); }
    static int recursive(int value) { return value <= 0 ? 0 : 1 + recursive(value - 1); }
    static void main(String[] ignored) {
        for (int value : new int[] {Integer.MIN_VALUE, -1, 0, 1, 7, Integer.MAX_VALUE})
            System.out.println("chain\t(I)I\tI:" + value + "\tI:" + chain(value));
        for (int value : new int[] {0, 1, 2, 7, 20})
            System.out.println("recursive\t(I)I\tI:" + value + "\tI:" + recursive(value));
    }
}
