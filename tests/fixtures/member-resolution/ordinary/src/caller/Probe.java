package caller;
import symbolic.Sub;
import value.K;
public final class Probe {
    public static int entered;
    public static int returned;
    public static int read() {
        entered++;
        int answer = Sub.slot == null ? 11 : 12;
        returned++;
        return answer;
    }
    public static int write() {
        entered++;
        Sub.slot = null;
        returned++;
        return 22;
    }
    public static int call() {
        entered++;
        K result = Sub.echo(null);
        returned++;
        return result == null ? 33 : 34;
    }
    public static int combined() {
        entered++;
        K token = new K();
        Sub.slot = token;
        K out = Sub.echo(Sub.slot);
        returned++;
        return out == token ? 44 : 45;
    }
}
