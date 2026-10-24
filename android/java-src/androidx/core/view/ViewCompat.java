package androidx.core.view;

import android.os.Build;
import android.view.View;
import android.view.WindowInsets;

public final class ViewCompat {
    private ViewCompat() {}

    public static WindowInsetsCompat getRootWindowInsets(View view) {
        WindowInsets wi = (Build.VERSION.SDK_INT >= 23) ? view.getRootWindowInsets() : null;
        if (wi == null) {
            return null;
        }
        return new WindowInsetsCompat(wi);
    }
}
