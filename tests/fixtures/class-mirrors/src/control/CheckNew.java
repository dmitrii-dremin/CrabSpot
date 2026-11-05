import java.lang.reflect.*;
public class CheckNew {
    public static void main(String[] args) throws Exception {
        Class<?> c=Class.forName("q.NewClass");
        String stage="lookup", outcome="";
        try {
            Method m=c.getMethod("run");
            stage="invoke";
            outcome="returned:"+m.invoke(null);
        } catch (Throwable x) {
            Throwable cause=x instanceof InvocationTargetException?x.getCause():x;
            outcome=cause.getClass().getName();
        }
        System.out.print(stage+" "+outcome+" phase="+c.getField("phase").getInt(null));
    }
}
