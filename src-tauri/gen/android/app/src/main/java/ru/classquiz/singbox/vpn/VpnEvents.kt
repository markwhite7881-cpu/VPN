package ru.classquiz.singbox.vpn

import app.tauri.plugin.JSObject

/**
 * Bridge between [CloakwireVpnService] (owns the real VPN state; runs
 * for as long as the process lives) and [VpnPlugin] (owns the webview
 * channel; recreated with every Activity). The service writes state
 * here, the plugin polls it via `status` and receives pushes via the
 * "status" plugin event.
 */
object VpnEvents {
  const val STATE_STOPPED = "stopped"
  const val STATE_STARTING = "starting"
  const val STATE_RUNNING = "running"
  const val STATE_ERROR = "error"

  @Volatile var state: String = STATE_STOPPED
    private set
  @Volatile var message: String = ""
    private set
  @Volatile var since: Long = 0L
    private set

  /** Set by VpnPlugin.load / cleared by onDestroy. */
  @Volatile var emitter: ((String, JSObject) -> Unit)? = null

  @Synchronized
  fun update(newState: String, newMessage: String = "") {
    state = newState
    message = newMessage
    since = System.currentTimeMillis()
    emit()
  }

  fun statusJson(): JSObject {
    val obj = JSObject()
    obj.put("state", state)
    obj.put("message", message)
    obj.put("since", since)
    return obj
  }

  private fun emit() {
    try {
      emitter?.invoke("status", statusJson())
    } catch (_: Exception) {
      // Webview may be gone (activity destroyed) — the next `status`
      // command returns the persisted state anyway.
    }
  }
}
