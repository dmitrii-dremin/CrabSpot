import java.security.DigestInputStream;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.io.OutputStream;

public class PlatformIdentity {
    public static void main(String[] args) throws Exception {
        for (String name : new String[]{"String", "StringLatin1", "Class", "ClassLoader"}) {
            try (var stream = Object.class.getResourceAsStream("/java/lang/" + name + ".class")) {
                if (stream == null) {
                    System.out.println(name + "=missing");
                } else {
                    var sha = MessageDigest.getInstance("SHA-256");
                    try (var in = new DigestInputStream(stream, sha)) {
                        in.transferTo(OutputStream.nullOutputStream());
                    }
                    System.out.println(name + "=" + HexFormat.of().formatHex(sha.digest()));
                }
            }
        }
    }
}
