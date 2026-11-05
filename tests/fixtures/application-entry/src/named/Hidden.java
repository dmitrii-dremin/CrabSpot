package named;
public class Hidden extends Child {
  static {
    Trace.mark(3);
  }
  public static void main(String[] a) {
    Trace.body = Trace.marks;
    if (Trace.body != 123)
      throw null;
  }
}
