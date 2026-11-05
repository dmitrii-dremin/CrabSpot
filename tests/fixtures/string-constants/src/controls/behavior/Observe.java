public class Observe {
    public static void main(String[] args) {
        if (args[0].equals("words")) {
            words.Entry.main(args);
            System.out.print(words.Entry.flags+"/"+words.Entry.hash+"/"+words.Entry.units+"/"+(words.Entry.saved==words.Peer.CONSTANT));
        } else if (args[0].equals("bare")) {
            bare.Literal.main(args);
            System.out.print(bare.Literal.flags);
        } else {
            checks.Enabled.main(args);
            System.out.print(checks.Enabled.flags+"/"+checks.Enabled.assertion);
        }
    }
}
