package constants;

public class Entry extends Parent {
    public static final int NUMBER = 41;
    public static final String FIRST = "alpha";
    public static final String SECOND = "beta";
    public static int entered, completed;

    static {
        entered++;
        if (Read.first() == FIRST && Read.second() == SECOND) Marks.order |= 2;
    }

    public static void main(String[] args) {
        if (Marks.order == 3) completed = NUMBER;
    }
}
