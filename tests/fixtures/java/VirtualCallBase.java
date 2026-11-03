package fixtures;

// Original public base avoids javac accessibility bridges that use invokespecial.
public class VirtualCallBase {
    int hidden;
    public int dispatchFromBase(int value) { hidden = value; return target(value) + hidden; }
    protected int target(int value) { hidden = value + 3; return hidden * 7; }
    protected long target(long value) { return value + 0x1020304050607080L; }
    public int inheritedBody(int value) { hidden = value; return hidden ^ 0x12345678; }
    public final int finalBody(int value) { return value * 13 - 7; }
    public int parity(int depth) { return depth <= 0 ? 11 : peer(depth - 1); }
    protected int peer(int depth) { return depth <= 0 ? 13 : parity(depth - 1); }
    protected static int baseHelper(int value) { return value * 19 - 5; }
    public int baseCallsStatic(int value) { hidden = baseHelper(value); return hidden; }
}
