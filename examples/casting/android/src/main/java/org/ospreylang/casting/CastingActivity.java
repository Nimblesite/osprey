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
    private String choosing;
    private Resident resident;
    private JSONObject permissionBrowse;
    private final ActivityResultLauncher<String> localNetwork = registerForActivityResult(
        new ActivityResultContracts.RequestPermission(), granted -> {
            JSONObject request = permissionBrowse;
            permissionBrowse = null;
            if (request == null) return;
            if (!granted) { complete(request.optString("ticket"), null, getString(R.string.local_network_denied)); return; }
            try { request(request); } catch (JSONException error) { fail(error); }
        });
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
    private NativeRenderer scene;
    private boolean alive = true, animate;
    private long lastFrame;
    private final Runnable frame = new Runnable() {
        @Override public void run() {
            long now = System.nanoTime();
            if (animate && lastFrame != 0) resident.frame(Math.min(0.25, (now - lastFrame) / 1e9));
            lastFrame = now;
            if (alive) main.postDelayed(this, 33);
        }
    };

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        discovery = new CastDiscovery(this);
        resident = new Resident(this::receive, this::fail);
        scene = new NativeRenderer(this, this::control);
        scene.setFitsSystemWindows(true);
        setContentView(scene);
        try {
            File folder = new File(getFilesDir(), "player");
            copyAssets("", folder);
            JSONArray arguments = new JSONArray().put("--machine").put(new File(folder, "player.machine.json").getAbsolutePath());
            String skin = getIntent().getStringExtra("skin");
            if (skin != null) arguments.put("--skin").put(skin);
            JSONObject context = object("now", System.currentTimeMillis() / 1000, "offset", 0,
                "platform", "android", "home", getFilesDir().getAbsolutePath(), "arguments", arguments);
            resident.start(context);
            main.post(frame);
        } catch (IOException | RuntimeException error) { fail(error); }
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
        if (alive) resident.send(event);
    }

    private void control(int ordinal, long revision, Object value) {
        send(object("kind", "ui", "region", "root", "revision", revision, "ordinal", ordinal, "value", value));
    }

    private void receive(JSONObject envelope, JSONArray streamed) {
        if (!alive) return;
        try { accept(envelope, streamed); } catch (IOException | JSONException | RuntimeException error) { fail(error); }
    }

    private void accept(JSONObject envelope, JSONArray streamed) throws JSONException, IOException {
        JSONArray diagnostics = envelope.getJSONArray("diagnostics");
        if (diagnostics.length() > 0) throw new JSONException(diagnostics.toString());
        JSONArray patches = envelope.getJSONArray("patches");
        for (int i = 0; i < patches.length(); i++) {
            JSONObject patch = patches.getJSONObject(i);
            if (patch.getString("key").equals("root")) {
                scene.replace(patch.getJSONObject("node"), patch.getLong("revision"), streamed);
                if ((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0 && getIntent().getBooleanExtra("captureEnvelope", false))
                    Files.write(new File(getFilesDir(), "test-scene.json").toPath(), utf8(patch.toString()));
            }
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
            if (android.os.Build.VERSION.SDK_INT >= 37 && checkSelfPermission(android.Manifest.permission.ACCESS_LOCAL_NETWORK) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
                permissionBrowse = request;
                localNetwork.launch(android.Manifest.permission.ACCESS_LOCAL_NETWORK);
                break;
            }
            String fixture = getIntent().getStringExtra("testReceiver");
            if ((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0 && fixture != null) {
                byte[] decoded = java.util.Base64.getDecoder().decode(fixture);
                if (decoded.length > 65536) throw new JSONException("Receiver fixture exceeds the browse limit");
                Native.slices.put("cast-browse", new String(decoded, StandardCharsets.UTF_8));
                main.post(() -> complete(ticket, object(), null));
                break;
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
        if (resident != null) resident.close();
        TextView message = new TextView(this);
        message.setText(getString(R.string.casting_failed, error.toString()));
        message.setPadding(24, 24, 24, 24);
        setContentView(message);
    }

    @Override protected void onDestroy() {
        alive = false;
        if (resident != null) resident.close();
        main.removeCallbacks(frame);
        discovery.cancel();
        io.shutdownNow();
        super.onDestroy();
    }
}
