package sample;

public class Entry {
    public static int phase;

    public static void main(String[] args) {
        phase = 1;
        String value = Value.value();
        if (value == value.intern()) phase = 2;
        if (args.length == 1 && args[0].intern() == value) phase = 3;
    }
}
