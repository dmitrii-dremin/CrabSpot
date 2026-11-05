public class Watch {
    public static void main(String[] args) {
        store.Entry.main(args);
        System.out.print(store.State.flags+"/"+store.State.initialized+" "+
            (store.State.global==store.Later.class)+"/"+(store.State.array[0]==store.State.global)+"/"+
            Thread.holdsLock(store.Later.class));
    }
}
