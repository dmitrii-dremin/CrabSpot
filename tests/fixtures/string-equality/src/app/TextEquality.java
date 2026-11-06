package app;

public class TextEquality {
    public static int flags;
    public static int calls;
    static boolean equal(Object a, Object b) {
        calls++;
        return a.equals(b);
    }
    public static void main(String[] args) {
        if (equal(args[0], args[0])) flags |= 1;
        if (!equal(args[0], null) && !equal(args[0], new Object())) flags |= 2;
        if (args[0] != args[1] && equal(args[0], args[1])) flags |= 4;
        if (!equal(args[0], args[2])) flags |= 8;
        if (args[3] != args[4] && equal(args[3], args[4])) flags |= 16;
        if (!equal(args[3], args[5])) flags |= 32;
        if (!equal(args[0], args[3])) flags |= 64;
        if (args[6] != args[7] && equal(args[6], args[7])) flags |= 128;
        if (!equal(args[3], args[8])) flags |= 256;
    }
}
