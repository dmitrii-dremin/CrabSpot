"""Generate/check Java 21 multidimensional-array outcomes."""
from numeric_reference import main

if __name__ == "__main__":
    main("MultiArrayCases", "multidimensional_arrays", r"(?:I:-?[0-9]+|E:java/lang/(?:ArrayStoreException|NegativeArraySizeException|ArrayIndexOutOfBoundsException))")
