package fixtures.virtualq;

// Transitive overriding reaches PackageCallBase through PackageCallBridge.
public class PackageCallLeaf extends fixtures.virtualp.PackageCallBridge {
    @Override public int target(int value) { return value ^ 0x12345678; }
}
