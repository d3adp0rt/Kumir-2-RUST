package androidx.core.graphics;

public final class Insets {
    public final int left;
    public final int top;
    public final int right;
    public final int bottom;

    public static final Insets NONE = new Insets(0, 0, 0, 0);

    public Insets(int left, int top, int right, int bottom) {
        this.left = left;
        this.top = top;
        this.right = right;
        this.bottom = bottom;
    }

    public static Insets add(Insets a, Insets b) {
        return new Insets(a.left + b.left, a.top + b.top, a.right + b.right, a.bottom + b.bottom);
    }

    public static Insets max(Insets a, Insets b) {
        return new Insets(
                Math.max(a.left, b.left),
                Math.max(a.top, b.top),
                Math.max(a.right, b.right),
                Math.max(a.bottom, b.bottom));
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Insets)) return false;
        Insets i = (Insets) o;
        return left == i.left && top == i.top && right == i.right && bottom == i.bottom;
    }

    @Override
    public int hashCode() {
        return left * 31 + top * 37 + right * 41 + bottom * 43;
    }

    @Override
    public String toString() {
        return "Insets(" + left + ", " + top + ", " + right + ", " + bottom + ")";
    }
}
