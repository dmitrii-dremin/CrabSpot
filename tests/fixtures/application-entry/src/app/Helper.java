package app;
final class Helper {
  static int init;
  static {
    init++;
  }
  static int call(Calc c, int x) {
    return c.apply(x);
  }
}
