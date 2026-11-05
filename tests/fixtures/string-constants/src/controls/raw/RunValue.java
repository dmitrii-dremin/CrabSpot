import java.lang.reflect.InvocationTargetException;

public class RunValue {
    public static void main(String[] args) throws Exception {
        char[] units = new char[Integer.parseInt(args[0])];
        for (int i = 0; i < units.length; i++)
            units[i] = (char)Integer.parseInt(args[1 + i % (args.length - 1)], 16);
        Class<?> entry = Class.forName("sample.Entry");
        String outcome = "returned";
        try {
            entry.getMethod("main", String[].class).invoke(null, (Object)new String[] {new String(units)});
        } catch (InvocationTargetException failure) {
            outcome = failure.getCause().getClass().getSimpleName();
        }
        System.out.print(outcome + "/" + entry.getField("phase").getInt(null));
    }
}
