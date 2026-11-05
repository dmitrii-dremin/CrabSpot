package caller;
import symbolic.Sub;
import value.K;
import api.Port;
public final class Dispatch {
    public static Sub held;
    public static int entered;
    public static int returned;
    static int virtual(Sub target,K token){ return target.hit(token); }
    static int itf(Port target,K token){ return target.hit(token); }
    public static int run(int receiver,int call) {
        entered++;
        Sub target;
        if(receiver==1) target=new dyn.First();
        else if(receiver==2) target=new dyn.Second();
        else target=new dyn.Plain();
        held=target;
        K token=new K();
        int result=call==0 ? virtual(target,token) : itf(target,token);
        returned++;
        return target.slot==token ? result : -1;
    }
}
