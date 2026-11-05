package locks;

public interface Job {

    int step(int[] array, Object lock);
}
