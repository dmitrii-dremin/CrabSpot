package peer;
public class Host {
    public static int phase;
    private static int secret = 7;
    public static class Inner {
        private Inner() { phase = secret; }
        private int privateValue() { return secret; }
        public int own() { return privateValue(); }
    }
    public static int run() { Inner i = new Inner(); return i.privateValue(); }
    public static void main(String[] args) { phase = 1; phase = run(); }
}
