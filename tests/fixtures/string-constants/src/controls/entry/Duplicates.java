public class Duplicates {
    public static void main(String[] args) {
        duplicates.Entry.main(args);
        String value = duplicates.Entry.first;
        System.out.print(duplicates.Entry.phase + "/" + (value == duplicates.Entry.second) + "/" +
            value.length() + "/" + (int)value.charAt(2));
    }
}
