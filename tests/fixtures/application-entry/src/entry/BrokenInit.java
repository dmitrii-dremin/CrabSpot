package entry;
public class BrokenInit extends Parent {
  static {
    Mark.add(5);
    fail();
  }
  static void fail() {
    throw null;
  }
}
