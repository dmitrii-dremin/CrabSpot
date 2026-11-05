package words;
public class Entry {
    public static int flags, hash, units;
    public static Object saved;
    public static void main(String[] args) {
        String a = "A\000\u00ff\u20ac\ud800\udc00\udfff";
        String b = Peer.word();
        if (a == b && a != "other") flags |= 1;
        Object[] values = new String[1];
        values[0] = a;
        saved = values[0];
        if (saved == b && saved.getClass() == String.class) flags |= 2;
        if (a.length() == 7 && a.charAt(1) == 0 && a.charAt(6) == 0xdfff) flags |= 4;
        hash = a.hashCode();
        for (int i = 0; i < a.length(); i++) units = units * 31 + a.charAt(i);
        if (hash == units && a.equals(b)) flags |= 8;
        if (a.intern() == a && "".length() == 0 && "\u00e9" != "e\u0301") flags |= 16;
        synchronized (saved) { synchronized (b) { flags |= 32; } }
    }
}
