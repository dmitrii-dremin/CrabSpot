# Java 21 monitor observations

The original application uses a synchronized static main, an inherited static synchronized helper, a dynamically selected synchronized instance method, and synchronized blocks over an object and two kinds of array. Zulu HotSpot 21.0.7 and OpenJ9 21.0.13.0 both returned with entries/calls/completions/sum equal to `1/2/1/9`. The external observer found all six visible locks released. A second application entered main, attempted to synchronize on null and raised NullPointerException before its body counter advanced. [Application sources](../../tests/fixtures/monitors/src/locks) and [separate observer](../../tests/fixtures/monitors/src/Observe.java).

## What the reference programs measure

Each VM ran 62 separate processes on the archived inputs and another 62 on regenerated inputs. The seven application classes come from javac. The 45 original synthetic encodings exercise states such as exiting a monitor acquired in another frame. One intentionally malformed class declares a synchronized constructor. It is archived as `.class.bin` so the mutation fuzzer receives only valid class-file seeds. The reference tool restores the exact bytes under the original binary name for JVM loading. The remaining Java sources supply two independent class-loader definitions and observer programs. Observer code runs in a separate directory from the application. [Input and compiler digests](../../tests/fixtures/monitors/manifest.json) and [reference runner](../../tools/monitor_reference.py).

| Processes per VM and input set | Probe |
| --- | --- |
| 17 | Monitor instructions, unfinished references, ownership and nested frames |
| 14 | The six typed return opcodes with balanced and outstanding acquisitions, plus local catch/return and catch/rethrow |
| 3 | Ordinary and flagged class initializers, and a flagged constructor |
| 1 | Actual static method declarers across two defining loaders |
| 5 | Direct application launches, retained fields/hold bits, and class-versus-receiver controls |
| 22 | Eleven raw Java main methods, each launched directly and from an observation process |

Both VMs accepted crossed release order for two different monitors. Both allowed the actual object behind a still-uninitialized reference to identify a monitor. In a same-frame exception handler, the lock was still held and the handler could release it before returning or rethrowing the original supplied object. On static method invocation inherited by a subclass, only the actual declaring class's monitor was held. With two independently defining loaders, the observed A.Base/A.Child/B.Base/B.Child bits were `0000/1000/1010/1010/1010/1000/0000`. [HotSpot observations](../../tests/fixtures/monitors/hotspot.json) and [OpenJ9 observations](../../tests/fixtures/monitors/openj9.json).

## Structured locking

The specification permits enforcement of both per-invocation structured-locking rules. CrabSpot chose to enforce both. Each explicit exit must match an explicit acquisition by that invocation on that object. A synchronized method's implicit entry/exit pair belongs to the calling invocation. Different VMs can therefore produce different results on programs that deliberately violate balance. [JVMS §2.11.10](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-2.html#jvms-2.11.10) and [CrabSpot contract](m2-single-thread-monitors.md).

| Program | HotSpot | OpenJ9 | CrabSpot policy |
| --- | --- | --- | --- |
| Balanced return, every return type | Returns original value, lock released | Same | Preserve result and release |
| Unbalanced return, every return type | IllegalMonitorStateException, lock released | Returns original value, lock held | Fail at the return instruction |
| Supplied throw escapes with outstanding explicit lock | Replaced by IllegalMonitorStateException, lock released | Original throwable escapes, lock held | Fail when the throw escapes the frame |
| Callee explicitly exits caller's lock | Fails in callee before the next marker | Reaches the callee marker before the caller fails | Fail at the callee's exit |
| Synchronized instance method explicitly exits its implicit lock | Reaches the next marker, then fails on return | Same | Fail immediately at the explicit exit |

The public CrabSpot checker compares the failure location, type, caller location when present and prior Java field writes. Its synchronized application requires 46 monitor operations. A ceiling of 45 reaches the final return in main with all four fields already updated. A zero ceiling fails at root acquisition, while a zero-ceiling null operand in an ordinary main still reports NullPointerException. This profile reports VM-produced NPE and IMSE as unmaterialized exceptions. The observations cover one thread. [Physical launcher and API check](../../tools/verify_monitor_entry.py).
