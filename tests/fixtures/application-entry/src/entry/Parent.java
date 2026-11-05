package entry;
public class Parent {
  static {
    Mark.add(1);
  }
  public static void main(String[] a) {
    System.out.println("Parent:" + Mark.n + ":" + a.length);
  }
}
