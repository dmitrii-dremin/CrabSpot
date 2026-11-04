package fixtures;
public class SpecialCallBase {
    int hidden;
    long wide;
    float single;
    double precise;
    protected int hook(int value) { return value * 7; }
    public int value(int value) { hidden = value; return hook(value) + 11; }
    public long wide(long value) { wide = value; return value ^ 0x123456789abcdef0L; }
    public float single(float value) { single = value; return single; }
    public double precise(double value) { precise = value; return precise; }
    public double mixed(int bias, long wide, float single, double precise) {
        hidden = bias; this.wide = wide; this.single = single; this.precise = precise;
        return hidden + this.wide + this.single + this.precise;
    }
    public void store(int value) { hidden = value; }
    public int divide(int value) { hidden = value; return 100 / value; }
    public final int finalBody(int value) { return value * 13 - 7; }
    private static int helper(int value) { return value * 19 - 3; }
    public int staticBody(int value) { hidden = helper(value); return hidden; }
    public int array(int length) {
        int[] values = new int[length];
        for (int index = 0; index < length; index++) values[index] = hook(index);
        int sum = 0; for (int value : values) sum += value; return sum;
    }
    public int recurse(int depth) { return depth <= 0 ? hidden : 1 + recurse(depth - 1); }
}
