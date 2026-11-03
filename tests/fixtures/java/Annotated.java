package fixtures;

import java.io.IOException;
import java.lang.annotation.*;
import java.util.List;

enum Choice { FIRST, SECOND }

@Retention(RetentionPolicy.RUNTIME)
@interface Nested { String value(); }

@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.FIELD, ElementType.METHOD,
         ElementType.PARAMETER, ElementType.RECORD_COMPONENT})
@interface RichAnnotation {
    byte small() default 7;
    char character() default '\u03bb';
    double decimal() default 2.5;
    float fraction() default 1.25f;
    int number() default 42;
    long wide() default 1234567890123L;
    short shortValue() default 1234;
    boolean enabled() default true;
    String text() default "annotation\u0000\ud83e\udd80";
    Choice choice() default Choice.SECOND;
    Class<?> type() default void.class;
    Nested nested() default @Nested("child");
    int[] numbers() default {1, 2, 3};
}

@Retention(RetentionPolicy.CLASS)
@Target({ElementType.TYPE, ElementType.FIELD, ElementType.METHOD, ElementType.PARAMETER})
@interface InvisibleDeclaration { String value() default "invisible"; }

@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
@interface VisibleType { int value() default 1; }

@Retention(RetentionPolicy.CLASS)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
@interface InvisibleType {}

@RichAnnotation(number = 17, type = String[].class, nested = @Nested("root"))
@InvisibleDeclaration
public class Annotated<@VisibleType T extends @VisibleType @InvisibleType Number>
        extends @VisibleType Object implements @VisibleType Runnable {
    @RichAnnotation @InvisibleDeclaration
    public List<@VisibleType @InvisibleType String> values;

    @RichAnnotation @InvisibleDeclaration
    public <@VisibleType U extends @InvisibleType Object> @VisibleType String method(
            @RichAnnotation @InvisibleDeclaration @InvisibleType String input)
            throws @VisibleType IOException {
        @VisibleType @InvisibleType String local = (@VisibleType String) input;
        try {
            if (input.isEmpty()) throw new @VisibleType IOException("empty");
            Object object = new @VisibleType String(local);
            return object instanceof @VisibleType String ? local : "fallback";
        } catch (@VisibleType IOException error) {
            throw error;
        }
    }

    @Override public void run() {}
}
