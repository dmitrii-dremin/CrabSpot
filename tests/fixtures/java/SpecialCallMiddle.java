package fixtures;
public class SpecialCallMiddle extends SpecialCallBase implements SpecialRootDefault {
    public int value(int value) { hidden = value + 1; return hook(value) * 3; }
    public long wide(long value) { wide = value; return value * 5; }
    public int parentValue(int value) { return super.value(value); }
    public int parentStatic(int value) { return super.staticBody(value) + 1; }
}
