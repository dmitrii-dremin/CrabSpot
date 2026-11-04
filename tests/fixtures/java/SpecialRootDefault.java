package fixtures;
public interface SpecialRootDefault {
    default int number(int value) { return value * 19 + 2; }
    default int inherited(int value) { return value ^ 0x3456789a; }
    default int chain(int value) { return number(value) + 17; }
    private int privateNumber(int value) { return value ^ 0x56789abc; }
    default int privateBody(int value) { return privateNumber(value); }
}
