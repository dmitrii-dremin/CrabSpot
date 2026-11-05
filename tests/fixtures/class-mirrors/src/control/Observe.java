import mirrors.*;
public class Observe {
    public static void main(String[] args) {
        Entry.main(args);
        System.out.print(Marks.flags+"/"+Marks.initializers+"/"+Marks.called+" "+
            Thread.holdsLock(Base.class)+"/"+Thread.holdsLock(Child.class));
    }
}
