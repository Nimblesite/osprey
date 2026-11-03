package org.ospreylang.casting;

import android.content.Context;
import android.content.res.ColorStateList;
import android.graphics.Color;
import android.graphics.Typeface;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.PopupMenu;
import android.widget.ProgressBar;
import android.widget.ScrollView;
import android.widget.TextView;
import java.util.HashMap;
import java.util.Map;
import java.util.WeakHashMap;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

// [CAST-MOBILE-SKINS] Project the same resident tree into native Android widgets.
// Reuse nodes across frames so animation never recreates focus or open pickers.
final class NativeRenderer extends FrameLayout {
    interface Control { void send(int ordinal, long revision, Object value); }
    private final Control control;
    private final WeakHashMap<View, JSONObject> nodes = new WeakHashMap<>();
    private long revision;
    private JSONArray streamed = new JSONArray();
    private View root;

    NativeRenderer(Context context, Control control) { super(context); this.control = control; }
    public NativeRenderer(Context context) {
        this(context, (ordinal, revision, value) -> { throw new IllegalStateException("A native renderer must be attached to its resident event handler"); });
    }

    void replace(JSONObject node, long revision, JSONArray pending) throws JSONException {
        this.revision = revision;
        if (pending.length() > 0) streamed = pending;
        View next = render(node, root, Map.of());
        if (next == root) return;
        root = next; removeAllViews();
        if (node.getString("kind").equals("scene")) {
            addView(root, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        } else {
            ScrollView scroll = new ScrollView(getContext()); scroll.setFillViewport(true);
            scroll.addView(root, new ScrollView.LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT));
            addView(scroll, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        }
    }

    private View render(JSONObject node, View previous, Map<String, Integer> inherited) throws JSONException {
        String kind = node.getString("kind");
        Map<String, Integer> theme = kind.equals("themed") ? theme(node.getJSONObject("theme")) : inherited;
        View view = previous != null && nodes.get(previous) != null && nodes.get(previous).optString("kind").equals(kind) ? previous : create(kind);
        nodes.put(view, node);
        if (view instanceof SceneCanvas scene) scene.replace(node, revision, streamed);
        else if (kind.equals("themed")) {
            view.setBackgroundColor(colour(theme, "background", Color.WHITE));
            children((LinearLayout) view, new JSONArray().put(node.getJSONObject("body")), theme);
        } else if (kind.equals("stack")) {
            ((LinearLayout) view).setOrientation(node.getString("axis").equals("horizontal") ? LinearLayout.HORIZONTAL : LinearLayout.VERTICAL);
            children((LinearLayout) view, node.getJSONArray("children"), theme);
        } else if (kind.equals("empty")) empty((LinearLayout) view, node, theme);
        else if (kind.equals("progress")) progress((LinearLayout) view, node, theme);
        else if (kind.equals("meter")) meter((ProgressBar) view, node, theme);
        else text((TextView) view, node, theme);
        return view;
    }

    private View create(String kind) throws JSONException {
        return switch (kind) {
            case "scene" -> new SceneCanvas(getContext(), (ordinal, ignored, value) -> control.send(ordinal, revision, value));
            case "themed", "stack", "empty", "progress" -> new LinearLayout(getContext());
            case "text", "badge", "label" -> new TextView(getContext());
            case "button", "picker" -> button();
            case "meter" -> new ProgressBar(getContext(), null, android.R.attr.progressBarStyleHorizontal);
            default -> throw new JSONException("Unsupported native node: " + kind);
        };
    }

    private Button button() {
        Button button = new Button(getContext());
        button.setAllCaps(false); button.setMinWidth(0); button.setMinimumWidth(0);
        button.setTextSize(14); button.setMaxLines(3);
        button.setOnClickListener(view -> {
            JSONObject node = nodes.get(view);
            if (node.optString("kind").equals("picker")) choose(button, node);
            else control.send(node.optInt("event"), revision, JSONObject.NULL);
        });
        return button;
    }

    private void choose(Button button, JSONObject node) {
        JSONArray choices = node.optJSONArray("choices");
        PopupMenu popup = new PopupMenu(getContext(), button);
        for (int i = 0; choices != null && i < choices.length(); i++) {
            JSONObject item = choices.optJSONObject(i);
            String key = item.optString("key");
            popup.getMenu().add(item.optString("label")).setOnMenuItemClickListener(ignored -> {
                JSONObject current = nodes.get(button);
                control.send(current.optInt("event"), revision, key);
                return true;
            });
        }
        popup.show();
    }

    private void children(LinearLayout parent, JSONArray children, Map<String, Integer> theme) throws JSONException {
        while (parent.getChildCount() > children.length()) parent.removeViewAt(parent.getChildCount() - 1);
        boolean horizontal = parent.getOrientation() == LinearLayout.HORIZONTAL;
        parent.setGravity(Gravity.CENTER_VERTICAL);
        if (nodes.get(parent).optString("kind").equals("themed")) parent.setPadding(dp(16), dp(16), dp(16), dp(16));
        for (int i = 0; i < children.length(); i++) {
            JSONObject child = children.getJSONObject(i);
            View previous = parent.getChildAt(i), view = render(child, previous, theme);
            if (view == previous) continue;
            if (previous != null) parent.removeViewAt(i);
            LinearLayout.LayoutParams size = new LinearLayout.LayoutParams(horizontal ? 0 : LayoutParams.MATCH_PARENT,
                child.getString("kind").equals("scene") ? dp((int) child.getDouble("height")) : LayoutParams.WRAP_CONTENT, horizontal ? 1 : 0);
            size.bottomMargin = dp(6);
            parent.addView(view, i, size);
        }
    }

    private void text(TextView view, JSONObject node, Map<String, Integer> theme) throws JSONException {
        String kind = node.getString("kind"), label = node.optString("label", node.optString("text"));
        String value = kind.equals("picker") ? label + ": " + selected(node) : label;
        if (!value.contentEquals(view.getText())) view.setText(value);
        view.setTextColor(colour(theme, node.optString("tone"), colour(theme, "text", Color.BLACK)));
        if (view instanceof Button button) {
            button.setContentDescription(label); button.setEnabled(node.optBoolean("enabled", true));
            button.setAlpha(button.isEnabled() ? 1 : 0.45f);
            button.setBackgroundTintList(ColorStateList.valueOf(Color.TRANSPARENT));
            if (node.optString("role").equals("prominent")) {
                int accent = colour(theme, "accent", Color.BLUE);
                button.setBackgroundTintList(ColorStateList.valueOf(accent));
                button.setTextColor(colour(theme, "onAccent", Color.luminance(accent) > 0.5 ? Color.BLACK : Color.WHITE));
            }
        } else {
            view.setTextSize(node.optString("role").equals("title") ? 24 : node.optString("role").equals("caption") ? 12 : 16);
            view.setTypeface(null, node.optString("role").equals("title") ? Typeface.BOLD : Typeface.NORMAL);
            view.setImportantForAccessibility(label.matches("[0-9]+:[0-9]{2}") ? IMPORTANT_FOR_ACCESSIBILITY_NO : IMPORTANT_FOR_ACCESSIBILITY_AUTO);
        }
    }

    private String selected(JSONObject node) throws JSONException {
        JSONArray choices = node.getJSONArray("choices");
        for (int i = 0; i < choices.length(); i++) {
            JSONObject item = choices.getJSONObject(i);
            if (item.getString("key").equals(node.getString("selected"))) return item.getString("label");
        }
        return node.getString("selected");
    }

    private void empty(LinearLayout view, JSONObject node, Map<String, Integer> theme) throws JSONException {
        view.setOrientation(LinearLayout.VERTICAL);
        JSONArray children = new JSONArray().put(CastingActivity.object("kind", "text", "text", node.getString("title"), "role", "title"))
            .put(CastingActivity.object("kind", "text", "text", node.getString("detail"), "tone", "secondary"));
        children(view, children, theme);
    }

    private void progress(LinearLayout view, JSONObject node, Map<String, Integer> theme) {
        if (view.getChildCount() == 0) {
            view.addView(new ProgressBar(getContext()), new LinearLayout.LayoutParams(dp(24), dp(24)));
            view.addView(new TextView(getContext()));
        }
        TextView label = (TextView) view.getChildAt(1);
        if (!node.optString("label").contentEquals(label.getText())) label.setText(node.optString("label"));
        label.setTextColor(colour(theme, "secondary", Color.GRAY));
    }

    private void meter(ProgressBar view, JSONObject node, Map<String, Integer> theme) throws JSONException {
        long value = node.getLong("value"), total = node.getLong("total");
        if (total < 1 || total > Integer.MAX_VALUE || value < 0 || value > total) throw new JSONException("Invalid meter range");
        view.setContentDescription(node.getString("label"));
        NativeRange.update(view, (int) value, (int) total, false, value + " / " + total);
        view.setProgressTintList(ColorStateList.valueOf(colour(theme, "accent", Color.BLUE)));
    }

    private Map<String, Integer> theme(JSONObject node) throws JSONException {
        Map<String, Integer> values = new HashMap<>();
        JSONArray tokens = node.getJSONArray("tokens");
        for (int i = 0; i < tokens.length(); i++) {
            JSONObject token = tokens.getJSONObject(i);
            values.put(token.getString("name"), Color.parseColor(token.getString("light")));
        }
        return values;
    }
    private static int colour(Map<String, Integer> theme, String name, int fallback) { return theme.getOrDefault(name, fallback); }
    private int dp(int value) { return Math.round(value * getResources().getDisplayMetrics().density); }
}
