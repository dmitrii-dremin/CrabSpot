package fixtures;
public class CastBase implements CastRootMarker {
    int inherited;
    public int baseTest(int value) {
        inherited = value; Object receiver = value == 0 ? null : this;
        return receiver instanceof CastChild ? value + 1 : value - 1;
    }
}
