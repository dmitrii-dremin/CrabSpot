package texts;
public class Entry {
    public static void main(String[] args) {
        State.argc = args.length;
        if (args.length != 4) return;
        State.retained=args;
        if (args[0]!=args[1]) State.flags |= 1;
        String arg = args[0].intern();
        if (arg==args[1].intern() && arg==Peer.word()) State.flags |= 2;
        if (args[2].intern()=="" && args[3].intern()=="left right") State.flags |= 4;
        String mixed=Peer.same();
        if (State.initialized==0 && mixed.getClass()==String.class) State.flags |= 8;
        Holder.touch();
        if (Read.value()==mixed && State.early==mixed && State.initialized==1) State.flags |= 16;
        Object[] alias = new String[1];
        alias[0]=mixed;
        String selected=(String)alias[0];
        if (selected.intern()==mixed && selected!=arg) State.flags |= 32;
        synchronized (selected) { synchronized (Read.value()) { State.seen++; } }
        if (State.seen==1 && args.getClass()==String[].class) State.flags |= 64;
    }
}
