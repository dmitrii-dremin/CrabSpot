package argv;
public class Inputs {
    public static int length, flags, units, order;
    public static void main(String[] args) {
        length = args.length;
        if (length != 5) return;
        if (args[0] != args[1]) flags |= 1;
        if (args[2] != args[3]) flags |= 2;
        if (args[0].equals(args[1]) && args[2].equals(args[3])) flags |= 4;
        if (args[0].intern() == args[1].intern() && args[2].intern() == args[3].intern()) flags |= 8;
        if (args[0] == "crab") flags |= 16;
        if (args[2] == "") flags |= 32;
        if (args[4].getClass() == String.class && args.getClass() == String[].class) flags |= 64;
        for (String s : args) {
            order = order * 31 + s.length();
            for (int i = 0; i < s.length(); i++) units = 31 * units + s.charAt(i);
        }
    }
}