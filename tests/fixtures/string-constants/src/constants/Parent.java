package constants;

public class Parent {
    static {
        if (Read.first() == "alpha" && Read.second() == "beta") Marks.order |= 1;
    }
}
