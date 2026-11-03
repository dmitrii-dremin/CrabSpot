"""Generate/check independent Java 21 instance-method-body reference outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("InstanceBodyCases", "instances",
         r"(?:[IL]:-?[0-9]+|F:(?:[0-9a-f]{8}|NaN)|D:(?:[0-9a-f]{16}|NaN)|V|"
         r"E:java/lang/(?:ArithmeticException|NullPointerException|NegativeArraySizeException))")
