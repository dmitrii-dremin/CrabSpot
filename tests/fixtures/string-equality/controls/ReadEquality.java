import app.TextEquality;

public class ReadEquality {
    public static void main(String[] ignored) {
        char[][] u = {{'A',0,255},{'A',0,255},{'A',0,254},{'A',0x20ac,0xdfff},
                      {'A',0x20ac,0xdfff},{'A',0x20ac,0xdffe},{},{},{'A',0x20ac}};
        String[] a = new String[u.length];
        for (int i=0; i<a.length; i++) a[i] = new String(u[i]);
        TextEquality.main(a);
        System.out.println(TextEquality.flags + "/" + TextEquality.calls);
    }
}
