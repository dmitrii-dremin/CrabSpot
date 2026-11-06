public final class TraceFilterObserver {
 static Throwable inner;
 static final class OuterFailure extends RuntimeException {
  OuterFailure() { super("outer"); inner = new RuntimeException("inner"); }
 }
 static final class PlainOuter {
  PlainOuter() { inner = new RuntimeException("inner-plain"); }
 }
 static final class DelegatingOverride extends RuntimeException {
  int calls;
  boolean held;
  @Override public synchronized Throwable fillInStackTrace() {
   calls++; held = Thread.holdsLock(this); return super.fillInStackTrace();
  }
 }
 static final class BypassingOverride extends RuntimeException {
  int calls;
  @Override public Throwable fillInStackTrace() { calls++; return this; }
 }
 static final class NonWritable extends RuntimeException {
  NonWritable() { super("off", null, true, false); }
 }
 static final class CovariantOverride extends RuntimeException {
  int calls;
  @Override public synchronized CovariantOverride fillInStackTrace() {
   calls++; super.fillInStackTrace(); return this;
  }
 }
 static final class OverloadFlow extends RuntimeException {
  int overrides, helpers;
  @Override public Throwable fillInStackTrace() { overrides++; return fillInStackTrace(0); }
  public synchronized Throwable fillInStackTrace(int ignored) { helpers++; return super.fillInStackTrace(); }
 }
 static String lines(Throwable t) {
  StringBuilder b=new StringBuilder();
  for(StackTraceElement e:t.getStackTrace()) {
   if(b.length()!=0)b.append(',');
   b.append(e.getClassName()).append('#').append(e.getMethodName()).append('#').append(e.getLineNumber());
  }
  return b.toString();
 }
 static String trace(Throwable t) {
  StringBuilder b = new StringBuilder();
  for (StackTraceElement e : t.getStackTrace()) {
   if (b.length() != 0) b.append(',');
   b.append(e.getClassName()).append('#').append(e.getMethodName());
  }
  return b.toString();
 }
 public static void main(String[] args) {
  OuterFailure outer = new OuterFailure();
  System.out.println("unrelated-throwable-constructor\tinner="+trace(inner)+"\touter="+trace(outer));
  new PlainOuter();
  System.out.println("unrelated-ordinary-constructor\tinner="+trace(inner));
  DelegatingOverride delegated = new DelegatingOverride();
  System.out.println("delegate-override\ttrace="+trace(delegated)+"\tcalls="+delegated.calls+"\theldInside="+delegated.held+"\theldAfter="+Thread.holdsLock(delegated));
  BypassingOverride bypass = new BypassingOverride();
  System.out.println("bypass-override\ttrace="+trace(bypass)+"\tcalls="+bypass.calls);
  NonWritable off = new NonWritable();
  System.out.println("non-writable\ttrace="+trace(off)+"\tsame="+(off.fillInStackTrace()==off)+"\ttraceAfter="+trace(off));
  CovariantOverride covariance=new CovariantOverride();
  System.out.println("covariant-override\ttrace="+trace(covariance)+"\tlines="+lines(covariance)+"\tcalls="+covariance.calls);
  OverloadFlow overload=new OverloadFlow();
  System.out.println("overload-flow\ttrace="+trace(overload)+"\tlines="+lines(overload)+"\toverrides="+overload.overrides+"\thelpers="+overload.helpers);
 }
}
