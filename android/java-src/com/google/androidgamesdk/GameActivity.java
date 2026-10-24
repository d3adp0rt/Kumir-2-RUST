/* Minimal replacement for Google's GameActivity Java glue.
 * Based on AOSP GameActivity.java (Apache-2.0), rewritten to avoid
 * androidx.appcompat / androidx.core runtime dependencies: the activity
 * extends android.app.Activity directly and uses framework APIs, with tiny
 * androidx.core stubs (see androidx/ sources in this directory) that the
 * gametextinput classes from the AAR call at runtime.
 */
package com.google.androidgamesdk;

import static android.view.inputmethod.EditorInfo.IME_ACTION_DONE;
import static android.view.inputmethod.EditorInfo.IME_FLAG_NO_FULLSCREEN;

import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.content.res.AssetManager;
import android.content.res.Configuration;
import android.graphics.PixelFormat;
import android.os.Build;
import android.os.Build.VERSION;
import android.os.Build.VERSION_CODES;
import android.os.Bundle;
import android.text.InputType;
import android.util.Log;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewTreeObserver.OnGlobalLayoutListener;
import android.view.WindowManager;
import android.view.WindowManager.LayoutParams;
import android.view.WindowInsets;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.FrameLayout;

import androidx.core.graphics.Insets;

public class GameActivity extends android.app.Activity
        implements SurfaceHolder.Callback2,
        com.google.androidgamesdk.gametextinput.Listener,
        View.OnApplyWindowInsetsListener,
        OnGlobalLayoutListener {
    private static final String LOG_TAG = "GameActivity";

    private static final String DEFAULT_NATIVE_LIB_NAME = "main";

    public static final String META_DATA_LIB_NAME = "android.app.lib_name";

    private static final String KEY_NATIVE_SAVED_STATE = "android:native_state";

    protected int contentViewId;

    private EditorInfo imeEditorInfo;

    private boolean softwareKeyboardVisible = false;

    protected InputEnabledSurfaceView mSurfaceView;

    protected boolean processMotionEvent(MotionEvent event) {
        if (isNativeDestroyed()) {
            return false;
        }
        int action = (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) ? event.getActionButton() : 0;
        int cls = (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ? event.getClassification() : 0;

        return onTouchEventNative(nativeHandle, event, event.getPointerCount(),
                event.getHistorySize(), event.getDeviceId(), event.getSource(), event.getAction(),
                event.getEventTime(), event.getDownTime(), event.getFlags(), event.getMetaState(),
                action, event.getButtonState(), cls, event.getEdgeFlags(), event.getXPrecision(),
                event.getYPrecision());
    }

    @Override
    public boolean onKeyUp(final int keyCode, KeyEvent event) {
        if (isNativeDestroyed()) {
            return false;
        }
        if (onKeyUpNative(nativeHandle, event)) {
            return true;
        } else {
            return super.onKeyUp(keyCode, event);
        }
    }

    @Override
    public boolean onKeyDown(final int keyCode, KeyEvent event) {
        if (isNativeDestroyed()) {
            return false;
        }
        if (onKeyDownNative(nativeHandle, event)) {
            return true;
        } else {
            return super.onKeyDown(keyCode, event);
        }
    }

    @Override
    public void stateChanged(com.google.androidgamesdk.gametextinput.State newState,
            boolean dismissed) {
        if (isNativeDestroyed()) {
            return;
        }
        onTextInputEventNative(nativeHandle, newState);
    }

    @Override
    public void onGlobalLayout() {
        if (isNativeDestroyed()) {
            return;
        }
        mSurfaceView.getLocationInWindow(mLocation);
        int w = mSurfaceView.getWidth();
        int h = mSurfaceView.getHeight();

        if (mLocation[0] != mLastContentX || mLocation[1] != mLastContentY
                || w != mLastContentWidth || h != mLastContentHeight) {
            mLastContentX = mLocation[0];
            mLastContentY = mLocation[1];
            mLastContentWidth = w;
            mLastContentHeight = h;

            onContentRectChangedNative(nativeHandle, mLastContentX, mLastContentY,
                    mLastContentWidth, mLastContentHeight);
        }
    }

    public void setTextInputState(com.google.androidgamesdk.gametextinput.State s) {
        if (mSurfaceView == null)
            return;

        if (mSurfaceView.mInputConnection == null)
            Log.w(LOG_TAG, "No input connection has been set yet");
        else
            mSurfaceView.mInputConnection.setState(s);
    }

    static private GameActivity nativeHandleOwner = null;
    static private long nativeHandle = 0;

    private SurfaceHolder mCurSurfaceHolder;

    protected final int[] mLocation = new int[2];
    protected int mLastContentX;
    protected int mLastContentY;
    protected int mLastContentWidth;
    protected int mLastContentHeight;

    protected native long initializeNativeCode(String internalDataPath, String obbPath,
            String externalDataPath, AssetManager assetMgr, byte[] savedState,
            Configuration config);

    protected native String getDlError();

    protected native void terminateNativeCode(long handle);

    protected native void onStartNative(long handle);

    protected native void onResumeNative(long handle);

    protected native byte[] onSaveInstanceStateNative(long handle);

    protected native void onPauseNative(long handle);

    protected native void onStopNative(long handle);

    protected native void onConfigurationChangedNative(long handle, Configuration newConfig);

    protected native void onTrimMemoryNative(long handle, int level);

    protected native void onWindowFocusChangedNative(long handle, boolean focused);

    protected native void onSurfaceCreatedNative(long handle, Surface surface);

    protected native void onSurfaceChangedNative(
            long handle, Surface surface, int format, int width, int height);

    protected native void onSurfaceRedrawNeededNative(long handle, Surface surface);

    protected native void onSurfaceDestroyedNative(long handle);

    protected native boolean onTouchEventNative(long handle, MotionEvent motionEvent,
            int pointerCount, int historySize, int deviceId, int source, int action, long eventTime,
            long downTime, int flags, int metaState, int actionButton, int buttonState,
            int classification, int edgeFlags, float precisionX, float precisionY);

    protected native boolean onKeyDownNative(long handle, KeyEvent keyEvent);

    protected native boolean onKeyUpNative(long handle, KeyEvent keyEvent);

    protected native void onTextInputEventNative(long handle,
            com.google.androidgamesdk.gametextinput.State softKeyboardEvent);

    protected native void setInputConnectionNative(long handle,
            com.google.androidgamesdk.gametextinput.InputConnection c);

    protected native void onWindowInsetsChangedNative(long handle);

    protected native void onContentRectChangedNative(long handle, int x, int y, int w, int h);

    protected native void onSoftwareKeyboardVisibilityChangedNative(long handle, boolean visible);

    protected native void onEditorActionNative(long handle, int action);

    public long getGameActivityNativeHandle() {
        if (isNativeDestroyed()) {
            return nativeHandle;
        }
        return 0;
    }

    protected InputEnabledSurfaceView createSurfaceView() {
        return new InputEnabledSurfaceView(this);
    }

    protected void onCreateSurfaceView() {
        mSurfaceView = createSurfaceView();

        if (mSurfaceView == null) {
            return;
        }

        FrameLayout frameLayout = new FrameLayout(this);
        contentViewId = View.generateViewId();
        frameLayout.setId(contentViewId);
        frameLayout.addView(mSurfaceView);

        setContentView(frameLayout);

        mSurfaceView.getHolder().addCallback(this);

        mSurfaceView.setOnApplyWindowInsetsListener(this);
    }

    protected void onSetUpWindow() {
        getWindow().setFormat(PixelFormat.RGB_565);
        getWindow().setSoftInputMode(LayoutParams.SOFT_INPUT_STATE_UNSPECIFIED
                | LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
        // Edge-to-edge: окно рисует и под статус-баром/вырезом. Без этого
        // после скрытия статус-бара сверху остаётся пустая чёрная полоса.
        androidx.core.view.WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        try {
            java.io.File tf = new java.io.File(getFilesDir(), "trace.log");
            if (tf.exists()) {
                String prev = readAll(tf);
                if (!prev.contains("onCreate_end")) {
                    // Previous launch died: show what we know and stop here.
                    showCrashScreen(prev);
                    return;
                }
                // Fresh run: reset trace
                java.io.FileWriter w0 = new java.io.FileWriter(tf, false);
                w0.close();
            }
        } catch (Throwable t) {
            Log.e(LOG_TAG, "trace check failed", t);
        }

        final Thread.UncaughtExceptionHandler prevHandler =
                Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler((Thread thread, Throwable e) -> {
            traceAppend("UNCAUGHT: " + e + "\n" + android.util.Log.getStackTraceString(e));
            if (prevHandler != null) {
                prevHandler.uncaughtException(thread, e);
            }
        });

        try {
            traceAppend("onCreate");
            onCreateSurfaceView();
            if (mSurfaceView != null) {
                traceAppend("surface_view_ok");
                mSurfaceView.getViewTreeObserver().addOnGlobalLayoutListener(this);
            } else {
                traceAppend("surface_view_null");
            }
            onSetUpWindow();
            traceAppend("window_ok");

            String libname = new String(DEFAULT_NATIVE_LIB_NAME);
            ActivityInfo ai;
            try {
                ai = getPackageManager().getActivityInfo(
                        getIntent().getComponent(), PackageManager.GET_META_DATA);
                if (ai.metaData != null) {
                    String ln = ai.metaData.getString(META_DATA_LIB_NAME);
                    if (ln != null)
                        libname = ln;
                }
            } catch (PackageManager.NameNotFoundException e) {
                throw new RuntimeException("Error getting activity info", e);
            }

            String path = null;
            dalvik.system.BaseDexClassLoader classLoader = (dalvik.system.BaseDexClassLoader) getClassLoader();
            path = classLoader.findLibrary(libname);
            traceAppend("lib_search=" + libname + " found=" + (path != null));

            if (path != null) {
                System.loadLibrary(libname);
                traceAppend("lib_loaded");
            } else if (!libname.equals(DEFAULT_NATIVE_LIB_NAME)) {
                throw new IllegalArgumentException("unable to find native library lib" + libname
                        + ".so using classloader");
            } else {
                Log.i(LOG_TAG, "Application should have loaded lib" + libname
                        + ".so explicitly by now. ");
            }

            traceAppend("native_init_start");
            byte[] nativeSavedState = savedInstanceState != null
                    ? savedInstanceState.getByteArray(KEY_NATIVE_SAVED_STATE)
                    : null;
            java.io.File extDir = null;
            java.io.File[] extPaths = getExternalFilesDirs(null);
            if (extPaths != null && extPaths.length > 0) {
                extDir = extPaths[0];
            }

            if (nativeHandle != 0) {
                terminateNativeCode(nativeHandle);
            }
            nativeHandle = initializeNativeCode(getAbsolutePath(getFilesDir()),
                    getAbsolutePath(getObbDir()), getAbsolutePath(extDir), getAssets(),
                    nativeSavedState, getResources().getConfiguration());
            nativeHandleOwner = this;

            if (nativeHandle == 0) {
                throw new UnsatisfiedLinkError("native_handle==0, dlError=" + getDlError());
            }
            traceAppend("native_init_ok handle=" + nativeHandle);

            if (mSurfaceView != null) {
                setInputConnectionNative(nativeHandle, mSurfaceView.mInputConnection);
                traceAppend("input_connection_set");

                if (VERSION.SDK_INT >= VERSION_CODES.O) {
                    mSurfaceView.setOnCapturedPointerListener(new View.OnCapturedPointerListener() {
                        @Override
                        public boolean onCapturedPointer(View v, MotionEvent event) {
                            if (processMotionEvent(event)) {
                                return true;
                            }
                            return false;
                        }
                    });
                }

                mSurfaceView.setOnGenericMotionListener(new View.OnGenericMotionListener() {
                    @Override
                    public boolean onGenericMotion(View v, MotionEvent event) {
                        if (processMotionEvent(event)) {
                            return true;
                        }
                        return false;
                    }
                });

                mSurfaceView.setOnTouchListener(new View.OnTouchListener() {
                    @Override
                    public boolean onTouch(View v, MotionEvent event) {
                        if (processMotionEvent(event)) {
                            return true;
                        }
                        return false;
                    }
                });
                traceAppend("listeners_ok");
            }
        } catch (Throwable t) {
            traceAppend("onCreate_FAIL: " + t + "\n"
                    + android.util.Log.getStackTraceString(t));
            showCrashScreen(android.util.Log.getStackTraceString(t));
            return;
        }
        traceAppend("onCreate_end");
    }

    private void showCrashScreen(String detail) {
        try {
            android.widget.TextView tv = new android.widget.TextView(this);
            tv.setText(detail);
            tv.setTextSize(android.util.TypedValue.COMPLEX_UNIT_SP, 10f);
            tv.setPadding(20, 20, 20, 20);
            setContentView(tv);
        } catch (Throwable t) {
            Log.e(LOG_TAG, "crash screen failed", t);
        }
    }

    private void traceAppend(String line) {
        try {
            java.io.File f = new java.io.File(getFilesDir(), "trace.log");
            java.io.Writer w = new java.io.FileWriter(f, true);
            w.append(line).append('\n');
            w.close();
        } catch (Throwable t) {
            Log.e("GameActivity", "trace failed: " + t);
        }
    }

    private static String readAll(java.io.File f) {
        try {
            java.io.BufferedReader r = new java.io.BufferedReader(new java.io.FileReader(f));
            StringBuilder sb = new StringBuilder();
            String line;
            while ((line = r.readLine()) != null) {
                sb.append(line).append('\n');
            }
            r.close();
            return sb.toString();
        } catch (Throwable t) {
            return "readAll failed: " + t;
        }
    }

    private static String getAbsolutePath(java.io.File file) {
        return (file != null) ? file.getAbsolutePath() : null;
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
        if (isNativeDestroyed()) {
            return;
        }
        nativeHandleOwner = null;
        if (mCurSurfaceHolder != null) {
            onSurfaceDestroyedNative(nativeHandle);
            mCurSurfaceHolder = null;
        }

        terminateNativeCode(nativeHandle);
        nativeHandle = 0;
    }

    @Override
    protected void onPause() {
        super.onPause();
        if (isNativeDestroyed()) {
            return;
        }
        onPauseNative(nativeHandle);
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (isNativeDestroyed()) {
            return;
        }
        onResumeNative(nativeHandle);
    }

    @Override
    protected void onSaveInstanceState(Bundle outState) {
        super.onSaveInstanceState(outState);
        if (isNativeDestroyed()) {
            return;
        }
        byte[] state = onSaveInstanceStateNative(nativeHandle);
        if (state != null) {
            outState.putByteArray(KEY_NATIVE_SAVED_STATE, state);
        }
    }

    @Override
    protected void onStart() {
        super.onStart();
        if (isNativeDestroyed()) {
            return;
        }
        onStartNative(nativeHandle);
    }

    @Override
    protected void onStop() {
        super.onStop();
        if (isNativeDestroyed()) {
            return;
        }
        onStopNative(nativeHandle);
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        if (isNativeDestroyed()) {
            return;
        }
        onConfigurationChangedNative(nativeHandle, newConfig);
    }

    @Override
    public void onTrimMemory(int level) {
        super.onTrimMemory(level);
        if (isNativeDestroyed()) {
            return;
        }
        onTrimMemoryNative(nativeHandle, level);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (isNativeDestroyed()) {
            return;
        }
        onWindowFocusChangedNative(nativeHandle, hasFocus);
        if (hasFocus && mSurfaceView != null) {
            mSurfaceView.setFocusableInTouchMode(true);
            mSurfaceView.requestFocus();
        }
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        if (isNativeDestroyed()) {
            return;
        }
        mCurSurfaceHolder = holder;
        onSurfaceCreatedNative(nativeHandle, holder.getSurface());
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        if (isNativeDestroyed()) {
            return;
        }
        mCurSurfaceHolder = holder;
        onSurfaceChangedNative(nativeHandle, holder.getSurface(), format, width, height);
    }

    @Override
    public void surfaceRedrawNeeded(SurfaceHolder holder) {
        if (isNativeDestroyed()) {
            return;
        }
        mCurSurfaceHolder = holder;
        onSurfaceRedrawNeededNative(nativeHandle, holder.getSurface());
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        if (isNativeDestroyed()) {
            return;
        }
        mCurSurfaceHolder = null;
        onSurfaceDestroyedNative(nativeHandle);
    }

    void setWindowFlags(int flags, int mask) {
        getWindow().setFlags(flags, mask);
    }

    void setWindowFormat(int format) {
        getWindow().setFormat(format);
    }

    @Override
    public android.view.WindowInsets onApplyWindowInsets(View v, android.view.WindowInsets insets) {
        if (isNativeDestroyed()) {
            return insets;
        }
        boolean keyboardVisible = isSoftwareKeyboardVisible(insets);
        if (keyboardVisible != softwareKeyboardVisible) {
            softwareKeyboardVisible = keyboardVisible;
            onSoftwareKeyboardVisibilityChanged(keyboardVisible);
        }
        onWindowInsetsChangedNative(nativeHandle);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.KITKAT_WATCH) {
            v.onApplyWindowInsets(insets);
        }
        return insets;
    }

    androidx.core.graphics.Insets mapInsets(android.view.WindowInsets wi, int type) {
        int left = 0, top = 0, right = 0, bottom = 0;
        if (Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets fi = wi.getInsets(type);
            return new androidx.core.graphics.Insets(fi.left, fi.top, fi.right, fi.bottom);
        }
        if ((type & TYPE_STATUS_BARS) != 0) {
            top += wi.getSystemWindowInsetTop();
        }
        if ((type & (TYPE_NAVIGATION_BARS | TYPE_SYSTEM_GESTURES | TYPE_TAPPABLE_ELEMENT)) != 0) {
            left += wi.getSystemWindowInsetLeft();
            right += wi.getSystemWindowInsetRight();
            bottom += wi.getSystemWindowInsetBottom();
        }
        return new androidx.core.graphics.Insets(left, top, right, bottom);
    }

    private static final int TYPE_STATUS_BARS = 1;
    private static final int TYPE_NAVIGATION_BARS = 2;
    private static final int TYPE_CAPTION_BAR = 4;
    private static final int TYPE_IME = 8;
    private static final int TYPE_SYSTEM_GESTURES = 16;
    private static final int TYPE_MANDATORY_SYSTEM_GESTURES = 32;
    private static final int TYPE_TAPPABLE_ELEMENT = 64;
    private static final int TYPE_DISPLAY_CUTOUT = 128;

    public androidx.core.graphics.Insets getWindowInsets(int type) {
        if (mSurfaceView == null) {
            return null;
        }
        android.view.WindowInsets wi = mSurfaceView.getRootWindowInsets();
        if (wi == null) {
            return null;
        }
        androidx.core.graphics.Insets res = mapInsets(wi, type);
        if (res.left == 0 && res.top == 0 && res.right == 0 && res.bottom == 0) {
            return null;
        }
        return res;
    }

    public androidx.core.graphics.Insets getWaterfallInsets() {
        return null;
    }

    // ---------------- Доступ к файлам (Android) ----------------

    public boolean hasFileAccess() {
        try {
            if (Build.VERSION.SDK_INT >= 30) {
                return android.os.Environment.isExternalStorageManager();
            }
            return checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE)
                    == PackageManager.PERMISSION_GRANTED;
        } catch (Throwable t) {
            traceAppend("file_access_check_fail: " + t);
            return false;
        }
    }

    public void requestFileAccess() {
        try {
            if (Build.VERSION.SDK_INT >= 30) {
                android.content.Intent intent = new android.content.Intent(
                        android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                        android.net.Uri.parse("package:" + getPackageName()));
                startActivity(intent);
            } else {
                requestPermissions(
                        new String[]{android.Manifest.permission.WRITE_EXTERNAL_STORAGE}, 1001);
            }
        } catch (Throwable t) {
            traceAppend("file_access_request_fail: " + t);
            try {
                startActivity(new android.content.Intent(
                        android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION));
            } catch (Throwable t2) {
                traceAppend("file_access_request_fail2: " + t2);
            }
        }
    }

    /** Каталог для сохранения: «Документы», если есть доступ, иначе приватный. */
    public String getStorageDir() {
        try {
            java.io.File dir = null;
            boolean granted = hasFileAccess();
            if (granted) {
                dir = android.os.Environment.getExternalStoragePublicDirectory(
                        android.os.Environment.DIRECTORY_DOCUMENTS);
            }
            if (dir == null) {
                java.io.File ext = getExternalFilesDir(null);
                dir = (ext != null) ? ext : getFilesDir();
            }
            if (dir != null && !dir.exists()) {
                dir.mkdirs();
            }
            if (dir != null) {
                return dir.getAbsolutePath();
            }
        } catch (Throwable t) {
            traceAppend("storage_dir_fail: " + t);
        }
        java.io.File internal = getFilesDir();
        return internal != null ? internal.getAbsolutePath() : "/";
    }

    @Override
    public void onImeInsetsChanged(androidx.core.graphics.Insets insets) {
    }

    @Override
    public void onSoftwareKeyboardVisibilityChanged(boolean visible) {
        if (isNativeDestroyed()) {
            return;
        }
        onSoftwareKeyboardVisibilityChangedNative(nativeHandle, visible);
    }

    @Override
    public void onEditorAction(int action) {
        traceAppend("editor_action=" + action);
        if (isNativeDestroyed()) {
            return;
        }
        onEditorActionNative(nativeHandle, action);
        // Некоторые IME не вставляют перевод строки при нажатии Enter
        // (присылают только action-событие). Вставляем сами.
        if (mSurfaceView != null && mSurfaceView.mInputConnection != null) {
            try {
                android.text.Editable ed = mSurfaceView.mInputConnection.getEditable();
                if (ed != null && ed.length() > 0) {
                    mSurfaceView.mInputConnection.commitText("\n", 1);
                } else {
                    // Пустой буфер: действие — просто перенести строку.
                    com.google.androidgamesdk.gametextinput.State st =
                            new com.google.androidgamesdk.gametextinput.State("\n", 1, 1, -1, -1);
                    mSurfaceView.mInputConnection.setState(st);
                }
            } catch (Throwable t) {
                traceAppend("enter_fail: " + t);
            }
        }
    }

    public EditorInfo getImeEditorInfo() {
        if (imeEditorInfo == null) {
            imeEditorInfo = new EditorInfo();
            imeEditorInfo.inputType = InputType.TYPE_CLASS_TEXT;
            imeEditorInfo.actionId = IME_ACTION_DONE;
            imeEditorInfo.imeOptions = IME_ACTION_DONE | IME_FLAG_NO_FULLSCREEN;
        }
        return imeEditorInfo;
    }

    public void setImeEditorInfo(EditorInfo info) {
        imeEditorInfo = info;
        mSurfaceView.mInputConnection.setEditorInfo(info);
    }

    public void setImeEditorInfoFields(final int inputType, final int actionId, final int imeOptions) {
        // Вызывается из game-потока; трогать View/InputConnection можно
        // только на UI-потоке.
        if (android.os.Looper.myLooper() != android.os.Looper.getMainLooper()) {
            runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    setImeEditorInfoFields(inputType, actionId, imeOptions);
                }
            });
            return;
        }
        EditorInfo info = getImeEditorInfo();
        info.inputType = inputType;
        info.actionId = actionId;
        info.imeOptions = imeOptions;
        mSurfaceView.mInputConnection.setEditorInfo(info);
        traceAppend("ime_info inputType=" + inputType + " actionId=" + actionId
                + " imeOptions=" + imeOptions);
    }

    protected class InputEnabledSurfaceView extends SurfaceView {
        public InputEnabledSurfaceView(GameActivity context) {
            super(context);
            EditorInfo editorInfo = context.getImeEditorInfo();
            mInputConnection = new com.google.androidgamesdk.gametextinput.InputConnection(context,
                    this, new com.google.androidgamesdk.gametextinput.Settings(editorInfo,
                            /* forwardKeyEvents= */ editorInfo.inputType == InputType.TYPE_NULL))
                    .setListener(context);
        }

        com.google.androidgamesdk.gametextinput.InputConnection mInputConnection;

        @Override
        public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
            if (outAttrs != null) {
                GameActivity.this.traceAppend("ime_conn inputType=" + outAttrs.inputType
                        + " actionId=" + outAttrs.actionId
                        + " imeOptions=" + outAttrs.imeOptions);
            }
            if (!mInputConnection.getSoftKeyboardActive()) {
                return null;
            }
            if (outAttrs != null) {
                com.google.androidgamesdk.gametextinput.GameTextInput.copyEditorInfo(
                        mInputConnection.getEditorInfo(), outAttrs);
            }
            return mInputConnection;
        }

        public EditorInfo getEditorInfo() {
            return mInputConnection.getEditorInfo();
        }

        public void setEditorInfo(EditorInfo e) {
            mInputConnection.setEditorInfo(e);
        }
    }

    private boolean isSoftwareKeyboardVisible(android.view.WindowInsets insets) {
        if (insets == null) {
            return false;
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            return insets.isVisible(android.view.WindowInsets.Type.ime());
        }
        return insets.getSystemWindowInsetBottom() > 0;
    }

    private boolean isNativeDestroyed() {
        return nativeHandleOwner != this;
    }
}
