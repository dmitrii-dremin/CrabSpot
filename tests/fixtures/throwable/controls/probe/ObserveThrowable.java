package probe;

import java.util.Arrays;
import java.util.IdentityHashMap;

public final class ObserveThrowable {
    static final IdentityHashMap<Throwable, Boolean> seen = new IdentityHashMap<>();
    static String hex(String value) {
        if (value == null) return "null";
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < value.length(); i++) b.append(String.format("%04x", (int)value.charAt(i)));
        return b.toString();
    }
    static String frame(StackTraceElement e) {
        return e.getClassName() + "#" + e.getMethodName() + "#" + e.getFileName()
            + "#" + e.getLineNumber() + "#" + e.isNativeMethod();
    }
    static void observe(String label, RuntimeException value, RuntimeException expected,
                        Throwable cause, boolean traceStable, boolean traceChanged) {
        StackTraceElement[] trace = value.getStackTrace();
        StringBuilder prefix = new StringBuilder();
        for (int i=0; i<Math.min(trace.length, 5); i++) {
            if (i != 0) prefix.append(',');
            prefix.append(frame(trace[i]));
        }
        System.out.println(label + "\tclass=" + value.getClass().getName()
            + "\texactArithmetic=" + (value.getClass() == ArithmeticException.class)
            + "\tmessage16=" + hex(value.getMessage())
            + "\tcause=" + (value.getCause() == null ? "null" : value.getCause().getClass().getName())
            + "\tcauseIdentity=" + (value.getCause() == cause)
            + "\tsameObject=" + (value == expected)
            + "\tfresh=" + (seen.put(value, true) == null)
            + "\ttraceSize=" + trace.length + "\ttrace=" + prefix
            + "\ttraceStable=" + traceStable + "\ttraceChanged=" + traceChanged
            + "\twrites=" + ThrowableGuest.writes + "\tbefore=" + ThrowableGuest.before
            + "\tafter=" + ThrowableGuest.after + "\tcatchCount=" + ThrowableGuest.catchCount
            + "\tmismatch=" + ThrowableGuest.mismatch);
    }
    public static void main(String[] args) {
        ThrowableGuest.UserFailure user = ThrowableGuest.makeUser();
        observe("user-created",user,user,null,true,false);
        StackTraceElement[] created = user.getStackTrace();
        RuntimeException again = ThrowableGuest.catchRethrow(user);
        if (again != user || ThrowableGuest.caught != user || ThrowableGuest.rethrown != user) throw new AssertionError("rethrow identity");
        observe("user-rethrow",again,user,null,Arrays.equals(created,again.getStackTrace()),false);
        if (ThrowableGuest.refill(user) != user) throw new AssertionError("refill identity");
        StackTraceElement[] filled = user.getStackTrace();
        observe("user-refill",user,user,null,false,!Arrays.equals(created,filled));
        again = ThrowableGuest.catchRethrow(user);
        observe("user-rethrow-after-refill",again,user,null,Arrays.equals(filled,again.getStackTrace()),false);
        RuntimeException cause = new RuntimeException("cause");
        if (user.initCause(cause) != user) throw new AssertionError("cause result");
        observe("user-cause",user,user,cause,Arrays.equals(filled,user.getStackTrace()),false);
        RuntimeException replacement = ThrowableGuest.constructorReplacement(user);
        observe("constructor-replacement",replacement,user,cause,Arrays.equals(filled,replacement.getStackTrace()),false);
        if (!(ThrowableGuest.escaped instanceof ThrowableGuest.ReplacementCtor) || ThrowableGuest.escaped == replacement) throw new AssertionError("escaped discarded object");
        observe("escaped-discarded",(RuntimeException)ThrowableGuest.escaped,(RuntimeException)ThrowableGuest.escaped,null,true,false);
        RuntimeException constructorFault = ThrowableGuest.constructorVMFailure();
        observe("constructor-vm-fault",constructorFault,constructorFault,null,true,false);
        if (!(ThrowableGuest.escaped instanceof ThrowableGuest.VMFailingCtor) || ThrowableGuest.escaped == constructorFault) throw new AssertionError("escaped incomplete object");
        observe("escaped-vm-constructor",(RuntimeException)ThrowableGuest.escaped,(RuntimeException)ThrowableGuest.escaped,null,true,false);
        ArithmeticException first = null;
        String[] names = {"idiv","irem","ldiv","lrem"};
        for(int operation=0;operation<4;operation++) {
            ArithmeticException previous = null;
            for(int iteration=0;iteration<2;iteration++) {
                ArithmeticException fault = ThrowableGuest.arithmetic(operation,iteration);
                if(ThrowableGuest.caught != fault) throw new AssertionError("catch identity");
                if(fault == previous || seen.containsKey(fault)) throw new AssertionError("VM exception reused");
                if(ThrowableGuest.after != 0 || ThrowableGuest.before != (operation+1)*1000+1 || ThrowableGuest.catchCount != 1 || ThrowableGuest.mismatch != 0) throw new AssertionError("retained fault effects");
                observe(names[operation]+"-"+iteration,fault,fault,null,true,false);
                if(first == null) first=fault;
                previous=fault;
            }
        }
        StackTraceElement[] faultTrace=first.getStackTrace();
        RuntimeException rethrow=ThrowableGuest.catchRethrow(first);
        observe("vm-rethrow",rethrow,first,null,Arrays.equals(faultTrace,rethrow.getStackTrace()),false);
        ThrowableGuest.refill(first);
        observe("vm-refill",first,first,null,false,!Arrays.equals(faultTrace,first.getStackTrace()));
        System.out.println("boundaries\tvalues="+(ThrowableGuest.idiv(Integer.MIN_VALUE,-1)+"/"+ThrowableGuest.irem(Integer.MIN_VALUE,-1)+"/"+ThrowableGuest.ldiv(Long.MIN_VALUE,-1L)+"/"+ThrowableGuest.lrem(Long.MIN_VALUE,-1L)));
    }
}
