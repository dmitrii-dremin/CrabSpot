package compact;

public class Entry {
    public static int flags, emptyLength, latinLength, mixedLength, isolatedLength, pairLength, argumentLength;
    public static String retained;
    public static void main(String[] args) {
        String[] values = {"", "A\000\u00ff", "A\u20ac\ud83d\ude80", "\udfff", "\ud83d\ude80"};
        emptyLength = values[0].length();
        if (values[0].isEmpty() && emptyLength == 0) flags |= 1;
        latinLength = values[1].length();
        if (!values[1].isEmpty() && latinLength == 3) flags |= 2;
        mixedLength = values[2].length();
        if (!values[2].isEmpty() && mixedLength == 4) flags |= 4;
        isolatedLength = values[3].length();
        if (!values[3].isEmpty() && isolatedLength == 1) flags |= 8;
        pairLength = values[4].length();
        if (!values[4].isEmpty() && pairLength == 2) flags |= 16;
        Object[] alias = values;
        retained = (String) alias[2];
        if (retained == values[2] && retained.length() == mixedLength) flags |= 32;
        for (String arg : args) argumentLength += arg.length();
        if (args.length == 2 && args[0].isEmpty() && argumentLength == 3) flags |= 64;
    }
}
