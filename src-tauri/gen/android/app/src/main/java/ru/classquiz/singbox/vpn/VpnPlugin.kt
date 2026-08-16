package ru.classquiz.singbox.vpn

import android.app.Activity
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import io.nekohasekai.libbox.Libbox
import kotlin.concurrent.thread

@InvokeArg
class StartArgs {
  lateinit var config: String
}

@InvokeArg
class ReadLogsArgs {
  // JS side sends `maxLines`; gomobile-style parseArgs matches by
  // exact field name, so keep it identical to the frontend contract.
  var maxLines: Int = 300
}

/**
 * Tauri plugin bridging the webview to [CloakwireVpnService].
 *
 * Registered from Rust (`register_android_plugin("ru.classquiz.singbox",
 * "VpnPlugin")`), so the JS side calls `invoke("plugin:vpn|<cmd>")` and
 * subscribes with `addPluginListener("vpn", "status", cb)`.
 *
 * Commands:
 *  - prepare()      → system VPN-consent dialog (once per install)
 *  - start(config)  → write config, startForegroundService(ACTION_START)
 *  - stop()         → ACTION_STOP
 *  - status()       → { state, message, since }
 *  - listApps()     → installed packages for the per-app picker
 *  - coreVersion()  → "1.14.0-lx.…" (bare string, like desktop
 *                     `get_core_version`)
 *  - readLogs(maxLines)→ tail of the sing-box log file (bare string:
 *                     lines joined with "\n", like desktop `read_logs`)
 */
@TauriPlugin
class VpnPlugin(private val activity: Activity) : Plugin(activity) {

  override fun load(webView: WebView) {
    VpnEvents.emitter = { event, payload -> trigger(event, payload) }
  }

  override fun onDestroy() {
    VpnEvents.emitter = null
  }

  // ---- prepare ------------------------------------------------------

  @Command
  fun prepare(invoke: Invoke) {
    val intent = VpnService.prepare(activity)
    if (intent == null) {
      val ret = JSObject()
      ret.put("prepared", true)
      invoke.resolve(ret)
    } else {
      // The consent dialog result arrives in prepareResult.
      activity.runOnUiThread {
        startActivityForResult(invoke, intent, "prepareResult")
      }
    }
  }

  @ActivityCallback
  fun prepareResult(invoke: Invoke, result: ActivityResult) {
    if (result.resultCode == Activity.RESULT_OK) {
      val ret = JSObject()
      ret.put("prepared", true)
      invoke.resolve(ret)
    } else {
      invoke.reject("VPN permission denied")
    }
  }

  // ---- start / stop ---------------------------------------------------

  @Command
  fun start(invoke: Invoke) {
    thread(name = "vpn-plugin-start") {
      try {
        if (VpnService.prepare(activity) != null) {
          invoke.reject("VPN permission not granted — call prepare first")
          return@thread
        }
        val args = invoke.parseArgs(StartArgs::class.java)
        val file = CloakwireVpnService.configFile(activity)
        file.writeText(args.config)
        val intent = Intent(activity, CloakwireVpnService::class.java)
          .setAction(CloakwireVpnService.ACTION_START)
          .putExtra(CloakwireVpnService.EXTRA_CONFIG_PATH, file.absolutePath)
        activity.startForegroundService(intent)
        invoke.resolve()
      } catch (e: Exception) {
        invoke.reject(e.message ?: e.toString())
      }
    }
  }

  @Command
  fun stop(invoke: Invoke) {
    try {
      val intent = Intent(activity, CloakwireVpnService::class.java)
        .setAction(CloakwireVpnService.ACTION_STOP)
      activity.startService(intent)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject(e.message ?: e.toString())
    }
  }

  // ---- status ---------------------------------------------------------

  @Command
  fun status(invoke: Invoke) {
    invoke.resolve(VpnEvents.statusJson())
  }

  // ---- app list (per-app routing picker) ------------------------------

  @Command
  fun listApps(invoke: Invoke) {
    thread(name = "vpn-plugin-list-apps") {
      try {
        val pm = activity.packageManager
        @Suppress("DEPRECATION")
        val installed = if (Build.VERSION.SDK_INT >= 33) {
          pm.getInstalledApplications(PackageManager.ApplicationInfoFlags.of(0))
        } else {
          pm.getInstalledApplications(0)
        }
        val items = installed
          .filter { it.packageName != activity.packageName }
          .map { info ->
            val label = runCatching {
              pm.getApplicationLabel(info).toString()
            }.getOrDefault(info.packageName)
            val system = (info.flags and ApplicationInfo.FLAG_SYSTEM) != 0
            @Suppress("DEPRECATION")
            val hasInternet = runCatching {
              pm.checkPermission(
                android.Manifest.permission.INTERNET,
                info.packageName
              ) == PackageManager.PERMISSION_GRANTED
            }.getOrDefault(true)
            Triple(label, info.packageName, system to hasInternet)
          }
          .sortedBy { it.first.lowercase() }

        val arr = JSArray()
        for ((label, pkg, flags) in items) {
          val obj = JSObject()
          obj.put("packageName", pkg)
          obj.put("label", label)
          obj.put("system", flags.first)
          obj.put("hasInternet", flags.second)
          arr.put(obj)
        }
        val ret = JSObject()
        ret.put("apps", arr)
        invoke.resolve(ret)
      } catch (e: Exception) {
        invoke.reject(e.message ?: e.toString())
      }
    }
  }

  // ---- misc -----------------------------------------------------------

  @Command
  fun coreVersion(invoke: Invoke) {
    thread(name = "vpn-plugin-version") {
      try {
        // Libbox.version() loads the Go runtime on first call — never
        // run it on the IPC thread.
        invoke.resolve(JSObject().put("value", Libbox.version()))
      } catch (e: Exception) {
        invoke.reject(e.message ?: e.toString())
      }
    }
  }

  @Command
  fun readLogs(invoke: Invoke) {
    thread(name = "vpn-plugin-logs") {
      try {
        val args = invoke.parseArgs(ReadLogsArgs::class.java)
        val file = CloakwireVpnService.logFile(activity)
        val text = if (file.exists()) {
          file.readLines()
            .takeLast(args.maxLines.coerceIn(1, 2000))
            .joinToString("\n")
        } else {
          ""
        }
        invoke.resolve(JSObject().put("value", text))
      } catch (e: Exception) {
        invoke.reject(e.message ?: e.toString())
      }
    }
  }
}
