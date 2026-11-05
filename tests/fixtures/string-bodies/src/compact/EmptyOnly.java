package compact;

public class EmptyOnly {
    public static int flags;

    public static void main(String[] args) {
        if ("".isEmpty()) flags |= 1;
        if (!"\udfff".isEmpty()) flags |= 2;
        if (args.length == 1 && args[0].isEmpty()) flags |= 4;
    }
}
