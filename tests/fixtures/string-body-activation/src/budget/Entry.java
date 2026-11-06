package budget;
public class Entry {
    public static int phase, units, empty, stable;
    public static Object comparator;
    public static void main(String[] args) {
        phase = 1;
        if (args[0].isEmpty()) empty = 1;
        phase = 2;
        units = args[0].length();
        phase = 3;
        comparator = String.CASE_INSENSITIVE_ORDER;
        if (comparator != null && comparator == String.CASE_INSENSITIVE_ORDER) stable = 1;
        phase = 4;
    }
}
