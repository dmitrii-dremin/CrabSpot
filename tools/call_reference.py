"""Generate/check JDK results for same-class and same-interface body graphs."""
from numeric_reference import main


if __name__ == "__main__":
    pattern = r"(?:[IL]:-?[0-9]+|F:(?:[0-9a-f]{8}|NaN)|D:(?:[0-9a-f]{16}|NaN)|V|E:java/lang/ArithmeticException)"
    main("CallCases", "calls", pattern)
    main("InterfaceCalls", "interface_calls", pattern)
