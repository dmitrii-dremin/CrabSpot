package named;
public class Broken extends Parent {
  static {
    Trace.mark(4);
    fail();
  }
  static void fail() {
    throw null;
  }
}
