package org.auroraviewer.aurora;

import android.app.NativeActivity;
import android.os.Build;
import android.os.Bundle;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;

public class AuroraActivity extends NativeActivity {
    private static native void nativeOnGamepadAxis(float lx, float ly, float rx, float ry);
    private static native void nativeOnGamepadKey(int keyCode, boolean down);

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
        int source = event.getSource();
        int keyCode = event.getKeyCode();
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
