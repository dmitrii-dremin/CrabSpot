package operand;
public final class OperandGuest {
    public static int before,after;
    public static Object published;
    public static boolean loadZ(boolean[] array, int index) {
        before=11;
        boolean value=array[index];
        after=22;
        return value;
    }
    public static void storeZ(boolean[] array, int index, boolean value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static boolean[] newZ(int size) {
        before=31;
        boolean[] value=new boolean[size];
        published=value;
        after=32;
        return value;
    }
    public static byte loadB(byte[] array, int index) {
        before=11;
        byte value=array[index];
        after=22;
        return value;
    }
    public static void storeB(byte[] array, int index, byte value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static byte[] newB(int size) {
        before=31;
        byte[] value=new byte[size];
        published=value;
        after=32;
        return value;
    }
    public static char loadC(char[] array, int index) {
        before=11;
        char value=array[index];
        after=22;
        return value;
    }
    public static void storeC(char[] array, int index, char value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static char[] newC(int size) {
        before=31;
        char[] value=new char[size];
        published=value;
        after=32;
        return value;
    }
    public static short loadS(short[] array, int index) {
        before=11;
        short value=array[index];
        after=22;
        return value;
    }
    public static void storeS(short[] array, int index, short value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static short[] newS(int size) {
        before=31;
        short[] value=new short[size];
        published=value;
        after=32;
        return value;
    }
    public static int loadI(int[] array, int index) {
        before=11;
        int value=array[index];
        after=22;
        return value;
    }
    public static void storeI(int[] array, int index, int value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static int[] newI(int size) {
        before=31;
        int[] value=new int[size];
        published=value;
        after=32;
        return value;
    }
    public static long loadJ(long[] array, int index) {
        before=11;
        long value=array[index];
        after=22;
        return value;
    }
    public static void storeJ(long[] array, int index, long value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static long[] newJ(int size) {
        before=31;
        long[] value=new long[size];
        published=value;
        after=32;
        return value;
    }
    public static float loadF(float[] array, int index) {
        before=11;
        float value=array[index];
        after=22;
        return value;
    }
    public static void storeF(float[] array, int index, float value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static float[] newF(int size) {
        before=31;
        float[] value=new float[size];
        published=value;
        after=32;
        return value;
    }
    public static double loadD(double[] array, int index) {
        before=11;
        double value=array[index];
        after=22;
        return value;
    }
    public static void storeD(double[] array, int index, double value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static double[] newD(int size) {
        before=31;
        double[] value=new double[size];
        published=value;
        after=32;
        return value;
    }
    public static Object loadA(Object[] array, int index) {
        before=11;
        Object value=array[index];
        after=22;
        return value;
    }
    public static void storeA(Object[] array, int index, Object value) {
        before=11;
        array[index]=value;
        after=22;
    }
    public static Object[] newA(int size) {
        before=31;
        Object[] value=new Object[size];
        published=value;
        after=32;
        return value;
    }
    public static int[][][] multiI(int a,int b,int c) {
        before=31;
        int[][][] value=new int[a][b][c];
        published=value;
        after=32;
        return value;
    }
    public static String[][][] multiA(int a,int b) {
        before=31;
        String[][][] value=new String[a][b][];
        published=value;
        after=32;
        return value;
    }
    public static Object castString(Object source) {
        before=11;
        String value=(String)source;
        after=22;
        return value;
    }
    public static Object castTag(Object source) {
        before=11;
        Tag value=(Tag)source;
        after=22;
        return value;
    }
    public static Object castRef(Object source) {
        before=11;
        String[] value=(String[])source;
        after=22;
        return value;
    }
    public static Object castObjectArray(Object source) {
        before=11;
        Object[] value=(Object[])source;
        after=22;
        return value;
    }
    public static Object castObjectRank(Object source) {
        before=11;
        Object[][] value=(Object[][])source;
        after=22;
        return value;
    }
    public static Object castInt(Object source) {
        before=11;
        int[] value=(int[])source;
        after=22;
        return value;
    }
    public static Object castIntRank(Object source) {
        before=11;
        int[][] value=(int[][])source;
        after=22;
        return value;
    }
    public static Object castRefRank(Object source) {
        before=11;
        String[][] value=(String[][])source;
        after=22;
        return value;
    }
    public static Object castClone(Object source) {
        before=11;
        Cloneable value=(Cloneable)source;
        after=22;
        return value;
    }
    public static Object castSerial(Object source) {
        before=11;
        java.io.Serializable value=(java.io.Serializable)source;
        after=22;
        return value;
    }
    public static Object castUnicode(Object source) {
        before=11;
        名 value=(名)source;
        after=22;
        return value;
    }
    public static void rethrow(Throwable value) throws Throwable { throw value; }
    public static void nullMessage() { before=11; throw new ClassCastException(); }
    public static void emptyMessage() { before=11; throw new ArrayStoreException(""); }
}
