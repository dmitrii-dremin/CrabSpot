package fixtures;

// The storage tests inspect these declarations without executing <clinit>.
// Explicit initializers and ConstantValue attributes must not replace defaults.
class StorageDefaults extends HierarchyChild {
    static boolean flag = true;
    static byte small = -12;
    static char letter = 'x';
    static short narrow = 300;
    static int integer = 77;
    static long wide = 123L;
    static float single = -0.0f;
    static double precise = -0.0d;
    static Object reference = new Object();
    static int[] primitiveArray = {1};
    static String[][] objectArray = {{"x"}};
    static int counter = 99;

    static final int INTEGER_CONSTANT = 42;
    static final long LONG_CONSTANT = 0x1020304050607080L;
    static final float FLOAT_CONSTANT = -0.0f;
    static final double DOUBLE_CONSTANT = Double.NaN;
    static final String STRING_CONSTANT = "constant";
    int onlyInstance = 5;
}
