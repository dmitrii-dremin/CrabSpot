package app;

import operand.OperandGuest;

/** Separate entry for real fault origins, refill and the still-unsupported NPE boundary. */
public final class OperandUncaught {
    public static int before, after;
    public static void main(String[] args) {
        before = 7;
        if ("bounds".equals(args[0])) OperandGuest.loadI(new int[2],-1);
        else if ("negative".equals(args[0])) OperandGuest.multiI(0,-4,2);
        else if ("store".equals(args[0])) OperandGuest.storeA(new String[1],0,new Object());
        else if ("cast".equals(args[0])) OperandGuest.castInt(new long[1]);
        else if ("null".equals(args[0])) OperandGuest.loadI(null,-1);
        else if ("refilled".equals(args[0])) {
            try { OperandGuest.loadI(new int[2],-1); }
            catch (ArrayIndexOutOfBoundsException value) { value.fillInStackTrace(); throw value; }
        } else throw new OperandMain.Failure();
        after = 9;
    }
}
