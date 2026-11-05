import java.lang.reflect.Field;
import java.util.Comparator;
public class Layout {
    static Field field(String name) throws ReflectiveOperationException {
        Field f = String.class.getDeclaredField(name); f.setAccessible(true); return f;
    }
    public static void main(String[] argv) throws ReflectiveOperationException {
        String[] cases = {"", "A\000\u00ff", "A\u20ac\ud83d\ude80", "\udfff", "\ud83d\ude80"};
        Field value = field("value"), coder = field("coder");
        System.out.println("compact=" + field("COMPACT_STRINGS").getBoolean(null));
        for (String s : cases) {
            byte[] bytes = (byte[])value.get(s);
            System.out.print(s.length() + "/" + coder.getByte(s) + "/" + bytes.length + "/");
            for(byte b : bytes) System.out.printf("%02x", b & 0xff);
            System.out.println();
        }
        Object fields = field("serialPersistentFields").get(null);
        Comparator<String> comparator = String.CASE_INSENSITIVE_ORDER;
        Class<?> actual = comparator.getClass();
        System.out.println(fields.getClass().getName() + "/" + java.lang.reflect.Array.getLength(fields));
        System.out.println(actual.getName() + "/" + (actual.getNestHost() == String.class)
            + "/" + actual.isNestmateOf(String.class) + "/" + (comparator.compare("a", "A") == 0));
    }
}
