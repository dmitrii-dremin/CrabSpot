import java.io.InputStream;
import java.security.MessageDigest;
import java.util.HexFormat;
public final class PlatformIdentity {
 public static void main(String[] args) throws Exception {
  for(String name:new String[]{"java/lang/Throwable","java/lang/Exception","java/lang/RuntimeException","java/lang/ArithmeticException","java/lang/StackTraceElement","java/util/Collections","java/util/Collections$EmptySet","java/util/Collections$EmptyList","java/util/Collections$EmptyMap"}) {
   try(InputStream s=ClassLoader.getSystemResourceAsStream(name+".class")) {
    System.out.println(name+"="+HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(s.readAllBytes())));
   }
  }
  for(String key:new String[]{"java.version","java.vendor","user.language","user.country","file.encoding","native.encoding"})System.out.println("property:"+key+"="+System.getProperty(key,"<absent>"));
 }
}
