package org.ospreylang.casting;

import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;

// [CAST-MOBILE-DELIVERY] Storage Access Framework grants only the selected file.
// A private copy lets the local HTTP server read it after the picker closes.
public final class MediaDocuments {
    private static final long LIMIT = 1L << 30;
    private MediaDocuments() {}

    public static File importAudio(Context context, Uri source) throws IOException {
        File folder = Files.createTempDirectory(context.getFilesDir().toPath(), "media-").toFile();
        File target = new File(folder, filename(context, source));
        try (InputStream input = context.getContentResolver().openInputStream(source)) {
            if (input == null) throw new IOException("The selected file cannot be opened");
            try (FileOutputStream output = new FileOutputStream(target)) { copy(input, output); }
            return target;
        } catch (IOException | RuntimeException error) {
            try { Files.deleteIfExists(target.toPath()); Files.deleteIfExists(folder.toPath()); }
            catch (IOException cleanup) { error.addSuppressed(cleanup); }
            throw error;
        }
    }

    private static String filename(Context context, Uri source) {
        try (Cursor cursor = context.getContentResolver().query(source,
                new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                String name = cursor.getString(0);
                if (name != null && !name.isBlank() && !name.equals(".") && !name.equals(".."))
                    return name.replace('/', '_').replace('\\', '_').replace('\0', '_');
            }
        }
        return "audio.mp3";
    }

    private static void copy(InputStream input, FileOutputStream output) throws IOException {
        byte[] buffer = new byte[65536];
        long total = 0;
        int count;
        while ((count = input.read(buffer)) != -1) {
            total += count;
            if (total > LIMIT) throw new IOException("Selected audio exceeds the 1 GiB import limit");
            if (Thread.currentThread().isInterrupted()) throw new java.io.InterruptedIOException("Import cancelled");
            output.write(buffer, 0, count);
        }
        if (total == 0) throw new IOException("The selected audio file is empty");
    }
}
