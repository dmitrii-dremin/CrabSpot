package fixtures;
import java.lang.reflect.InvocationTargetException;
interface ReferenceCallContract {
    Object route(Object first, long wide, Object second, double precise, int pick);
    default Object defaultEcho(Object value) { return privateEcho(value); }
    private Object privateEcho(Object value) { return value; }
}
class ReferenceCallBase {
    int state;
    Object saved;
    Object special(Object value, long wide, double precise) {
        state = (int) wide + (int) precise + 17; saved = value; return value;
    }
}
class ReferenceCallChild extends ReferenceCallCases {
    @Override public Object route(Object first, long wide, Object second, double precise, int pick) {
        state = (int) wide + (int) precise + pick * 7 + 100;
        return pick == 0 ? second : pick == 1 ? first : null;
    }
}
public class ReferenceCallCases extends ReferenceCallBase implements ReferenceCallContract {
    Object input;
    Object alternate;
    @Override public Object route(Object first, long wide, Object second, double precise, int pick) {
        state = (int) wide + (int) precise + pick * 7;
        return pick == 0 ? first : pick == 1 ? second : null;
    }
    private Object make(int mode) {
        input = switch (mode) {
            case 0 -> null;
            case 1 -> this;
            case 2 -> new int[2];
            case 3 -> new Object[2];
            case 4 -> new int[1][2];
            default -> new ReferenceCallCases[2];
        };
        alternate = this; return input;
    }
    private int observe(Object value) {
        saved = value;
        return state * 31 + (value == null ? 1 : 0) + (value == input ? 2 : 0)
            + (value == this ? 4 : 0) + (value instanceof int[] ? 8 : 0)
            + (value instanceof Object[] ? 16 : 0);
    }
    private static Object choose(Object first, long wide, Object second, double precise, int pick) {
        return pick == 0 ? first : pick == 1 ? second : null;
    }
    public int virtualTrip(int mode, int pick, int value) {
        Object first = make(mode);
        return observe(route(first, (long) value * 4294967297L, this, value + 0.75, pick));
    }
    public int interfaceTrip(int mode, int pick, int value) {
        Object first = make(mode); ReferenceCallContract receiver = this;
        return observe(receiver.route(first, (long) value * 4294967297L, this, value + 0.75, pick));
    }
    public int staticTrip(int mode, int pick, int value) {
        Object first = make(mode); state = value;
        return observe(choose(first, (long) value * 4294967297L, this, value + 0.75, pick));
    }
    public int specialTrip(int mode, int value) {
        Object first = make(mode);
        return observe(super.special(first, (long) value * 4294967297L, value + 0.75));
    }
    public int defaultTrip(int mode) { return observe(ReferenceCallContract.super.defaultEcho(make(mode))); }
    private ReferenceCallCases self(ReferenceCallCases value) { return value; }
    private ReferenceCallContract contract(ReferenceCallContract value) { return value; }
    private Object[] objects(Object[] value) { return value; }
    private int[][] integers(int[][] value) { return value; }
    public int typedTrip(int mode) {
        state = mode;
        if (mode == 0) { input = null; return observe(self(null)); }
        if (mode == 1) { input = this; return observe(contract(self(this))); }
        if (mode == 2) { input = new ReferenceCallCases[2]; return observe(objects((ReferenceCallCases[]) input)); }
        input = new int[1][2]; return observe(integers((int[][]) input));
    }
    private Object recurse(int depth, Object first, Object second) {
        if (depth <= 0) return first;
        return recurse(depth - 1, second, first);
    }
    public int recursiveTrip(int mode, int depth) {
        Object first = make(mode); state = depth; return observe(recurse(depth, first, this));
    }
    private Object store(Object[] target, Object value, int index) {
        saved = value; target[index] = value; return target[index];
    }
    public int storeTrip(int mode, int index) {
        Object[] target = mode == 0 ? new Object[2] : mode == 1 ? new ReferenceCallCases[2] : new int[2][];
        input = target; alternate = this; state = mode;
        return observe(store(target, this, index));
    }
    public int cycleTrip(int index) {
        Object[] target = new Object[2]; input = target; alternate = target; state = index;
        return observe(store(target, target, index));
    }
    public int nullTrip(int mode) {
        Object first = make(mode); saved = first; state = mode;
        ReferenceCallContract absent = null;
        return observe(absent.route(first, 1L, this, 2.0, 0));
    }
    private static String value(Object value, boolean result) {
        if (value == null) return "V";
        if (value instanceof Integer integer) return "I:" + integer;
        throw new IllegalArgumentException("unexpected oracle value");
    }
    private static void emit(String kind, String method, String descriptor, Object... inputs) throws Exception {
        Class<?>[] types = new Class<?>[inputs.length];
        var arguments = new StringBuilder();
        for (int index = 0; index < inputs.length; index++) {
            types[index] = int.class;
            if (index != 0) arguments.append(','); arguments.append(value(inputs[index], false));
        }
        String result;
        try {
            result = value(ReferenceCallCases.class.getMethod(method, types).invoke(
                kind.equals("root") ? new ReferenceCallCases() : new ReferenceCallChild(), inputs), true);
        } catch (InvocationTargetException error) {
            result = "E:" + error.getCause().getClass().getName().replace('.', '/');
        }
        System.out.println(kind + "/" + method + "\t" + descriptor + "\t" + arguments + "\t" + result);
    }
    public static void main(String[] ignored) throws Exception {
        for (String receiver : new String[] {"root", "child"}) {
            for (int mode = 0; mode < 6; mode++) {
                emit(receiver, "defaultTrip", "(I)I", mode);
                emit(receiver, "nullTrip", "(I)I", mode);
                for (int depth : new int[] {-1, 0, 1, 2, 7}) emit(receiver, "recursiveTrip", "(II)I", mode, depth);
                for (int value : new int[] {Integer.MIN_VALUE, -1, 0, 1, Integer.MAX_VALUE}) {
                    emit(receiver, "specialTrip", "(II)I", mode, value);
                    for (int pick : new int[] {-1, 0, 1, 2}) for (String method : new String[] {"virtualTrip", "interfaceTrip", "staticTrip"})
                        emit(receiver, method, "(III)I", mode, pick, value);
                }
            }
            for (int mode = 0; mode < 4; mode++) emit(receiver, "typedTrip", "(I)I", mode);
            for (int index : new int[] {-1, 0, 1, 2}) {
                emit(receiver, "cycleTrip", "(I)I", index);
                for (int mode = 0; mode < 3; mode++) emit(receiver, "storeTrip", "(II)I", mode, index);
            }
        }
    }
}
