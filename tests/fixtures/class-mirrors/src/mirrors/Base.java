package mirrors;
public class Base {
    static { Marks.initializers++; }
    public static synchronized void run() {
        synchronized(Base.class) { Marks.called++; }
    }
}
