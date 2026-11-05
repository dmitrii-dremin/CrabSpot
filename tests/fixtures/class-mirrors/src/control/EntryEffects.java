import java.lang.reflect.InvocationTargetException;

public class EntryEffects {
    public static void main(String[] args) throws Exception {
        Class<?> entry = Class.forName("probe.Entry");
        String outcome = "returned";
        try {
            entry.getMethod("main", String[].class).invoke(null, (Object)new String[0]);
        } catch (InvocationTargetException failure) {
            outcome = failure.getCause().getClass().getSimpleName();
        } catch (LinkageError failure) {
            outcome = failure.getClass().getSimpleName();
        }
        System.out.print(outcome + " " + entry.getField("phase").getInt(null));
    }
}
