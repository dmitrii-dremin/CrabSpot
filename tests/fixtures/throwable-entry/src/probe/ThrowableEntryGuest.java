package probe;

public final class ThrowableEntryGuest {
    public static int writes, before, after, catchCount, mismatch;
    public static RuntimeException caught, rethrown;
    public static Object escaped;

    public static final class UserFailure extends RuntimeException {
        public UserFailure(String message) { super(message); }
    }
    public static final class ReplacementCtor extends RuntimeException {
        public ReplacementCtor(RuntimeException replacement) {
            super("discarded");
            escaped = this;
            writes = 9101;
            throw replacement;
        }
    }
    public static final class VMFailingCtor extends RuntimeException {
        public VMFailingCtor(int zero) {
            super("partially-constructed");
            escaped = this;
            writes = 9201;
            int result = 1 / zero;
            writes = result;
        }
    }
    public static UserFailure makeUser() {
        writes = 10;
        UserFailure value = new UserFailure("guest:\u0000\udfff\u20ac");
        writes = 11;
        return value;
    }
    public static RuntimeException catchRethrow(RuntimeException supplied) {
        writes = 20;
        try {
            writes = 21;
            throw supplied;
        } catch (RuntimeException first) {
            caught = first;
            writes = 22;
            try {
                throw first;
            } catch (RuntimeException second) {
                rethrown = second;
                writes = 23;
                return second;
            }
        }
    }
    public static Throwable refill(RuntimeException supplied) {
        writes = 24;
        return supplied.fillInStackTrace();
    }
    public static RuntimeException constructorReplacement(RuntimeException supplied) {
        try {
            new ReplacementCtor(supplied);
        } catch (RuntimeException failure) {
            caught = failure;
            return failure;
        }
        throw new AssertionError("constructor returned");
    }
    public static RuntimeException constructorVMFailure() {
        try {
            new VMFailingCtor(0);
        } catch (RuntimeException failure) {
            caught = failure;
            return failure;
        }
        throw new AssertionError("constructor returned");
    }
    public static int idiv(int a, int b) {
        before = 1001;
        int result = a / b;
        after = 1002;
        return result;
    }
    public static int irem(int a, int b) {
        before = 2001;
        int result = a % b;
        after = 2002;
        return result;
    }
    public static long ldiv(long a, long b) {
        before = 3001;
        long result = a / b;
        after = 3002;
        return result;
    }
    public static long lrem(long a, long b) {
        before = 4001;
        long result = a % b;
        after = 4002;
        return result;
    }
    public static ArithmeticException arithmetic(int operation, int iteration) {
        before = after = catchCount = mismatch = 0;
        try {
            switch (operation) {
                case 0: idiv(iteration == 0 ? (-2147483647 - 1) : 11, 0); break;
                case 1: irem(iteration == 0 ? (-2147483647 - 1) : 11, 0); break;
                case 2: ldiv(iteration == 0 ? (-9223372036854775807L - 1L) : 11L, 0L); break;
                case 3: lrem(iteration == 0 ? (-9223372036854775807L - 1L) : 11L, 0L); break;
                default: throw new AssertionError("bad operation");
            }
        } catch (NullPointerException wrong) {
            mismatch++;
            throw wrong;
        } catch (ArithmeticException failure) {
            catchCount++;
            caught = failure;
            return failure;
        }
        throw new AssertionError("no ArithmeticException");
    }
}
