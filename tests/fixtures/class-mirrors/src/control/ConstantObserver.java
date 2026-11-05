import java.lang.reflect.*;
public class ConstantObserver {
    public static void main(String[] args) {
        String stage="load";
        try {
            Class<?> caller=Class.forName("q.Caller",false,ConstantObserver.class.getClassLoader());
            stage="lookup";
            Method run=caller.getDeclaredMethod("run");
            stage="invoke";
            Class<?> result=(Class<?>)run.invoke(null);
            System.out.print(stage+" "+result.getName()+" "+result.isPrimitive()+" "+result.isArray());
        } catch (Throwable t) {
            Throwable cause=t instanceof InvocationTargetException ? t.getCause() : t;
            System.out.print(stage+" "+cause.getClass().getSimpleName());
        }
    }
}
