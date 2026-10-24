package androidx.core.view;

import android.os.Build;
import android.view.WindowInsets;

/** Minimal stub backed by framework WindowInsets. Type flag values match the
 * real androidx WindowInsetsCompat.Type (which mirror framework values):
 * statusBars=1, navigationBars=2, captionBar=4, ime=8, systemGestures=16,
 * mandatorySystemGestures=32, tappableElement=64, displayCutout=128. */
public class WindowInsetsCompat {
    public static final int TYPE_STATUS_BARS = 1;
    public static final int TYPE_NAVIGATION_BARS = 2;
    public static final int TYPE_CAPTION_BAR = 4;
    public static final int TYPE_IME = 8;
    public static final int TYPE_SYSTEM_GESTURES = 16;
    public static final int TYPE_MANDATORY_SYSTEM_GESTURES = 32;
    public static final int TYPE_TAPPABLE_ELEMENT = 64;
    public static final int TYPE_DISPLAY_CUTOUT = 128;

    protected final WindowInsets wi;

    public WindowInsetsCompat(WindowInsets wi) {
        this.wi = wi;
    }

    /** Compat Type bitmask -> framework WindowInsets mask (same numeric values API 30+). */
    private static int toFramework(int compatType) {
        return compatType & (TYPE_STATUS_BARS | TYPE_NAVIGATION_BARS | TYPE_CAPTION_BAR
                | TYPE_IME | TYPE_SYSTEM_GESTURES | TYPE_MANDATORY_SYSTEM_GESTURES
                | TYPE_TAPPABLE_ELEMENT | TYPE_DISPLAY_CUTOUT);
    }

    public androidx.core.graphics.Insets getInsets(int type) {
        if (wi == null) {
            return androidx.core.graphics.Insets.NONE;
        }
        if (Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets fi = wi.getInsets(toFramework(type));
            return new androidx.core.graphics.Insets(fi.left, fi.top, fi.right, fi.bottom);
        }
        int left = 0, top = 0, right = 0, bottom = 0;
        if ((type & TYPE_STATUS_BARS) != 0) {
            top = wi.getSystemWindowInsetTop();
        }
        if ((type & (TYPE_NAVIGATION_BARS | TYPE_SYSTEM_GESTURES | TYPE_MANDATORY_SYSTEM_GESTURES
                | TYPE_TAPPABLE_ELEMENT)) != 0) {
            left = wi.getSystemWindowInsetLeft();
            right = wi.getSystemWindowInsetRight();
            bottom = wi.getSystemWindowInsetBottom();
        }
        if ((type & TYPE_IME) != 0 && Build.VERSION.SDK_INT >= 23) {
            bottom = Math.max(bottom, wi.getSystemWindowInsetBottom());
        }
        if (left == 0 && top == 0 && right == 0 && bottom == 0) {
            return androidx.core.graphics.Insets.NONE;
        }
        return new androidx.core.graphics.Insets(left, top, right, bottom);
    }

    public boolean isVisible(int type) {
        if (wi == null) {
            return false;
        }
        if (Build.VERSION.SDK_INT >= 30) {
            return wi.isVisible(toFramework(type));
        }
        if ((type & TYPE_IME) != 0) {
            return wi.getSystemWindowInsetBottom() > 0;
        }
        return getInsets(type) != androidx.core.graphics.Insets.NONE;
    }

    public WindowInsets toWindowInsets() {
        return wi;
    }

    public static final class Type {
        private Type() {}

        public static int statusBars() { return TYPE_STATUS_BARS; }

        public static int navigationBars() { return TYPE_NAVIGATION_BARS; }

        public static int captionBar() { return TYPE_CAPTION_BAR; }

        public static int ime() { return TYPE_IME; }

        public static int systemGestures() { return TYPE_SYSTEM_GESTURES; }

        public static int mandatorySystemGestures() {
            return TYPE_MANDATORY_SYSTEM_GESTURES;
        }

        public static int tappableElement() { return TYPE_TAPPABLE_ELEMENT; }

        public static int displayCutout() { return TYPE_DISPLAY_CUTOUT; }

        public static int systemBars() {
            return TYPE_STATUS_BARS | TYPE_NAVIGATION_BARS | TYPE_CAPTION_BAR;
        }

        public static int all() {
            return TYPE_STATUS_BARS | TYPE_NAVIGATION_BARS | TYPE_CAPTION_BAR | TYPE_IME
                    | TYPE_SYSTEM_GESTURES | TYPE_MANDATORY_SYSTEM_GESTURES
                    | TYPE_TAPPABLE_ELEMENT | TYPE_DISPLAY_CUTOUT;
        }
    }
}
