package bare;
public class Literal {
    public static int flags;
    public static void main(String[] args) {
        Object first = "crab";
        Object second = "crab";
        if (first == second) flags = 1;
    }
}
