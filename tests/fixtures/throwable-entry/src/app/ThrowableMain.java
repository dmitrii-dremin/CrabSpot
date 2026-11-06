package app;

import probe.ThrowableEntryGuest;

public final class ThrowableMain {
    public static int flags, arithmeticCases, localCases, completed;
    public static Throwable retained;

    public static final class Forms extends Throwable {
        public Forms() { super(); }
        public Forms(String message) { super(message); }
        public Forms(String message, Throwable cause) { super(message, cause); }
        public Forms(String message, Throwable cause, boolean suppression, boolean writable) {
            super(message, cause, suppression, writable);
        }
    }

    private static void mark(boolean result, int bit) {
        if (result) flags |= bit;
    }

    private static ArithmeticException local(int operation, int zero) {
        try {
            switch (operation) {
                case 0: completed = 31 / zero; break;
                case 1: completed = 31 % zero; break;
                case 2: completed = (int) (31L / (long) zero); break;
                default: completed = (int) (31L % (long) zero); break;
            }
        } catch (NullPointerException wrong) {
            throw wrong;
        } catch (ArithmeticException expected) {
            localCases++;
            return expected;
        }
        return null;
    }

    private static boolean monitorRefill(Throwable value) {
        synchronized (value) {
            synchronized (value) {
                if (value.fillInStackTrace() != value) return false;
            }
        }
        synchronized (value) {
            return value.fillInStackTrace() == value;
        }
    }

    public static void main(String[] args) throws Throwable {
        ThrowableEntryGuest.UserFailure user = ThrowableEntryGuest.makeUser();
        retained = user;
        mark(user.getMessage() == "guest:\u0000\udfff\u20ac", 1);
        mark(user.getCause() == null && user.getLocalizedMessage() == user.getMessage(), 2);
        RuntimeException cause = new RuntimeException("cause");
        mark(user.initCause(cause) == user && user.getCause() == cause, 4);
        mark(ThrowableEntryGuest.catchRethrow(user) == user && ThrowableEntryGuest.caught == user
                && ThrowableEntryGuest.rethrown == user, 8);
        mark(ThrowableEntryGuest.refill(user) == user, 16);
        mark(ThrowableEntryGuest.constructorReplacement(user) == user
                && ThrowableEntryGuest.escaped instanceof ThrowableEntryGuest.ReplacementCtor
                && ThrowableEntryGuest.escaped != user && ThrowableEntryGuest.writes == 9101, 32);
        RuntimeException constructorFault = ThrowableEntryGuest.constructorVMFailure();
        mark(constructorFault.getClass() == ArithmeticException.class
                && ThrowableEntryGuest.escaped instanceof ThrowableEntryGuest.VMFailingCtor
                && ThrowableEntryGuest.escaped != constructorFault && ThrowableEntryGuest.writes == 9201, 64);

        ArithmeticException[] faults = new ArithmeticException[8];
        boolean arithmetic = true;
        for (int operation = 0; operation < 4; operation++) {
            for (int iteration = 0; iteration < 2; iteration++) {
                ArithmeticException fault = ThrowableEntryGuest.arithmetic(operation, iteration);
                arithmetic &= fault != null && fault.getClass() == ArithmeticException.class
                        && fault.getCause() == null && fault.getMessage() != null
                        && ThrowableEntryGuest.before == (operation + 1) * 1000 + 1
                        && ThrowableEntryGuest.after == 0 && ThrowableEntryGuest.catchCount == 1
                        && ThrowableEntryGuest.mismatch == 0 && ThrowableEntryGuest.caught == fault;
                for (int i = 0; i < arithmeticCases; i++) arithmetic &= faults[i] != fault;
                faults[arithmeticCases++] = fault;
            }
        }
        mark(arithmetic && arithmeticCases == 8, 128);
        boolean locals = true;
        for (int operation = 0; operation < 4; operation++) {
            ArithmeticException fault = local(operation, 0);
            locals &= fault != null && fault.getClass() == ArithmeticException.class;
        }
        mark(locals && localCases == 4 && completed == 0, 256);
        mark(ThrowableEntryGuest.idiv((-2147483647 - 1), -1) == (-2147483647 - 1)
                && ThrowableEntryGuest.irem((-2147483647 - 1), -1) == 0
                && ThrowableEntryGuest.ldiv((-9223372036854775807L - 1L), -1L) == (-9223372036854775807L - 1L)
                && ThrowableEntryGuest.lrem((-9223372036854775807L - 1L), -1L) == 0L
                && ThrowableEntryGuest.idiv(19, 3) == 6 && ThrowableEntryGuest.irem(19, 3) == 1
                && ThrowableEntryGuest.ldiv(19L, 3L) == 6L && ThrowableEntryGuest.lrem(19L, 3L) == 1L
                && floatingZero(0.0f, 0.0), 512);

        Forms empty = new Forms();
        Forms message = new Forms("same-message");
        Forms pair = new Forms("same-message", cause);
        Forms disabled = new Forms("same-message", cause, false, false);
        mark(empty.getMessage() == null && empty.getCause() == null, 1024);
        mark(message.getMessage() == "same-message" && message.getCause() == null, 2048);
        mark(pair.getMessage() == "same-message" && pair.getCause() == cause, 4096);
        mark(disabled.getMessage() == "same-message" && disabled.getCause() == cause
                && disabled.fillInStackTrace() == disabled, 8192);
        mark(empty.getSuppressed().length == 0 && disabled.getSuppressed().length == 0, 16384);
        mark(monitorRefill(user), 32768);
        if (flags != (args.length == 0 ? 65535 : 65534)) throw user;
        completed = 1;
    }

    private static boolean floatingZero(float zero, double wideZero) {
        return 1.0f / zero > 3.4028235e38f
                && 1.0 / wideZero > 1.7976931348623157e308
                && 0.0f / zero != 0.0f / zero && 0.0 / wideZero != 0.0 / wideZero;
    }
}
