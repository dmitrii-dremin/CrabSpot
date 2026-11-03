"""Generate/check Java 21 raw putfield service-reference outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("FieldStoreCases", "fields",
         r"(?:[IL]:-?[0-9]+|F:(?:[0-9a-f]{8}|NaN)|D:(?:[0-9a-f]{16}|NaN))")
