package fixtures;
public class ReferenceFieldBase extends referencep.ReferenceOwner {
    ReferenceFieldBase link;
    Object payload;
    int touched;
    public int compute(int value) { touched = value; return value * 7 + 3; }
}
