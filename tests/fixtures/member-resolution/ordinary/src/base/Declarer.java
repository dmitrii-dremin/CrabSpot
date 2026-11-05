package base;
import value.K;
public class Declarer {
    public static K slot;
    public static int calls;
    public static K echo(K input) { calls++; return input; }
}
