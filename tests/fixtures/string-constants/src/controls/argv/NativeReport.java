public class NativeReport {
    public static void main(String[] args) {
        argv.Inputs.main(args);
        System.out.print(argv.Inputs.length + "/" + argv.Inputs.flags + "/" +
            argv.Inputs.units + "/" + argv.Inputs.order);
        for (String arg : args) {
            System.out.print("|");
            for (int i = 0; i < arg.length(); i++) System.out.printf("%04x", (int)arg.charAt(i));
        }
    }
}
