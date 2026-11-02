package io.github.dixonsolutions.dyt

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.Color
import android.media.MediaScannerConnection
import android.os.Build
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.MimeTypeMap
import android.webkit.WebView
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONArray
import org.json.JSONObject

/**
 * The Android side of dYT: links shared to the app ("Share → dYT", "Open with dYT"), the
 * system's light/dark scheme as it changes, the system bars, and the media store.
 * The web UI reaches it through `window.DytAndroid`; it reaches the UI through
 * `dyt-android` DOM events.
 */
class MainActivity : TauriActivity() {
    private var webView: WebView? = null
    private var pendingLink: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        applyBars()
        super.onCreate(savedInstanceState)
        pendingLink = linkFrom(intent)
        // Android 9 and older need the permission to write to Download/, Movies/ and Music/.
        if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.P &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), 1)
        }
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
        webView.addJavascriptInterface(Bridge(), "DytAndroid")
        // Keep the page clear of the status bar, the gesture bar, cut-outs and the keyboard.
        ViewCompat.setOnApplyWindowInsetsListener(webView) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            v.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
            WindowInsetsCompat.CONSUMED
        }
        webView.setBackgroundColor(background())
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        val link = linkFrom(intent) ?: return
        pendingLink = link
        emit(JSONObject().put("type", "link").put("link", link))
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // The activity handles uiMode itself (no restart, so nothing in the page is lost):
        // repaint the bars and tell the page.
        applyBars()
        webView?.setBackgroundColor(background())
        emit(JSONObject().put("type", "theme").put("dark", isDark()))
    }

    private fun isDark(): Boolean =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    /** The page's own background, so the bars and the page are one surface. */
    private fun background(): Int = if (isDark()) Color.rgb(20, 17, 18) else Color.rgb(252, 251, 251)

    private fun applyBars() {
        val style = if (isDark()) SystemBarStyle.dark(Color.TRANSPARENT) else SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        enableEdgeToEdge(style, style)
        window.decorView.setBackgroundColor(background())
    }

    private fun linkFrom(intent: Intent?): String? {
        intent ?: return null
        val text = when (intent.action) {
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)
            Intent.ACTION_VIEW -> intent.dataString
            else -> null
        } ?: return null
        // Shares often wrap the link in a sentence ("Watch this: https://youtu.be/…").
        return Regex("""https?://\S+""").find(text)?.value ?: text.trim().takeIf { it.isNotEmpty() }
    }

    private fun emit(detail: JSONObject) {
        val view = webView ?: return
        view.post {
            view.evaluateJavascript("window.dispatchEvent(new CustomEvent('dyt-android', { detail: $detail }))", null)
        }
    }

    private fun mimeOf(path: String): String =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(path.substringAfterLast('.', "").lowercase()) ?: "*/*"

    inner class Bridge {
        /** The link the app was opened or shared with, once. */
        @JavascriptInterface
        fun takeLink(): String? {
            val l = pendingLink
            pendingLink = null
            return l
        }

        @JavascriptInterface
        fun isDark(): Boolean = this@MainActivity.isDark()

        /** Make new files visible to galleries, players and file managers right away. */
        @JavascriptInterface
        fun scanFiles(json: String) {
            val arr = JSONArray(json)
            val paths = Array(arr.length()) { arr.getString(it) }
            MediaScannerConnection.scanFile(this@MainActivity, paths, paths.map { mimeOf(it) }.toTypedArray(), null)
        }

        /** Open a downloaded file in whichever app handles it. */
        @JavascriptInterface
        fun openFile(path: String) {
            MediaScannerConnection.scanFile(this@MainActivity, arrayOf(path), arrayOf(mimeOf(path))) { _, uri ->
                if (uri == null) return@scanFile
                val view = Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, mimeOf(path))
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
                runOnUiThread {
                    try {
                        startActivity(Intent.createChooser(view, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    } catch (_: Exception) {
                    }
                }
            }
        }
    }
}
