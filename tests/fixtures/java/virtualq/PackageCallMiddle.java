package fixtures.virtualq;

// This same-signature declaration does not override the inaccessible package method.
public class PackageCallMiddle extends fixtures.virtualp.PackageCallBase {
    public int target(int value) { return value * 17; }
}
