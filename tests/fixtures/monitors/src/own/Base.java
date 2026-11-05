package own;

public class Base {

    public static synchronized void visit(Runnable query) {
        query.run();
    }
}
