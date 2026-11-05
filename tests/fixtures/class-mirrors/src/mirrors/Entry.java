package mirrors;
public class Entry {
    public static void main(String[] args) {
        Class<?> c = Child.class;
        Class<?> unused = Never.class;
        if (c != unused && c != Base.class && Marks.initializers == 0) Marks.flags |= 1;
        if (new Child[1].getClass() == Child[].class && Marks.initializers == 0) Marks.flags |= 2;
        if (Class.class.getClass() == Class.class && c.getClass() == Class.class) Marks.flags |= 4;
        if (int[][].class == new int[1][1].getClass() && int[].class != (Object)int[][].class) Marks.flags |= 8;
        if (args.getClass() == String[].class) Marks.flags |= 16;
        Object child = new Child();
        if (child.getClass() == c && Marks.initializers == 3) Marks.flags |= 32;
        synchronized(Base.class) { Child.run(); }
        if (Marks.called == 1 && Marks.initializers == 3) Marks.flags |= 64;
    }
}
