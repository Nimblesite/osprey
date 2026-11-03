package org.ospreylang.casting.press;

import android.app.Activity;
import android.app.Instrumentation;
import android.app.UiAutomation;
import android.os.Bundle;
import android.view.accessibility.AccessibilityNodeInfo;

/** Finds a node whose accessible label starts with the words given and, unless only looking, performs ACTION_CLICK, as a screen reader's double tap does. */
public final class Press extends Instrumentation {
    private String label;
    private String role;
    private boolean look;

    @Override public void onCreate(Bundle arguments) {
        label = arguments.getString("label");
        role = arguments.getString("role", "Button");
        look = arguments.containsKey("look");
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        result.putString("pressed", press());
        finish(Activity.RESULT_OK, result);
    }

    /** The node labelled with these words, in the role asked for, anywhere under `node`. */
    private AccessibilityNodeInfo button(AccessibilityNodeInfo node) {
        CharSequence words = node.getText() == null ? node.getContentDescription() : node.getText();
        if (words != null && words.toString().startsWith(label) && node.getClassName().toString().endsWith(role)) return node;
        for (int index = 0; index < node.getChildCount(); index++) {
            AccessibilityNodeInfo child = node.getChild(index);
            AccessibilityNodeInfo found = child == null ? null : button(child);
            if (found != null) return found;
        }
        return null;
    }

    private String press() {
        UiAutomation automation = getUiAutomation();
        for (int attempt = 0; attempt < 50; attempt++) {
            AccessibilityNodeInfo root = automation.getRootInActiveWindow();
            AccessibilityNodeInfo button = root == null ? null : button(root);
            if (button != null && look) return "true";
            if (button != null) return button.performAction(AccessibilityNodeInfo.ACTION_CLICK) ? "true" : "refused";
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
