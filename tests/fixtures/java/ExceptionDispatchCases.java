package fixtures;
import java.lang.reflect.InvocationTargetException;
class DispatchThrownBase extends Exception {}
class DispatchThrownChild extends DispatchThrownBase {}
class DispatchThrownOther extends Exception {}
interface DispatchThrowsContract {
    int call(Throwable value) throws Throwable;
    default int defaultThrow(Throwable value) throws Throwable { throw value; }
}
class DispatchCallBase {
    int baseThrow(Throwable value) throws Throwable { throw value; }
}
public class ExceptionDispatchCases extends DispatchCallBase implements DispatchThrowsContract {
    int state;
    Throwable first;
    Throwable second;
    Throwable saved;
    private Throwable pick(int choice) { return choice == 0 ? first : second; }
    private int observe() {
        return state * 31 + (saved == first ? 1 : 0) + (saved == second ? 2 : 0)
            + (saved instanceof DispatchThrownChild ? 4 : 0)
            + (saved instanceof DispatchThrownBase ? 8 : 0);
    }
    private void raise(Throwable value) throws Throwable { state = state * 10 + 1; throw value; }
    private void pass(int depth, Throwable value) throws Throwable {
        state++;
        if (depth <= 0) raise(value);
        pass(depth - 1, value);
        state = -999;
    }
    public int localOrder(int choice) throws Throwable {
        state = 2;
        try { throw pick(choice); }
        catch (DispatchThrownChild caught) { saved = caught; state = 31; }
        catch (DispatchThrownBase caught) { saved = caught; state = 41; }
        catch (Throwable caught) { saved = caught; state = 51; }
        return observe();
    }
    public int crossFrame(int choice, int depth) throws Throwable {
        state = 2;
        try { pass(depth, pick(choice)); }
        catch (DispatchThrownBase caught) { saved = caught; state = state * 3 + 1; }
        catch (Throwable caught) { saved = caught; state = state * 5 + 2; }
        return observe();
    }
    public int nestedFinally(int choice, int depth) throws Throwable {
        state = 1;
        try {
            try { pass(depth, pick(choice)); }
            finally { state = state * 3 + 2; }
        } catch (DispatchThrownBase caught) { saved = caught; state = state * 5 + 3; }
          catch (Throwable caught) { saved = caught; state = state * 7 + 4; }
        return observe();
    }
    public int rethrow(int choice) throws Throwable {
        try {
            try { throw pick(choice); }
            catch (Throwable caught) { saved = caught; state = 7; throw caught; }
        } catch (DispatchThrownChild caught) { state = 11; }
          catch (DispatchThrownBase caught) { state = 13; }
          catch (Throwable caught) { state = 17; }
        return observe();
    }
    public int replacement(int choice) throws Throwable {
        try {
            try { throw pick(choice); }
            catch (Throwable caught) { saved = caught; state = 7; throw second; }
        } catch (Throwable caught) { saved = caught; state += 11; }
        return observe();
    }
    public int finallyOverride(int choice) throws Throwable {
        saved = pick(choice);
        try { throw saved; }
        finally { state = 19; throw second; }
    }
    public int uncaught(int choice, int depth) throws Throwable {
        state = 0; pass(depth, pick(choice)); return -1;
    }
    private int quiet() throws DispatchThrownBase { return 1; }
    public int outsideRange(int choice) throws Throwable {
        state = 23;
        try { state += quiet(); }
        catch (DispatchThrownBase caught) { saved = caught; state = -1; }
        raise(pick(choice)); return -1;
    }
    private int hop(int choice) throws Throwable { raise(pick(choice)); return -1; }
    public int callSite(int choice) throws Throwable {
        state = 1;
        try { return hop(choice); }
        catch (DispatchThrownOther caught) { saved = caught; state = 23; return observe(); }
        catch (Throwable caught) { saved = caught; state = 29; return observe(); }
    }
    private int after(Throwable caught) { state = 29; return caught == saved ? observe() : -1; }
    public int handlerCalls(int choice) throws Throwable {
        try { throw pick(choice); }
        catch (Throwable caught) { saved = caught; state = 5; return after(caught); }
    }
    public int handlerThrows(int choice) throws Throwable {
        try { throw pick(choice); }
        catch (DispatchThrownBase caught) { saved = caught; state = 31; throw second; }
    }
    @Override public int call(Throwable value) throws Throwable { throw value; }
    private static int staticThrow(Throwable value) throws Throwable { throw value; }
    private int dispatch(int form, Throwable value) throws Throwable {
        return switch (form) {
            case 0 -> call(value);
            case 1 -> ((DispatchThrowsContract) this).call(value);
            case 2 -> super.baseThrow(value);
            case 3 -> staticThrow(value);
            default -> DispatchThrowsContract.super.defaultThrow(value);
        };
    }
    public int allCalls(int choice, int form) throws Throwable {
        try { return dispatch(form, pick(choice)); }
        catch (Throwable caught) { saved = caught; state = form; return observe(); }
    }
    private static void emit(String kind, String method, String descriptor, Object... inputs) throws Exception {
        Class<?>[] types = new Class<?>[inputs.length];
        var arguments = new StringBuilder();
        for (int index = 0; index < inputs.length; index++) {
            types[index] = int.class;
            if (index != 0) arguments.append(','); arguments.append("I:").append(inputs[index]);
        }
        var receiver = new ExceptionDispatchCases();
        receiver.first = switch (kind) {
            case "base" -> new DispatchThrownBase();
            case "child" -> new DispatchThrownChild();
            default -> new DispatchThrownOther();
        };
        receiver.second = new DispatchThrownOther();
        String result;
        try {
            result = "I:" + ExceptionDispatchCases.class.getMethod(method, types).invoke(receiver, inputs);
        } catch (InvocationTargetException error) {
            Throwable thrown = error.getCause();
            if (thrown == receiver.first) result = "T:first";
            else if (thrown == receiver.second) result = "T:second";
            else throw new IllegalStateException("unexpected exception identity", thrown);
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String kind : new String[] {"base", "child", "other"}) {
            for (int choice : new int[] {-1, 0, 1, Integer.MAX_VALUE}) {
                for (String method : new String[] {"localOrder", "rethrow", "replacement", "finallyOverride", "outsideRange", "callSite", "handlerCalls", "handlerThrows"})
                    emit(kind, method, "(I)I", choice);
                for (int depth : new int[] {-1, 0, 1, 2, 7}) for (String method : new String[] {"crossFrame", "nestedFinally", "uncaught"})
                    emit(kind, method, "(II)I", choice, depth);
                for (int form = 0; form < 5; form++) emit(kind, "allCalls", "(II)I", choice, form);
            }
        }
    }
}
