package io.github.kvasfan.folitecta;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.ClipData;
import android.content.Intent;
import android.content.Context;
import android.content.res.ColorStateList;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.graphics.pdf.PdfRenderer;
import android.net.Uri;
import android.os.Bundle;
import android.os.Build;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.text.InputType;
import android.view.Gravity;
import android.view.ScaleGestureDetector;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowInsets;
import android.widget.Button;
import android.widget.EditText;
import android.widget.HorizontalScrollView;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Offline PDF reader. All file I/O and PDF rendering run on a serial worker. */
public final class MainActivity extends Activity {
    private static final int PICK_PDF = 10;
    private static final int INK = 0xff1c2130, MUTED = 0xff646c7d, ACCENT = 0xff4f46e5, BG = 0xfff3f5fa;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    // The renderer belongs exclusively to worker. UI fields below belong to the main thread.
    private PdfRenderer renderer;
    private File activeFile;
    private String documentName;
    private int pageIndex, pageCount, generation;
    private float zoom = 1;
    private boolean busy;
    private LinearLayout root, reader, welcome;
    private TextView title, status;
    private ImageView pageImage;
    private Button open, previous, next, pageNumber, share, zoomOut, zoomIn;
    private Bitmap displayed;

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        buildUi();
        if (saved != null && saved.getString("file") != null) {
            File file = new File(new File(getCacheDir(), "documents"), saved.getString("file"));
            if (file.isFile() && file.getName().matches("document-[a-f0-9-]+\\.pdf")) {
                openCached(file, saved.getString("name", getString(R.string.document)), saved.getInt("page", 0));
                return;
            }
        }
        openIntent(getIntent());
    }

    private int dp(float n) { return Math.round(n * getResources().getDisplayMetrics().density); }
    private LinearLayout column() { LinearLayout l = new LinearLayout(this); l.setOrientation(LinearLayout.VERTICAL); return l; }
    private TextView text(String value, int size, int color) {
        TextView t = new TextView(this); t.setText(value); t.setTextSize(size); t.setTextColor(color); return t;
    }
    private Button button(String label, Runnable action) {
        Button b = new Button(this); b.setText(label); b.setAllCaps(false); b.setTextColor(ACCENT);
        b.setTextColor(new ColorStateList(new int[][]{new int[]{android.R.attr.state_enabled}, new int[]{}}, new int[]{ACCENT, MUTED}));
        b.setMinHeight(dp(48)); b.setOnClickListener(v -> action.run()); return b;
    }
    private void buildUi() {
        root = column(); root.setBackgroundColor(BG);
        root.setPadding(dp(16), 0, dp(16), 0);
        root.setOnApplyWindowInsetsListener((v, insets) -> {
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                v.setPadding(bars.left + dp(16), bars.top, bars.right + dp(16), bars.bottom);
            } else {
                v.setPadding(insets.getSystemWindowInsetLeft() + dp(16), insets.getSystemWindowInsetTop(),
                    insets.getSystemWindowInsetRight() + dp(16), insets.getSystemWindowInsetBottom());
            }
            return insets;
        });
        title = text("Folitecta", 24, INK); title.setPadding(dp(4), dp(20), dp(4), dp(4));
        title.setMaxLines(2); title.setEllipsize(android.text.TextUtils.TruncateAt.END); root.addView(title);
        TextView subtitle = text(getString(R.string.tagline), 14, MUTED); subtitle.setPadding(dp(4), 0, 0, dp(12)); root.addView(subtitle);
        LinearLayout actions = new LinearLayout(this);
        open = button(getString(R.string.open), this::pickPdf);
        open.setBackgroundTintList(ColorStateList.valueOf(ACCENT)); open.setTextColor(Color.WHITE);
        actions.addView(open, new LinearLayout.LayoutParams(0, dp(52), 1));
        share = button(getString(R.string.share), this::sharePdf); share.setEnabled(false);
        actions.addView(share, new LinearLayout.LayoutParams(0, dp(52), 1)); root.addView(actions);
        status = text("", 13, MUTED); status.setPadding(dp(4), dp(6), dp(4), dp(6)); status.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE); root.addView(status);
        welcome = column(); welcome.setGravity(Gravity.CENTER); welcome.setPadding(dp(24), dp(16), dp(24), dp(16));
        ImageView mark = new ImageView(this); mark.setImageResource(R.drawable.ic_launcher); mark.setContentDescription(null);
        LinearLayout.LayoutParams logoSize = new LinearLayout.LayoutParams(dp(64), dp(64)); logoSize.bottomMargin = dp(24); welcome.addView(mark, logoSize);
        TextView intro = text(getString(R.string.intro), 17, INK); intro.setGravity(Gravity.CENTER); intro.setLineSpacing(dp(6), 1); welcome.addView(intro);
        TextView scope = text(getString(R.string.scope), 12, MUTED); scope.setGravity(Gravity.CENTER); scope.setPadding(0, dp(24), 0, 0); welcome.addView(scope);
        root.addView(welcome, new LinearLayout.LayoutParams(-1, 0, 1));
        reader = column(); reader.setVisibility(View.GONE);
        ZoomScrollView vertical = new ZoomScrollView(this); vertical.setFillViewport(true);
        HorizontalScrollView horizontal = new HorizontalScrollView(this); horizontal.setFillViewport(true);
        pageImage = new ImageView(this); pageImage.setAdjustViewBounds(true); pageImage.setScaleType(ImageView.ScaleType.FIT_CENTER);
        pageImage.setContentDescription(getString(R.string.document));
        horizontal.addView(pageImage, new android.widget.FrameLayout.LayoutParams(-1, -2)); vertical.addView(horizontal);
        reader.addView(vertical, new LinearLayout.LayoutParams(-1, 0, 1));
        ScaleGestureDetector scaling = new ScaleGestureDetector(this, new ScaleGestureDetector.SimpleOnScaleGestureListener() {
            private float factor;
            @Override public boolean onScaleBegin(ScaleGestureDetector d) { factor = zoom; return !busy; }
            @Override public boolean onScale(ScaleGestureDetector d) { factor = Math.max(1, Math.min(3, factor * d.getScaleFactor())); return true; }
            @Override public void onScaleEnd(ScaleGestureDetector d) { zoom = factor; renderPage(); }
        });
        vertical.scaling = scaling;
        LinearLayout navigation = new LinearLayout(this); navigation.setGravity(Gravity.CENTER);
        previous = button("‹", () -> { pageIndex--; renderPage(); }); previous.setContentDescription(getString(R.string.previous));
        next = button("›", () -> { pageIndex++; renderPage(); }); next.setContentDescription(getString(R.string.next));
        pageNumber = button("", this::goToPage);
        navigation.addView(previous, new LinearLayout.LayoutParams(dp(56), dp(52)));
        navigation.addView(pageNumber, new LinearLayout.LayoutParams(0, dp(52), 1));
        navigation.addView(next, new LinearLayout.LayoutParams(dp(56), dp(52))); reader.addView(navigation);
        LinearLayout zoomBar = new LinearLayout(this); zoomBar.setGravity(Gravity.CENTER);
        zoomOut = button("−", () -> { zoom = Math.max(1, zoom - .5f); renderPage(); }); zoomOut.setContentDescription(getString(R.string.zoom_out));
        zoomIn = button("+", () -> { zoom = Math.min(3, zoom + .5f); renderPage(); }); zoomIn.setContentDescription(getString(R.string.zoom_in));
        zoomBar.addView(zoomOut, new LinearLayout.LayoutParams(dp(64), dp(48)));
        zoomBar.addView(zoomIn, new LinearLayout.LayoutParams(dp(64), dp(48))); reader.addView(zoomBar);
        root.addView(reader, new LinearLayout.LayoutParams(-1, 0, 1));
        setContentView(root); root.requestApplyInsets();
    }
    private static final class ZoomScrollView extends ScrollView {
        ScaleGestureDetector scaling;
        ZoomScrollView(Context context) { super(context); }
        @Override public boolean dispatchTouchEvent(MotionEvent event) {
            if (scaling != null) {
                scaling.onTouchEvent(event);
                if (scaling.isInProgress() || event.getPointerCount() > 1) {
                    // Cancel a child's drag before consuming a pinch.
                    MotionEvent cancel = MotionEvent.obtain(event); cancel.setAction(MotionEvent.ACTION_CANCEL);
                    super.dispatchTouchEvent(cancel); cancel.recycle(); return true;
                }
            }
            return super.dispatchTouchEvent(event);
        }
    }
    private void pickPdf() {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT); i.setType("application/pdf");
        i.addCategory(Intent.CATEGORY_OPENABLE); i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        startActivityForResult(i, PICK_PDF);
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == PICK_PDF && result == RESULT_OK && data != null && data.getData() != null) importPdf(data.getData());
    }
    private void openIntent(Intent intent) {
        if (Intent.ACTION_VIEW.equals(intent.getAction()) && intent.getData() != null) importPdf(intent.getData());
        else if (Intent.ACTION_SEND.equals(intent.getAction())) {
            Uri uri = Build.VERSION.SDK_INT >= 33 ? intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri.class) : intent.getParcelableExtra(Intent.EXTRA_STREAM);
            if (uri != null) importPdf(uri);
        }
    }
    private void importPdf(Uri uri) {
        if (!"content".equals(uri.getScheme())) return;
        final int request = ++generation;
        setBusy(true, R.string.loading);
        worker.execute(() -> {
            File candidate = null;
            try {
                String name = getString(R.string.document);
                try (Cursor c = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
                    if (c != null && c.moveToFirst() && !c.isNull(0)) name = c.getString(0);
                }
                File directory = new File(getCacheDir(), "documents");
                if (!directory.isDirectory() && !directory.mkdirs()) throw new java.io.IOException();
                // Leave recent shared copies available; remove abandoned cache entries after one day.
                File[] old = directory.listFiles();
                if (old != null) for (File f : old) if (System.currentTimeMillis() - f.lastModified() > 86400000L) f.delete();
                candidate = new File(directory, "document-" + UUID.randomUUID() + ".pdf");
                try (InputStream in = getContentResolver().openInputStream(uri); FileOutputStream out = new FileOutputStream(candidate)) {
                    if (in == null) throw new java.io.IOException();
                    byte[] buffer = new byte[65536]; long total = 0; int n;
                    while ((n = in.read(buffer)) != -1) {
                        total += n; if (total > 256L * 1024 * 1024) throw new FileTooLarge();
                        out.write(buffer, 0, n);
                    }
                }
                loadOnWorker(candidate, name, 0, request);
            } catch (Exception e) {
                if (candidate != null) candidate.delete();
                final int error = e instanceof FileTooLarge ? R.string.too_large : R.string.error;
                runOnUiThread(() -> showOpenError(request, error));
            }
        });
    }
    private static final class FileTooLarge extends java.io.IOException { }
    private void openCached(File file, String name, int page) {
        final int request = ++generation; setBusy(true, R.string.loading);
        worker.execute(() -> {
            try { loadOnWorker(file, name, page, request); }
            catch (Exception e) { runOnUiThread(() -> showOpenError(request, R.string.error)); }
        });
    }
    private void loadOnWorker(File file, String name, int page, int request) throws java.io.IOException {
        ParcelFileDescriptor fd = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
        PdfRenderer opened;
        try { opened = new PdfRenderer(fd); } catch (Exception e) { fd.close(); throw e; }
        if (opened.getPageCount() == 0) { opened.close(); throw new java.io.IOException(); }
        if (renderer != null) renderer.close(); renderer = opened;
        final int count = renderer.getPageCount();
        runOnUiThread(() -> {
            if (isDestroyed() || request != generation) return;
            activeFile = file; documentName = name; pageCount = count; pageIndex = Math.max(0, Math.min(count - 1, page)); zoom = 1;
            title.setText(name); welcome.setVisibility(View.GONE); reader.setVisibility(View.VISIBLE); renderPage();
        });
    }
    private void showOpenError(int request, int message) {
        if (isDestroyed() || request != generation) return;
        setBusy(false, 0);
        new AlertDialog.Builder(this).setTitle(R.string.error_title).setMessage(message).setPositiveButton(android.R.string.ok, null).show();
    }
    private void renderPage() {
        if (activeFile == null) return;
        final int request = ++generation, index = pageIndex;
        final int width = Math.max(1, Math.min(3072, Math.round((getResources().getDisplayMetrics().widthPixels - dp(32)) * zoom)));
        setBusy(true, R.string.rendering);
        worker.execute(() -> {
            Bitmap bitmap = null;
            try (PdfRenderer.Page page = renderer.openPage(index)) {
                // Bound both dimensions and total pixels for malformed or unusually tall pages.
                float scale = Math.min((float) width / page.getWidth(), 4096f / page.getHeight());
                scale = Math.min(scale, (float) Math.sqrt(8_000_000.0 / ((double) page.getWidth() * page.getHeight())));
                bitmap = Bitmap.createBitmap(Math.max(1, Math.round(page.getWidth() * scale)), Math.max(1, Math.round(page.getHeight() * scale)), Bitmap.Config.ARGB_8888);
                bitmap.eraseColor(Color.WHITE); page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY);
                final Bitmap image = bitmap;
                runOnUiThread(() -> {
                    if (isDestroyed() || request != generation) { image.recycle(); return; }
                    Bitmap old = displayed; displayed = image; pageImage.setImageBitmap(image);
                    pageImage.getLayoutParams().width = width; pageImage.getLayoutParams().height = Math.round((float) width * image.getHeight() / image.getWidth()); pageImage.requestLayout();
                    pageImage.setContentDescription(documentName + ", " + getString(R.string.page, pageIndex + 1, pageCount));
                    if (old != null) old.recycle(); setBusy(false, 0);
                });
            } catch (Exception | OutOfMemoryError e) {
                if (bitmap != null) bitmap.recycle();
                runOnUiThread(() -> { if (!isDestroyed() && request == generation) { pageImage.setImageDrawable(null); if (displayed != null) { displayed.recycle(); displayed = null; } setBusy(false, R.string.render_error); } });
            }
        });
    }
    private void setBusy(boolean value, int message) {
        busy = value; open.setEnabled(!busy); status.setText(message == 0 ? "" : getString(message));
        share.setEnabled(!busy && activeFile != null); previous.setEnabled(!busy && pageIndex > 0);
        next.setEnabled(!busy && pageIndex + 1 < pageCount); pageNumber.setEnabled(!busy);
        pageNumber.setText(getString(R.string.page, pageIndex + 1, pageCount));
        zoomOut.setEnabled(!busy && zoom > 1); zoomIn.setEnabled(!busy && zoom < 3);
    }
    private void goToPage() {
        EditText input = new EditText(this); input.setInputType(InputType.TYPE_CLASS_NUMBER); input.setSingleLine();
        input.setText(String.format(java.util.Locale.getDefault(), "%d", pageIndex + 1)); input.selectAll();
        AlertDialog dialog = new AlertDialog.Builder(this).setTitle(R.string.page_title).setView(input)
            .setNegativeButton(R.string.cancel, null).setPositiveButton(R.string.go, null).create();
        dialog.setOnShowListener(d -> dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v -> {
            try {
                int page = Integer.parseInt(input.getText().toString());
                if (page < 1 || page > pageCount) throw new NumberFormatException();
                pageIndex = page - 1; renderPage(); dialog.dismiss();
            } catch (NumberFormatException e) { input.setError(getString(R.string.page_invalid, pageCount)); }
        })); dialog.show();
    }
    private void sharePdf() {
        if (activeFile == null) return;
        Uri uri = Uri.parse("content://" + getPackageName() + ".files/" + activeFile.getName());
        Intent intent = new Intent(Intent.ACTION_SEND).setType("application/pdf").putExtra(Intent.EXTRA_STREAM, uri);
        intent.setClipData(ClipData.newRawUri("PDF", uri)); intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try { startActivity(Intent.createChooser(intent, getString(R.string.share))); }
        catch (android.content.ActivityNotFoundException e) { Toast.makeText(this, R.string.share_error, Toast.LENGTH_LONG).show(); }
    }
    @Override protected void onSaveInstanceState(Bundle out) {
        super.onSaveInstanceState(out);
        if (activeFile != null) { out.putString("file", activeFile.getName()); out.putString("name", documentName); out.putInt("page", pageIndex); }
    }
    @Override protected void onDestroy() {
        generation++;
        worker.execute(() -> { if (renderer != null) { renderer.close(); renderer = null; } }); worker.shutdown();
        pageImage.setImageDrawable(null); if (displayed != null) displayed.recycle();
        super.onDestroy();
    }
}
