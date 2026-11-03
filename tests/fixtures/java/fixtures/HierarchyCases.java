package fixtures;

// Original structural fixtures: the Rust layout tests do not execute constructors
// or claim initialization, access checking, sealed checks, or JVM object offsets.
class HierarchyBase {
    int hidden;
    long wide;
    Object reference;
    static int counter;
}

class HierarchyChild extends HierarchyBase implements HierarchyContract {
    private int hidden;
    double fraction;
    static long counter;
}

interface HierarchyParentContract {
    long TOKEN = 0x1020304050607080L;
}

interface HierarchyContract extends HierarchyParentContract {
    String TEXT = "metadata only";
}
