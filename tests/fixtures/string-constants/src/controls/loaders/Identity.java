public class Identity {
    public static void main(String[] args) {
        String prefix = java.util.UUID.randomUUID().toString();
        for (String tail : new String[] {"abc", "\000\u20ac", "\udfff", "\ud83d\ude00"}) {
            char[] input = (prefix + tail).toCharArray();
            String candidate = new String(input);
            String canonical = candidate.intern();
            String equal = new String(input);
            System.out.println((candidate == canonical) + "/" + candidate.equals(canonical) + "/" +
                (equal.intern() == canonical) + "/" + (canonical.intern() == canonical) + "/" +
                (canonical.length() == candidate.length()));
        }
    }
}
