package base;
import value.K;
public class Owner implements api.Port {
    public K slot;
    public static int calls;
    public int hit(K k) { slot=k; calls++; return 3; }
}
