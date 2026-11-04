"""Generate/check Java 21 reference-array execution outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("ReferenceArrayCases", "reference_arrays", r"(?:I:-?[0-9]+|E:java/lang/(?:ArrayStoreException|NegativeArraySizeException|ArrayIndexOutOfBoundsException|NullPointerException))")
