package entry;
public class Hidden extends Child {
  static {
    Mark.add(3);
  }
  public static void main(String[] a) {
    System.out.println("Hidden:" + Mark.n + ":" + a.length);
  }
}
