public class Control {
    static int n;

    static class Base {

        static synchronized void method() {
            if (Thread.holdsLock(Base.class)) n += 1;
            if (!Thread.holdsLock(Child.class)) n += 2;
            synchronized (Base.class) {
                if (Thread.holdsLock(Base.class)) n += 4;
            }
        }

        synchronized void first() {
            if (Thread.holdsLock(this)) n += 8;
            again();
        }

        synchronized void again() {
            if (Thread.holdsLock(this)) n += 16;
        }
    }

    static class Child extends Base {
    }

    public static synchronized void main(String[] args) {
        if (Thread.holdsLock(Control.class)) n += 32;
        Child c = new Child();
        Child.method();
        c.first();
        if (!Thread.holdsLock(c) && !Thread.holdsLock(Base.class)) n += 64;
        System.out.println(n);
    }
}
