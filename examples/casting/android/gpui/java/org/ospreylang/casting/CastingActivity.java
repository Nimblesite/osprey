package org.ospreylang.casting;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

// [CAST-MOBILE-GPUI] No interface lives here: hosts/gpui draws everything. This activity only
// answers what the platform alone can do, one JSON request in and one JSON answer out, on the
// host's background thread: browsing for receivers and picking a document.
public final class CastingActivity extends NativeActivity {
    private static final int PICK_DOCUMENT = 1;
    private static final int LOCAL_NETWORK = 2;
    private static final long BROWSE_SECONDS = 10;
    private static final long PICKER_SECONDS = 600;
    private static final int FIXTURE_LIMIT = 65536;

    private final Handler main = new Handler(Looper.getMainLooper());
    private CompletableFuture<Uri> picking;
    private CompletableFuture<Boolean> permitting;

    /** Called by the GPUI host, never on the main thread. Every failure is a typed answer. */
    public String answer(String request) {
        try {
            JSONObject members = new JSONObject(request);
            switch (members.getString("kind")) {
                case "run": return browse(members).toString();
                case "choose-file": return choose().toString();
                default: return failed("unsupported capability " + members.getString("kind"));
            }
        } catch (Exception error) {
            return failed(String.valueOf(error.getMessage()));
        }
    }

    private JSONObject browse(JSONObject members) throws Exception {
        if (!"discover".equals(members.optString("ticket")) || !"cast-browse".equals(members.optString("slice"))) {
            throw new IllegalArgumentException("Mobile casting does not execute processes");
        }
        if (!localNetworkAllowed()) throw new SecurityException(getString(R.string.local_network_denied));
        String fixture = getIntent().getStringExtra("testReceiver");
        if ((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0 && fixture != null) {
            byte[] decoded = Base64.getDecoder().decode(fixture);
            if (decoded.length > FIXTURE_LIMIT) throw new IllegalArgumentException("Receiver fixture exceeds the browse limit");
            return browsed(new String(decoded, StandardCharsets.UTF_8));
        }
        CompletableFuture<String> found = new CompletableFuture<>();
        main.post(() -> new CastDiscovery(this).start(found::complete, found::completeExceptionally));
        return browsed(found.get(BROWSE_SECONDS, TimeUnit.SECONDS));
    }

    private static JSONObject browsed(String transcript) throws JSONException {
        JSONObject slice = new JSONObject().put("key", "cast-browse").put("text", transcript);
        return new JSONObject().put("ok", true).put("result", new JSONObject()).put("slices", new JSONArray().put(slice));
    }

    private boolean localNetworkAllowed() throws Exception {
        String permission = android.Manifest.permission.ACCESS_LOCAL_NETWORK;
        if (android.os.Build.VERSION.SDK_INT < 37 || checkSelfPermission(permission) == android.content.pm.PackageManager.PERMISSION_GRANTED) return true;
        CompletableFuture<Boolean> asked = new CompletableFuture<>();
        main.post(() -> {
            permitting = asked;
            requestPermissions(new String[]{permission}, LOCAL_NETWORK);
        });
        return asked.get(PICKER_SECONDS, TimeUnit.SECONDS);
    }

    @Override public void onRequestPermissionsResult(int code, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(code, permissions, results);
        if (code == LOCAL_NETWORK && permitting != null) {
            permitting.complete(results.length > 0 && results[0] == android.content.pm.PackageManager.PERMISSION_GRANTED);
            permitting = null;
        }
    }

    private JSONObject choose() throws Exception {
        CompletableFuture<Uri> chosen = new CompletableFuture<>();
        main.post(() -> {
            if (picking != null) {
                chosen.completeExceptionally(new IllegalStateException("A file picker is already open"));
                return;
            }
            picking = chosen;
            startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("audio/mpeg"), PICK_DOCUMENT);
        });
        Uri uri = chosen.get(PICKER_SECONDS, TimeUnit.SECONDS);
        JSONObject result = new JSONObject();
        if (uri == null) {
            result.put("cancelled", true).put("path", "");
        } else {
            File file = MediaDocuments.importAudio(this, uri);
            result.put("cancelled", false).put("path", file.getAbsolutePath());
        }
        return new JSONObject().put("ok", true).put("result", result).put("slices", new JSONArray());
    }

    @Override protected void onActivityResult(int code, int result, Intent data) {
        super.onActivityResult(code, result, data);
        if (code == PICK_DOCUMENT && picking != null) {
            picking.complete(result == RESULT_OK && data != null ? data.getData() : null);
            picking = null;
        }
    }

    private static String failed(String problem) {
        try {
            return new JSONObject().put("ok", false).put("error", problem).toString();
        } catch (JSONException impossible) {
            throw new IllegalStateException(impossible);
        }
    }
}
