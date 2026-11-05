public class Fields {
    public static void main(String[] args) throws Exception {
        if (args[0].equals("constants")) {
            constants.Entry.main(new String[0]);
            System.out.print(constants.Entry.class.getField("NUMBER").getInt(null) + "/" +
                constants.Entry.entered + "/" + constants.Entry.completed + "/" + constants.Marks.order);
        } else {
            String[] inputs = args[0].equals("fresh")
                ? new String[] {new String("same"), new String("same")}
                : new String[] {new String("same"), new String("same"), ""};
            locks.Strings.main(inputs);
            System.out.print(locks.Strings.phase);
        }
    }
}
