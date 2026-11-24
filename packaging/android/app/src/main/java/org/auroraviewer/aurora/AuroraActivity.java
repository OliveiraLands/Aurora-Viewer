package org.auroraviewer.aurora;

import android.app.NativeActivity;
import android.content.Context;
import android.graphics.Color;
import android.graphics.Insets;
import android.graphics.Typeface;
import android.os.Build;
import android.os.Bundle;
import android.util.TypedValue;
import android.view.DisplayCutout;
import android.view.Gravity;
import android.view.HapticFeedbackConstants;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.view.WindowInsetsAnimation;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;
import java.util.List;

public class AuroraActivity extends NativeActivity {
    static {
        System.loadLibrary("aurora_viewer");
    }

    private static native void nativeOnGamepadAxis(float lx, float ly, float rx, float ry);
    private static native void nativeOnGamepadKey(int keyCode, boolean down);
    private static native boolean nativeOnBackPressed();
    private static native void nativeOnWindowInsets(int top, int bottom, int left, int right, int imeBottom);

    private FrameLayout loadingOverlay;
    private boolean backConsumedDown;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams lp = getWindow().getAttributes();
            lp.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(lp);
        }
        applyImmersiveFullscreen();
        installWindowInsetsBridge();
        showLoadingOverlay();
    }

    private void installWindowInsetsBridge() {
        View decor = getWindow().getDecorView();
        decor.setOnApplyWindowInsetsListener((v, insets) -> {
            dispatchWindowInsets(insets);
            return v.onApplyWindowInsets(insets);
        });
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            decor.setWindowInsetsAnimationCallback(
                    new WindowInsetsAnimation.Callback(
                            WindowInsetsAnimation.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                        @Override
                        public WindowInsets onProgress(
                                WindowInsets insets,
                                List<WindowInsetsAnimation> runningAnimations) {
                            dispatchWindowInsets(insets);
                            return insets;
                        }
                    });
        }
        decor.requestApplyInsets();
    }

    @SuppressWarnings("deprecation")
    private void dispatchWindowInsets(WindowInsets insets) {
        if (insets == null) {
            return;
        }
        int top = 0;
        int bottom = 0;
        int left = 0;
        int right = 0;
        int imeBottom = 0;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Insets safe = insets.getInsets(
                    WindowInsets.Type.displayCutout() | WindowInsets.Type.systemBars());
            top = safe.top;
            bottom = safe.bottom;
            left = safe.left;
            right = safe.right;
            imeBottom = insets.getInsets(WindowInsets.Type.ime()).bottom;
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                DisplayCutout cutout = insets.getDisplayCutout();
                if (cutout != null) {
                    top = cutout.getSafeInsetTop();
                    bottom = cutout.getSafeInsetBottom();
                    left = cutout.getSafeInsetLeft();
                    right = cutout.getSafeInsetRight();
                }
            }
            int sysBottom = insets.getSystemWindowInsetBottom();
            int stableBottom = insets.getStableInsetBottom();
            if (sysBottom > stableBottom) {
                imeBottom = sysBottom;
            }
        }
        nativeOnWindowInsets(top, bottom, left, right, imeBottom);
    }

    public void showSoftKeyboard() {
        runOnUiThread(() -> {
            View decor = getWindow().getDecorView();
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                WindowInsetsController controller = getWindow().getInsetsController();
                if (controller != null) {
                    controller.show(WindowInsets.Type.ime());
                    return;
                }
            }
            InputMethodManager imm =
                    (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
            if (imm != null) {
                imm.showSoftInput(decor, InputMethodManager.SHOW_IMPLICIT);
            }
        });
    }

    public void hideSoftKeyboard() {
        runOnUiThread(() -> {
            View decor = getWindow().getDecorView();
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                WindowInsetsController controller = getWindow().getInsetsController();
                if (controller != null) {
                    controller.hide(WindowInsets.Type.ime());
                }
            } else {
                InputMethodManager imm =
                        (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
                if (imm != null) {
                    imm.hideSoftInputFromWindow(decor.getWindowToken(), 0);
                }
            }
            applyImmersiveFullscreen();
        });
    }

    public void performHapticFeedback(int kind) {
        runOnUiThread(() -> {
            View decor = getWindow().getDecorView();
            int flag;
            if (kind == 1) {
                flag = HapticFeedbackConstants.LONG_PRESS;
            } else if (kind == 2 && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                flag = HapticFeedbackConstants.CONFIRM;
            } else {
                flag = HapticFeedbackConstants.VIRTUAL_KEY;
            }
            decor.performHapticFeedback(flag);
        });
    }

    private int dp(int value) {
        return Math.round(TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP, value, getResources().getDisplayMetrics()));
    }

    private void showLoadingOverlay() {
        if (loadingOverlay != null) {
            return;
        }
        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.parseColor("#0E131F"));
        root.setClickable(false);
        root.setFocusable(false);

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setGravity(Gravity.CENTER_HORIZONTAL);

        ImageView logo = new ImageView(this);
        int iconRes = getApplicationInfo().icon;
        logo.setImageResource(iconRes != 0 ? iconRes : R.mipmap.ic_launcher);
        LinearLayout.LayoutParams logoParams = new LinearLayout.LayoutParams(dp(96), dp(96));
        logoParams.bottomMargin = dp(16);
        column.addView(logo, logoParams);

        TextView title = new TextView(this);
        title.setText(R.string.app_name);
        title.setTextColor(Color.parseColor("#F2F5FA"));
        title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 22);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        LinearLayout.LayoutParams titleParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        titleParams.bottomMargin = dp(20);
        column.addView(title, titleParams);

        ProgressBar spinner = new ProgressBar(this);
        spinner.setIndeterminate(true);
        LinearLayout.LayoutParams spinnerParams = new LinearLayout.LayoutParams(dp(36), dp(36));
        spinnerParams.bottomMargin = dp(12);
        column.addView(spinner, spinnerParams);

        TextView status = new TextView(this);
        status.setText(R.string.loading_status);
        status.setTextColor(Color.parseColor("#9BA8C2"));
        status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        column.addView(status, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));

        FrameLayout.LayoutParams centerParams = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, Gravity.CENTER);
        root.addView(column, centerParams);

        addContentView(root, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        loadingOverlay = root;
    }

    public void hideLoadingOverlay() {
        runOnUiThread(() -> {
            final FrameLayout overlay = loadingOverlay;
            if (overlay == null) {
                return;
            }
            loadingOverlay = null;
            overlay.animate()
                    .alpha(0f)
                    .setDuration(250)
                    .withEndAction(() -> {
                        ViewGroup parent = (ViewGroup) overlay.getParent();
                        if (parent != null) {
                            parent.removeView(overlay);
                        }
                    })
                    .start();
        });
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            applyImmersiveFullscreen();
        }
    }

    @SuppressWarnings("deprecation")
    private void applyImmersiveFullscreen() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            getWindow().setDecorFitsSystemWindows(false);
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.statusBars() | WindowInsets.Type.navigationBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            View decor = getWindow().getDecorView();
            decor.setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_FULLSCREEN);
        }
    }

    @Override
    public boolean onGenericMotionEvent(MotionEvent event) {
        int source = event.getSource();
        if ((source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK
                || (source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD) {
            if (event.getAction() == MotionEvent.ACTION_MOVE) {
                float lx = event.getAxisValue(MotionEvent.AXIS_X);
                float ly = event.getAxisValue(MotionEvent.AXIS_Y);
                float hatX = event.getAxisValue(MotionEvent.AXIS_HAT_X);
                float hatY = event.getAxisValue(MotionEvent.AXIS_HAT_Y);
                if (Math.abs(lx) < 0.05f && Math.abs(hatX) > 0.1f) {
                    lx = hatX;
                }
                if (Math.abs(ly) < 0.05f && Math.abs(hatY) > 0.1f) {
                    ly = hatY;
                }
                float rx = event.getAxisValue(MotionEvent.AXIS_Z);
                float ry = event.getAxisValue(MotionEvent.AXIS_RZ);
                if (Math.abs(rx) < 0.05f) {
                    rx = event.getAxisValue(MotionEvent.AXIS_RX);
                }
                if (Math.abs(ry) < 0.05f) {
                    ry = event.getAxisValue(MotionEvent.AXIS_RY);
                }
                nativeOnGamepadAxis(lx, ly, rx, ry);
                return true;
            }
        }
        return super.onGenericMotionEvent(event);
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        int keyCode = event.getKeyCode();
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            int action = event.getAction();
            if (action == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0) {
                backConsumedDown = nativeOnBackPressed();
                if (backConsumedDown) {
                    return true;
                }
            } else if (action == KeyEvent.ACTION_UP && backConsumedDown) {
                backConsumedDown = false;
                return true;
            }
        }
        int source = event.getSource();
        boolean isGamepadOrDpad = (source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK
                || (source & InputDevice.SOURCE_DPAD) == InputDevice.SOURCE_DPAD
                || KeyEvent.isGamepadButton(keyCode)
                || keyCode == KeyEvent.KEYCODE_DPAD_UP
                || keyCode == KeyEvent.KEYCODE_DPAD_DOWN
                || keyCode == KeyEvent.KEYCODE_DPAD_LEFT
                || keyCode == KeyEvent.KEYCODE_DPAD_RIGHT
                || keyCode == KeyEvent.KEYCODE_DPAD_CENTER;
        if (isGamepadOrDpad) {
            int action = event.getAction();
            if (action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP) {
                nativeOnGamepadKey(keyCode, action == KeyEvent.ACTION_DOWN);
                return true;
            }
        }
        return super.dispatchKeyEvent(event);
    }
}
