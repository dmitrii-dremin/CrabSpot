public class Observe {
    public static void main(String[] argv) throws Exception {
        String phase="load";
        try {
            Class<?> k=Class.forName("sample.Value",false,Observe.class.getClassLoader());
            phase="lookup";
            var m=k.getDeclaredMethod("value");
            phase="invoke";
            String s=(String)m.invoke(null);
            int hash=0;
            for (int i=0;i<s.length();i++) hash=31*hash+s.charAt(i);
            System.out.print(s.length()+"/"+hash+"/"+(s==s.intern()));
        } catch (Throwable x) {
            var e=x instanceof java.lang.reflect.InvocationTargetException ? x.getCause() : x;
            System.out.print(phase+"/"+e.getClass().getSimpleName());
        }
    }
}