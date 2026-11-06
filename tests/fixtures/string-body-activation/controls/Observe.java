import budget.Entry;

public class Observe {
    public static void main(String[] args) {
        char[] units = switch (args[0]) {
            case "empty" -> new char[0];
            case "latin" -> new char[] {65, 0, 255};
            case "pair" -> new char[] {0xd83d, 0xde80};
            default -> throw new IllegalArgumentException(args[0]);
        };
        Entry.main(new String[] {new String(units)});
        System.out.println(Entry.phase + "/" + Entry.units + "/" + Entry.empty + "/" +
            Entry.stable + "/" + Entry.comparator.getClass().getName());
    }
}
