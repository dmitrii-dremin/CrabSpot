package fixtures;

// Original metadata fixture. Runtime memory-service tests do not run its constructor.
class InstanceFields extends HierarchyChild {
    boolean flag;
    byte small;
    char letter;
    short narrow;
    int integer;
    long wide;
    float single;
    double precise;
    Object reference;
    int[] primitiveArray;
    Object[][] objectArray;
    int hidden;
    final int immutable = 42;
    static int staticOnly = 19;
}
