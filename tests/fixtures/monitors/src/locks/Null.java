package locks;

public class Null {

    public static void main(String[] a) {
        Marks.entries++;
        synchronized ((Object)null) {
            Marks.calls++;
        }
        Marks.completed++;
    }
}
