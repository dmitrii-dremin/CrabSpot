import compact.Entry;
public class Observe {
    public static void main(String[] args) {
        String[] input = args.length == 0 ? new String[0] : new String[]{"", new String(new char[]{0, 0xdfff, 0x20ac})};
        Entry.main(input);
        System.out.println(Entry.flags + "/" + Entry.emptyLength + "/" + Entry.latinLength + "/" + Entry.mixedLength + "/" + Entry.isolatedLength + "/" + Entry.pairLength + "/" + Entry.argumentLength);
    }
}
