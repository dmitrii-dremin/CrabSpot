import probe.Compare;
import query.Query;

public class ObserveEquality {
    static String text(int count, int mismatch) {
        char[] units = new char[count];
        for (int i = 0; i < count; i++) units[i] = (i == mismatch) ? 'x' : 'q';
        return new String(units);
    }
    public static void main(String[] mode) {
        if (mode[0].equals("query")) {
            Query.main(new String[Integer.parseInt(mode[1])]);
            System.out.println(Query.phase + "/" + Query.enabled + "/" + Query.initialized);
            return;
        }
        String[] pair = switch (mode[0]) {
            case "alias" -> new String[]{text(128, -1)};
            case "equal" -> new String[]{text(128, -1), text(128, -1)};
            case "first" -> new String[]{text(128, -1), text(128, 0)};
            case "last" -> new String[]{text(128, -1), text(128, 127)};
            case "cross-coder" -> new String[]{new String(new char[]{0xfe}), new String(new char[]{0xdfff})};
            default -> throw new IllegalArgumentException(mode[0]);
        };
        Compare.main(pair);
        System.out.println(Compare.phase + "/" + Compare.same);
    }
}
