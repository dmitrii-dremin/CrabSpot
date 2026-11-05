package locks;

public class Worker implements Job {

    public synchronized int step(int[] a, Object lock) {
        synchronized (this) {
            Marks.calls++;
            synchronized (a) {
                synchronized (lock) {
                    return inner(a, lock);
                }
            }
        }
    }

    private synchronized int inner(int[] a, Object lock) {
        synchronized (lock) {
            for (int i = 0; i < 3; i++) {
                synchronized (a) {
                    if (i == 2) break;
                    a[0] += i + 1;
                }
            }
        }
        return a[0];
    }
}
