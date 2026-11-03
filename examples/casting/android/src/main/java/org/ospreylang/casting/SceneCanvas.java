package org.ospreylang.casting;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.Typeface;
import android.view.MotionEvent;
import android.view.View;
import android.os.Bundle;
import android.graphics.Rect;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import android.view.accessibility.AccessibilityEvent;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

// [UI-SCENE-STREAM] Draws the machine's vector art and sends its pointer events.
final class SceneCanvas extends View {
    interface Control { void send(int ordinal, long revision, JSONObject value); }
    private final Control control;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final HashMap<String, Bitmap> images = new HashMap<>();
    private final ArrayList<JSONArray> shapes = new ArrayList<>();
    private JSONArray atlas = new JSONArray(), streamed = new JSONArray();
    private double width, height;
    private float scale = 1, left, top;
    private long revision;
    private int ordinal;
    private JSONArray access = new JSONArray();
    private final AccessibilityNodeProvider accessibility = new AccessibilityNodeProvider() {
        @Override public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
            AccessibilityNodeInfo info = new AccessibilityNodeInfo(SceneCanvas.this, id);
            info.setPackageName(getContext().getPackageName());
            if (id == HOST_VIEW_ID) {
                onInitializeAccessibilityNodeInfo(info);
                for (int i = 0; i < access.length(); i++) info.addChild(SceneCanvas.this, i);
                return info;
            }
            JSONObject item = access.optJSONObject(id);
            if (item == null) return null;
            info.setParent(SceneCanvas.this);
            info.setContentDescription(item.optString("label"));
            info.setEnabled(true); info.setFocusable(true); info.setVisibleToUser(isShown());
            String role = item.optString("role");
            info.setClassName(role.equals("slider") ? "android.widget.SeekBar" : "android.widget.Button");
            int[] screen = new int[2]; getLocationOnScreen(screen);
            Rect bounds = bounds(item); bounds.offset(screen[0], screen[1]); info.setBoundsInScreen(bounds);
            if (role.equals("slider") && item.optBoolean("movable")) {
                info.setRangeInfo(new AccessibilityNodeInfo.RangeInfo(AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_FLOAT,
                    (float) item.optDouble("min"), (float) item.optDouble("max"), (float) item.optDouble("value")));
                info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS);
            } else { info.setClickable(true); info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK); }
            return info;
        }
        @Override public boolean performAction(int id, int action, Bundle arguments) {
            JSONObject item = access.optJSONObject(id);
            if (item == null) return false;
            double fraction = 0.5;
            if (action == AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.getId() && arguments != null) {
                double min = item.optDouble("min"), max = item.optDouble("max");
                fraction = Math.max(0, Math.min(1, (arguments.getFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE) - min) / Math.max(0.000001, max - min)));
            } else if (action != AccessibilityNodeInfo.ACTION_CLICK) return false;
            double x = item.optDouble("x") + item.optDouble("w") * fraction;
            double y = item.optDouble("y") + item.optDouble("h") / 2;
            for (boolean down : new boolean[]{true, false}) control.send(ordinal, revision, CastingActivity.object("x", x, "y", y, "down", down, "shift", false));
            return true;
        }
    };

    private Rect bounds(JSONObject item) {
        return new Rect(Math.round(left + (float) item.optDouble("x") * scale), Math.round(top + (float) item.optDouble("y") * scale),
            Math.round(left + (float) (item.optDouble("x") + item.optDouble("w")) * scale), Math.round(top + (float) (item.optDouble("y") + item.optDouble("h")) * scale));
    }

    @Override public AccessibilityNodeProvider getAccessibilityNodeProvider() { return accessibility; }

    SceneCanvas(Context context, Control control) {
        super(context);
        this.control = control;
        setFocusable(true);
        setContentDescription("Casting player");
    }

    void replace(JSONObject node, long revision, JSONArray pending) throws JSONException {
        if (!node.getString("kind").equals("scene")) throw new JSONException("Android does not draw " + node.optString("kind") + " yet");
        this.revision = revision;
        width = node.getDouble("width"); height = node.getDouble("height");
        if (width <= 0 || height <= 0) throw new JSONException("Invalid scene dimensions");
        ordinal = node.getInt("event");
        JSONArray nextAccess = node.optJSONArray("access");
        if (nextAccess == null) nextAccess = new JSONArray();
        boolean changed = !access.toString().equals(nextAccess.toString());
        access = nextAccess;
        atlas = node.getJSONArray("atlas");
        if (pending.length() > 0) streamed = pending;
        shapes.clear();
        JSONArray own = node.getJSONArray("shapes");
        for (int i = 0; i < own.length(); i++) shapes.add(own.getJSONArray(i));
        if (node.optInt("streamed") > 0) for (int i = 0; i < streamed.length(); i++) shapes.add(streamed.getJSONArray(i));
        shapes.sort(Comparator.comparingDouble(shape -> shape.optDouble(shape.optInt(0) == 6 ? 6 : shape.length() - 1)));
        for (int i = 0; i < atlas.length(); i++) {
            String path = atlas.getString(i);
            if (!images.containsKey(path)) {
                Bitmap image = BitmapFactory.decodeFile(path);
                if (image == null) throw new JSONException("Cannot load scene artwork: " + path);
                images.put(path, image);
            }
        }
        invalidate();
        if (changed) sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
    }

    @Override protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        if (width <= 0 || height <= 0) return;
        scale = (float) Math.min(getWidth() / width, getHeight() / height);
        left = (getWidth() - (float) width * scale) / 2;
        top = (getHeight() - (float) height * scale) / 2;
        canvas.save(); canvas.translate(left, top); canvas.scale(scale, scale);
        try { for (JSONArray shape : shapes) draw(shape, canvas); }
        catch (JSONException error) {
            canvas.drawColor(Color.WHITE); paint.setColor(Color.RED); paint.setTextSize(24);
            canvas.drawText("Scene refused: " + error.getMessage(), 10, 30, paint);
        }
        canvas.restore();
    }

    private static int rgba(double packed) {
        long bits = (long) packed;
        return Color.argb((int) (bits & 255), (int) ((bits >> 24) & 255), (int) ((bits >> 16) & 255), (int) ((bits >> 8) & 255));
    }

    private void draw(JSONArray shape, Canvas canvas) throws JSONException {
        canvas.save();
        paint.setStyle(Paint.Style.FILL); paint.setAlpha(255); paint.setShader(null);
        int code = shape.getInt(0);
        float x = (float) shape.optDouble(code == 3 ? 2 : 1), y = (float) shape.optDouble(code == 3 ? 3 : 2);
        switch (code) {
        case 0:
            float w = (float) shape.getDouble(3), h = (float) shape.getDouble(4);
            paint.setColor(rgba(shape.getDouble(6)));
            canvas.rotate((float) Math.toDegrees(shape.getDouble(5)), x, y);
            canvas.drawRect(x - w / 2, y - h / 2, x + w / 2, y + h / 2, paint); break;
        case 1:
            paint.setColor(rgba(shape.getDouble(4))); canvas.drawCircle(x, y, (float) shape.getDouble(3), paint); break;
        case 2: strip(shape, canvas); break;
        case 3:
            Bitmap image = images.get(atlas.getString(shape.getInt(1)));
            float sw = (float) shape.getDouble(4), sh = (float) shape.getDouble(5);
            canvas.rotate((float) Math.toDegrees(shape.getDouble(6)), x, y);
            paint.setAlpha((int) ((long) shape.getDouble(7) & 255));
            canvas.drawBitmap(image, null, new android.graphics.RectF(x - sw / 2, y - sh / 2, x + sw / 2, y + sh / 2), paint); break;
        case 4: case 6:
            paint.setColor(rgba(shape.getDouble(5))); paint.setTextSize((float) shape.getDouble(4));
            paint.setTypeface(Typeface.create("sans-serif", shape.optDouble(8) >= 600 ? Typeface.BOLD : Typeface.NORMAL));
            paint.setTextAlign(code == 6 && shape.optInt(7) == 1 ? Paint.Align.CENTER : code == 6 && shape.optInt(7) == 2 ? Paint.Align.RIGHT : Paint.Align.LEFT);
            canvas.drawText(shape.getString(1), (float) shape.getDouble(2), (float) shape.getDouble(3) - paint.ascent(), paint); break;
        case 5: vector(shape, canvas); break;
        default: throw new JSONException("Unknown scene primitive " + code);
        }
        canvas.restore();
    }

    private void strip(JSONArray shape, Canvas canvas) throws JSONException {
        int count = shape.getInt(1);
        if (count < 1 || shape.length() != count * 2 + 5) throw new JSONException("Invalid line strip");
        Path path = new Path();
        for (int i = 0; i < count; i++) {
            float x = (float) shape.getDouble(2 + i * 2), y = (float) shape.getDouble(3 + i * 2);
            if (i == 0) path.moveTo(x, y); else path.lineTo(x, y);
        }
        paint.setColor(rgba(shape.getDouble(count * 2 + 2)));
        paint.setStyle(Paint.Style.STROKE); paint.setStrokeWidth((float) shape.getDouble(count * 2 + 3));
        canvas.drawPath(path, paint);
    }

    private void vector(JSONArray shape, Canvas canvas) throws JSONException {
        JSONArray steps = shape.getJSONArray(1), outline = shape.getJSONArray(2), ink = shape.getJSONArray(3), transform = shape.getJSONArray(4);
        if (ink.getInt(0) != 0) throw new JSONException("Gradient paths are not implemented");
        Path path = new Path();
        for (int i = 0; i < steps.length(); i++) pathStep(path, steps.getJSONArray(i));
        if (transform.length() == 3) { canvas.translate((float) transform.getDouble(0), (float) transform.getDouble(1)); canvas.scale((float) transform.getDouble(2), (float) transform.getDouble(2)); }
        JSONArray tint = ink.getJSONArray(1);
        paint.setColor(tint(tint));
        if (outline.getInt(0) == 1) { paint.setStyle(Paint.Style.STROKE); paint.setStrokeWidth((float) outline.getDouble(1)); }
        canvas.drawPath(path, paint);
    }

    private static int tint(JSONArray tint) throws JSONException {
        float a = (float) tint.getDouble(4);
        if (tint.getInt(0) == 0) return Color.argb(a, (float) tint.getDouble(1), (float) tint.getDouble(2), (float) tint.getDouble(3));
        float h = (float) tint.getDouble(1), s = (float) tint.getDouble(2), l = (float) tint.getDouble(3);
        float v = l + s * Math.min(l, 1 - l);
        return Color.HSVToColor(Math.round(a * 255), new float[]{h * 360, v == 0 ? 0 : 2 * (1 - l / v), v});
    }

    private static void pathStep(Path path, JSONArray step) throws JSONException {
        float x = (float) step.optDouble(1), y = (float) step.optDouble(2);
        switch (step.getInt(0)) {
        case 0: path.moveTo(x, y); break;
        case 1: path.lineTo(x, y); break;
        case 2: path.quadTo((float) step.getDouble(3), (float) step.getDouble(4), x, y); break;
        case 3: path.cubicTo((float) step.getDouble(3), (float) step.getDouble(4), (float) step.getDouble(5), (float) step.getDouble(6), x, y); break;
        case 5:
            path.moveTo((float) step.getDouble(2), (float) step.getDouble(3));
            for (int i = 4; i + 1 < step.length(); i += 2) path.lineTo((float) step.getDouble(i), (float) step.getDouble(i + 1));
            if (step.getInt(1) == 1) path.close(); break;
        case 6: path.close(); break;
        default: throw new JSONException("Unsupported path command " + step.getInt(0));
        }
    }

    @Override public boolean onTouchEvent(MotionEvent event) {
        if (width <= 0 || event.getActionMasked() == MotionEvent.ACTION_CANCEL) return false;
        boolean down = event.getActionMasked() != MotionEvent.ACTION_UP;
        control.send(ordinal, revision, CastingActivity.object("x", (event.getX() - left) / scale, "y", (event.getY() - top) / scale, "down", down, "shift", false));
        if (!down) performClick();
        return true;
    }

    @Override public boolean performClick() { super.performClick(); return true; }
}
