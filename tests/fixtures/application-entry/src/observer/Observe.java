package observer;
import app.Marks;
public class Observe {
  public static void main(String[] options) {
    String stage = "resolve-entry", outcome = "returned";
    try {
      Class<?> entry = Class.forName("app." + options[0], false, Observe.class.getClassLoader());
      var main = entry.getMethod("main", String[].class);
      stage = "invoke";
      for (int i = 0; i < Integer.parseInt(options[1]); i++)
        main.invoke(null, (Object) new String[0]);
    } catch (Throwable e) {
      Throwable cause = e instanceof java.lang.reflect.InvocationTargetException ? e.getCause() : e;
      outcome = cause.getClass().getSimpleName();
    }
    System.out.println(stage + " " + outcome + " " + Marks.entered + "/" + Marks.completed + "/"
        + Marks.total + "/" + Marks.initialized + "/" + Marks.slots);
  }
}
