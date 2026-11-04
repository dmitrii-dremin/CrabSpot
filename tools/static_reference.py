"""Generate/check explicit static-memory body outcomes with reset JDK state."""
from numeric_reference import main

if __name__ == "__main__":
    main("StaticBodyCases", "static_bodies",
         r"(?:[IL]:-?[0-9]+|F:(?:[0-9a-f]{8}|NaN)|D:(?:[0-9a-f]{16}|NaN)|"
         r"T:(?:first|second)|E:java/lang/(?:ArithmeticException|NegativeArraySizeException))")
