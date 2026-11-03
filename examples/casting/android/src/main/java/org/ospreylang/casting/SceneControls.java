package org.ospreylang.casting;

import android.graphics.Color;
import android.graphics.Rect;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.StateListDrawable;
import android.util.TypedValue;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.SeekBar;
import java.util.ArrayList;
import java.util.function.Function;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

// [CAST-MOBILE-DELIVERY] Real Android controls own touch, keyboard and accessibility.
// Their transparent normal state lets the same machine artwork supply every skin.
final class SceneControls {
    private static final int STEPS = 10000;
    private static final int MINIMUM_TOUCH_DP = 48;
    private final SceneCanvas scene;
    private final ArrayList<Binding> bindings = new ArrayList<>();

    SceneControls(SceneCanvas scene) { this.scene = scene; }

    void update(JSONArray access) throws JSONException {
        while (bindings.size() > access.length()) scene.removeView(bindings.remove(bindings.size() - 1).view);
        for (int i = 0; i < access.length(); i++) {
            JSONObject item = access.getJSONObject(i);
            String role = item.getString("role");
            if (!role.equals("slider") && !role.equals("button") && !role.equals("popup") && !role.equals("menuitem"))
                throw new JSONException("Unsupported native control role: " + role);
            if (i == bindings.size()) bindings.add(create(item));
            Binding binding = bindings.get(i);
            if ((binding.view instanceof SeekBar) != role.equals("slider")) {
                scene.removeView(binding.view); binding = create(item); bindings.set(i, binding);
            }
            binding.update(item);
        }
    }

    private Binding create(JSONObject item) {
        Binding binding = new Binding(item);
        scene.addView(binding.view, new ViewGroup.LayoutParams(0, 0));
        return binding;
    }

    void layout(Function<JSONObject, Rect> bounds) {
        int minimum = Math.round(MINIMUM_TOUCH_DP * scene.getResources().getDisplayMetrics().density);
        for (Binding binding : bindings) {
            Rect frame = bounds.apply(binding.item);
            int width = Math.max(minimum, frame.width()), height = Math.max(minimum, frame.height());
            int left = frame.centerX() - width / 2, top = frame.centerY() - height / 2;
            // Grow visible targets inward at viewport edges; never relocate off-screen menu rows.
            if (frame.left >= 0 && frame.right <= scene.getWidth()) left = Math.max(0, Math.min(left, scene.getWidth() - width));
            if (frame.top >= 0 && frame.bottom <= scene.getHeight()) top = Math.max(0, Math.min(top, scene.getHeight() - height));
            View view = binding.view;
            view.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                         View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
            view.layout(left, top, left + width, top + height);
        }
    }

    private StateListDrawable focusBackground() {
        TypedValue accent = new TypedValue();
        scene.getContext().getTheme().resolveAttribute(android.R.attr.colorAccent, accent, true);
        GradientDrawable focused = new GradientDrawable();
        focused.setColor(Color.TRANSPARENT);
        focused.setCornerRadius(8 * scene.getResources().getDisplayMetrics().density);
        focused.setStroke(Math.max(1, Math.round(scene.getResources().getDisplayMetrics().density)), accent.data);
        StateListDrawable states = new StateListDrawable();
        states.addState(new int[]{android.R.attr.state_focused}, focused);
        states.addState(new int[]{}, new ColorDrawable(Color.TRANSPARENT));
        return states;
    }

    private final class Binding implements SeekBar.OnSeekBarChangeListener {
        private final View view;
        private JSONObject item;
        private boolean tracking;

        Binding(JSONObject item) {
            this.item = item;
            view = item.optString("role").equals("slider") ? slider() : button();
            view.setId(View.generateViewId());
            view.setFocusable(true);
            view.setPadding(0, 0, 0, 0);
            view.setBackground(focusBackground());
            view.setOnFocusChangeListener((focusedView, focused) -> {
                if (focused && focusedView instanceof SeekBar slider) sync(slider, true);
            });
        }

        private Button button() {
            Button button = new Button(scene.getContext());
            button.setTextColor(Color.TRANSPARENT);
            button.setMinWidth(0); button.setMinHeight(0);
            button.setMinimumWidth(0); button.setMinimumHeight(0);
            button.setBackgroundTintList(null);
            button.setOnClickListener(ignored -> scene.act(item, 0.5, null));
            return button;
        }

        private SeekBar slider() {
            SeekBar slider = new SeekBar(scene.getContext());
            slider.setMax(STEPS);
            slider.setKeyProgressIncrement(STEPS / 100);
            slider.setProgressDrawable(new ColorDrawable(Color.TRANSPARENT));
            slider.setThumb(new ColorDrawable(Color.TRANSPARENT));
            slider.setThumbOffset(0);
            slider.setSplitTrack(false);
            slider.setOnSeekBarChangeListener(this);
            return slider;
        }

        void update(JSONObject next) {
            item = next;
            String label = item.optString("label");
            if (!label.contentEquals(view.getContentDescription() == null ? "" : view.getContentDescription())) view.setContentDescription(label);
            if (view instanceof Button button && !label.contentEquals(button.getText())) button.setText(label);
            if (view instanceof SeekBar slider && !tracking) sync(slider, false);
        }

        // Canvas animation stays smooth. Programmatic native range announcements
        // are paced; focus and direct user gestures always get the current value.
        private void sync(SeekBar slider, boolean force) {
            double min = item.optDouble("min"), max = item.optDouble("max");
            int value = (int) Math.round(STEPS * (item.optDouble("value") - min) / Math.max(0.000001, max - min));
            NativeRange.update(slider, value, STEPS, force, null);
        }

        @Override public void onProgressChanged(SeekBar slider, int value, boolean fromUser) {
            if (fromUser) scene.act(item, (double) value / STEPS, tracking ? true : null);
        }
        @Override public void onStartTrackingTouch(SeekBar slider) {
            sync(slider, true);
            tracking = true;
            scene.act(item, (double) slider.getProgress() / STEPS, true);
        }
        @Override public void onStopTrackingTouch(SeekBar slider) {
            tracking = false;
            scene.act(item, (double) slider.getProgress() / STEPS, false);
        }
    }
}
