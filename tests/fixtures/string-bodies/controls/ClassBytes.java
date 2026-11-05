import java.nio.file.*;
import java.util.HexFormat;
import java.security.MessageDigest;
public class ClassBytes {
    public static void main(String[] argv) throws Exception {
        for (String cls : new String[]{"java/lang/String","java/lang/String$CaseInsensitiveComparator","java/io/ObjectStreamField","java/util/Comparator","java/lang/Object"}) {
            byte[] bytes = Files.readAllBytes(Path.of(java.net.URI.create("jrt:/java.base/" + cls + ".class")));
            Path out = Path.of(argv[0], cls + ".class");
            Files.createDirectories(out.getParent()); Files.write(out, bytes);
            System.out.println(cls + " " + bytes.length + " " + HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes)));
        }
    }
}
