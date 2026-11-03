package fixtures.virtualp;

public class PackageCallBase {
    int target(int value) { return value + 11; }
    public int dispatch(int value) { return target(value); }
}
