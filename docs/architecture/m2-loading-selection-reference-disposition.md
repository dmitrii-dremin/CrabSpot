# Preparation selection and the reference JDK

**Director decision, 2026-11-05.** Preserve the frozen SE 21 preparation rules. The reference JDK exhibits a stage difference for five specific class/interface graphs. Keep the original observations and independent specification assertions as separate evidence. This decision changes no production API or contract.

## Selected declarations

All cases use the same name and descriptor, `take(Lvalue/K;)V`. Inspection of the original classfiles confirmed that iface.Service declares it public abstract. Loader B defines the prepared child. Its loader routes the actual interface to C and, in the inherited class cases, the parent to A. Each domain can define distinct bytes-identical value.K classes. The driver asks Class.forName for each K under the named loader and observes the actual defining loader. Every scenario creates fresh loaders.

| Prepared shape | SE 21 producer and required agreement |
| --- | --- |
| B's class inherits A's public abstract instance method | Rule 2 selects A's nearest class-chain declaration for C's public interface method. A.K and C.K must agree. Abstractness matters when invoking the selected method. |
| B's class inherits A's protected concrete instance method | Rule 2 selects the matching nonprivate declaration. A.K and C.K must agree. |
| B's class inherits A's package-access concrete instance method | The method being overridden is C's **public** interface declaration. The inherited candidate is nonprivate. Rule 2 selects A's method, requiring A.K and C.K to agree. |
| B's interface inherits A's sole default and C's abstract method | For C's declaration, rule 2 selects A's unique concrete maximal. A.K and C.K must agree. |
| B's interface redeclares C's method as public abstract | Rule 1 covers B's declared overriding instance method. B.K and C.K must agree. |

Preparation uses both producers for a class or interface. The class-chain selection rule admits an eligible abstract or nonpublic declaration. The additional public/concrete checks on interface invocation have their own execution stage. Maximal-superinterface fallback considers eligible concrete maxima when class-chain lookup finds no candidate. The specification explicitly includes an interface as the prepared selection root. [JVMS 5.4.2](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.2), [5.4.5](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.5), [5.4.6](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.4.6) and [invokeinterface](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-6.html#jvms-6.5.invokeinterface).

## Measured behavior

The original 27-row split/late/shared matrix was run on Zulu 21.0.7. For each of the five shapes, preloading distinct K identities still allowed initialization of the selected child. With both K definitions deferred until after child initialization, both subsequently completed with distinct identities. Shared delegation succeeded. These are ten divergent native observations (five preexisting and five later conflicts). Private and static superclass declarations, a public concrete bridge, and a reabstracted superinterface provide four control shapes. The bridge required A/C agreement. The other three permitted distinct K identities.

Successful initialization is significant for both ordinary classes and interfaces: preparation must already have completed. The normative constraint must reject an inconsistent insertion immediately, whether a binding or an equality is added second. Lazy resolution of a later symbolic invocation cannot explain a split binding that has already succeeded after initialization. This conclusion concerns only the observable selected preparation/binding rule in these measured graphs. [JVMS 5.3.4](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.3.4) and [5.5](https://docs.oracle.com/javase/specs/jvms/se21/html/jvms-5.html#jvms-5.5).

An additional 18 split/shared runs used constructor-bearing counterparts and actual calls. Both arrangements reached invoke-stage AbstractMethodError for Abs, Private, Static and Suppress. Prot and Package reached invoke-stage IllegalAccessError under either arrangement. The bridge split failed at the child's initialization; the shared bridge invocation succeeded. RootInterface split initialized the original interface, then failed with LinkageError while initializing a separate helper class. Its shared invocation succeeded. DeclaredInterface reached invoke-stage AbstractMethodError under split and shared bindings. A later failure on a different owner or for a different reason does not erase the original successful child initialization.

The 27-row development TSV SHA-256 is `8B22F984A00D7A7EEDBE0F8FDCAD2D3F0A39912BCD3CB24380DDBA7B35FAE391`. The 18-row follow-up TSV SHA-256 is `C544702A41F7AF33E992C93616000DC60AE08960E2758215F4141DD77B58B785`.

## Acceptance treatment

For each graph, the independent Rust test must require the specified relation, immediate disagreement on an already split pair, failed later conflicting definition with its retained prior agreement, and success under common delegation. Preserve the independently captured JDK loader identities, version, trigger and stage/outcome. Report exact divergence counts for each tested vendor build. A vendor showing the normative result is a legitimate observation. Keep controls strict, retain complete raw results and require review of newly divergent shapes or error categories. Report this data separately from semantic matches. Neither an ordinary invoke-stage failure nor a successful downstream trigger counts as execution coverage of the preparation service.
