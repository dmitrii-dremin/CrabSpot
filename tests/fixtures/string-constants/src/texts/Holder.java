package texts;
public class Holder {
    public static final String VALUE = "A\000\u20ac\udfff";
    static { State.initialized++; State.early=Read.value(); }
    public static void touch() {}
}
