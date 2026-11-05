public class Transport {
    public static void main(String[] args) {
        String[] input = args;
        if (args.length == 1 && args[0].equals("units")) {
            input = new String[] {new String("repeat"), new String("repeat"), new String(""),
                new String(""), "--", "--platform", "left right",
                new String(new char[] {0x20ac, 0xd83d, 0xde80}), new String(new char[] {0, 0xdfff})};
        }
        transport.Entry.main(input);
        System.out.print(transport.Entry.count + "/" + transport.Entry.flags + "/" + transport.Entry.aliases);
    }
}
