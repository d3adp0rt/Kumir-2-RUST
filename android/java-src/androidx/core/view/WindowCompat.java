package androidx.core.view;

import android.os.Build;
import android.view.Window;
import android.view.WindowManager;

public final class WindowCompat {
    private WindowCompat() {}

    public static void setDecorFitsSystemWindows(Window window, boolean decorFitsSystemWindows) {
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(decorFitsSystemWindows);
        } else {
            int flags = WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS;
            if (decorFitsSystemWindows) {
                window.setFlags(0, flags);
            } else {
                window.setFlags(flags, flags);
            }
        }
    }
}
