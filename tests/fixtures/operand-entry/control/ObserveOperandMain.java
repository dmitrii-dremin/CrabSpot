import app.OperandMain;
import app.OperandUncaught;
import operand.OperandGuest;

/** External reference observer; never supplied as a CrabSpot product class. */
public final class ObserveOperandMain {
    private static String decode(String token) {
        if (!token.startsWith("T") || (token.length()-1) % 4 != 0)
            throw new IllegalArgumentException("invalid expected UTF16 argument");
        char[] units = new char[(token.length()-1)/4];
        for (int i=0; i<units.length; i++) {
            int value = 0;
            for (int j=0; j<4; j++) {
                char unit = token.charAt(1+i*4+j);
                int digit = unit >= '0' && unit <= '9' ? unit-'0'
                    : unit >= 'a' && unit <= 'f' ? unit-'a'+10 : -1;
                if (digit < 0) throw new IllegalArgumentException("invalid expected UTF16 unit");
                value = value*16+digit;
            }
            units[i] = (char)value;
        }
        return new String(units);
    }
    public static void main(String[] args) {
        if (args.length == 2 && "uncaught".equals(args[0])) {
            try { OperandUncaught.main(new String[]{args[1]}); throw new AssertionError("expected fault"); }
            catch (RuntimeException value) {
                StackTraceElement head = value.getStackTrace()[0];
                System.out.println(value.getClass().getName() + "/" + head.getClassName() + "." + head.getMethodName()
                    + "/" + OperandUncaught.before + "/" + OperandUncaught.after
                    + "/" + OperandGuest.before + "/" + OperandGuest.after);
            }
            return;
        }
        if (args.length == 2 && "decode".equals(args[0])) {
            if ("N".equals(args[1])) System.out.println("null");
            else {
                String value = decode(args[1]);
                for (int i=0; i<value.length(); i++) System.out.printf("%04x",(int)value.charAt(i));
                System.out.println();
            }
            return;
        }
        if (args.length != 18) throw new IllegalArgumentException("expected 17 message arguments");
        // ASCII UTF16 encoding avoids the reference Java launcher's Windows
        // best-fit conversion. The product guest still receives actual Strings.
        String[] expected = new String[34];
        for (int i=0; i<17; i++) {
            boolean absent = "N".equals(args[i+1]);
            expected[i*2] = absent ? "null" : "text";
            expected[i*2+1] = absent ? "" : decode(args[i+1]);
        }
        switch (args[0]) {
            case "wrong-bounds": expected[1] = "wrong bounds message"; break;
            case "wrong-store": expected[18] = "text"; expected[19] = "wrong store message"; break;
            case "wrong-cast": expected[25] = "wrong cast message"; break;
            case "wrong-arithmetic": expected[33] = "wrong arithmetic message"; break;
            case "normal": break;
            default: throw new IllegalArgumentException("unknown observation case");
        }
        String result;
        try { OperandMain.main(expected); result = "returned"; }
        catch (OperandMain.Failure expectedFailure) { result = "oracle-failed"; }
        System.out.println(result + "/" + OperandMain.bounds + "/" + OperandMain.negative
            + "/" + OperandMain.stores + "/" + OperandMain.casts + "/" + OperandMain.arithmetic
            + "/" + OperandMain.checked + "/" + OperandMain.positives + "/" + OperandMain.completed);
    }
}
