package app;
public class Invalid {
  static {
    Marks.initialized++;
  }
  public static void main(String[] args) {
    Marks.entered++;
  }
  static int defect() {
    return 69;
  }
}
