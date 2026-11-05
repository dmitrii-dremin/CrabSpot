package locks;

public class Entry {

    public static synchronized void main(String[] args) {
        Marks.entries++;
        Worker w = new Worker();
        int[] a = new int[1];
        Object l = new Object();
        Marks.object = l;
        Marks.array = a;
        Marks.worker = w;
        synchronized (args) {
            synchronized (l) {
                Marks.sum += Child.call(w, a, l);
            }
        }
        Marks.sum += Base.call(w, a, l);
        Marks.completed++;
    }
}
