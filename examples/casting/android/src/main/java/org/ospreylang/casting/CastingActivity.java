package org.ospreylang.casting;

import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.widget.TextView;
import androidx.activity.ComponentActivity;
import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

// [CAST-MOBILE-DELIVERY] Android owns platform services; Osprey owns the player.
public final class CastingActivity extends ComponentActivity {
    private final Handler main = new Handler(Looper.getMainLooper());
    private final ExecutorService io = Executors.newSingleThreadExecutor();
    private String state = "", choosing;
    private final ActivityResultLauncher<String[]> picker = registerForActivityResult(
        new ActivityResultContracts.OpenDocument(), uri -> {
            String ticket = choosing;
            choosing = null;
            if (ticket == null) return;
            if (uri == null) { complete(ticket, object("cancelled", true, "path", ""), null); return; }
            io.execute(() -> {
                try {
                    File file = MediaDocuments.importAudio(this, uri);
                    main.post(() -> complete(ticket, object("cancelled", false, "path", file.getAbsolutePath()), null));
                } catch (IOException | RuntimeException error) { main.post(() -> complete(ticket, null, error.getMessage())); }
            });
        });
    private CastDiscovery discovery;
    private SceneCanvas scene;
    private boolean alive = true, animate;
    private long lastFrame;
    private final Runnable frame = new Runnable() {
        @Override public void run() {
            long now = System.nanoTime();
            if (animate && lastFrame != 0) send(object("kind", "frame", "elapsed", Math.min(0.25, (now - lastFrame) / 1e9)));
            lastFrame = now;
            if (alive) main.postDelayed(this, 33);
        }
    };

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        discovery = new CastDiscovery(this);
        scene = new SceneCanvas(this, this::control);
        scene.setFitsSystemWindows(true);
        setContentView(scene);
        try {
            File folder = new File(getFilesDir(), "player");
            copyAssets("", folder);
            JSONArray arguments = new JSONArray().put("--machine").put(new File(folder, "player.machine.json").getAbsolutePath());
            JSONObject context = object("now", System.currentTimeMillis() / 1000, "offset", 0,
                "platform", "android", "home", getFilesDir().getAbsolutePath(), "arguments", arguments);
            Native.pending = new JSONArray();
            accept(Native.start(utf8(context.toString())));
            main.post(frame);
        } catch (IOException | JSONException | RuntimeException error) { fail(error); }
    }

    private void copyAssets(String path, File target) throws IOException {
        String[] children = getAssets().list(path);
        if (children == null) throw new IOException("Cannot enumerate player assets");
        if (children.length == 0) {
            File parent = target.getParentFile();
            if (parent == null || (!parent.isDirectory() && !parent.mkdirs())) throw new IOException("Cannot create player asset folder");
            try (InputStream input = getAssets().open(path)) { Files.copy(input, target.toPath(), StandardCopyOption.REPLACE_EXISTING); }
        } else {
            for (String child : children) copyAssets(path.isEmpty() ? child : path + "/" + child, new File(target, child));
        }
    }

    private static byte[] utf8(String text) { return text.getBytes(StandardCharsets.UTF_8); }

    static JSONObject object(Object... pairs) {
        JSONObject object = new JSONObject();
        try { for (int i = 0; i < pairs.length; i += 2) object.put((String) pairs[i], pairs[i + 1]); }
        catch (JSONException error) { throw new IllegalArgumentException("Invalid host event", error); }
        return object;
    }

    private void send(JSONObject event) {
        if (!alive || state.isEmpty()) return;
        try {
            event.put("now", System.currentTimeMillis() / 1000);
            Native.pending = new JSONArray();
            accept(Native.dispatch(utf8(state), utf8(event.toString())));
        } catch (JSONException | RuntimeException error) { fail(error); }
    }

    private void control(int ordinal, long revision, JSONObject value) {
        send(object("kind", "ui", "region", "root", "revision", revision, "ordinal", ordinal, "value", value));
    }

    private void accept(byte[] bytes) throws JSONException {
        JSONObject envelope = new JSONObject(new String(bytes, StandardCharsets.UTF_8));
        if (envelope.getInt("version") != 1) throw new JSONException("Unsupported native envelope version");
        state = envelope.getString("state");
        JSONArray slices = envelope.getJSONArray("slices");
        for (int i = 0; i < slices.length(); i++) {
            JSONObject slice = slices.getJSONObject(i);
            Native.slices.put(slice.getString("key"), slice.getString("text"));
        }
        JSONArray diagnostics = envelope.getJSONArray("diagnostics");
        if (diagnostics.length() > 0) throw new JSONException(diagnostics.toString());
        JSONArray patches = envelope.getJSONArray("patches");
        for (int i = 0; i < patches.length(); i++) {
            JSONObject patch = patches.getJSONObject(i);
            if (patch.getString("key").equals("root")) scene.replace(patch.getJSONObject("node"), patch.getLong("revision"), Native.pending);
        }
        JSONArray requests = envelope.getJSONArray("requests");
        for (int i = 0; i < requests.length(); i++) request(requests.getJSONObject(i));
    }

    private void request(JSONObject request) throws JSONException {
        String ticket = request.getString("ticket");
        switch (request.getString("kind")) {
        case "animate": animate = request.getBoolean("on"); main.post(() -> complete(ticket, object(), null)); break;
        case "run":
            if (!ticket.equals("discover") || !request.getString("slice").equals("cast-browse")) {
                main.post(() -> complete(ticket, null, "Mobile casting does not execute processes")); break;
            }
            discovery.start(text -> {
                Native.slices.put("cast-browse", text);
                complete(ticket, object(), null);
            }, error -> complete(ticket, null, error.toString()));
            break;
        case "choose-file":
            if (choosing != null) main.post(() -> complete(ticket, null, "A file picker is already open"));
            else { choosing = ticket; picker.launch(new String[]{"audio/mpeg"}); }
            break;
        default: main.post(() -> complete(ticket, null, "Unsupported mobile capability: " + request.optString("kind")));
        }
    }

    private void complete(String ticket, JSONObject value, String error) {
        send(error == null ? object("kind", "completed", "ticket", ticket, "ok", true, "result", value) :
            object("kind", "completed", "ticket", ticket, "ok", false, "error", error));
    }

    private void fail(Exception error) {
        animate = false;
        TextView message = new TextView(this);
        message.setText("Casting could not continue: " + error.getMessage());
        message.setPadding(24, 24, 24, 24);
        setContentView(message);
    }

    @Override protected void onDestroy() {
        alive = false;
        main.removeCallbacks(frame);
        discovery.cancel();
        io.shutdownNow();
        super.onDestroy();
    }
}
