package org.ospreylang.casting;

import android.os.SystemClock;
import android.view.View;
import android.widget.ProgressBar;
import java.util.WeakHashMap;

// [CAST-MOBILE-SKINS] Pace programmatic range announcements independently of art.
// Direct user input and focus synchronization retain their full precision.
final class NativeRange {
    private static final long REFRESH_MS = 2000;
    private static final WeakHashMap<View, Long> next = new WeakHashMap<>();
    private NativeRange() {}

    static boolean refresh(View view, boolean force) {
        long now = SystemClock.elapsedRealtime();
        if (!force && now < next.getOrDefault(view, 0L)) return false;
        next.put(view, now - now % REFRESH_MS + REFRESH_MS);
        return true;
    }

    static void update(ProgressBar view, int value, int total, boolean force, String description) {
        if (!refresh(view, force)) return;
        if (view.getMax() != total) view.setMax(total);
        view.setProgress(value);
        if (description != null) view.setStateDescription(description);
    }
}
