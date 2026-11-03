package org.ospreylang.casting;

import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import org.json.JSONArray;
import org.json.JSONException;

// [ANDROID-HOST-ABI] Osprey calls and streamed shapes stay on the UI thread.
final class Native {
    static { System.loadLibrary("osprey_casting"); }
    static final HashMap<String, String> slices = new HashMap<>();
    static JSONArray pending = new JSONArray();
    static native byte[] start(byte[] context);
    static native byte[] dispatch(byte[] state, byte[] event);
    private Native() {}

    static byte[] slice(String key) { return slices.getOrDefault(key, "").getBytes(StandardCharsets.UTF_8); }

    static long shape(long code, long index, double a, double b, double c, double d, double e, long tint, long layer) {
        if (!Double.isFinite(a) || !Double.isFinite(b) || !Double.isFinite(c) || !Double.isFinite(d) || !Double.isFinite(e)) return -1;
        double[] values = code == 1 ? new double[]{1,a,b,c,tint,layer} : code == 3 ?
            new double[]{3,index,a,b,c,d,e,tint,layer} : new double[]{0,a,b,c,d,e,tint,layer};
        try {
            JSONArray shape = new JSONArray();
            for (double value : values) shape.put(value);
            pending.put(shape);
            return 0;
        } catch (JSONException error) { throw new IllegalStateException("Invalid scene primitive", error); }
    }
}
