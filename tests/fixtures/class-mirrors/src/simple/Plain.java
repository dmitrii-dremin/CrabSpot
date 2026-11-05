package simple;
public class Plain {
    public static int flag;
    public static void main(String[] args) {
        Object first = Plain.class;
        Object second = Plain.class;
        if (first == second) flag = 1;
    }
}
