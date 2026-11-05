public class See {
    public static void main(String[] args) {
        texts.Entry.main(args);
        System.out.print(texts.State.flags+"/"+texts.State.initialized+"/"+texts.State.seen+"/"+
            texts.State.argc+"/"+(args==texts.State.retained)+"/"+
            (texts.State.early!=null && texts.State.early==texts.Peer.same()));
    }
}