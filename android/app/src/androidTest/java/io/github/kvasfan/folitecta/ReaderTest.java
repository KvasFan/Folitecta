package io.github.kvasfan.folitecta;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Context;
import android.content.Intent;
import android.content.ClipData;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.TextView;
import androidx.test.platform.app.InstrumentationRegistry;
import org.junit.Test;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import static org.junit.Assert.*;

/** Exercises the real renderer, content grants, navigation and error recovery on a device. */
public class ReaderTest {
    private final Instrumentation instrumentation = InstrumentationRegistry.getInstrumentation();
    private final Context app = instrumentation.getTargetContext();
    private Activity launch(Intent intent) {
        intent.setClass(app, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        return instrumentation.startActivitySync(intent);
    }
    private Uri fixture(String name, boolean valid) throws Exception {
        File dir = new File(app.getCacheDir(), "documents"); dir.mkdirs();
        File pdf = new File(dir, name);
        try (FileOutputStream out = new FileOutputStream(pdf)) {
            if (valid) {
                try (InputStream in = instrumentation.getContext().getAssets().open("sample.pdf")) { byte[] bytes = new byte[8192]; int n; while ((n = in.read(bytes)) != -1) out.write(bytes, 0, n); }
            } else out.write("This is not a PDF".getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        return Uri.parse("content://" + app.getPackageName() + ".files/" + name);
    }
    private TextView find(View view, String text, boolean description) {
        if (view instanceof TextView && text.contentEquals(description ?
            (view.getContentDescription() == null ? "" : view.getContentDescription()) : ((TextView) view).getText())) return (TextView) view;
        if (view instanceof ViewGroup) for (int i = 0; i < ((ViewGroup) view).getChildCount(); i++) {
            TextView match = find(((ViewGroup) view).getChildAt(i), text, description); if (match != null) return match;
        }
        return null;
    }
    private void await(Activity activity, String text, boolean enabled) throws Exception {
        for (int n = 0; n < 150; n++) {
            boolean[] found = {false};
            instrumentation.runOnMainSync(() -> { TextView v = find(activity.getWindow().getDecorView(), text, false); found[0] = v != null && (!enabled || v.isEnabled()); });
            if (found[0]) return;
            Thread.sleep(100);
        }
        fail("UI did not reach: " + text);
    }
    @Test public void readsAndNavigatesPdf() throws Exception {
        Uri pdf = fixture("document-11111111-1111-1111-1111-111111111111.pdf", true);
        Activity activity = launch(new Intent(Intent.ACTION_VIEW, pdf).setType("application/pdf").setData(pdf).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION));
        try {
            await(activity, app.getString(R.string.page, 1, 4), true);
            instrumentation.runOnMainSync(() -> {
                Button next = (Button) find(activity.getWindow().getDecorView(), app.getString(R.string.next), true);
                assertNotNull(next); assertTrue(next.isEnabled()); next.performClick();
            });
            await(activity, app.getString(R.string.page, 2, 4), true);
            instrumentation.runOnMainSync(() -> {
                Button share = (Button) find(activity.getWindow().getDecorView(), app.getString(R.string.share), false);
                assertNotNull(share); assertTrue(share.isEnabled());
            });
        } finally { instrumentation.runOnMainSync(activity::finish); }
    }
    @Test public void rejectsDamagedPdfAndRecovers() throws Exception {
        Uri bad = fixture("document-22222222-2222-2222-2222-222222222222.pdf", false);
        Intent send = new Intent(Intent.ACTION_SEND).setType("application/pdf").putExtra(Intent.EXTRA_STREAM, bad);
        send.setClipData(ClipData.newRawUri("PDF", bad)); send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        Activity activity = launch(send);
        try {
            await(activity, app.getString(R.string.open), true);
            // Opening another document stays available after the failure; process stays alive.
            assertFalse(activity.isFinishing());
        } finally { instrumentation.runOnMainSync(activity::finish); }
    }
    @Test public void sharedFilesAreReadOnlyAndConfined() throws Exception {
        Uri pdf = fixture("document-33333333-3333-3333-3333-333333333333.pdf", true);
        try (ParcelFileDescriptor descriptor = app.getContentResolver().openFileDescriptor(pdf, "r")) { assertNotNull(descriptor); }
        for (Uri uri : new Uri[]{pdf, Uri.parse("content://" + app.getPackageName() + ".files/../secret.pdf")}) {
            try { app.getContentResolver().openFileDescriptor(uri, "rw"); fail("Write grant accepted"); }
            catch (java.io.FileNotFoundException | SecurityException expected) { }
        }
        try { app.getContentResolver().openFileDescriptor(Uri.parse("content://" + app.getPackageName() + ".files/../secret.pdf"), "r"); fail("Traversal accepted"); }
        catch (java.io.FileNotFoundException | SecurityException expected) { }
    }
}
