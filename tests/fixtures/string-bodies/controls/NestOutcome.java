import peer.Host;
public class NestOutcome {
    public static void main(String[] args) throws Exception {
        Class<?> outer = Host.class, inner = Class.forName("peer.Host$Inner", false, outer.getClassLoader());
        if (args.length > 0) System.out.print(inner.getNestHost().getName() + "/" + outer.isNestmateOf(inner) + "/");
        try { Host.main(new String[0]); System.out.print("returned"); } catch (LinkageError e) { System.out.print(e.getClass().getSimpleName()); }
        System.out.println("/" + Host.phase);
    }
}
