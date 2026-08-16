package ru.classquiz.singbox.vpn

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import io.nekohasekai.libbox.CommandServer
import io.nekohasekai.libbox.CommandServerHandler
import io.nekohasekai.libbox.Libbox
import io.nekohasekai.libbox.OverrideOptions
import io.nekohasekai.libbox.SetupOptions
import io.nekohasekai.libbox.SystemProxyStatus
import org.json.JSONObject
import ru.classquiz.singbox.R
import java.io.File
import kotlin.concurrent.thread

/**
 * Foreground [VpnService] hosting the sing-box core (libbox) in-process.
 *
 * Lifecycle: the webview calls `plugin:vpn|start` → [VpnPlugin] writes
 * the generated config to `filesDir/vpn-config.json` and sends
 * [ACTION_START] → we startForeground, read the config, inject a log
 * file path, bring up libbox ([Libbox.setup] once per process, then a
 * fresh [CommandServer] per session) → [CloakwirePlatform.openTun]
 * establishes the actual tunnel. [ACTION_STOP] tears it down.
 *
 * All libbox calls are blocking and run on a dedicated worker thread;
 * state reaches the UI through [VpnEvents] (poll + plugin events).
 */
class CloakwireVpnService : VpnService() {

  companion object {
    const val ACTION_START = "ru.classquiz.singbox.START"
    const val ACTION_STOP = "ru.classquiz.singbox.STOP"
    const val EXTRA_CONFIG_PATH = "configPath"

    private const val TAG = "CloakwireVpnService"
    private const val NOTIFICATION_ID = 1
    private const val CHANNEL_ID = "cloakwire_vpn"

    /** libbox global setup is process-wide and must happen once. */
    @Volatile private var libboxReady = false

    /** Live service instance (null while stopped). */
    @Volatile var active: CloakwireVpnService? = null
      private set

    fun configFile(service: android.content.Context): File =
      File(service.filesDir, "vpn-config.json")

    fun logFile(context: android.content.Context): File =
      File(File(context.filesDir, "singbox"), "box.log")
  }

  private var commandServer: CommandServer? = null

  /**
   * The ParcelFileDescriptor returned by Builder.establish(). Ownership
   * of the fd passes to Go via detachFd(); we keep the wrapper only so
   * a failed start can close it before that happens.
   */
  @Volatile private var tunFd: ParcelFileDescriptor? = null

  /** Called by [CloakwirePlatform.openTun] right after establish(). */
  fun onTunEstablished(pfd: ParcelFileDescriptor) {
    tunFd = pfd
  }

  fun mainActivityPendingIntent(): PendingIntent {
    val intent = packageManager.getLaunchIntentForPackage(packageName)
    val flags = PendingIntent.FLAG_UPDATE_CURRENT or
      (if (Build.VERSION.SDK_INT >= 23) PendingIntent.FLAG_IMMUTABLE else 0)
    return PendingIntent.getActivity(this, 0, intent, flags)
  }

  override fun onCreate() {
    super.onCreate()
    active = this
    createNotificationChannel()
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    when (intent?.action) {
      ACTION_STOP -> {
        stopVpn()
        return START_NOT_STICKY
      }
      ACTION_START -> {
        val configPath = intent.getStringExtra(EXTRA_CONFIG_PATH)
        if (configPath.isNullOrEmpty()) {
          VpnEvents.update(VpnEvents.STATE_ERROR, "missing config path")
          stopSelf()
          return START_NOT_STICKY
        }
        // Foreground must be entered quickly after
        // startForegroundService, before any heavy work.
        startForegroundWith("Connecting…")
        VpnEvents.update(VpnEvents.STATE_STARTING)
        thread(name = "vpn-start") { runVpn(configPath) }
        return START_STICKY
      }
      else -> {
        // Restarted by the system without an action (process recreated
        // while the VPN was up). Report the truth: without a config we
        // cannot resume — stay stopped.
        if (commandServer == null) {
          VpnEvents.update(VpnEvents.STATE_STOPPED)
          stopSelf()
        }
        return START_NOT_STICKY
      }
    }
  }

  private fun runVpn(configPath: String) {
    try {
      val raw = File(configPath).readText()
      val config = injectLogFile(raw)

      setupLibboxOnce()

      // A previous session (server switch while connected = another
      // ACTION_START) — tear it down before creating a fresh server.
      commandServer?.let { old ->
        runCatching { old.closeService() }
        runCatching { old.close() }
      }
      commandServer = null

      val server = Libbox.newCommandServer(Handler(), CloakwirePlatform(this))
      commandServer = server
      server.start()

      val overrides = OverrideOptions()
      // auto_redirect would let the core manage fwmark-based per-app
      // rules itself; we do per-app via VpnService.Builder + route
      // rules instead, which keeps the semantics visible in the UI.
      overrides.autoRedirect = false

      server.startOrReloadService(config, overrides)
      VpnEvents.update(VpnEvents.STATE_RUNNING)
      startForegroundWith("Connected")
    } catch (e: Exception) {
      Log.e(TAG, "VPN start failed", e)
      VpnEvents.update(VpnEvents.STATE_ERROR, e.message ?: e.toString())
      runCatching { commandServer?.setError(e.message ?: "start failed") }
      stopVpn()
    }
  }

  /** Route sing-box's text log to a file the UI can tail. */
  private fun injectLogFile(configContent: String): String {
    return try {
      val json = JSONObject(configContent)
      val log = json.optJSONObject("log") ?: JSONObject().also { json.put("log", it) }
      val file = logFile(this)
      file.parentFile?.mkdirs()
      // Start each session with a fresh log — sing-box opens the file
      // in append mode and we don't want unbounded growth.
      if (file.exists()) file.delete()
      log.put("output", file.absolutePath)
      json.toString()
    } catch (e: Exception) {
      Log.w(TAG, "log injection failed, using config as-is", e)
      configContent
    }
  }

  @Synchronized
  private fun setupLibboxOnce() {
    if (libboxReady) return
    val options = SetupOptions()
    options.basePath = filesDir.absolutePath
    options.workingPath = File(filesDir, "singbox").apply { mkdirs() }.absolutePath
    options.tempPath = cacheDir.absolutePath
    // golang/go#68760 — without this the Go runtime's netstack calls
    // can misbehave on Android.
    options.fixAndroidStack = true
    options.logMaxLines = 300L // setLogMaxLines takes a Java long
    options.debug = false
    Libbox.setup(options)
    libboxReady = true
  }

  @Synchronized
  fun stopVpn() {
    VpnEvents.update(VpnEvents.STATE_STOPPED)
    val server = commandServer
    commandServer = null
    if (server != null) {
      thread(name = "vpn-stop") {
        runCatching { server.closeService() }
        runCatching { server.close() }
      }
    }
    runCatching { tunFd?.close() }
    tunFd = null
    stopForeground(true)
    stopSelf()
  }

  override fun onDestroy() {
    // If the system kills the VPN (revoke/always-on change), onDestroy
    // fires without ACTION_STOP — make sure the core goes down too.
    val server = commandServer
    commandServer = null
    if (server != null) {
      runCatching { server.closeService() }
      runCatching { server.close() }
    }
    runCatching { tunFd?.close() }
    tunFd = null
    active = null
    if (VpnEvents.state != VpnEvents.STATE_STOPPED) {
      VpnEvents.update(VpnEvents.STATE_STOPPED)
    }
    super.onDestroy()
  }

  override fun onRevoke() {
    Log.i(TAG, "VPN permission revoked by the system")
    VpnEvents.update(VpnEvents.STATE_STOPPED, "VPN permission revoked")
    stopVpn()
    super.onRevoke()
  }

  // ---- Notification ---------------------------------------------------

  private fun createNotificationChannel() {
    if (Build.VERSION.SDK_INT >= 26) {
      val channel = NotificationChannel(
        CHANNEL_ID,
        "VPN status",
        NotificationManager.IMPORTANCE_LOW
      ).apply {
        description = "Shown while the Cloakwire VPN is active"
        setShowBadge(false)
      }
      getSystemService(NotificationManager::class.java)?.createNotificationChannel(channel)
    }
  }

  private fun startForegroundWith(text: String) {
    val notification = buildNotification(text)
    if (Build.VERSION.SDK_INT >= 34) {
      startForeground(
        NOTIFICATION_ID,
        notification,
        ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
      )
    } else {
      startForeground(NOTIFICATION_ID, notification)
    }
  }

  private fun buildNotification(text: String): Notification {
    val stopIntent = PendingIntent.getService(
      this, 0,
      Intent(this, CloakwireVpnService::class.java).setAction(ACTION_STOP),
      PendingIntent.FLAG_UPDATE_CURRENT or
        (if (Build.VERSION.SDK_INT >= 23) PendingIntent.FLAG_IMMUTABLE else 0)
    )
    val builder = if (Build.VERSION.SDK_INT >= 26) {
      Notification.Builder(this, CHANNEL_ID)
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(this)
    }
    return builder
      .setContentTitle("Cloakwire")
      .setContentText(text)
      .setSmallIcon(R.drawable.ic_vpn_key)
      .setContentIntent(mainActivityPendingIntent())
      .setOngoing(true)
      .addAction(
        Notification.Action.Builder(null, "Disconnect", stopIntent).build()
      )
      .build()
  }

  // ---- Core callbacks ---------------------------------------------------

  /**
   * [CommandServerHandler] — the core calls into these for
   * desktop-centric facilities. Only [serviceStop] matters here (the
   * core asks us to shut down, e.g. after a fatal error).
   */
  private inner class Handler : CommandServerHandler {
    override fun serviceStop() {
      stopVpn()
    }

    override fun serviceReload() {}

    override fun getSystemProxyStatus(): SystemProxyStatus {
      val status = SystemProxyStatus()
      status.available = false
      status.enabled = false
      return status
    }

    override fun setSystemProxyEnabled(enabled: Boolean) {}

    override fun triggerNativeCrash() {
      throw Exception("triggerNativeCrash is a debug facility — not wired up")
    }

    override fun writeDebugMessage(message: String) {
      Log.d(TAG, "core: $message")
    }

    override fun connectSSHAgent(): Int = -1
  }
}
