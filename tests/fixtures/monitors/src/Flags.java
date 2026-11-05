public class Flags {

    public static void main(String[] args) {
        String phase = "load";
        try {
            Class<?> c = Class.forName("locks.Flags", false, Flags.class.getClassLoader());
            phase = "initialize";
            Class.forName("locks.Flags", true, Flags.class.getClassLoader());
            System.out.print(c.getField("init").getInt(null) + "/" + c.getField("call").getInt(null) + "/" + Thread.holdsLock(c));
        } catch (Throwable x) {
            System.out.print(phase + " " + x.getClass().getSimpleName());
        }
    }
}
