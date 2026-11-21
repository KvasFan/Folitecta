# Folitecta for Android

An offline PDF reader for Android 8.0 and newer, in English and Italian. Open PDF files with the
system document picker, from another app's **Open with** or **Share** menu; move between pages,
go to a page number, zoom and share a read-only copy. No account, network or storage permission
is required. Files are copied into the app's private cache, including files from cloud providers.

This first Android app is a reader. Editing, OCR, forms, signatures and the other desktop tools
remain in the Rust desktop app. Password-protected PDFs are not supported by this reader.

## Build

Use JDK 17 or 21, Android SDK platform 35, build-tools 35.0.0 and Gradle 8.11.1:

```sh
cd android
./gradlew assembleDebug lint
```

Install a local development build with `adb install -r app/build/outputs/apk/debug/app-debug.apk`.
For a release use `./gradlew assembleRelease -PreleaseVersion=0.113.0` with these environment
variables: `ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`,
`ANDROID_KEY_PASSWORD`. Without them Gradle deliberately leaves release APKs unsigned.
The GitHub release job requires the repository's Android signing secrets and verifies the APK
signature before upload. Keep the same key for subsequent releases, so installed apps can update.
Do not commit a keystore or passwords.

PDF rendering uses the Android platform's [PdfRenderer](https://developer.android.com/reference/android/graphics/pdf/PdfRenderer)
on a background worker, with bounded bitmap sizes. Temporary share grants are read-only and
restricted to PDF cache files; other private files cannot be exposed by the content provider.
