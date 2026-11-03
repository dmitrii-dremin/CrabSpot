package fixtures.virtualp;

public class PackageCallBridge extends fixtures.virtualq.PackageCallMiddle {
    @Override public int target(int value) { return value * 31 + 7; }
}
