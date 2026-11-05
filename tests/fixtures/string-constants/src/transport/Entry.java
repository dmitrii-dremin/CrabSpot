package transport;

public class Entry {
    public static int count, flags, aliases;

    public static void main(String[] args) {
        count = args.length;
        if (count < 8) return;
        if (args[0] == args[1]) aliases |= 1;
        if (args[2] == args[3]) aliases |= 2;
        if (args[0].intern() == "repeat" && args[1].intern() == "repeat") flags |= 1;
        if (args[2].intern() == "" && args[3].intern() == "") flags |= 2;
        if (args[4].intern() == "--") flags |= 4;
        if (args[5].intern() == "--platform") flags |= 8;
        if (args[6].intern() == "left right") flags |= 16;
        if (args[7].intern() == "\u20ac\ud83d\ude80") flags |= 32;
        if (count == 9 && args[8].intern() == "\000\udfff") flags |= 64;
    }
}
