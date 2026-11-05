package app;
public class Entry {
  public static void main(String[] args) {
    Marks.entered++;
    Marks.slots = args.length;
    if (args.length != 0)
      throw null;
    Marks.total += Helper.call(new First(), 3) + Helper.call(new Second(), 4);
    Marks.initialized = Helper.init;
    Marks.completed++;
  }
}
