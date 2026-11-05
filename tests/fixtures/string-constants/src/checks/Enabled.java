package checks;
public class Enabled {
    public static int flags, assertion;
    public static void main(String[] args) {
        if (Object.class.desiredAssertionStatus()) flags |= 1;
        if (Enabled.class.desiredAssertionStatus()) flags |= 2;
        if (int[].class.desiredAssertionStatus()) flags |= 4;
        assert (assertion = 7) == 7;
    }
}
