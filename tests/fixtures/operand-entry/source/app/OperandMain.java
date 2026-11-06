package app;

import operand.*;

/** Genuine product guest. Expected diagnostic arguments come from separate oracles. */
public final class OperandMain {
    public static int bounds, negative, stores, casts, arithmetic, checked, positives, completed;
    private static RuntimeException previous;
    private static String[] messages;

    public static final class Failure extends RuntimeException {
        public Failure() { super("operand oracle mismatch"); }
    }
    private static void require(boolean value) {
        if (!value) throw new Failure();
    }
    private static void reset() {
        OperandGuest.before = 0;
        OperandGuest.after = 0;
        OperandGuest.published = null;
    }
    private static boolean messageMatches(RuntimeException value, int slot) {
        String actual = value.getMessage();
        String tag = messages[slot * 2];
        String expected = messages[slot * 2 + 1];
        return "null".equals(tag) ? actual == null && expected.isEmpty()
            : "text".equals(tag) && expected.equals(actual);
    }
    private static void check(RuntimeException value, int slot, int before) {
        Class<?> exact = slot < 5 ? ArrayIndexOutOfBoundsException.class
            : slot < 9 ? NegativeArraySizeException.class : slot < 12 ? ArrayStoreException.class
            : slot < 16 ? ClassCastException.class : ArithmeticException.class;
        require(value.getClass() == exact);
        require(value != previous && value.getCause() == null);
        require(OperandGuest.before == before && OperandGuest.after == 0);
        require(messageMatches(value, slot));
        previous = value;
        boolean rethrown = false;
        try { OperandGuest.rethrow(value); }
        catch (Throwable again) { require(again == value); rethrown = true; }
        require(rethrown);
        require(value.fillInStackTrace() == value);
        require(messageMatches(value, slot));
        checked++;
    }
    private static Object array(int kind, int size) {
        switch (kind) {
            case 0: return OperandGuest.newZ(size);
            case 1: return OperandGuest.newB(size);
            case 2: return OperandGuest.newC(size);
            case 3: return OperandGuest.newS(size);
            case 4: return OperandGuest.newI(size);
            case 5: return OperandGuest.newJ(size);
            case 6: return OperandGuest.newF(size);
            case 7: return OperandGuest.newD(size);
            default: return OperandGuest.newA(size);
        }
    }
    private static void access(int kind, boolean store, Object array, int index) {
        switch (kind) {
            case 0: if (store) OperandGuest.storeZ((boolean[])array,index,true); else OperandGuest.loadZ((boolean[])array,index); break;
            case 1: if (store) OperandGuest.storeB((byte[])array,index,(byte)-7); else OperandGuest.loadB((byte[])array,index); break;
            case 2: if (store) OperandGuest.storeC((char[])array,index,(char)65535); else OperandGuest.loadC((char[])array,index); break;
            case 3: if (store) OperandGuest.storeS((short[])array,index,(short)-300); else OperandGuest.loadS((short[])array,index); break;
            case 4: if (store) OperandGuest.storeI((int[])array,index,0x12345678); else OperandGuest.loadI((int[])array,index); break;
            case 5: if (store) OperandGuest.storeJ((long[])array,index,(-9223372036854775807L-1L)); else OperandGuest.loadJ((long[])array,index); break;
            case 6: if (store) OperandGuest.storeF((float[])array,index,-0.0f); else OperandGuest.loadF((float[])array,index); break;
            case 7: if (store) OperandGuest.storeD((double[])array,index,-0.0); else OperandGuest.loadD((double[])array,index); break;
            default: if (store) OperandGuest.storeA((Object[])array,index,"value"); else OperandGuest.loadA((Object[])array,index); break;
        }
    }
    private static boolean matches(int kind, Object array, int index) {
        switch (kind) {
            case 0: return OperandGuest.loadZ((boolean[])array,index);
            case 1: return OperandGuest.loadB((byte[])array,index) == -7;
            case 2: return OperandGuest.loadC((char[])array,index) == 65535;
            case 3: return OperandGuest.loadS((short[])array,index) == -300;
            case 4: return OperandGuest.loadI((int[])array,index) == 0x12345678;
            case 5: return OperandGuest.loadJ((long[])array,index) == (-9223372036854775807L-1L);
            case 6: return 1.0f / OperandGuest.loadF((float[])array,index) == (-1.0f / 0.0f);
            case 7: return 1.0 / OperandGuest.loadD((double[])array,index) == (-1.0 / 0.0);
            default: return OperandGuest.loadA((Object[])array,index) == "value";
        }
    }
    private static void boundsCases() {
        int[] indices = {-1, 2, (-2147483647-1), 2147483647, 0};
        for (int kind=0; kind<9; kind++) {
            for (int store=0; store<2; store++) {
                for (int test=0; test<5; test++) {
                    for (int repeat=0; repeat<2; repeat++) {
                        Object value = array(kind, test == 4 ? 0 : 2);
                        reset();
                        try { access(kind,store != 0,value,indices[test]); throw new Failure(); }
                        catch (ArrayIndexOutOfBoundsException e) { check(e,test,11); bounds++; }
                    }
                }
            }
            Object value = array(kind,2);
            for (int endpoint=0; endpoint<2; endpoint++) {
                reset(); access(kind,true,value,endpoint);
                require(OperandGuest.before == 11 && OperandGuest.after == 22);
                require(matches(kind,value,endpoint)); positives++;
            }
        }
        for (int repeat=0; repeat<2; repeat++) {
            Object[] destination = new String[0];
            reset();
            try { OperandGuest.storeA(destination,0,new Object()); throw new Failure(); }
            catch (ArrayIndexOutOfBoundsException e) { check(e,4,11); bounds++; }
            int[] value = new int[2];
            reset(); OperandGuest.before = 11;
            try { int ignored = value[-1]; throw new Failure(); }
            catch (ArrayIndexOutOfBoundsException e) { check(e,0,11); bounds++; }
        }
    }
    private static void negativeCases() {
        for (int kind=0; kind<9; kind++) {
            for (int test=0; test<2; test++) {
                for (int repeat=0; repeat<2; repeat++) {
                    reset();
                    try { array(kind,test == 0 ? -1 : (-2147483647-1)); throw new Failure(); }
                    catch (NegativeArraySizeException e) {
                        check(e,5+test,31); require(OperandGuest.published == null); negative++;
                    }
                }
            }
        }
        for (int test=0; test<3; test++) {
            for (int repeat=0; repeat<2; repeat++) {
                reset();
                try {
                    if (test == 0) OperandGuest.multiI(-3,-4,-5);
                    else if (test == 1) OperandGuest.multiI(0,-4,2);
                    else OperandGuest.multiA(0,-4);
                    throw new Failure();
                } catch (NegativeArraySizeException e) {
                    check(e,test == 0 ? 7 : 8,31); require(OperandGuest.published == null); negative++;
                }
            }
        }
    }
    private static void storeCases() {
        for (int test=0; test<3; test++) {
            for (int repeat=0; repeat<2; repeat++) {
                Object source = test == 0 ? new Object() : test == 1 ? new int[1] : new \u00c9l\u00e9ment();
                Object[] destination = new String[]{"old"};
                reset();
                try { OperandGuest.storeA(destination,0,source); throw new Failure(); }
                catch (ArrayStoreException e) {
                    check(e,9+test,11); require(destination[0] == "old"); stores++;
                }
            }
        }
    }
    private static void castCases() {
        for (int test=0; test<4; test++) {
            for (int repeat=0; repeat<2; repeat++) {
                Object source = test == 0 ? new long[1] : test == 1 ? new int[1]
                    : test == 2 ? new Object() : new \u00c9l\u00e9ment();
                reset();
                try {
                    if (test == 0) OperandGuest.castInt(source);
                    else if (test == 1) OperandGuest.castRef(source);
                    else if (test == 2) OperandGuest.castString(source);
                    else OperandGuest.castUnicode(source);
                    throw new Failure();
                } catch (ClassCastException e) { check(e,12+test,11); casts++; }
            }
        }
        int[] ints = new int[2];
        require(OperandGuest.castInt(ints) == ints); positives++;
        require(OperandGuest.castClone(ints) == ints); positives++;
        require(OperandGuest.castSerial(ints) == ints); positives++;
        require(OperandGuest.castString(null) == null); positives++;
        require(OperandGuest.castInt(null) == null); positives++;
        Tagged tagged = new Tagged(); require(OperandGuest.castTag(tagged) == tagged); positives++;
        String[] strings = new String[1]; require(OperandGuest.castObjectArray(strings) == strings); positives++;
        require(OperandGuest.multiI(0,2,2).length == 0); positives++;
        String[][][] partial = OperandGuest.multiA(1,2);
        require(partial.length == 1 && partial[0].length == 2 && partial[0][0] == null); positives++;
        float[] fs = new float[1]; OperandGuest.storeF(fs,0,0.0f/0.0f);
        float f = OperandGuest.loadF(fs,0); require(f != f); positives++;
        double[] ds = new double[1]; OperandGuest.storeD(ds,0,0.0/0.0);
        double d = OperandGuest.loadD(ds,0); require(d != d); positives++;
    }
    private static long arithmetic(int kind, int zero) {
        OperandGuest.before = 43;
        switch (kind) {
            case 0: return 7 / zero;
            case 1: return 7 % zero;
            case 2: return 7L / zero;
            default: return 7L % zero;
        }
    }
    public static void main(String[] args) {
        require(args.length == 34); messages = args;
        boundsCases(); negativeCases(); storeCases(); castCases();
        for (int kind=0; kind<4; kind++) {
            for (int repeat=0; repeat<2; repeat++) {
                reset();
                try { arithmetic(kind,0); throw new Failure(); }
                catch (ArithmeticException e) { check(e,16,43); arithmetic++; }
            }
        }
        require(bounds == 184 && negative == 42 && stores == 6 && casts == 8);
        require(arithmetic == 8 && checked == 248 && positives == 29);
        completed = 1;
    }
}
