package named;
public class Trace {
  public static int marks, body;
  static void mark(int x) {
    marks = 10 * marks + x;
  }
}
