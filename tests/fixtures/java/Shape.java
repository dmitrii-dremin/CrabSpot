package fixtures;

public sealed interface Shape permits Circle, Square {}
final class Circle implements Shape {}
non-sealed class Square implements Shape {}
