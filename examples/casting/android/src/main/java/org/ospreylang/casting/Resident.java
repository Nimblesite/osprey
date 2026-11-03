package org.ospreylang.casting;

import android.os.Handler;
import android.os.Looper;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BiConsumer;
import java.util.function.Consumer;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

// [ANDROID-HOST-ABI] One process-wide engine thread owns every Osprey call.
// Socket operations never run on Android's UI thread; only envelopes cross it.
final class Resident {
    private static final ExecutorService ENGINE = Executors.newSingleThreadExecutor(task -> new Thread(task, "casting-core"));
    private final Handler main = new Handler(Looper.getMainLooper());
    private final BiConsumer<JSONObject, JSONArray> received;
    private final Consumer<Exception> failed;
    private final AtomicBoolean closing = new AtomicBoolean(), framePending = new AtomicBoolean();
    private String state = "";
    private JSONObject scene;
    private long revision;

    Resident(BiConsumer<JSONObject, JSONArray> received, Consumer<Exception> failed) {
        this.received = received; this.failed = failed;
    }

    void start(JSONObject context) {
        ENGINE.execute(() -> {
            if (closing.get()) return;
            try { Native.pending = new JSONArray(); accept(Native.start(utf8(context.toString()))); }
            catch (Exception error) { main.post(() -> failed.accept(error)); }
        });
    }

    void send(JSONObject event) { if (!closing.get()) ENGINE.execute(() -> dispatch(event)); }

    void frame(double elapsed) {
        if (closing.get() || !framePending.compareAndSet(false, true)) return;
        ENGINE.execute(() -> {
            try { dispatch(CastingActivity.object("kind", "frame", "elapsed", elapsed)); }
            finally { framePending.set(false); }
        });
    }

    private void dispatch(JSONObject event) {
        if (state.isEmpty()) return;
        try {
            event.put("now", System.currentTimeMillis() / 1000);
            // Scene drags use the current binding revision, as the desktop host does.
            if (scene != null && scene.optString("kind").equals("scene") && event.optString("kind").equals("ui") &&
                event.optInt("ordinal", -1) == scene.optInt("event", -2)) event.put("revision", revision);
            Native.pending = new JSONArray();
            accept(Native.dispatch(utf8(state), utf8(event.toString())));
        } catch (Exception error) { main.post(() -> failed.accept(error)); }
    }

    private void accept(byte[] bytes) throws JSONException {
        JSONObject envelope = new JSONObject(new String(bytes, StandardCharsets.UTF_8));
        if (envelope.getInt("version") != 1) throw new JSONException("Unsupported native envelope version");
        state = envelope.getString("state");
        JSONArray slices = envelope.getJSONArray("slices"), patches = envelope.getJSONArray("patches");
        for (int i = 0; i < slices.length(); i++) {
            JSONObject slice = slices.getJSONObject(i);
            Native.slices.put(slice.getString("key"), slice.getString("text"));
        }
        for (int i = 0; i < patches.length(); i++) {
            JSONObject patch = patches.getJSONObject(i);
            if (patch.getString("key").equals("root")) { scene = patch.getJSONObject("node"); revision = patch.getLong("revision"); }
        }
        JSONArray streamed = Native.pending;
        main.post(() -> { if (!closing.get()) received.accept(envelope, streamed); });
    }

    void close() {
        if (closing.compareAndSet(false, true)) ENGINE.execute(() -> dispatch(CastingActivity.object("kind", "lifecycle", "phase", "closed")));
    }

    private static byte[] utf8(String text) { return text.getBytes(StandardCharsets.UTF_8); }
}
