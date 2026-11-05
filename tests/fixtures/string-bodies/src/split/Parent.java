package split;
public class Parent {
    private static int secret = 41;
    public static class Child {
        public static int read() { return secret; }
    }
}
