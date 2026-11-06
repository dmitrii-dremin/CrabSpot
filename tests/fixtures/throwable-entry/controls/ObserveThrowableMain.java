import app.*;
public final class ObserveThrowableMain {
    private static void describe(Throwable value) {
        StackTraceElement first = value.getStackTrace()[0];
        System.out.print(value.getClass().getName() + "/" + first.getClassName()
                + "." + first.getMethodName());
    }
    public static void main(String[] args) throws Throwable {
        switch (args[0]) {
            case "main":
                ThrowableMain.main(new String[0]);
                System.out.println(ThrowableMain.flags + "/" + ThrowableMain.arithmeticCases
                        + "/" + ThrowableMain.localCases + "/" + ThrowableMain.completed);
                return;
            case "wrong":
                try { ThrowableMain.main(new String[]{"wrong"}); }
                catch (Throwable value) {
                    describe(value);
                    System.out.println("/" + ThrowableMain.flags + "/" + ThrowableMain.completed
                            + "/" + (value == ThrowableMain.retained));
                    return;
                }
                break;
            case "uncaught":
                try { ThrowableUncaught.main(new String[0]); }
                catch (ArithmeticException value) {
                    describe(value);
                    System.out.println("/" + ThrowableUncaught.before + "/" + ThrowableUncaught.after);
                    return;
                }
                break;
            case "refilled":
                try { ThrowableRefilled.main(new String[0]); }
                catch (RuntimeException value) { describe(value); System.out.println(); return; }
                break;
            case "trace":
                ThrowableTraceAccess.main(new String[0]);
                System.out.println(ThrowableTraceAccess.before + "/" + (ThrowableTraceAccess.after > 0));
                return;
            default: throw new AssertionError("unknown case");
        }
        throw new AssertionError("missing expected throw");
    }
}
