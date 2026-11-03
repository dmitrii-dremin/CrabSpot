package fixtures;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.util.function.IntUnaryOperator;

@Retention(RetentionPolicy.RUNTIME)
@interface Marker {
    String value();
    int[] numbers() default {1, 2, 3};
}

@Marker(value = "class", numbers = {4, 5})
public class Fixture<T extends Number> {
    public static final long WIDE = 0x1020304050607080L;
    public static final double PI = 3.141592653589793;
    public static final float FRACTION = 1.25f;
    public static final String TEXT = "nul\u0000supplementary\ud83e\udd80";
    private final T value;

    public Fixture(T value) { this.value = value; }
    public T value() { return value; }

    public static int branch(int n) {
        try {
            if (n < 0) throw new IllegalArgumentException("negative");
            return switch (n) { case 0 -> 11; case 2 -> 22; default -> 100 / n; };
        } catch (IllegalArgumentException | ArithmeticException error) {
            return -1;
        }
    }

    public static String dynamic(int n) {
        IntUnaryOperator increment = x -> x + 1;
        return "answer=" + increment.applyAsInt(n);
    }

    public class Inner {
        public T read() { return value; }
    }
}
