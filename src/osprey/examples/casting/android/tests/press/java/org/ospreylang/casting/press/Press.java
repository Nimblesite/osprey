package org.ospreylang.casting.press;

import android.app.Activity;
import android.app.Instrumentation;
import android.app.UiAutomation;
import android.accessibilityservice.AccessibilityServiceInfo;
import android.view.accessibility.AccessibilityWindowInfo;
import android.graphics.Rect;
import android.os.Bundle;
import android.view.accessibility.AccessibilityNodeInfo;

/** Finds a node whose accessible label starts with the words given and, unless only looking, performs ACTION_CLICK, as a screen reader's double tap does. A bounds query returns the live screen rectangle instead. */
public final class Press extends Instrumentation {
    private String label;
    private String role;
    private boolean look;
    private boolean bounds;
    private boolean exact;

    @Override public void onCreate(Bundle arguments) {
        label = arguments.getString("label");
        bounds = arguments.containsKey("bounds");
        exact = bounds && !"prefix".equals(arguments.getString("match", "exact"));
        role = bounds ? "" : arguments.getString("role", "Button");
        look = arguments.containsKey("look");
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        result.putString("pressed", press(result));
        finish(Activity.RESULT_OK, result);
    }

    /** The node labelled with these words, in the role asked for, anywhere under `node`. */
    private AccessibilityNodeInfo button(AccessibilityNodeInfo node) {
        CharSequence words = node.getText() == null ? node.getContentDescription() : node.getText();
        CharSequence kind = node.getClassName();
        if (words != null && (exact ? words.toString().equals(label) : words.toString().startsWith(label)) && (role.isEmpty() || kind != null && kind.toString().endsWith(role))) return node;
        for (int index = 0; index < node.getChildCount(); index++) {
            AccessibilityNodeInfo child = node.getChild(index);
            AccessibilityNodeInfo found = child == null ? null : button(child);
            if (found != null) return found;
        }
        return null;
    }

    private String press(Bundle result) {
        UiAutomation automation = getUiAutomation();
        AccessibilityServiceInfo service = automation.getServiceInfo();
        service.flags |= AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS;
        automation.setServiceInfo(service);
        for (int attempt = 0; attempt < 50; attempt++) {
            AccessibilityNodeInfo root = automation.getRootInActiveWindow();
            AccessibilityNodeInfo button = root == null ? null : button(root);
            if (button != null && look) return "true";
            if (button != null && bounds) {
                Rect area = new Rect();
                button.getBoundsInScreen(area);
                Rect window = new Rect();
                AccessibilityWindowInfo owner = button.getWindow();
                if (owner != null) owner.getBoundsInScreen(window);
                if (!area.isEmpty() && window.contains(area.centerX(), area.centerY())) {
                    result.putString("selected", Boolean.toString(button.isSelected()));
                    result.putString("bounds", area.left + " " + area.top + " " + area.right + " " + area.bottom);
                    return "true";
                }
            } else if (button != null) return button.performAction(AccessibilityNodeInfo.ACTION_CLICK) ? "true" : "refused";
            try {
                Thread.sleep(200);
            } catch (InterruptedException stopped) {
                return "interrupted";
            }
        }
        AccessibilityNodeInfo root = automation.getRootInActiveWindow();
        StringBuilder seen = new StringBuilder("not found in");
        for (int index = 0; root != null && index < root.getChildCount(); index++) {
            AccessibilityNodeInfo child = root.getChild(index);
            seen.append(" | ").append(child == null ? "gone" : child.getClassName() + ":" + child.getText() + ":" + child.getContentDescription() + ":" + child.getChildCount());
        }
        return seen.toString();
    }
}
