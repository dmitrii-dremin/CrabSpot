package query;

public class Query {
    public static int phase, enabled, initialized;
    public static void main(String[] args) {
        phase = 1;
        Class<?> type;
        if (args.length == 0) type = String.class;
        else if (args.length == 1) type = Dormant.class;
        else type = String[].class;
        enabled = type.desiredAssertionStatus() ? 1 : 0;
        phase = 2;
    }
}

class Dormant {
    static { Query.initialized++; }
}
