package app;
public class Abrupt {
  public static void main(String[] args) {
    Marks.entered++;
    throw null;
  }
}
