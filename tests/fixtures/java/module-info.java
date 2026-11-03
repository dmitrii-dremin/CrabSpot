@Deprecated
module crabspot.fixtures {
    requires static java.logging;
    exports fixtures;
    opens fixtures to java.logging;
    uses fixtures.Service;
    provides fixtures.Service with fixtures.ServiceProvider;
}
