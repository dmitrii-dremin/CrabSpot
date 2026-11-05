package named;
public class Parent {
  static {
    Trace.mark(1);
  }
  public static void main(String[] a) {
    Trace.body = Trace.marks;
    if (Trace.body != 12 || a.length != 0)
      throw null;
  }
}
