# Class objects on two Java 21 VMs

The reference programs exercise class literals, the native `Object.getClass()`, class-object storage, and synchronization on a class object. The product target and the bounds on its first implementation are in the [class-object contract](m2-class-mirrors.md).

## Reproduce the observations

Set `JAVA_HOME` to a Java 21 HotSpot JDK. The second command also takes the Java executable from the project's pinned OpenJ9 runtime.

```text
python -m unittest discover -s tools -p test_class_mirror_reference.py
python tools/class_mirror_reference.py --check --compare-openj9 PATH_TO_OPENJ9_21_JAVA
```

The nine archived application classes came from three Java 21 programs. A simple application stores 1 when two literals for its own class compare equal. Its classfile contains no symbolic spelling of `java/lang/Class`. A five-class application produces flags 127, initializer count 3, and call count 1. Merely evaluating a class literal or creating an array leaves its component uninitialized. The program later explicitly constructs a child and takes the same lock through `Base.class` and an inherited synchronized static method. A third program produces flags 63 with initializer count 0 after storing mirrors in static fields, instance fields and arrays, passing them through Java methods and locking through stored aliases.

The runner executes each of 78 observations against archived inputs and independently regenerated inputs. The Java observer programs run outside the application being tested. On the original compiler the runner requires identical class bytes. A different Java 21 compiler must reproduce the class inventory and observable results. Archived bytes, source bytes, helper identities and encoded probes have separate SHA-256 checks. [Fixture manifest](../../tests/fixtures/class-mirrors/manifest.json), [HotSpot results](../../tests/fixtures/class-mirrors/hotspot.json) and [OpenJ9 results](../../tests/fixtures/class-mirrors/openj9.json).

## Cases that distinguish implementations

| Case | Reference result |
| --- | --- |
| `[I` and `[LI;` | The first denotes an array of primitive int. The second resolves an ordinary class named `I`. Removing that class from the search path causes the same bytes to throw `NoClassDefFoundError`. A plain Class constant named `V` can also resolve an ordinary class. |
| Different defining loaders | Two loaders defining the same bytes get distinct class and array mirrors. The mirrors' own runtime class is shared. |
| Failed resolution followed by a late definition | Both VMs retain the first missing-class failure at the original constant-pool site. An independently defined caller resolves the newly available class. |
| Array or interface symbolic owner | An actual int array reaches the final Object native through an array Methodref and through either array marker interface. A `Thing[]` receiver called through an `Object[]` owner returns the mirror for `Thing[]`. |
| Unrelated interface receiver | Passing an int array directly to an unrelated interface call fails verification. Passing the same array through an `Object` parameter passes verification and throws `IncompatibleClassChangeError` at invocation. |
| Native name collision | A Java `getClass(int)` overload executes its own body. An unbound native overload throws `UnsatisfiedLinkError` on the reference VMs. |
| Raw `new java/lang/Class` | HotSpot throws `IllegalAccessError` before a second phase write in the discarded and unfinished-lock cases. OpenJ9 returns after that write. Both reject the attempted inaccessible constructor and permit a genuine literal and a Class array. |

Each raw launch driver writes phase 1 before invoking the original method. It writes phase 2 only when a successful method returns the expected mirror or integer. The phase separates a correct result from a method that simply returns. The closed CrabSpot image separately checks whether an application can directly name a bootstrap type. For example, proving `GenericDeclaration` as an ancestor of `Class` gives the VM layout and verification facts. Direct application access still requires an allowed import.
